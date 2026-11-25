// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

mod antigravity;
mod claims;
pub(crate) mod claude_budget;
pub mod hubs;
mod preferences;
mod private;
pub use preferences::{MetricEdit, MetricState};
mod refresh;
mod settings;
pub use refresh::{FailureKind, RefreshFailure};
mod types;
mod vault;

use std::collections::BTreeMap;
use std::fs::File;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use jackin_core::{CriticalWrite, SecretBytes};
use rusqlite::{Connection, OpenFlags, OptionalExtension, Row, TransactionBehavior, params};
use serde_json::Value;
use thiserror::Error;
use uuid::Uuid;

pub use types::{
    AccountPreferences, CandidateIdentity, ConnectedAccount, CredentialVersion, KeyFingerprint,
    UsageProvider,
};
pub use vault::{CredentialVault, system_vault};

pub const DATABASE_PATH: &str = "usage/accounts.sqlite3";

const SCHEMA: &str = "
CREATE TABLE IF NOT EXISTS accounts (
    id TEXT PRIMARY KEY, provider TEXT NOT NULL, label TEXT NOT NULL,
    kind TEXT NOT NULL, identity TEXT, slot TEXT UNIQUE NOT NULL, position INTEGER NOT NULL,
    preferences TEXT NOT NULL DEFAULT '{}', fetched_at INTEGER, snapshot TEXT,
    UNIQUE(provider, kind, identity)
);
CREATE TABLE IF NOT EXISTS antigravity_enrollments (
    ticket TEXT PRIMARY KEY, email TEXT UNIQUE NOT NULL, expires INTEGER NOT NULL,
    fetched_at INTEGER, snapshot TEXT
);
CREATE TRIGGER IF NOT EXISTS cancel_removed_antigravity_enrollment AFTER DELETE ON accounts
WHEN OLD.provider='antigravity' AND OLD.kind='statusline' BEGIN
    DELETE FROM antigravity_enrollments WHERE email=OLD.identity;
END;
CREATE TABLE IF NOT EXISTS refresh_failures (
    id TEXT PRIMARY KEY, slot TEXT NOT NULL, kind TEXT NOT NULL, failures INTEGER NOT NULL,
    retry_at INTEGER NOT NULL, message TEXT NOT NULL
);
CREATE TRIGGER IF NOT EXISTS clear_removed_failure AFTER DELETE ON accounts BEGIN
    DELETE FROM refresh_failures WHERE id = OLD.id;
END;
CREATE TRIGGER IF NOT EXISTS clear_old_failure AFTER UPDATE OF slot, snapshot ON accounts BEGIN
    DELETE FROM refresh_failures WHERE id = OLD.id;
END;
CREATE TABLE IF NOT EXISTS claude_windows (
    scope TEXT PRIMARY KEY, generation INTEGER NOT NULL, strikes INTEGER NOT NULL, until INTEGER NOT NULL
);
CREATE TABLE IF NOT EXISTS claude_profile_scopes (
    account TEXT PRIMARY KEY REFERENCES accounts(id) ON DELETE CASCADE, slot TEXT NOT NULL, scope TEXT NOT NULL
);
CREATE TRIGGER IF NOT EXISTS clear_claude_profile AFTER DELETE ON accounts BEGIN
    DELETE FROM claude_profile_scopes WHERE account=OLD.id;
END;
CREATE TABLE IF NOT EXISTS reset_claims (
    account TEXT NOT NULL, target TEXT NOT NULL, request TEXT NOT NULL, done INTEGER NOT NULL DEFAULT 0,
    PRIMARY KEY(account,target)
);
CREATE TRIGGER IF NOT EXISTS clear_removed_claims AFTER DELETE ON accounts BEGIN
    DELETE FROM reset_claims WHERE account=OLD.id;
