// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Secure source capture and SQLite-only-private-copy materialization.

use std::fs::{File, OpenOptions};
use std::io::{Read, Write};
use std::path::{Component, Path, PathBuf};
use std::time::{Duration, Instant};

use rusqlite::{Connection, OpenFlags, limits::Limit};
use tempfile::{Builder, TempDir};
use zeroize::Zeroizing;

use crate::query;
use crate::wal::{
    MAX_DATABASE_BYTES, MAX_WAL_BYTES, WalError, validate_database, validate_wal,
};
use crate::query::OmpCredential;
use crate::{OmpAccount, OmpError, OmpSelector};

const OPERATION_BUDGET: Duration = Duration::from_secs(10);
const SQLITE_MIN_VERSION: i32 = 3_051_003;
const MAX_SECRET_VALUE_BYTES: i32 = 64 * 1024;
const MAX_SQL_BYTES: i32 = 64 * 1024;
const MAX_SQLITE_VIRTUAL_OPS: i32 = 100_000;

/// One stable, strictly validated OMP credential-store capture.
///
/// This type deliberately has no `Debug` or serialization implementation:
/// its connection and private files contain credential values.
pub struct OmpSnapshot {
    scratch: TempDir,
    connection: Option<Connection>,
    accounts: Vec<OmpAccount>,
    deadline: Instant,
}

/// A unique exact selection borrowed from one [`OmpSnapshot`].
///
/// The account identity is safe to inspect; the underlying secret is not
/// exposed. Consuming this value can only materialize the selected snapshot.
pub struct OmpSelectedAccount<'a> {
    snapshot: &'a mut OmpSnapshot,
    account: OmpAccount,
    credential: OmpCredential,
}

/// Removes the private materialized image and SQLite sidecars at scope exit.
/// The source capture remains owned by `OmpSnapshot`, but backup output is
/// temporary and must not outlive the attempt that created it.
struct PrivateDatabaseCleanup {
    database_path: PathBuf,
    active: bool,
}

impl PrivateDatabaseCleanup {
    fn new(database_path: PathBuf) -> Self {
        Self {
            database_path,
            active: true,
        }
    }

    fn cleanup(&mut self) -> Result<(), OmpError> {
        for path in self.paths() {
            match std::fs::remove_file(path) {
                Ok(()) => {}
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                Err(_) => return Err(OmpError::Unavailable),
            }
        }
        self.active = false;
        Ok(())
    }

    fn paths(&self) -> impl Iterator<Item = PathBuf> + '_ {
        std::iter::once(self.database_path.clone()).chain(["-wal", "-shm", "-journal"].map(
            |suffix| {
                let mut path = self.database_path.as_os_str().to_owned();
                path.push(suffix);
                PathBuf::from(path)
            },
        ))
    }
}

impl Drop for PrivateDatabaseCleanup {
    fn drop(&mut self) {
        if self.active {
            for path in self.paths() {
                let _ = std::fs::remove_file(path);
            }
        }
    }
}

impl Drop for OmpSnapshot {
    fn drop(&mut self) {
        // Close SQLite before TempDir removes the private database and any
        // SQLite-created sidecars from the scratch directory.
        drop(self.connection.take());
    }
}

impl OmpSnapshot {
    /// Capture `.omp/agent/agent.db` and its optional WAL from a directory
    /// opened by the caller with its own source lock and path policy.
    #[cfg(unix)]
    pub fn capture_from_root(root: &File) -> Result<Option<Self>, OmpError> {
        let deadline = Instant::now() + OPERATION_BUDGET;
        capture_from_root_inner(root, deadline, || {})
    }

    #[cfg(unix)]
    fn capture_from_root_inner(
        root: &File,
        deadline: Instant,
        after_first_pass: impl FnOnce(),
    ) -> Result<Option<Self>, OmpError> {
        let root_identity = secure_directory_identity(root, true)?;
        let first = read_source_pair(root, root_identity, deadline)?;
        after_first_pass();
        let second = read_source_pair(root, root_identity, deadline)?;
        match (first, second) {
            (None, None) => Ok(None),
            (Some(first), Some(second)) if first.same_capture(&second) => {
                Self::from_captured_pair(first, deadline).map(Some)
            }
            _ => Err(OmpError::Unavailable),
        }
    }

    /// Capture a store beneath `source_directory`. Every path component is
    /// opened relative to a directory descriptor with symlink following
    /// disabled; missing source directories return `Ok(None)`.
    pub fn capture_from_directory(
        source_directory: impl AsRef<Path>,
    ) -> Result<Option<Self>, OmpError> {
        #[cfg(unix)]
        {
            let deadline = Instant::now() + OPERATION_BUDGET;
            let Some(root) = open_source_directory(source_directory.as_ref(), deadline)? else {
                return Ok(None);
            };
            Self::capture_from_root_inner(&root, deadline, || {})
        }
        #[cfg(not(unix))]
        {
            let _ = source_directory;
            Err(OmpError::Unavailable)
        }
    }

    /// Secret-free account identities from this exact captured image.
    pub fn accounts(&self) -> &[OmpAccount] {
        &self.accounts
    }

    /// Select one exact source row. A selector never falls back to a sibling;
    /// when it is absent, the provider must have exactly one active row.
    pub fn select(
        &mut self,
        provider: Option<&str>,
        selector: Option<&OmpSelector>,
    ) -> Result<OmpSelectedAccount<'_>, OmpError> {
        let account = if let Some(selector) = selector {
            let selected_id = selector
                .profile
                .as_deref()
                .and_then(parse_row_profile)
                .ok_or(OmpError::SelectionUnavailable)?;
            self.accounts
                .iter()
                .find(|account| {
                    account.id == selected_id
                        && account.entry == selector.entry
                        && account.profile == selector.profile.as_deref().unwrap_or_default()
                        && provider.is_none_or(|provider| account.entry == provider)
                })
                .cloned()
                .ok_or(OmpError::SelectionUnavailable)?
        } else {
            let mut candidates = self
                .accounts
                .iter()
                .filter(|account| provider.is_none_or(|provider| account.entry == provider));
            let account = candidates
                .next()
                .filter(|_| candidates.next().is_none())
                .cloned()
                .ok_or(OmpError::SelectionUnavailable)?;
            account
        };
        let connection = self.connection.as_ref().ok_or(OmpError::Unavailable)?;
        let credential = query::selected_credential(connection, &account, self.deadline)?;
        Ok(OmpSelectedAccount {
            snapshot: self,
            account,
            credential,
        })
    }

    #[cfg(unix)]
    fn from_captured_pair(pair: SourcePair, deadline: Instant) -> Result<Self, OmpError> {
        check_deadline(deadline)?;
        if rusqlite::version_number() < SQLITE_MIN_VERSION {
            return Err(OmpError::Unavailable);
        }
        let database_header = validate_database(&pair.database, deadline).map_err(map_wal_error)?;
        if let Some(wal) = pair.wal.as_deref() {
            validate_wal(wal, database_header, deadline).map_err(map_wal_error)?;
        }

        let scratch = Builder::new()
            .prefix("jackin-omp-private-")
            .tempdir()
            .map_err(|_| OmpError::Unavailable)?;
        validate_private_directory(scratch.path())?;
        let database_path = scratch.path().join("captured.db");
        write_private_file(&database_path, &pair.database, deadline)?;
        if let Some(wal) = pair.wal.as_deref() {
            let mut wal_name = database_path.as_os_str().to_owned();
            wal_name.push("-wal");
            let wal_path = PathBuf::from(wal_name);
            write_private_file(&wal_path, wal, deadline)?;
        }
        check_deadline(deadline)?;

        let connection = Connection::open_with_flags(
            &database_path,
            OpenFlags::SQLITE_OPEN_READWRITE,
        )
        .map_err(|_| OmpError::Unavailable)?;
        configure_connection(&connection, deadline, true)?;
        let accounts = query::enumerate(&connection, deadline)?;
        check_deadline(deadline)?;
        if sidecar_exists(&database_path, "-journal")? {
            return Err(OmpError::Unavailable);
        }

        Ok(Self {
            scratch,
            connection: Some(connection),
            accounts,
            deadline,
        })
    }
}

impl OmpSelectedAccount<'_> {
    /// Non-secret provider/profile identity validated against this snapshot.
    pub fn account(&self) -> &OmpAccount {
        &self.account
    }

    /// Write a fresh OMP v7 role database containing only this selected row.
    /// No source database page, schema object, sidecar or sibling credential
    /// crosses into the role-owned output.
    pub fn write_standalone_database(
        self,
        destination: &mut impl Write,
    ) -> Result<(), OmpError> {
        let bytes = self.snapshot.materialize_selected(&self.credential)?;
        check_deadline(self.snapshot.deadline)?;
        destination
            .write_all(bytes.as_slice())
            .map_err(|_| OmpError::Unavailable)?;
        destination.flush().map_err(|_| OmpError::Unavailable)
    }
}

impl OmpSnapshot {
    fn materialize_selected(
        &mut self,
        credential: &OmpCredential,
    ) -> Result<Zeroizing<Vec<u8>>, OmpError> {
        check_deadline(self.deadline)?;
        drop(self.connection.take());
        let destination_path = self.scratch.path().join("selected-role.db");
        create_private_file(&destination_path)?;
        let mut destination_cleanup = PrivateDatabaseCleanup::new(destination_path.clone());
        let destination = Connection::open_with_flags(
            &destination_path,
            OpenFlags::SQLITE_OPEN_READWRITE,
        )
        .map_err(|_| OmpError::Unavailable)?;

        let materialization = write_selected_role_database(&destination, credential, self.deadline)
            .and_then(|()| verify_selected_role_database(&destination, credential, self.deadline));
        let destination_closed = close_succeeded(destination.close());
        materialization?;
        if !destination_closed {
            return Err(OmpError::Unavailable);
        }

        for suffix in ["-wal", "-shm", "-journal"] {
            if sidecar_exists(&destination_path, suffix)? {
                return Err(OmpError::Unavailable);
            }
        }
        let metadata = std::fs::symlink_metadata(&destination_path)
            .map_err(|_| OmpError::Unavailable)?;
        if !metadata.is_file()
            || metadata.file_type().is_symlink()
            || usize::try_from(metadata.len()).map_or(true, |size| size > MAX_DATABASE_BYTES)
        {
            return Err(OmpError::LimitExceeded);
        }
        let mut output = Zeroizing::new(Vec::with_capacity(
            usize::try_from(metadata.len()).map_err(|_| OmpError::LimitExceeded)?,
        ));
        let mut file = File::open(&destination_path).map_err(|_| OmpError::Unavailable)?;
        read_private_output(&mut file, &mut output, metadata.len(), self.deadline)?;
        validate_database(&output, self.deadline).map_err(map_wal_error)?;
        drop(file);
        destination_cleanup.cleanup()?;
        Ok(output)
    }
}