END;
CREATE TABLE IF NOT EXISTS dashboard_settings (id INTEGER PRIMARY KEY CHECK(id=1), value TEXT NOT NULL);
CREATE TABLE IF NOT EXISTS usage_proxy(id INTEGER PRIMARY KEY CHECK(id=1),slot TEXT NOT NULL);
CREATE TABLE IF NOT EXISTS hubs(id TEXT PRIMARY KEY,address TEXT UNIQUE NOT NULL,slot TEXT NOT NULL,refused INTEGER NOT NULL DEFAULT 0);
CREATE TABLE IF NOT EXISTS mirrors(id TEXT PRIMARY KEY,hub TEXT NOT NULL,login TEXT NOT NULL,auth_index TEXT NOT NULL,workspace TEXT, UNIQUE(hub,login));
CREATE TRIGGER IF NOT EXISTS clear_mirror AFTER DELETE ON accounts BEGIN DELETE FROM mirrors WHERE id=OLD.id; END;
CREATE TABLE IF NOT EXISTS retired_slots (slot TEXT PRIMARY KEY);
";
const COLUMNS: &str = "id, provider, label, position, preferences";

#[derive(Debug, Error)]
pub enum ConnectionError {
    #[error("manage this login on its hub, or remove the local hub connection")]
    HubManaged,
    #[error("the hub refused its management key; replace the management key")]
    HubRefused,
    #[error("usage metadata could not be read or written")]
    Database(#[from] rusqlite::Error),
    #[error("usage metadata or lock is inaccessible")]
    Io(#[from] std::io::Error),
    #[error("usage metadata must be in a private user-owned directory")]
    Private,
    #[error("this Google account is already being connected")]
    EnrollmentPending,
    #[error("usage metadata is invalid")]
    Invalid,
    #[error("unknown usage provider")]
    Provider,
    #[error("this credential type is unavailable for this account")]
    CredentialKind,
    #[error("unlock the OS credential store and retry; on Linux start a Secret Service session")]
    Vault,
    #[error(
        "the credential store is busy or waiting for Keychain access; approve the Keychain prompt and retry"
    )]
    StoreBusy,
    #[error("usage account removed; unlock the credential store and retry cleanup")]
    RemovedPendingCleanup,
    #[error(
        "this sign-in or key is already connected. To add another account, switch accounts on the provider page or use a key for a different account"
    )]
    Duplicate,
    #[error(
        "this Google account is already connected through the Antigravity status line; replace that connection with direct sign-in instead of adding another"
    )]
    DuplicateStatusline,
    #[error(
        "this Google account is already connected with direct sign-in; remove the status-line connection or keep using the existing account"
    )]
    DuplicateDirect,
    #[error("the sign-in belongs to a different account; use Add Account")]
    DifferentIdentity,
    #[error("the usage account changed or was removed; reopen it and retry")]
    Changed,
    #[error("the account label is empty")]
    EmptyLabel,
    #[error("the credential is missing; sign in again")]
    MissingCredential,
}

#[derive(Debug)]
pub struct AccountRegistry {
    db: Connection,
    directory: PathBuf,
    vault: Arc<dyn CredentialVault>,
    lock_wait: Duration,
}

const LOCK_WAIT: Duration = Duration::from_secs(30);
const LOCK_POLL: Duration = Duration::from_millis(25);

#[derive(Debug)]
pub struct KeptGrant<'a> {
    pub identity: &'a CandidateIdentity,
    pub secret: &'a SecretBytes,
    pub now: i64,
    pub kind: FailureKind,
    pub retry_after: Option<u64>,
    pub message: &'a str,
}

#[derive(Debug)]
pub enum InitialRead<'a> {
    Pending,
    Snapshot(i64, &'a Value),
    Failed {
        at: i64,
        kind: FailureKind,
        retry_after: Option<u64>,
        message: String,
    },
}

pub type StoredSnapshot = (ConnectedAccount, Option<i64>, Option<Value>);

impl AccountRegistry {
    pub fn proxy_configured(&self) -> Result<bool, ConnectionError> {
        Ok(self
            .db
            .query_row("SELECT EXISTS(SELECT 1 FROM usage_proxy)", [], |row| {
                row.get(0)
            })?)
    }

    pub fn open_read_only_with_vault(
        path: &Path,
        vault: Arc<dyn CredentialVault>,
    ) -> Result<Option<Self>, ConnectionError> {
        Self::open_existing(path, OpenFlags::SQLITE_OPEN_READ_ONLY, vault)
    }

    pub(crate) fn open_existing(
        path: &Path,
        mode: OpenFlags,
        vault: Arc<dyn CredentialVault>,
    ) -> Result<Option<Self>, ConnectionError> {
        match std::fs::symlink_metadata(path) {
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(e) => return Err(e.into()),
            Ok(_) => {}
        }
        let directory = path.parent().ok_or(ConnectionError::Private)?.to_owned();
        private::check_directory(&directory)?;
        private::check_metadata(&std::fs::symlink_metadata(path)?)?;
        let directory = directory.canonicalize()?;
        let db = Connection::open_with_flags(
            directory.join(path.file_name().ok_or(ConnectionError::Private)?),
            mode | OpenFlags::SQLITE_OPEN_NOFOLLOW,
        )?;
        db.busy_timeout(Duration::from_secs(10))?;
        Ok(Some(Self {
            db,
            directory,
            vault,
            lock_wait: LOCK_WAIT,
        }))
    }

    pub fn with_vault(
        path: &Path,
        vault: Arc<dyn CredentialVault>,
    ) -> Result<Self, ConnectionError> {
        let registry = Self::open_metadata(path, vault)?;
        let _cleanup = registry.clean_idle_slots();
        Ok(registry)
    }

    fn open_metadata(
        path: &Path,
        vault: Arc<dyn CredentialVault>,
    ) -> Result<Self, ConnectionError> {
        let directory = path.parent().ok_or(ConnectionError::Private)?.to_owned();
        private::directory(&directory)?;
        let _private_file = private::file(path)?;
        let directory = directory.canonicalize()?;
        let db = Connection::open_with_flags(
            directory.join(path.file_name().ok_or(ConnectionError::Private)?),
            OpenFlags::SQLITE_OPEN_READ_WRITE | OpenFlags::SQLITE_OPEN_NOFOLLOW,
        )?;
        db.busy_timeout(Duration::from_secs(10))?;
        db.execute_batch(SCHEMA)?;
        db.execute_batch(
            "INSERT OR IGNORE INTO retired_slots
             SELECT slot FROM accounts WHERE provider='gemini' AND kind != 'statusline';
             DELETE FROM accounts WHERE provider='gemini';",
        )?;
        Ok(Self {
            db,
            directory,
            vault,
            lock_wait: LOCK_WAIT,
        })
    }

    #[must_use]
    pub fn with_lock_wait(mut self, wait: Duration) -> Self {
        self.lock_wait = wait;
        self
    }

    pub fn list(&self) -> Result<Vec<ConnectedAccount>, ConnectionError> {
        let mut stmt = self.db.prepare(&format!(
            "SELECT {COLUMNS} FROM accounts ORDER BY position,id"
        ))?;
        Ok(stmt.query_map([], account)?.collect::<Result<_, _>>()?)
    }

    pub fn identities(&self) -> Result<BTreeMap<Uuid, String>, ConnectionError> {
        let mut stmt = self.db.prepare(
            "SELECT id,CASE WHEN provider='antigravity' THEN lower(identity) ELSE identity END
             FROM accounts WHERE identity IS NOT NULL",
        )?;
        Ok(stmt
            .query_map([], |row| Ok((uuid(row, 0)?, row.get(1)?)))?
            .collect::<Result<_, _>>()?)
    }

    pub fn version(&self, id: Uuid) -> Result<CredentialVersion, ConnectionError> {
        self.db
            .query_row(
                "SELECT slot FROM accounts WHERE id=?1",
                [id.to_string()],
                |r| uuid(r, 0),
            )
            .optional()?
            .map(CredentialVersion)
            .ok_or(ConnectionError::Changed)
    }