const ROLE_DATABASE_SCHEMA: &str = r#"
CREATE TABLE auth_schema_version (
    id INTEGER PRIMARY KEY CHECK (id = 1),
    version INTEGER NOT NULL
);
INSERT INTO auth_schema_version (id, version) VALUES (1, 7);
CREATE TABLE auth_credentials (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    provider TEXT NOT NULL,
    credential_type TEXT NOT NULL,
    data TEXT NOT NULL,
    disabled_cause TEXT DEFAULT NULL,
    identity_key TEXT DEFAULT NULL,
    created_at INTEGER NOT NULL DEFAULT (CAST(strftime('%s','now') AS INTEGER)),
    updated_at INTEGER NOT NULL DEFAULT (CAST(strftime('%s','now') AS INTEGER))
);
CREATE INDEX idx_auth_provider ON auth_credentials(provider);
CREATE INDEX idx_auth_provider_identity
    ON auth_credentials(provider, identity_key) WHERE identity_key IS NOT NULL;
"#;

fn write_selected_role_database(
    connection: &Connection,
    credential: &OmpCredential,
    deadline: Instant,
) -> Result<(), OmpError> {
    check_deadline(deadline)?;
    connection
        .pragma_update(None, "journal_mode", "DELETE")
        .map_err(|_| OmpError::Unavailable)?;
    connection
        .pragma_update(None, "secure_delete", "ON")
        .map_err(|_| OmpError::Unavailable)?;
    configure_connection(connection, deadline, false)?;
    let transaction = connection
        .unchecked_transaction()
        .map_err(|_| OmpError::Unavailable)?;
    transaction
        .execute_batch(ROLE_DATABASE_SCHEMA)
        .map_err(|_| OmpError::Unavailable)?;
    transaction
        .execute(
            "INSERT INTO auth_credentials
                (id, provider, credential_type, data, disabled_cause, identity_key, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, NULL, ?5, ?6, ?7)",
            rusqlite::params![
                credential.id,
                credential.provider,
                credential.credential_type,
                credential.data.as_str(),
                credential.identity_key,
                credential.created_at,
                credential.updated_at,
            ],
        )
        .map_err(|_| OmpError::Unavailable)?;
    check_deadline(deadline)?;
    transaction.commit().map_err(|_| OmpError::Unavailable)?;
    check_deadline(deadline)
}

fn verify_selected_role_database(
    connection: &Connection,
    credential: &OmpCredential,
    deadline: Instant,
) -> Result<(), OmpError> {
    check_deadline(deadline)?;
    let accounts = query::enumerate(connection, deadline)?;
    if accounts.len() != 1
        || accounts[0].id != credential.id
        || accounts[0].entry != credential.provider
    {
        return Err(OmpError::Unavailable);
    }

    let row = connection
        .query_row(
            "SELECT provider, credential_type, data, disabled_cause, identity_key, created_at, updated_at
             FROM auth_credentials WHERE id = ?1",
            [credential.id],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    Zeroizing::new(row.get::<_, String>(2)?),
                    row.get::<_, Option<String>>(3)?,
                    row.get::<_, Option<String>>(4)?,
                    row.get::<_, i64>(5)?,
                    row.get::<_, i64>(6)?,
                ))
            },
        )
        .map_err(|_| OmpError::Unavailable)?;
    if row.0 != credential.provider
        || row.1 != credential.credential_type
        || row.2.as_str() != credential.data.as_str()
        || row.3.is_some()
        || row.4 != credential.identity_key
        || row.5 != credential.created_at
        || row.6 != credential.updated_at
    {
        return Err(OmpError::Unavailable);
    }

    let mut statement = connection
        .prepare(
            "SELECT type, name FROM main.sqlite_master
             WHERE name NOT LIKE 'sqlite_%' ORDER BY type, name",
        )
        .map_err(|_| OmpError::Unavailable)?;
    let mut rows = statement.query([]).map_err(|_| OmpError::Unavailable)?;
    let expected = [
        ("index", "idx_auth_provider"),
        ("index", "idx_auth_provider_identity"),
        ("table", "auth_credentials"),
        ("table", "auth_schema_version"),
    ];
    for (expected_type, expected_name) in expected {
        check_deadline(deadline)?;
        let row = rows.next().map_err(|_| OmpError::Unavailable)?;
        let Some(row) = row else {
            return Err(OmpError::Unavailable);
        };
        let object_type: String = row.get(0).map_err(|_| OmpError::Unavailable)?;
        let name: String = row.get(1).map_err(|_| OmpError::Unavailable)?;
        if object_type != expected_type || name != expected_name {
            return Err(OmpError::Unavailable);
        }
    }
    if rows.next().map_err(|_| OmpError::Unavailable)?.is_some() {
        return Err(OmpError::Unavailable);
    }
    check_deadline(deadline)
}

fn parse_row_profile(profile: &str) -> Option<i64> {
    let id = profile.strip_prefix("row:")?.parse::<i64>().ok()?;
    (id > 0 && profile == format!("row:{id}")).then_some(id)
}

fn configure_connection(
    connection: &Connection,
    deadline: Instant,
    query_only: bool,
) -> Result<(), OmpError> {
    if rusqlite::version_number() < SQLITE_MIN_VERSION {
        return Err(OmpError::Unavailable);
    }
    connection
        .busy_timeout(Duration::ZERO)
        .map_err(|_| OmpError::Unavailable)?;
    connection
        .progress_handler(1000, Some(move || Instant::now() >= deadline))
        .map_err(|_| OmpError::Unavailable)?;
    for (limit, value) in [
        (Limit::SQLITE_LIMIT_LENGTH, MAX_SECRET_VALUE_BYTES),
        (Limit::SQLITE_LIMIT_SQL_LENGTH, MAX_SQL_BYTES),
        (Limit::SQLITE_LIMIT_COLUMN, 64),
        (Limit::SQLITE_LIMIT_EXPR_DEPTH, 100),
        (Limit::SQLITE_LIMIT_COMPOUND_SELECT, 16),
        (Limit::SQLITE_LIMIT_VDBE_OP, MAX_SQLITE_VIRTUAL_OPS),
        (Limit::SQLITE_LIMIT_FUNCTION_ARG, 32),
        (Limit::SQLITE_LIMIT_ATTACHED, 0),
        (Limit::SQLITE_LIMIT_VARIABLE_NUMBER, 64),
        (Limit::SQLITE_LIMIT_WORKER_THREADS, 0),
    ] {
        connection
            .set_limit(limit, value)
            .map_err(|_| OmpError::Unavailable)?;
    }
    connection
        .pragma_update(None, "trusted_schema", false)
        .map_err(|_| OmpError::Unavailable)?;
    if query_only {
        connection
            .pragma_update(None, "query_only", true)
            .map_err(|_| OmpError::Unavailable)?;
    }
    Ok(())
}

#[cfg(unix)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct FileIdentity {
    device: u64,
    inode: u64,
    uid: u32,
    mode: u32,
    links: u64,
    size: i64,
    modified: i64,
    changed: i64,
}

#[cfg(unix)]
impl FileIdentity {
    fn from_stat(stat: &nix::sys::stat::FileStat) -> Self {
        Self {
            device: stat.st_dev as u64,
            inode: stat.st_ino as u64,
            uid: stat.st_uid,
            mode: stat.st_mode,
            links: stat.st_nlink as u64,
            size: stat.st_size as i64,
            modified: stat.st_mtime as i64,
            changed: stat.st_ctime as i64,
        }
    }
}

#[cfg(unix)]
struct CapturedFile {
    identity: FileIdentity,
    bytes: Zeroizing<Vec<u8>>,
}

#[cfg(unix)]
struct SourcePair {
    agent_identity: FileIdentity,
    database: Zeroizing<Vec<u8>>,
    database_identity: FileIdentity,
    wal: Option<Zeroizing<Vec<u8>>>,
    wal_identity: Option<FileIdentity>,
}

#[cfg(unix)]
impl SourcePair {
    fn same_capture(&self, other: &Self) -> bool {
        self.agent_identity == other.agent_identity
            && self.database_identity == other.database_identity
            && self.database.as_slice() == other.database.as_slice()
            && self.wal_identity == other.wal_identity
            && self.wal.as_deref().map(Vec::as_slice) == other.wal.as_deref().map(Vec::as_slice)
    }
}

#[cfg(unix)]
fn read_source_pair(
    root: &File,
    expected_root: FileIdentity,
    deadline: Instant,
) -> Result<Option<SourcePair>, OmpError> {
    use std::ffi::CString;
    use std::os::fd::OwnedFd;

    use nix::errno::Errno;
    use nix::fcntl::{AtFlags, OFlag, openat};
    use nix::sys::stat::{Mode, SFlag, fstat, fstatat};
    use nix::unistd::geteuid;

    check_deadline(deadline)?;
    if FileIdentity::from_stat(&fstat(root).map_err(|_| OmpError::Unavailable)?) != expected_root {
        return Err(OmpError::Unavailable);
    }
    let agent_name = CString::new("agent").map_err(|_| OmpError::Unavailable)?;
    let agent_stat = match fstatat(root, agent_name.as_c_str(), AtFlags::AT_SYMLINK_NOFOLLOW) {
        Ok(stat) => stat,
        Err(Errno::ENOENT) => {
            if FileIdentity::from_stat(&fstat(root).map_err(|_| OmpError::Unavailable)?)
                != expected_root
            {
                return Err(OmpError::Unavailable);
            }
            return Ok(None);
        }
        Err(_) => return Err(OmpError::Unavailable),
    };
    if !SFlag::from_bits_truncate(agent_stat.st_mode).contains(SFlag::S_IFDIR)
        || agent_stat.st_uid != geteuid().as_raw()
        || agent_stat.st_mode & 0o022 != 0
    {
        return Err(OmpError::Unavailable);
    }
    let agent_fd = openat(
        root,
        agent_name.as_c_str(),
        OFlag::O_RDONLY | OFlag::O_DIRECTORY | OFlag::O_NOFOLLOW | OFlag::O_CLOEXEC,
        Mode::empty(),
    )
    .map_err(|_| OmpError::Unavailable)?;
    let agent = File::from(OwnedFd::from(agent_fd));
    let agent_identity = FileIdentity::from_stat(&fstat(&agent).map_err(|_| OmpError::Unavailable)?);
    if agent_identity != FileIdentity::from_stat(&agent_stat) {
        return Err(OmpError::Unavailable);
    }
    reject_rollback_journal(&agent)?;

    let db_name = CString::new("agent.db").map_err(|_| OmpError::Unavailable)?;
    let database = read_source_file(&agent, &db_name, MAX_DATABASE_BYTES, deadline)?;
    let Some(database) = database else {
        let wal_name = CString::new("agent.db-wal").map_err(|_| OmpError::Unavailable)?;
        if entry_identity(&agent, &wal_name)?.is_some() {
            return Err(OmpError::Unavailable);
        }
        reject_rollback_journal(&agent)?;
        let agent_after = FileIdentity::from_stat(&fstat(&agent).map_err(|_| OmpError::Unavailable)?);
        let root_after = FileIdentity::from_stat(&fstat(root).map_err(|_| OmpError::Unavailable)?);
        if agent_after != agent_identity || root_after != expected_root {
            return Err(OmpError::Unavailable);
        }
        return Ok(None);
    };
    let wal_name = CString::new("agent.db-wal").map_err(|_| OmpError::Unavailable)?;
    let wal = read_source_file(&agent, &wal_name, MAX_WAL_BYTES, deadline)?;
    reject_rollback_journal(&agent)?;

    let agent_after = FileIdentity::from_stat(&fstat(&agent).map_err(|_| OmpError::Unavailable)?);
    let root_after = FileIdentity::from_stat(&fstat(root).map_err(|_| OmpError::Unavailable)?);
    if agent_after != agent_identity || root_after != expected_root {
        return Err(OmpError::Unavailable);
    }
    let (wal, wal_identity) = match wal {
        Some(file) => (Some(file.bytes), Some(file.identity)),
        None => (None, None),
    };
    Ok(Some(SourcePair {
        agent_identity,
        database: database.bytes,
        database_identity: database.identity,
        wal,
        wal_identity,
    }))
}