    pub fn uses_api_key(&self, id: Uuid) -> Result<bool, ConnectionError> {
        self.db
            .query_row(
                "SELECT kind='key' FROM accounts WHERE id=?1",
                [id.to_string()],
                |row| row.get(0),
            )
            .optional()?
            .ok_or(ConnectionError::Changed)
    }

    pub fn is_statusline(&self, id: Uuid) -> Result<bool, ConnectionError> {
        Ok(self
            .db
            .query_row(
                "SELECT kind='statusline' FROM accounts WHERE id=?1",
                [id.to_string()],
                |row| row.get::<_, bool>(0),
            )
            .optional()?
            .unwrap_or(false))
    }

    pub fn credential(
        &self,
        id: Uuid,
    ) -> Result<(CredentialVersion, SecretBytes), ConnectionError> {
        loop {
            let (slot, statusline) = self
                .db
                .query_row(
                    "SELECT slot,kind='statusline' FROM accounts WHERE id=?1",
                    [id.to_string()],
                    |row| Ok((uuid(row, 0)?, row.get::<_, bool>(1)?)),
                )
                .optional()?
                .ok_or(ConnectionError::Changed)?;
            if statusline {
                return Err(ConnectionError::CredentialKind);
            }
            match self.vault.load(slot) {
                Err(ConnectionError::MissingCredential) if self.version(id)?.0 != slot => {}
                loaded => return Ok((CredentialVersion(slot), loaded?)),
            }
        }
    }

    fn mutation_lock(&self, wait: Option<Duration>) -> Result<File, ConnectionError> {
        let f = private::file(&self.directory.join("credentials.lock"))?;
        let Some(wait) = wait else {
            f.lock()?;
            return Ok(f);
        };
        let deadline = std::time::Instant::now() + wait;
        loop {
            match f.try_lock() {
                Ok(()) => return Ok(f),
                Err(std::fs::TryLockError::Error(error)) => return Err(error.into()),
                Err(std::fs::TryLockError::WouldBlock) => {
                    if std::time::Instant::now() >= deadline {
                        return Err(ConnectionError::StoreBusy);
                    }
                    std::thread::park_timeout(LOCK_POLL);
                }
            }
        }
    }

    pub(crate) fn renewal_file(&self, id: Uuid) -> Result<File, ConnectionError> {
        self.version(id)?;
        private::file(&self.directory.join(format!("renew-{id}.lock")))
    }

    pub fn add(
        &mut self,
        write: &CriticalWrite,
        provider: UsageProvider,
        label: &str,
        identity: &CandidateIdentity,
        secret: &SecretBytes,
    ) -> Result<Uuid, ConnectionError> {
        self.add_candidate(
            write,
            provider,
            label,
            identity,
            secret,
            InitialRead::Pending,
        )
    }

    pub fn add_with_snapshot(
        &mut self,
        write: &CriticalWrite,
        provider: UsageProvider,
        label: &str,
        identity: &CandidateIdentity,
        secret: &SecretBytes,
        reading: (i64, &Value),
    ) -> Result<Uuid, ConnectionError> {
        self.add_candidate(
            write,
            provider,
            label,
            identity,
            secret,
            InitialRead::Snapshot(reading.0, reading.1),
        )
    }

    pub fn add_candidate(
        &mut self,
        write: &CriticalWrite,
        provider: UsageProvider,
        label: &str,
        identity: &CandidateIdentity,
        secret: &SecretBytes,
        initial: InitialRead<'_>,
    ) -> Result<Uuid, ConnectionError> {
        let label = nonempty(label)?;
        let (kind, identity) = identity.parts(provider)?;
        let id = Uuid::new_v4();
        self.install(write, secret, |tx, slot| {
            reject_duplicate(tx, provider, kind, identity, id)?;
            reject_cross_kind(tx, provider, kind, identity, id)?;
            tx.execute(
                "INSERT INTO accounts(id, provider, label, kind, identity, slot, position)
                SELECT ?1,?2,?3,?4,?5,?6,COALESCE(MAX(position),-1)+1 FROM accounts",
                params![
                    id.to_string(),
                    provider.as_str(),
                    label,
                    kind,
                    identity,
                    slot.to_string()
                ],
            )?;
            match initial {
                InitialRead::Pending => {}
                InitialRead::Snapshot(at, payload) => {
                    write_snapshot(tx, id, CredentialVersion(slot), at, payload)?;
                }
                InitialRead::Failed {
                    at,
                    kind,
                    retry_after,
                    message,
                } => refresh::write_failure(
                    tx,
                    id,
                    CredentialVersion(slot),
                    at,
                    kind,
                    retry_after,
                    &message,
                )?,
            }
            Ok(())
        })?;
        Ok(id)
    }

    pub fn replace(
        &mut self,
        write: &CriticalWrite,
        id: Uuid,
        expected: CredentialVersion,
        identity: &CandidateIdentity,
        secret: &SecretBytes,
    ) -> Result<(), ConnectionError> {
        self.replace_candidate(write, id, expected, identity, secret, None)
    }

    pub fn replace_with_snapshot(
        &mut self,
        write: &CriticalWrite,
        id: Uuid,
        expected: CredentialVersion,
        identity: &CandidateIdentity,
        secret: &SecretBytes,
        reading: (i64, &Value),
    ) -> Result<(), ConnectionError> {
        self.replace_candidate(write, id, expected, identity, secret, Some(reading))
    }

    pub fn replace_candidate(
        &mut self,
        write: &CriticalWrite,
        id: Uuid,
        expected: CredentialVersion,
        identity: &CandidateIdentity,
        secret: &SecretBytes,
        snapshot: Option<(i64, &Value)>,
    ) -> Result<(), ConnectionError> {
        self.install(write, secret, |tx, slot| {
            let (provider, old_kind, known): (String, String, Option<String>) = tx
                .query_row(
                    "SELECT provider,kind,identity FROM accounts WHERE id=?1 AND slot=?2",
                    params![id.to_string(), expected.0.to_string()],
                    |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
                )
                .optional()?
                .ok_or(ConnectionError::Changed)?;
            let provider = provider.parse()?;
            let (kind, candidate) = identity.parts(provider)?;
            if converts_statusline(provider, &old_kind, kind) {
                convert_statusline(tx, id, expected, slot, known.as_deref(), candidate)?;
                if let Some((at, payload)) = snapshot {
                    write_snapshot(tx, id, CredentialVersion(slot), at, payload)?;
                }
                return Ok(());
            }
            if kind != old_kind {
                return Err(ConnectionError::CredentialKind);
            }
            if kind == "oauth" && known.is_some() && candidate != known.as_deref() {
                return Err(ConnectionError::DifferentIdentity);
            }
            reject_duplicate(tx, provider, kind, candidate, id)?;
            tx.execute(
                "UPDATE accounts SET identity=?1, fetched_at=NULL, snapshot=NULL WHERE id=?2",
                params![candidate, id.to_string()],
            )?;
            replace_slot(tx, id, expected, slot)?;
            if let Some((at, payload)) = snapshot {
                write_snapshot(tx, id, CredentialVersion(slot), at, payload)?;
            }
            Ok(())
        })
        .map(|_| ())
    }

    pub fn replace_grant_keeping_reading(
        &mut self,
        write: &CriticalWrite,
        id: Uuid,
        expected: CredentialVersion,
        grant: KeptGrant<'_>,
    ) -> Result<(), ConnectionError> {
        self.install(write, grant.secret, |tx, slot| {
            let (provider, old_kind, known): (String, String, Option<String>) = tx
                .query_row(
                    "SELECT provider,kind,identity FROM accounts WHERE id=?1 AND slot=?2",
                    params![id.to_string(), expected.0.to_string()],
                    |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
                )
                .optional()?
                .ok_or(ConnectionError::Changed)?;
            let provider = provider.parse()?;
            let (kind_name, candidate) = grant.identity.parts(provider)?;
            if converts_statusline(provider, &old_kind, kind_name) {
                convert_statusline(tx, id, expected, slot, known.as_deref(), candidate)?;
            } else {
                if kind_name != old_kind {
                    return Err(ConnectionError::CredentialKind);
                }
                if kind_name == "oauth" && known.is_some() && candidate != known.as_deref() {
                    return Err(ConnectionError::DifferentIdentity);
                }
                reject_duplicate(tx, provider, kind_name, candidate, id)?;
                tx.execute(
                    "UPDATE accounts SET identity=?1 WHERE id=?2",
                    params![candidate, id.to_string()],
                )?;
                replace_slot(tx, id, expected, slot)?;
            }
            refresh::write_failure(
                tx,
                id,
                CredentialVersion(slot),
                grant.now,
                grant.kind,
                grant.retry_after,
                grant.message,
            )?;
            Ok(())
        })
        .map(|_| ())
    }