#[cfg(unix)]
fn read_source_file(
    directory: &File,
    name: &std::ffi::CStr,
    limit: usize,
    deadline: Instant,
) -> Result<Option<CapturedFile>, OmpError> {
    use std::os::fd::OwnedFd;

    use nix::errno::Errno;
    use nix::fcntl::{AtFlags, OFlag, openat};
    use nix::sys::stat::{Mode, SFlag, fstat, fstatat};
    use nix::unistd::geteuid;

    let entry = match fstatat(directory, name, AtFlags::AT_SYMLINK_NOFOLLOW) {
        Ok(stat) => stat,
        Err(Errno::ENOENT) => return Ok(None),
        Err(_) => return Err(OmpError::Unavailable),
    };
    let identity = FileIdentity::from_stat(&entry);
    if !SFlag::from_bits_truncate(entry.st_mode).contains(SFlag::S_IFREG)
        || entry.st_uid != geteuid().as_raw()
        || entry.st_mode & 0o022 != 0
        || entry.st_nlink != 1
        || entry.st_size < 0
    {
        return Err(OmpError::Unavailable);
    }
    if usize::try_from(entry.st_size).map_or(true, |size| size > limit) {
        return Err(OmpError::LimitExceeded);
    }
    let fd = openat(
        directory,
        name,
        OFlag::O_RDONLY | OFlag::O_NONBLOCK | OFlag::O_NOFOLLOW | OFlag::O_CLOEXEC,
        Mode::empty(),
    )
    .map_err(|_| OmpError::Unavailable)?;
    let mut file = File::from(OwnedFd::from(fd));
    let opened = FileIdentity::from_stat(&fstat(&file).map_err(|_| OmpError::Unavailable)?);
    if opened != identity {
        return Err(OmpError::Unavailable);
    }
    let expected_size = usize::try_from(entry.st_size).map_err(|_| OmpError::LimitExceeded)?;
    let mut bytes = Zeroizing::new(Vec::with_capacity(expected_size));
    let mut chunk = Zeroizing::new([0_u8; 8192]);
    while bytes.len() < expected_size {
        check_deadline(deadline)?;
        let read_size = (expected_size - bytes.len()).min(chunk.len());
        let count = file
            .read(&mut chunk[..read_size])
            .map_err(|_| OmpError::Unavailable)?;
        if count == 0 {
            return Err(OmpError::Unavailable);
        }
        bytes.extend_from_slice(&chunk[..count]);
    }
    check_deadline(deadline)?;
    let mut trailing = Zeroizing::new([0_u8; 1]);
    if file
        .read(&mut trailing[..])
        .map_err(|_| OmpError::Unavailable)?
        != 0
    {
        return Err(OmpError::Unavailable);
    }
    check_deadline(deadline)?;
    let final_fd = FileIdentity::from_stat(&fstat(&file).map_err(|_| OmpError::Unavailable)?);
    let final_entry = entry_identity(directory, name)?.ok_or(OmpError::Unavailable)?;
    if bytes.len() != expected_size || final_fd != identity || final_entry != Some(identity) {
        return Err(OmpError::Unavailable);
    }
    Ok(Some(CapturedFile { identity, bytes }))
}

#[cfg(unix)]
fn entry_identity(
    directory: &File,
    name: &std::ffi::CStr,
) -> Result<Option<FileIdentity>, OmpError> {
    use nix::errno::Errno;
    use nix::fcntl::AtFlags;
    use nix::sys::stat::fstatat;

    match fstatat(directory, name, AtFlags::AT_SYMLINK_NOFOLLOW) {
        Ok(stat) => Ok(Some(FileIdentity::from_stat(&stat))),
        Err(Errno::ENOENT) => Ok(None),
        Err(_) => Err(OmpError::Unavailable),
    }
}

#[cfg(unix)]
fn reject_rollback_journal(agent: &File) -> Result<(), OmpError> {
    use std::ffi::CString;

    let journal = CString::new("agent.db-journal").map_err(|_| OmpError::Unavailable)?;
    if entry_identity(agent, &journal)?.is_some() {
        return Err(OmpError::Unavailable);
    }
    Ok(())
}

#[cfg(unix)]
fn secure_directory_identity(file: &File, require_owner: bool) -> Result<FileIdentity, OmpError> {
    use nix::sys::stat::{SFlag, fstat};
    use nix::unistd::geteuid;

    let stat = fstat(file).map_err(|_| OmpError::Unavailable)?;
    let identity = FileIdentity::from_stat(&stat);
    if !SFlag::from_bits_truncate(stat.st_mode).contains(SFlag::S_IFDIR)
        || (require_owner && stat.st_uid != geteuid().as_raw())
        || (require_owner && stat.st_mode & 0o022 != 0)
    {
        return Err(OmpError::Unavailable);
    }
    Ok(identity)
}