    pub fn rotate(
        &mut self,
        write: &CriticalWrite,
        id: Uuid,
        expected: CredentialVersion,
        secret: &SecretBytes,
    ) -> Result<CredentialVersion, ConnectionError> {
        self.install_within(None, write, secret, |tx, slot| {
            replace_slot(tx, id, expected, slot)
        })
    }

    fn install(
        &mut self,
        write: &CriticalWrite,
        secret: &SecretBytes,
        change: impl FnOnce(&Connection, Uuid) -> Result<(), ConnectionError>,
    ) -> Result<CredentialVersion, ConnectionError> {
        self.install_within(Some(self.lock_wait), write, secret, change)
    }

    fn install_within(
        &mut self,
        wait: Option<Duration>,
        _write: &CriticalWrite,
        secret: &SecretBytes,
        change: impl FnOnce(&Connection, Uuid) -> Result<(), ConnectionError>,
    ) -> Result<CredentialVersion, ConnectionError> {
        let _lock = self.mutation_lock(wait)?;
        let slot = Uuid::new_v4();
        self.db
            .execute("INSERT INTO retired_slots VALUES (?1)", [slot.to_string()])?;
        let result = (|| {
            self.vault.store(slot, secret)?;
            let tx = self
                .db
                .transaction_with_behavior(TransactionBehavior::Immediate)?;
            change(&tx, slot)?;
            tx.execute(
                "DELETE FROM retired_slots WHERE slot=?1",
                [slot.to_string()],
            )?;
            tx.commit()?;
            Ok(CredentialVersion(slot))
        })();
        let _cleanup = self.drain_retired();
        result
    }

    pub fn rename(&self, id: Uuid, label: &str) -> Result<(), ConnectionError> {
        changed(self.db.execute(
            "UPDATE accounts SET label=?1 WHERE id=?2",
            params![nonempty(label)?, id.to_string()],
        )?)
    }

    pub fn set_preferences(
        &self,
        id: Uuid,
        prefs: &AccountPreferences,
    ) -> Result<(), ConnectionError> {
        let mut normalized = prefs.clone();
        normalized
            .pinned
            .retain(|id| !normalized.hidden.contains(id));
        let mut seen = std::collections::BTreeSet::new();
        normalized.metric_order.retain(|id| seen.insert(id.clone()));
        let json = serde_json::to_string(&normalized).map_err(|_| ConnectionError::Invalid)?;
        changed(self.db.execute(
            "UPDATE accounts SET preferences=?1 WHERE id=?2",
            params![json, id.to_string()],
        )?)
    }

    pub fn reorder(&mut self, ids: &[Uuid]) -> Result<(), ConnectionError> {
        let tx = self
            .db
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let count: i64 = tx.query_row("SELECT count(*) FROM accounts", [], |r| r.get(0))?;
        let count = usize::try_from(count).map_err(|_| ConnectionError::Invalid)?;
        if ids.len() != count
            || ids.iter().collect::<std::collections::BTreeSet<_>>().len() != count
        {
            return Err(ConnectionError::Changed);
        }
        for (i, id) in (0_i64..).zip(ids) {
            changed(tx.execute(
                "UPDATE accounts SET position=?1 WHERE id=?2",
                params![i, id.to_string()],
            )?)?;
        }
        tx.commit()?;
        Ok(())
    }