#[cfg(unix)]
fn open_source_directory(path: &Path, deadline: Instant) -> Result<Option<File>, OmpError> {
    use std::ffi::CString;
    use std::os::fd::OwnedFd;
    use std::os::unix::ffi::OsStrExt;

    use nix::errno::Errno;
    use nix::fcntl::{OFlag, open, openat};
    use nix::sys::stat::{Mode, SFlag, fstat, fstatat};

    let start = if path.is_absolute() { "/" } else { "." };
    let start_fd = open(
        start,
        OFlag::O_RDONLY | OFlag::O_DIRECTORY | OFlag::O_NOFOLLOW | OFlag::O_CLOEXEC,
        Mode::empty(),
    )
    .map_err(|_| OmpError::Unavailable)?;
    let mut directory = File::from(OwnedFd::from(start_fd));
    for component in path.components() {
        check_deadline(deadline)?;
        let Component::Normal(name) = component else {
            match component {
                Component::RootDir | Component::CurDir => continue,
                Component::ParentDir | Component::Prefix(_) => return Err(OmpError::Unavailable),
                Component::Normal(_) => unreachable!(),
            }
        };
        let name = CString::new(name.as_bytes()).map_err(|_| OmpError::Unavailable)?;
        let entry = match fstatat(&directory, name.as_c_str(), nix::fcntl::AtFlags::AT_SYMLINK_NOFOLLOW) {
            Ok(stat) => stat,
            Err(Errno::ENOENT) => return Ok(None),
            Err(_) => return Err(OmpError::Unavailable),
        };
        if !SFlag::from_bits_truncate(entry.st_mode).contains(SFlag::S_IFDIR) {
            return Err(OmpError::Unavailable);
        }
        let opened = openat(
            &directory,
            name.as_c_str(),
            OFlag::O_RDONLY | OFlag::O_DIRECTORY | OFlag::O_NOFOLLOW | OFlag::O_CLOEXEC,
            Mode::empty(),
        )
        .map_err(|_| OmpError::Unavailable)?;
        let next = File::from(OwnedFd::from(opened));
        let opened_stat = fstat(&next).map_err(|_| OmpError::Unavailable)?;
        if FileIdentity::from_stat(&entry) != FileIdentity::from_stat(&opened_stat) {
            return Err(OmpError::Unavailable);
        }
        directory = next;
    }
    check_deadline(deadline)?;
    secure_directory_identity(&directory, true)?;
    Ok(Some(directory))
}

#[cfg(unix)]
fn validate_private_directory(path: &Path) -> Result<(), OmpError> {
    use std::os::unix::fs::MetadataExt;

    let metadata = std::fs::symlink_metadata(path).map_err(|_| OmpError::Unavailable)?;
    if !metadata.is_dir()
        || metadata.file_type().is_symlink()
        || metadata.uid() != nix::unistd::geteuid().as_raw()
        || metadata.mode() & 0o777 != 0o700
    {
        return Err(OmpError::Unavailable);
    }
    Ok(())
}

#[cfg(not(unix))]
fn validate_private_directory(path: &Path) -> Result<(), OmpError> {
    let metadata = std::fs::symlink_metadata(path).map_err(|_| OmpError::Unavailable)?;
    if !metadata.is_dir() || metadata.file_type().is_symlink() {
        return Err(OmpError::Unavailable);
    }
    Ok(())
}

#[cfg(unix)]
fn create_private_file(path: &Path) -> Result<File, OmpError> {
    use std::os::unix::fs::OpenOptionsExt;

    OpenOptions::new()
        .write(true)
        .read(true)
        .create_new(true)
        .mode(0o600)
        .open(path)
        .map_err(|_| OmpError::Unavailable)
}

#[cfg(not(unix))]
fn create_private_file(path: &Path) -> Result<File, OmpError> {
    OpenOptions::new()
        .write(true)
        .read(true)
        .create_new(true)
        .open(path)
        .map_err(|_| OmpError::Unavailable)
}

fn write_private_file(path: &Path, bytes: &[u8], deadline: Instant) -> Result<(), OmpError> {
    let mut file = create_private_file(path)?;
    for chunk in bytes.chunks(8192) {
        check_deadline(deadline)?;
        file.write_all(chunk).map_err(|_| OmpError::Unavailable)?;
    }
    file.flush().map_err(|_| OmpError::Unavailable)
}

fn read_private_output(
    file: &mut File,
    output: &mut Zeroizing<Vec<u8>>,
    expected_len: u64,
    deadline: Instant,
) -> Result<(), OmpError> {
    let expected_size = usize::try_from(expected_len).map_err(|_| OmpError::LimitExceeded)?;
    if expected_size > MAX_DATABASE_BYTES {
        return Err(OmpError::LimitExceeded);
    }
    let mut chunk = Zeroizing::new([0_u8; 8192]);
    while output.len() < expected_size {
        check_deadline(deadline)?;
        let read_size = (expected_size - output.len()).min(chunk.len());
        let count = file
            .read(&mut chunk[..read_size])
            .map_err(|_| OmpError::Unavailable)?;
        if count == 0 {
            return Err(OmpError::Unavailable);
        }
        output.extend_from_slice(&chunk[..count]);
    }
    check_deadline(deadline)?;
    let mut trailing = Zeroizing::new([0_u8; 1]);
    if file
        .read(&mut trailing[..])
        .map_err(|_| OmpError::Unavailable)?
        != 0
    {
        return Err(OmpError::Unavailable);
    }
    check_deadline(deadline)?;
    Ok(())
}

fn sidecar_exists(database_path: &Path, suffix: &str) -> Result<bool, OmpError> {
    use std::ffi::OsString;

    let mut sidecar_name: OsString = database_path.as_os_str().to_owned();
    sidecar_name.push(suffix);
    let sidecar = PathBuf::from(sidecar_name);
    match std::fs::symlink_metadata(sidecar) {
        Ok(_) => Ok(true),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(_) => Err(OmpError::Unavailable),
    }
}