    pub fn remove(&mut self, id: Uuid) -> Result<(), ConnectionError> {
        let _lock = self.mutation_lock(Some(self.lock_wait))?;
        if self.hub_link(id)?.is_some() {
            return Err(ConnectionError::HubManaged);
        }
        let tx = self
            .db
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let id = id.to_string();
        let retire = tx.execute(
            "INSERT INTO retired_slots SELECT slot FROM accounts WHERE id=?1 AND kind != 'statusline'",
            [&id],
        )?;
        changed(tx.execute("DELETE FROM accounts WHERE id=?1", [&id])?)?;
        tx.commit()?;
        if retire == 0 {
            return Ok(());
        }
        self.drain_retired()
            .map_err(|_| ConnectionError::RemovedPendingCleanup)
    }

    fn clean_idle_slots(&self) -> Result<(), ConnectionError> {
        if !self
            .db
            .query_row("SELECT EXISTS(SELECT 1 FROM retired_slots)", [], |row| {
                row.get::<_, bool>(0)
            })?
        {
            return Ok(());
        }
        let lock = private::file(&self.directory.join("credentials.lock"))?;
        match lock.try_lock() {
            Ok(()) => self.drain_retired(),
            Err(std::fs::TryLockError::WouldBlock) => Ok(()),
            Err(std::fs::TryLockError::Error(error)) => Err(error.into()),
        }
    }

    pub fn clean_retired(&mut self) -> Result<(), ConnectionError> {
        let _lock = self.mutation_lock(Some(self.lock_wait))?;
        self.drain_retired()
    }

    fn drain_retired(&self) -> Result<(), ConnectionError> {
        let mut stmt = self.db.prepare("SELECT slot FROM retired_slots")?;
        let slots = stmt
            .query_map([], |r| uuid(r, 0))?
            .collect::<Result<Vec<_>, _>>()?;
        for slot in slots {
            self.vault.delete(slot)?;
            self.db.execute(
                "DELETE FROM retired_slots WHERE slot=?1",
                [slot.to_string()],
            )?;
        }
        Ok(())
    }

    pub fn put_snapshot(
        &self,
        id: Uuid,
        expected: CredentialVersion,
        fetched_at: i64,
        payload: &Value,
    ) -> Result<(), ConnectionError> {
        write_snapshot(&self.db, id, expected, fetched_at, payload)
    }

    pub fn snapshots(&self) -> Result<Vec<StoredSnapshot>, ConnectionError> {
        let mut stmt = self.db.prepare(&format!(
            "SELECT {COLUMNS},fetched_at,snapshot FROM accounts ORDER BY position,id"
        ))?;
        Ok(stmt
            .query_map([], |r| {
                let raw: Option<String> = r.get(6)?;
                let snapshot = raw
                    .map(|s| serde_json::from_str(&s).map_err(|e| bad(6, e)))
                    .transpose()?;
                Ok((account(r)?, r.get(5)?, snapshot))
            })?
            .collect::<Result<_, _>>()?)
    }
}

fn write_snapshot(
    db: &Connection,
    id: Uuid,
    expected: CredentialVersion,
    at: i64,
    payload: &Value,
) -> Result<(), ConnectionError> {
    let count=db.execute("UPDATE accounts SET fetched_at=?1,snapshot=?2 WHERE id=?3 AND slot=?4 AND (fetched_at IS NULL OR fetched_at<=?1)",params![at,payload.to_string(),id.to_string(),expected.0.to_string()])?;
    changed(count)
}

fn replace_slot(
    tx: &Connection,
    id: Uuid,
    expected: CredentialVersion,
    slot: Uuid,
) -> Result<(), ConnectionError> {
    changed(tx.execute(
        "UPDATE accounts SET slot=?1 WHERE id=?2 AND slot=?3 AND kind!='statusline'",
        params![slot.to_string(), id.to_string(), expected.0.to_string()],
    )?)?;
    tx.execute(
        "INSERT INTO retired_slots VALUES (?1)",
        [expected.0.to_string()],
    )?;
    Ok(())
}