fn close_succeeded(result: Result<(), (Connection, rusqlite::Error)>) -> bool {
    match result {
        Ok(()) => true,
        Err((connection, _)) => {
            drop(connection);
            false
        }
    }
}

fn check_deadline(deadline: Instant) -> Result<(), OmpError> {
    if Instant::now() >= deadline {
        Err(OmpError::Deadline)
    } else {
        Ok(())
    }
}

fn map_wal_error(error: WalError) -> OmpError {
    match error {
        WalError::Deadline => OmpError::Deadline,
        WalError::PageLimit | WalError::WalTooLarge => OmpError::LimitExceeded,
        WalError::InvalidDatabase
        | WalError::InvalidHeader
        | WalError::InvalidLength
        | WalError::InvalidSalt
        | WalError::InvalidChecksum => OmpError::Unavailable,
    }
}

#[cfg(all(test, unix))]
mod tests {
    use std::fs;
    use std::os::fd::AsFd;
    use std::path::Path;
    use std::time::{Duration, Instant};

    use rusqlite::Connection;
    use tempfile::tempdir;

    use super::{OmpError, OmpSnapshot, PrivateDatabaseCleanup, close_succeeded};
    use crate::query::AUTH_SCHEMA_VERSION;
    use crate::wal::{validate_database, validate_wal};
    use crate::{OmpAccount, OmpSelector};

    const SOURCE_SCHEMA: &str = r#"
CREATE TABLE auth_schema_version (
    id INTEGER PRIMARY KEY CHECK (id = 1),
    version INTEGER NOT NULL
);
INSERT INTO auth_schema_version (id, version) VALUES (1, 7);
CREATE TABLE auth_credentials (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    provider TEXT NOT NULL,
    credential_type TEXT NOT NULL,
    data TEXT NOT NULL,
    disabled_cause TEXT DEFAULT NULL,
    identity_key TEXT DEFAULT NULL,
    created_at INTEGER NOT NULL DEFAULT (CAST(strftime('%s','now') AS INTEGER)),
    updated_at INTEGER NOT NULL DEFAULT (CAST(strftime('%s','now') AS INTEGER))
);
CREATE INDEX idx_auth_provider ON auth_credentials(provider);
CREATE INDEX idx_auth_provider_identity
    ON auth_credentials(provider, identity_key) WHERE identity_key IS NOT NULL;
"#;

    fn source_directory(parent: &Path) -> std::path::PathBuf {
        let source = parent.join(".omp");
        fs::create_dir_all(source.join("agent")).unwrap();
        source
    }