fn converts_statusline(provider: UsageProvider, old_kind: &str, kind: &str) -> bool {
    provider == UsageProvider::Antigravity && old_kind == "statusline" && kind == "oauth"
}

fn convert_statusline(
    tx: &Connection,
    id: Uuid,
    expected: CredentialVersion,
    slot: Uuid,
    known: Option<&str>,
    candidate: Option<&str>,
) -> Result<(), ConnectionError> {
    if let Some(known) = known
        && !candidate.is_some_and(|candidate| candidate.eq_ignore_ascii_case(known))
    {
        return Err(ConnectionError::DifferentIdentity);
    }
    reject_duplicate(tx, UsageProvider::Antigravity, "oauth", candidate, id)?;
    reject_cross_kind(tx, UsageProvider::Antigravity, "oauth", candidate, id)?;
    if let Some(known) = known {
        tx.execute(
            "DELETE FROM antigravity_enrollments WHERE email=?1",
            [known],
        )?;
    }
    changed(tx.execute(
        "UPDATE accounts SET kind='oauth', identity=?1, slot=?2, fetched_at=NULL, snapshot=NULL
         WHERE id=?3 AND slot=?4 AND kind='statusline'",
        params![
            candidate,
            slot.to_string(),
            id.to_string(),
            expected.0.to_string()
        ],
    )?)
}

fn reject_duplicate(
    db: &Connection,
    provider: UsageProvider,
    kind: &str,
    identity: Option<&str>,
    except: Uuid,
) -> Result<(), ConnectionError> {
    let found:bool=db.query_row("SELECT EXISTS(SELECT 1 FROM accounts WHERE provider=?1 AND kind=?2 AND identity=?3 AND id<>?4)",params![provider.as_str(),kind,identity,except.to_string()],|r|r.get(0))?;
    if found {
        return Err(ConnectionError::Duplicate);
    }
    Ok(())
}

fn reject_cross_kind(
    db: &Connection,
    provider: UsageProvider,
    kind: &str,
    identity: Option<&str>,
    except: Uuid,
) -> Result<(), ConnectionError> {
    if provider != UsageProvider::Antigravity {
        return Ok(());
    }
    let Some(identity) = identity else {
        return Ok(());
    };
    let other: Option<String> = db
        .query_row(
            "SELECT kind FROM accounts
             WHERE provider='antigravity' AND kind<>?1 AND lower(identity)=lower(?2) AND id<>?3
             ORDER BY position LIMIT 1",
            params![kind, identity, except.to_string()],
            |r| r.get(0),
        )
        .optional()?;
    match other.as_deref() {
        None => Ok(()),
        Some("statusline") => Err(ConnectionError::DuplicateStatusline),
        Some(_) => Err(ConnectionError::DuplicateDirect),
    }
}

fn nonempty(label: &str) -> Result<&str, ConnectionError> {
    let label = label.trim();
    if label.is_empty() {
        return Err(ConnectionError::EmptyLabel);
    }
    Ok(label)
}

fn changed(n: usize) -> Result<(), ConnectionError> {
    if n == 0 {
        return Err(ConnectionError::Changed);
    }
    Ok(())
}

fn bad(column: usize, e: impl std::error::Error + Send + Sync + 'static) -> rusqlite::Error {
    rusqlite::Error::FromSqlConversionFailure(column, rusqlite::types::Type::Text, Box::new(e))
}

fn uuid(row: &Row<'_>, column: usize) -> rusqlite::Result<Uuid> {
    Uuid::parse_str(&row.get::<_, String>(column)?).map_err(|e| bad(column, e))
}

fn account(row: &Row<'_>) -> rusqlite::Result<ConnectedAccount> {
    Ok(ConnectedAccount {
        id: uuid(row, 0)?,
        provider: row.get::<_, String>(1)?.parse().map_err(|e| bad(1, e))?,
        label: row.get(2)?,
        position: row.get(3)?,
        preferences: serde_json::from_str(&row.get::<_, String>(4)?).map_err(|e| bad(4, e))?,
    })
}