    fn create_source(parent: &Path, sibling_rows: bool) -> (std::path::PathBuf, Connection) {
        let source = source_directory(parent);
        let database = source.join("agent/agent.db");
        let connection = Connection::open(database).unwrap();
        connection
            .execute_batch(
                "PRAGMA page_size = 512;
                 PRAGMA journal_mode = WAL;
                 PRAGMA wal_autocheckpoint = 0;
                 PRAGMA secure_delete = OFF;",
            )
            .unwrap();
        connection.execute_batch(SOURCE_SCHEMA).unwrap();
        connection
            .execute(
                "INSERT INTO auth_credentials
                    (id, provider, credential_type, data, identity_key, created_at, updated_at)
                 VALUES (41, 'openai', 'api_key', ?1, NULL, 10, 11)",
                [r#"{"key":"selected-old-canary"}"#],
            )
            .unwrap();
        if sibling_rows {
            connection
                .execute(
                    "INSERT INTO auth_credentials
                        (id, provider, credential_type, data, identity_key, created_at, updated_at)
                     VALUES (42, 'openai', 'api_key', ?1, NULL, 12, 13)",
                    [r#"{"key":"same-provider-sibling-canary"}"#],
                )
                .unwrap();
            connection
                .execute(
                    "INSERT INTO auth_credentials
                        (id, provider, credential_type, data, identity_key, created_at, updated_at)
                     VALUES (43, 'anthropic', 'oauth', ?1, 'email:other@example.test', 14, 15)",
                    [r#"{"access":"other-access-canary","refresh":"other-refresh-canary","expires":99}"#],
                )
                .unwrap();
        }
        Ok::<(), rusqlite::Error>(())
            .and_then(|()| {
                connection.execute_batch("CREATE TABLE unrelated_data(value TEXT);")?;
                connection.execute(
                    "INSERT INTO unrelated_data(value) VALUES ('unrelated-table-canary')",
                    [],
                )?;
                Ok(())
            })
            .unwrap();
        (source, connection)
    }

    fn selector(id: i64) -> OmpSelector {
        OmpSelector {
            entry: "openai".to_owned(),
            profile: Some(format!("row:{id}")),
        }
    }

    fn deadline() -> Instant {
        Instant::now() + Duration::from_secs(10)
    }

    fn checkpoint(connection: &Connection) {
        let _: (i64, i64, i64) = connection
            .query_row("PRAGMA wal_checkpoint(TRUNCATE)", [], |row| {
                Ok((row.get(0)?, row.get(1)?, row.get(2)?))
            })
            .unwrap();
    }

    fn write_uncommitted_spill(connection: &Connection) {
        connection
            .execute_batch("CREATE TABLE spill_pages(value TEXT); BEGIN IMMEDIATE;")
            .unwrap();
        for index in 0..100 {
            let value = format!("uncommitted-spill-{index}-{}", "x".repeat(2_000));
            connection
                .execute("INSERT INTO spill_pages(value) VALUES (?1)", [value])
                .unwrap();
        }
    }

    #[test]
    fn selected_role_database_contains_only_the_bound_omp_credential_row() {
        let temp = tempdir().unwrap();
        let (source, writer) = create_source(temp.path(), true);
        writer
            .execute_batch(
                "CREATE TABLE deleted_data(value TEXT);
                 INSERT INTO deleted_data(value)
                 VALUES ('deleted-freelist-secret-canary-repeated-");
        drop(writer);
    }

    #[test]
    fn source_writers_that_commit_or_checkpoint_between_capture_passes_are_rejected() {
        for checkpoint_after_write in [false, true] {
            let temp = tempdir().unwrap();
            let (source, writer) = create_source(temp.path(), false);
            let root = fs::File::open(&source).unwrap();
            let result = OmpSnapshot::capture_from_root_inner(&root, deadline(), || {
                writer
                    .execute(
                        "UPDATE auth_credentials SET data = ?1 WHERE id = 41",
                        [r#"{"key":"changed-during-capture"}"#],
                    )
                    .unwrap();
                if checkpoint_after_write {
                    checkpoint(&writer);
                }
            });
            assert!(
                matches!(result, Err(OmpError::Unavailable)),
                "source mutation was accepted with checkpoint_after_write={checkpoint_after_write}"
            );
        }
    }

    #[test]
    fn a_stable_uncommitted_wal_spill_exports_the_last_committed_selected_row() {
        let temp = tempdir().unwrap();
        let (source, writer) = create_source(temp.path(), false);
        write_uncommitted_spill(&writer);
        let database = fs::read(source.join("agent/agent.db")).unwrap();
        let wal = fs::read(source.join("agent/agent.db-wal")).unwrap();
        let header = validate_database(&database, deadline()).unwrap();
        let wal_summary = validate_wal(&wal, header, deadline()).unwrap();
        let last_commit = wal_summary.last_commit.expect("fixture has a committed row");
        assert!(last_commit.final_frame + 1 < wal_summary.frame_count);

        let mut snapshot = OmpSnapshot::capture_from_directory(&source)
            .unwrap()
            .expect("source exists");
        assert_eq!(
            snapshot.accounts(),
            &[OmpAccount {
                id: 41,
                entry: "openai".to_owned(),
                profile: "row:41".to_owned(),
            }]
        );
        let selected = snapshot.select(Some("openai"), Some(&selector(41))).unwrap();
        let mut output = zeroize::Zeroizing::new(Vec::new());
        selected.write_standalone_database(&mut *output).unwrap();
        assert!(output.windows(b"selected-old-canary".len()).any(|bytes| {
            bytes == b"selected-old-canary"
        }));
        assert!(!output.windows(b"uncommitted-spill".len()).any(|bytes| {
            bytes == b"uncommitted-spill"
        }));
        writer.execute_batch("ROLLBACK").unwrap();
    }

    #[test]
    fn missing_or_stale_exact_selectors_never_fall_back_to_a_sibling() {
        let temp = tempdir().unwrap();
        let (source, _writer) = create_source(temp.path(), true);
        let mut snapshot = OmpSnapshot::capture_from_directory(&source)
            .unwrap()
            .expect("source exists");
        assert_eq!(
            snapshot.select(Some("openai"), None).err(),
            Some(OmpError::SelectionUnavailable)
        );
        assert_eq!(
            snapshot.select(Some("openai"), Some(&selector(99))).err(),
            Some(OmpError::SelectionUnavailable)
        );
        let selected = snapshot
            .select(Some("openai"), Some(&selector(42)))
            .unwrap();
        assert_eq!(selected.account().id, 42);
    }

    #[test]
    fn private_database_cleanup_removes_all_secret_bearing_sidecars() {
        let temp = tempdir().unwrap();
        let database = temp.path().join("selected-role.db");
        fs::write(&database, b"synthetic-selected-secret").unwrap();
        for suffix in ["-wal", "-shm", "-journal"] {
            let mut sidecar = database.as_os_str().to_owned();
            sidecar.push(suffix);
            fs::write(sidecar, b"synthetic-private-sidecar").unwrap();
        }
        let mut cleanup = PrivateDatabaseCleanup::new(database.clone());
        cleanup.cleanup().unwrap();
        assert!(!database.exists());
        for suffix in ["-wal", "-shm", "-journal"] {
            let mut sidecar = database.as_os_str().to_owned();
            sidecar.push(suffix);
            assert!(!std::path::PathBuf::from(sidecar).exists());
        }
    }

    #[test]
    fn successful_capture_cleanup_removes_the_private_role_database() {
        let temp = tempdir().unwrap();
        let database = temp.path().join("selected-role.db");
        fs::write(&database, b"synthetic-selected-secret").unwrap();
        let mut cleanup = PrivateDatabaseCleanup::new(database.clone());
        cleanup.cleanup().unwrap();
        drop(cleanup);
        assert!(!database.exists());
    }

    #[test]
    fn close_errors_are_not_accepted() {
        assert!(close_succeeded(Ok(())));
        let connection = Connection::open_in_memory().unwrap();
        assert!(!close_succeeded(Err((
            connection,
            rusqlite::Error::InvalidQuery,
        ))));
    }

    #[test]
    fn shared_file_descriptor_remains_open_after_a_read_only_capture() {
        let temp = tempdir().unwrap();
        let (source, _writer) = create_source(temp.path(), false);
        let root = fs::File::open(source).unwrap();
        let snapshot = OmpSnapshot::capture_from_root(&root)
            .unwrap()
            .expect("source exists");
        assert!(root.as_fd().try_clone_to_owned().is_ok());
        assert_eq!(AUTH_SCHEMA_VERSION, 7);
        drop(snapshot);
    }
}
