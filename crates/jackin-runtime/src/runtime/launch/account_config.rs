// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Materialize selected API account settings in the private capsule home.
#![cfg_attr(
    not(unix),
    expect(
        clippy::disallowed_methods,
        reason = "private config publication runs inside the launch blocking task"
    )
)]

use std::collections::BTreeMap;
#[cfg(unix)]
use std::ffi::{CStr, CString};
#[cfg(unix)]
use std::fs::File;
#[cfg(unix)]
use std::io::{Read as _, Write as _};
#[cfg(unix)]
use std::os::unix::fs::PermissionsExt as _;
use std::path::{Component, Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use anyhow::Context as _;
use jackin_config::{AccountCredential, AiProvider, AppConfig};
use jackin_core::Agent;
#[cfg(unix)]
use nix::dir::Dir;
#[cfg(unix)]
use nix::fcntl::{AtFlags, OFlag, open, openat, renameat};
#[cfg(unix)]
use nix::sys::stat::{Mode, SFlag, fchmod, fstatat, mkdirat};
#[cfg(unix)]
use nix::unistd::{UnlinkatFlags, unlinkat};

static PRIVATE_CONFIG_SWAP_COUNTER: AtomicU64 = AtomicU64::new(0);
const PRIVATE_CONFIG_TRANSACTION_VERSION: u8 = 1;
const PRIVATE_CONFIG_TRANSACTION_FILE: &str = ".jackin-private-config-transaction";
#[cfg(unix)]
const PRIVATE_CONFIG_LOCK_FILE: &str = ".jackin-private-config-lock";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum PrivateConfigFailurePoint {
    StagedFile(&'static str),
    BeforeSwap,
    AfterPreviousRename,
    AfterInstall,
    #[cfg(test)]
    SimulatedCrashAfterPreviousRename,
    #[cfg(test)]
    SimulatedCrashAfterPreviousDeletion,
    #[cfg(test)]
    JournalAfterPreviousMoved,
    #[cfg(test)]
    JournalAfterInstalled,
}

#[derive(serde::Deserialize, serde::Serialize)]
struct PrivateConfigTransaction {
    schema_version: u8,
    target: String,
    staged: String,
    previous: Option<String>,
    phase: PrivateConfigTransactionPhase,
}

#[derive(serde::Deserialize, serde::Serialize)]
enum PrivateConfigTransactionPhase {
    Prepared,
    PreviousMoved,
    Installed,
}

#[cfg(test)]
thread_local! {
    static PRIVATE_CONFIG_FAILURE: std::cell::Cell<Option<PrivateConfigFailurePoint>> =
        const { std::cell::Cell::new(None) };
}

fn maybe_inject_private_config_failure(point: PrivateConfigFailurePoint) -> anyhow::Result<()> {
    #[cfg(test)]
    if PRIVATE_CONFIG_FAILURE.with(|failure| failure.get() == Some(point)) {
        anyhow::bail!("injected private-config publication failure at {point:?}");
    }

    #[cfg(not(test))]
    let _ = point;
    Ok(())
}

#[cfg(unix)]
struct PrivateConfigPublication {
    parent_path: PathBuf,
    parent: File,
    _lock: File,
}

#[cfg(unix)]
fn private_config_name(name: &str) -> anyhow::Result<CString> {
    let components = Path::new(name).components().collect::<Vec<_>>();
    anyhow::ensure!(
        components.len() == 1 && matches!(components[0], Component::Normal(_)),
        "private config entry must be a single file name: {name:?}"
    );
    CString::new(name.as_bytes()).context("private config entry contains NUL")
}

#[cfg(unix)]
fn private_config_parent_components(root: &Path, parent: &Path) -> anyhow::Result<Vec<CString>> {
    let relative = parent.strip_prefix(root).with_context(|| {
        format!(
            "private config parent {} escapes root {}",
            parent.display(),
            root.display()
        )
    })?;
    relative
        .components()
        .map(|component| {
            let Component::Normal(component) = component else {
                anyhow::bail!(
                    "private config parent contains a non-normal component: {}",
                    parent.display()
                );
            };
            let component = component
                .to_str()
                .context("private config path component is not valid UTF-8")?;
            CString::new(component.as_bytes()).context("private config path component contains NUL")
        })
        .collect()
}

#[cfg(unix)]
fn open_private_config_directory(parent: &File, name: &CStr) -> anyhow::Result<Option<File>> {
    match openat(
        parent,
        name,
        OFlag::O_RDONLY | OFlag::O_DIRECTORY | OFlag::O_NOFOLLOW | OFlag::O_CLOEXEC,
        Mode::empty(),
    ) {
        Ok(fd) => Ok(Some(File::from(fd))),
        Err(nix::errno::Errno::ENOENT) => Ok(None),
        Err(nix::errno::Errno::ELOOP) => {
            anyhow::bail!("private config directory is a symlink: {name:?}")
        }
        Err(error) => Err(error.into()),
    }
}

#[cfg(unix)]
fn begin_private_config_publication(
    root: &Path,
    parent_path: &Path,
) -> anyhow::Result<PrivateConfigPublication> {
    let root_fd = open(
        root,
        OFlag::O_RDONLY | OFlag::O_DIRECTORY | OFlag::O_NOFOLLOW | OFlag::O_CLOEXEC,
        Mode::empty(),
    )
    .with_context(|| format!("open private config root {}", root.display()))?;
    let mut parent = File::from(root_fd);
    for component in private_config_parent_components(root, parent_path)? {
        match mkdirat(
            &parent,
            component.as_c_str(),
            Mode::from_bits_truncate(0o700),
        ) {
            Ok(()) | Err(nix::errno::Errno::EEXIST) => {}
            Err(error) => return Err(error.into()),
        }
        let child = match openat(
            &parent,
            component.as_c_str(),
            OFlag::O_RDONLY | OFlag::O_DIRECTORY | OFlag::O_NOFOLLOW | OFlag::O_CLOEXEC,
            Mode::empty(),
        ) {
            Ok(child) => child,
            Err(nix::errno::Errno::ELOOP) => {
                anyhow::bail!("private config ancestor is a symlink: {component:?}")
            }
            Err(nix::errno::Errno::ENOTDIR)
                if fstatat(&parent, component.as_c_str(), AtFlags::AT_SYMLINK_NOFOLLOW)
                    .is_ok_and(|stat| {
                        SFlag::from_bits_truncate(stat.st_mode).contains(SFlag::S_IFLNK)
                    }) =>
            {
                anyhow::bail!("private config ancestor is a symlink: {component:?}")
            }
            Err(error) => {
                return Err(anyhow::Error::new(error).context(format!(
                    "open private config ancestor below {}: {:?}",
                    root.display(),
                    component
                )));
            }
        };
        parent = File::from(child);
    }

    let lock_name = private_config_name(PRIVATE_CONFIG_LOCK_FILE)?;
    let lock = File::from(
        openat(
            &parent,
            lock_name.as_c_str(),
            OFlag::O_RDWR | OFlag::O_CREAT | OFlag::O_NOFOLLOW | OFlag::O_CLOEXEC,
            Mode::from_bits_truncate(0o600),
        )
        .context("open private config publication lock")?,
    );
    anyhow::ensure!(
        lock.metadata()?.is_file(),
        "private config publication lock is not a regular file"
    );
    fs4::FileExt::lock(&lock).context("lock private config publication parent")?;

    Ok(PrivateConfigPublication {
        parent_path: parent_path.to_owned(),
        parent,
        _lock: lock,
    })
}

#[cfg(unix)]
fn private_config_directory_entries(directory: &File) -> anyhow::Result<Vec<CString>> {
    let mut entries = Dir::openat(
        directory,
        ".",
        OFlag::O_RDONLY | OFlag::O_DIRECTORY | OFlag::O_NOFOLLOW | OFlag::O_CLOEXEC,
        Mode::empty(),
    )?;
    entries
        .iter()
        .map(|entry| {
            let entry = entry?;
            let name = entry.file_name();
            if name.to_bytes() == b"." || name.to_bytes() == b".." {
                return Ok(None);
            }
            Ok(Some(CString::new(name.to_bytes())?))
        })
        .filter_map(Result::transpose)
        .collect::<Result<Vec<_>, anyhow::Error>>()
}

#[cfg(unix)]
fn private_config_remove_tree_at(parent: &File, name: &CStr) -> anyhow::Result<()> {
    let Some(directory) = open_private_config_directory(parent, name)? else {
        return Ok(());
    };
    for child in private_config_directory_entries(&directory)? {
        let stat = match fstatat(&directory, child.as_c_str(), AtFlags::AT_SYMLINK_NOFOLLOW) {
            Ok(stat) => stat,
            Err(nix::errno::Errno::ENOENT) => continue,
            Err(error) => return Err(error.into()),
        };
        let kind = SFlag::from_bits_truncate(stat.st_mode);
        if kind.contains(SFlag::S_IFDIR) {
            private_config_remove_tree_at(&directory, child.as_c_str())?;
            match unlinkat(&directory, child.as_c_str(), UnlinkatFlags::RemoveDir) {
                Ok(()) | Err(nix::errno::Errno::ENOENT) => {}
                Err(error) => return Err(error.into()),
            }
        } else {
            match unlinkat(&directory, child.as_c_str(), UnlinkatFlags::NoRemoveDir) {
                Ok(()) | Err(nix::errno::Errno::ENOENT) => {}
                Err(error) => return Err(error.into()),
            }
        }
    }
    drop(directory);
    match unlinkat(parent, name, UnlinkatFlags::RemoveDir) {
        Ok(()) | Err(nix::errno::Errno::ENOENT) => Ok(()),
        Err(error) => Err(error.into()),
    }
}

#[cfg(unix)]
fn private_config_copy_tree(source: &File, destination: &File) -> anyhow::Result<()> {
    for name in private_config_directory_entries(source)? {
        let stat = fstatat(source, name.as_c_str(), AtFlags::AT_SYMLINK_NOFOLLOW)?;
        let kind = SFlag::from_bits_truncate(stat.st_mode);
        let mode = Mode::from_bits_truncate(stat.st_mode);
        if kind.contains(SFlag::S_IFLNK) {
            anyhow::bail!("refusing to publish private config through symlink {name:?}");
        }
        if kind.contains(SFlag::S_IFDIR) {
            mkdirat(destination, name.as_c_str(), mode)?;
            let child = File::from(openat(
                destination,
                name.as_c_str(),
                OFlag::O_RDONLY | OFlag::O_DIRECTORY | OFlag::O_NOFOLLOW | OFlag::O_CLOEXEC,
                Mode::empty(),
            )?);
            let source_child = File::from(openat(
                source,
                name.as_c_str(),
                OFlag::O_RDONLY | OFlag::O_DIRECTORY | OFlag::O_NOFOLLOW | OFlag::O_CLOEXEC,
                Mode::empty(),
            )?);
            private_config_copy_tree(&source_child, &child)?;
            fchmod(&child, mode)?;
        } else if kind.contains(SFlag::S_IFREG) {
            let source_file = File::from(openat(
                source,
                name.as_c_str(),
                OFlag::O_RDONLY | OFlag::O_NOFOLLOW | OFlag::O_NONBLOCK | OFlag::O_CLOEXEC,
                Mode::empty(),
            )?);
            anyhow::ensure!(
                source_file.metadata()?.is_file(),
                "private config entry changed to a non-file during publication: {name:?}"
            );
            let mut destination_file = File::from(openat(
                destination,
                name.as_c_str(),
                OFlag::O_WRONLY
                    | OFlag::O_CREAT
                    | OFlag::O_EXCL
                    | OFlag::O_NOFOLLOW
                    | OFlag::O_CLOEXEC,
                mode,
            )?);
            std::io::copy(&mut &source_file, &mut destination_file)?;
            destination_file.sync_all()?;
            fchmod(&destination_file, mode)?;
        } else {
            anyhow::bail!("refusing to publish private config with special entry {name:?}");
        }
    }
    Ok(())
}

#[cfg(unix)]
fn private_config_read_file_at(directory: &File, name: &str) -> anyhow::Result<Option<Vec<u8>>> {
    let name = private_config_name(name)?;
    let Some(mut file) = (match openat(
        directory,
        name.as_c_str(),
        OFlag::O_RDONLY | OFlag::O_NOFOLLOW | OFlag::O_CLOEXEC,
        Mode::empty(),
    ) {
        Ok(fd) => Some(File::from(fd)),
        Err(nix::errno::Errno::ENOENT) => None,
        Err(error) => return Err(error.into()),
    }) else {
        return Ok(None);
    };
    anyhow::ensure!(
        file.metadata()?.is_file(),
        "private config entry is not a regular file: {name:?}"
    );
    let mut bytes = Vec::new();
    file.read_to_end(&mut bytes)?;
    Ok(Some(bytes))
}

#[cfg(unix)]
fn private_config_existing_mode(
    directory: Option<&File>,
    name: &str,
) -> anyhow::Result<Option<Mode>> {
    let Some(directory) = directory else {
        return Ok(None);
    };
    let name = private_config_name(name)?;
    match fstatat(directory, name.as_c_str(), AtFlags::AT_SYMLINK_NOFOLLOW) {
        Ok(stat) => {
            let kind = SFlag::from_bits_truncate(stat.st_mode);
            anyhow::ensure!(
                kind.contains(SFlag::S_IFREG),
                "private config entry is not a regular file: {name:?}"
            );
            Ok(Some(Mode::from_bits_truncate(stat.st_mode)))
        }
        Err(nix::errno::Errno::ENOENT) => Ok(None),
        Err(error) => Err(error.into()),
    }
}

#[cfg(unix)]
fn private_config_write_file_at(
    directory: &File,
    name: &str,
    bytes: &[u8],
    mode: Option<Mode>,
) -> anyhow::Result<()> {
    let name = private_config_name(name)?;
    let temporary = private_config_name(&format!(
        ".jackin-private-config-file-{}-{}",
        std::process::id(),
        PRIVATE_CONFIG_SWAP_COUNTER.fetch_add(1, Ordering::Relaxed)
    ))?;
    let mut file = File::from(openat(
        directory,
        temporary.as_c_str(),
        OFlag::O_WRONLY | OFlag::O_CREAT | OFlag::O_EXCL | OFlag::O_NOFOLLOW | OFlag::O_CLOEXEC,
        mode.unwrap_or_else(|| Mode::from_bits_truncate(0o600)),
    )?);
    let result = (|| {
        file.write_all(bytes)?;
        if let Some(mode) = mode {
            fchmod(&file, mode)?;
        }
        file.sync_all()?;
        renameat(directory, temporary.as_c_str(), directory, name.as_c_str())?;
        Ok::<(), anyhow::Error>(())
    })();
    if result.is_err() {
        let _ignored = unlinkat(directory, temporary.as_c_str(), UnlinkatFlags::NoRemoveDir);
    }
    result
}

#[cfg(unix)]
fn private_config_remove_file_at(directory: &File, name: &str) -> anyhow::Result<()> {
    let name = private_config_name(name)?;
    match fstatat(directory, name.as_c_str(), AtFlags::AT_SYMLINK_NOFOLLOW) {
        Ok(stat) => {
            let kind = SFlag::from_bits_truncate(stat.st_mode);
            anyhow::ensure!(
                kind.contains(SFlag::S_IFREG),
                "private config entry is not a regular file: {name:?}"
            );
            unlinkat(directory, name.as_c_str(), UnlinkatFlags::NoRemoveDir)?;
        }
        Err(nix::errno::Errno::ENOENT) => {}
        Err(error) => return Err(error.into()),
    }
    Ok(())
}

#[cfg(unix)]
fn private_config_allocate_sibling(parent: &File, prefix: &str) -> anyhow::Result<String> {
    for _ in 0..128 {
        let name = format!(
            ".{prefix}-{}-{}",
            std::process::id(),
            PRIVATE_CONFIG_SWAP_COUNTER.fetch_add(1, Ordering::Relaxed)
        );
        let name_c = private_config_name(&name)?;
        match fstatat(parent, name_c.as_c_str(), AtFlags::AT_SYMLINK_NOFOLLOW) {
            Ok(_) => {}
            Err(nix::errno::Errno::ENOENT) => return Ok(name),
            Err(error) => return Err(error.into()),
        }
    }
    anyhow::bail!("could not allocate a private config swap path")
}

#[cfg(unix)]
struct PrivateConfigStage<'a> {
    parent: &'a File,
    name: CString,
    directory: File,
    armed: bool,
}

#[cfg(unix)]
impl PrivateConfigStage<'_> {
    fn disarm(&mut self) {
        self.armed = false;
    }
}

#[cfg(unix)]
impl Drop for PrivateConfigStage<'_> {
    fn drop(&mut self) {
        if self.armed {
            let _ignored = private_config_remove_tree_at(self.parent, &self.name);
        }
    }
}

#[cfg(unix)]
fn private_config_stage(parent: &File) -> anyhow::Result<PrivateConfigStage<'_>> {
    for _ in 0..128 {
        let name = private_config_name(&format!(
            ".jackin-private-config-stage-{}-{}",
            std::process::id(),
            PRIVATE_CONFIG_SWAP_COUNTER.fetch_add(1, Ordering::Relaxed)
        ))?;
        match mkdirat(parent, name.as_c_str(), Mode::from_bits_truncate(0o700)) {
            Ok(()) => {
                let Some(directory) = open_private_config_directory(parent, name.as_c_str())?
                else {
                    anyhow::bail!("created private config stage disappeared");
                };
                return Ok(PrivateConfigStage {
                    parent,
                    name,
                    directory,
                    armed: true,
                });
            }
            Err(nix::errno::Errno::EEXIST) => {}
            Err(error) => return Err(error.into()),
        }
    }
    anyhow::bail!("could not allocate a private config staging directory")
}

#[cfg(unix)]
fn private_config_transaction_names(
    transaction: &PrivateConfigTransaction,
) -> anyhow::Result<(CString, CString, Option<CString>)> {
    let target = private_config_name(&transaction.target)?;
    let staged = private_config_name(&transaction.staged)?;
    let previous = transaction
        .previous
        .as_deref()
        .map(private_config_name)
        .transpose()?;
    anyhow::ensure!(
        transaction.target != PRIVATE_CONFIG_TRANSACTION_FILE
            && transaction.staged != PRIVATE_CONFIG_TRANSACTION_FILE
            && transaction.previous.as_deref() != Some(PRIVATE_CONFIG_TRANSACTION_FILE),
        "private config transaction journal targets its own journal"
    );
    anyhow::ensure!(
        transaction.target != transaction.staged
            && transaction.previous.as_deref() != Some(transaction.target.as_str())
            && transaction.previous.as_deref() != Some(transaction.staged.as_str()),
        "private config transaction journal contains duplicate paths"
    );
    Ok((target, staged, previous))
}

#[cfg(unix)]
fn private_config_persist_transaction(
    publication: &PrivateConfigPublication,
    transaction: &PrivateConfigTransaction,
) -> anyhow::Result<()> {
    let bytes = serde_json::to_vec(transaction)?;
    private_config_write_file_at(
        &publication.parent,
        PRIVATE_CONFIG_TRANSACTION_FILE,
        &bytes,
        None,
    )?;
    publication.parent.sync_all()?;
    #[cfg(test)]
    match transaction.phase {
        PrivateConfigTransactionPhase::PreviousMoved => {
            maybe_inject_private_config_failure(
                PrivateConfigFailurePoint::JournalAfterPreviousMoved,
            )?;
        }
        PrivateConfigTransactionPhase::Installed => {
            maybe_inject_private_config_failure(PrivateConfigFailurePoint::JournalAfterInstalled)?;
        }
        PrivateConfigTransactionPhase::Prepared => {}
    }
    Ok(())
}

#[cfg(unix)]
fn private_config_clear_transaction(publication: &PrivateConfigPublication) -> anyhow::Result<()> {
    private_config_remove_file_at(&publication.parent, PRIVATE_CONFIG_TRANSACTION_FILE)?;
    publication.parent.sync_all()?;
    Ok(())
}

#[cfg(unix)]
fn private_config_recover_transaction(
    publication: &PrivateConfigPublication,
) -> anyhow::Result<()> {
    let Some(bytes) =
        private_config_read_file_at(&publication.parent, PRIVATE_CONFIG_TRANSACTION_FILE)?
    else {
        return Ok(());
    };
    let transaction: PrivateConfigTransaction =
        serde_json::from_slice(&bytes).context("parse private config transaction journal")?;
    anyhow::ensure!(
        transaction.schema_version == PRIVATE_CONFIG_TRANSACTION_VERSION,
        "unsupported private config transaction schema {}",
        transaction.schema_version
    );
    let (target, staged, previous) = private_config_transaction_names(&transaction)?;
    let target_exists =
        open_private_config_directory(&publication.parent, target.as_c_str())?.is_some();
    let staged_exists =
        open_private_config_directory(&publication.parent, staged.as_c_str())?.is_some();
    let previous_exists = previous
        .as_ref()
        .map(|name| open_private_config_directory(&publication.parent, name.as_c_str()))
        .transpose()?
        .flatten()
        .is_some();

    match transaction.phase {
        PrivateConfigTransactionPhase::Prepared => {
            if previous_exists {
                anyhow::ensure!(
                    !target_exists,
                    "private config transaction has both live and previous directories"
                );
                renameat(
                    &publication.parent,
                    previous
                        .as_ref()
                        .context("prepared transaction has no previous")?
                        .as_c_str(),
                    &publication.parent,
                    target.as_c_str(),
                )?;
            } else if transaction.previous.is_some() {
                anyhow::ensure!(
                    target_exists,
                    "private config transaction lost both live and previous directories"
                );
            } else {
                anyhow::ensure!(
                    target_exists || staged_exists,
                    "private config transaction lost both live and staged directories"
                );
            }
            if staged_exists {
                private_config_remove_tree_at(&publication.parent, staged.as_c_str())?;
            }
        }
        PrivateConfigTransactionPhase::PreviousMoved => {
            anyhow::ensure!(
                transaction.previous.is_some(),
                "previous-moved transaction has no previous directory"
            );
            match (target_exists, previous_exists) {
                (true, true) => {
                    anyhow::ensure!(
                        !staged_exists,
                        "private config transaction has ambiguous live, staged, and previous directories"
                    );
                    private_config_remove_tree_at(
                        &publication.parent,
                        previous
                            .as_ref()
                            .context("missing previous directory")?
                            .as_c_str(),
                    )?;
                }
                (false, true) => {
                    if staged_exists {
                        private_config_remove_tree_at(&publication.parent, staged.as_c_str())?;
                    }
                    renameat(
                        &publication.parent,
                        previous
                            .as_ref()
                            .context("missing previous directory")?
                            .as_c_str(),
                        &publication.parent,
                        target.as_c_str(),
                    )?;
                }
                (true, false) => {
                    // The previous directory may have been durably deleted
                    // before a crash. The live target proves publication won;
                    // only an absent staged sibling makes this state unambiguous.
                    anyhow::ensure!(
                        !staged_exists,
                        "private config transaction has ambiguous live and staged directories"
                    );
                }
                (false, false) => {
                    anyhow::bail!(
                        "private config transaction lost its previous directory before recovery"
                    );
                }
            }
        }
        PrivateConfigTransactionPhase::Installed => {
            anyhow::ensure!(
                target_exists,
                "installed private config transaction has no live directory"
            );
            if staged_exists {
                private_config_remove_tree_at(&publication.parent, staged.as_c_str())?;
            }
            if previous_exists {
                private_config_remove_tree_at(
                    &publication.parent,
                    previous
                        .as_ref()
                        .context("installed transaction has no previous")?
                        .as_c_str(),
                )?;
            }
        }
    }
    publication.parent.sync_all()?;
    private_config_clear_transaction(publication)
}

#[cfg(unix)]
fn private_config_restore_swap(
    publication: &PrivateConfigPublication,
    target: &CStr,
    previous: Option<&CStr>,
    installed: bool,
) -> anyhow::Result<()> {
    if installed {
        private_config_remove_tree_at(&publication.parent, target)?;
    }
    if let Some(previous) = previous {
        renameat(&publication.parent, previous, &publication.parent, target)?;
    }
    publication.parent.sync_all()?;
    Ok(())
}

#[cfg(unix)]
fn private_config_abort_transaction(
    publication: &PrivateConfigPublication,
    target: &CStr,
    previous: Option<&CStr>,
    installed: bool,
    cause: anyhow::Error,
) -> anyhow::Result<()> {
    if let Err(rollback_error) =
        private_config_restore_swap(publication, target, previous, installed)
    {
        return Err(cause.context(format!(
            "private config publication failed and rollback failed: {rollback_error:#}"
        )));
    }
    if let Err(clear_error) = private_config_clear_transaction(publication) {
        return Err(cause.context(format!(
            "private config publication failed and journal cleanup failed: {clear_error:#}"
        )));
    }
    Err(cause)
}

#[cfg(unix)]
fn publish_private_config_directory_locked(
    publication: &PrivateConfigPublication,
    directory: &Path,
    files: &[(&'static str, Vec<u8>)],
    remove_files: &[&str],
) -> anyhow::Result<()> {
    anyhow::ensure!(
        directory.parent() == Some(publication.parent_path.as_path()),
        "private config publication parent changed: {}",
        directory.display()
    );
    let target_name = directory
        .file_name()
        .and_then(|name| name.to_str())
        .context("private config directory name is not valid UTF-8")?;
    let target = private_config_name(target_name)?;
    let existing = open_private_config_directory(&publication.parent, target.as_c_str())?;
    let existing_mode = existing
        .as_ref()
        .map(|directory| {
            directory
                .metadata()
                .map(|metadata| metadata.permissions().mode())
        })
        .transpose()?;

    let mut staged = private_config_stage(&publication.parent)?;
    if let Some(existing) = existing.as_ref() {
        private_config_copy_tree(existing, &staged.directory)?;
        fchmod(
            &staged.directory,
            Mode::from_bits_truncate(existing_mode.context("missing live mode")? as _),
        )?;
    }
    for name in remove_files {
        private_config_remove_file_at(&staged.directory, name)?;
    }
    for (name, bytes) in files {
        let mode = private_config_existing_mode(existing.as_ref(), name)?;
        private_config_write_file_at(&staged.directory, name, bytes, mode)?;
        maybe_inject_private_config_failure(PrivateConfigFailurePoint::StagedFile(name))?;
    }
    staged.directory.sync_all()?;
    maybe_inject_private_config_failure(PrivateConfigFailurePoint::BeforeSwap)?;

    let previous_name = if existing.is_some() {
        Some(private_config_allocate_sibling(
            &publication.parent,
            "jackin-private-config-previous",
        )?)
    } else {
        None
    };
    let previous = previous_name
        .as_deref()
        .map(private_config_name)
        .transpose()?;
    let mut transaction = PrivateConfigTransaction {
        schema_version: PRIVATE_CONFIG_TRANSACTION_VERSION,
        target: target_name.to_owned(),
        staged: staged.name.to_string_lossy().into_owned(),
        previous: previous_name,
        phase: PrivateConfigTransactionPhase::Prepared,
    };
    private_config_persist_transaction(publication, &transaction)?;

    if let Some(previous) = previous.as_ref() {
        renameat(
            &publication.parent,
            target.as_c_str(),
            &publication.parent,
            previous.as_c_str(),
        )?;
        transaction.phase = PrivateConfigTransactionPhase::PreviousMoved;
        if let Err(error) = private_config_persist_transaction(publication, &transaction) {
            return private_config_abort_transaction(
                publication,
                target.as_c_str(),
                Some(previous),
                false,
                error,
            );
        }
    }

    #[cfg(test)]
    if PRIVATE_CONFIG_FAILURE.with(|failure| {
        failure.get() == Some(PrivateConfigFailurePoint::SimulatedCrashAfterPreviousRename)
    }) {
        staged.disarm();
        return Err(anyhow::anyhow!(
            "simulated process crash after private config rename"
        ));
    }

    if let Err(error) =
        maybe_inject_private_config_failure(PrivateConfigFailurePoint::AfterPreviousRename)
    {
        return private_config_abort_transaction(
            publication,
            target.as_c_str(),
            previous.as_deref(),
            false,
            error,
        );
    }
    if let Err(error) = renameat(
        &publication.parent,
        staged.name.as_c_str(),
        &publication.parent,
        target.as_c_str(),
    ) {
        return private_config_abort_transaction(
            publication,
            target.as_c_str(),
            previous.as_deref(),
            false,
            error.into(),
        );
    }
    staged.disarm();
    transaction.phase = PrivateConfigTransactionPhase::Installed;
    if let Err(error) = private_config_persist_transaction(publication, &transaction) {
        return private_config_abort_transaction(
            publication,
            target.as_c_str(),
            previous.as_deref(),
            true,
            error,
        );
    }
    if let Err(error) = maybe_inject_private_config_failure(PrivateConfigFailurePoint::AfterInstall)
    {
        return private_config_abort_transaction(
            publication,
            target.as_c_str(),
            previous.as_deref(),
            true,
            error,
        );
    }
    if let Err(error) = publication.parent.sync_all() {
        return private_config_abort_transaction(
            publication,
            target.as_c_str(),
            previous.as_deref(),
            true,
            error.into(),
        );
    }
    if let Some(previous) = previous.as_ref() {
        private_config_remove_tree_at(&publication.parent, previous.as_c_str())?;
        #[cfg(test)]
        if PRIVATE_CONFIG_FAILURE.with(|failure| {
            failure.get() == Some(PrivateConfigFailurePoint::SimulatedCrashAfterPreviousDeletion)
        }) {
            return Err(anyhow::anyhow!(
                "simulated process crash after previous private config deletion"
            ));
        }
        // The Installed journal is already durable. If this fsync or journal
        // cleanup fails, the next launch sees the new live tree and completes
        // the idempotent Installed recovery path.
        publication.parent.sync_all()?;
    }
    private_config_clear_transaction(publication)
}

#[cfg(unix)]
fn publish_private_config_directory(
    root: &Path,
    directory: &Path,
    files: &[(&'static str, Vec<u8>)],
    remove_files: &[&str],
) -> anyhow::Result<()> {
    let parent = directory
        .parent()
        .context("private config directory has no parent")?;
    let publication = begin_private_config_publication(root, parent)?;
    private_config_recover_transaction(&publication)?;
    publish_private_config_directory_locked(&publication, directory, files, remove_files)
}

#[cfg(unix)]
fn read_private_config_file_at(
    publication: &PrivateConfigPublication,
    name: &str,
) -> anyhow::Result<Option<Vec<u8>>> {
    private_config_read_file_at(&publication.parent, name)
}

fn account_with_effective_model(
    account: &jackin_config::AccountConfig,
    model: Option<&str>,
) -> jackin_config::AccountConfig {
    let Some(model) = model else {
        return account.clone();
    };
    let mut account = account.clone();
    if let AccountCredential::ApiKey {
        model: account_model,
        ..
    } = &mut account.credential
    {
        *account_model = Some(model.to_owned());
    }
    account
}

pub(super) fn configure_accounts(
    root: &Path,
    config: &AppConfig,
    instances: &[jackin_config::ResolvedInstance],
    slots: &BTreeMap<String, crate::instance::ProvisionedInstanceAuth>,
    models: &BTreeMap<String, String>,
    efforts: &BTreeMap<String, String>,
) -> anyhow::Result<()> {
    for instance in instances {
        let Some(slot) = slots.get(&instance.config_id) else {
            if matches!(instance.agent, Agent::Codex | Agent::Opencode) {
                anyhow::bail!(
                    "instance {:?} has no provisioned config slot",
                    instance.config_id
                );
            }
            continue;
        };
        anyhow::ensure!(
            slot.agent == instance.agent,
            "provisioned config slot for {:?} belongs to {}, not {}",
            instance.config_id,
            slot.agent,
            instance.agent
        );
        anyhow::ensure!(
            slot.account_id == instance.account_id,
            "provisioned config slot for {:?} belongs to account {:?}, not {:?}",
            instance.config_id,
            slot.account_id,
            instance.account_id
        );
        let model = models
            .get(&instance.config_id)
            .map(String::as_str)
            .or(instance.model.as_deref());
        match instance.agent {
            Agent::Codex => configure_codex(
                root,
                config,
                instance,
                slot,
                model,
                efforts.get(&instance.config_id).map(String::as_str),
            )?,
            Agent::Opencode => configure_opencode(root, config, instance, slot, model)?,
            _ => {}
        }
    }
    Ok(())
}

fn configure_codex(
    root: &Path,
    config: &AppConfig,
    instance: &jackin_config::ResolvedInstance,
    slot: &crate::instance::ProvisionedInstanceAuth,
    model: Option<&str>,
    effort: Option<&str>,
) -> anyhow::Result<()> {
    let account = config
        .accounts
        .get(&instance.account_id)
        .ok_or_else(|| anyhow::anyhow!("unknown account {:?}", instance.account_id))?;
    let AccountCredential::ApiKey { .. } = &account.credential else {
        return Ok(());
    };
    // `container_home_rel` is computed by the auth provisioner from the same
    // slot layout used by mounts and the Capsule's CODEX_HOME value. Never
    // collapse multiple admitted Codex instances onto the primary home.
    let directory = root.join("home").join(&slot.container_home_rel);
    let publication = {
        let parent = directory
            .parent()
            .context("private Codex configuration directory has no parent")?;
        let publication = begin_private_config_publication(root, parent)?;
        private_config_recover_transaction(&publication)?;
        publication
    };
    let existing = read_private_config_file_at(&publication, "config.toml")?;
    let files = build_codex_private_config_files(
        account,
        slot,
        instance,
        model,
        effort,
        existing.as_deref(),
    )?;
    publish_private_config_directory_locked(
        &publication,
        &directory,
        &files,
        &["account-models.json"],
    )
    .context("publish private Codex account configuration")
}

fn build_codex_private_config_files(
    account: &jackin_config::AccountConfig,
    slot: &crate::instance::ProvisionedInstanceAuth,
    instance: &jackin_config::ResolvedInstance,
    model: Option<&str>,
    effort: Option<&str>,
    existing: Option<&[u8]>,
) -> anyhow::Result<Vec<(&'static str, Vec<u8>)>> {
    let base_url = instance.base_url.as_deref();
    let (default_url, key) = match account.provider {
        AiProvider::Moonshot => ("https://api.kimi.com/coding/v1", "KIMI_API_KEY"),
        AiProvider::Zai => ("https://api.z.ai/api/v1", "OPENAI_API_KEY"),
        AiProvider::Minimax => ("https://api.minimax.io/v1", "MINIMAX_API_KEY"),
        AiProvider::OpenAi => ("https://api.openai.com/v1", "OPENAI_API_KEY"),
        _ => anyhow::bail!("selected provider cannot authenticate Codex"),
    };
    let cross_provider = account.provider != AiProvider::OpenAi;
    anyhow::ensure!(
        !cross_provider || model.is_some(),
        "a model is required for a Codex provider account"
    );
    let mut document: toml::Table = match existing {
        Some(contents) => {
            toml::from_slice(contents).context("parse private Codex configuration")?
        }
        None => toml::Table::new(),
    };
    let mut provider = toml::Table::new();
    provider.insert("name".into(), account.provider.slug().into());
    provider.insert("base_url".into(), base_url.unwrap_or(default_url).into());
    provider.insert("env_key".into(), key.into());
    provider.insert("wire_api".into(), "responses".into());
    provider.insert("requires_openai_auth".into(), false.into());
    let providers = document
        .entry("model_providers")
        .or_insert_with(|| toml::Value::Table(toml::Table::new()));
    let providers = providers
        .as_table_mut()
        .context("Codex model_providers must be a table")?;
    providers.insert("jackin_account".into(), provider.into());
    document.insert("model_provider".into(), "jackin_account".into());
    let mut files = Vec::new();
    if let Some(model) = model {
        document.insert("model".into(), model.into());
        if let Some(catalog) = model_catalog(account.provider, model) {
            if let Some(effort) = effort {
                anyhow::ensure!(
                    catalog_supports_effort(&catalog, effort),
                    "Codex model {model:?} does not support reasoning effort {effort:?}"
                );
            }
            files.push(("account-models.json", serde_json::to_vec_pretty(&catalog)?));
            let catalog_target = Path::new(&slot.folder_target)
                .join("account-models.json")
                .to_string_lossy()
                .into_owned();
            document.insert("model_catalog_json".into(), catalog_target.into());
        } else {
            document.remove("model_catalog_json");
        }
    } else {
        document.remove("model");
        document.remove("model_catalog_json");
    }
    if let Some(effort) = effort {
        document.insert("model_reasoning_effort".into(), effort.into());
    } else {
        document.remove("model_reasoning_effort");
    }
    files.push((
        "config.toml",
        toml::to_string_pretty(&document)?.into_bytes(),
    ));
    Ok(files)
}

/// Provider identifiers from `OpenCode`'s catalog; config and CLI model use the same ID.
fn opencode_provider(
    provider: AiProvider,
) -> anyhow::Result<(&'static str, &'static str, &'static str)> {
    Ok(match provider {
        AiProvider::Anthropic => (
            "anthropic",
            "@ai-sdk/anthropic",
            "https://api.anthropic.com/v1",
        ),
        AiProvider::OpenAi => ("openai", "@ai-sdk/openai", "https://api.openai.com/v1"),
        AiProvider::Xai => ("xai", "@ai-sdk/xai", "https://api.x.ai/v1"),
        AiProvider::Moonshot => (
            "kimi-for-coding",
            "@ai-sdk/anthropic",
            "https://api.kimi.com/coding/v1",
        ),
        AiProvider::Zai => (
            "zai-coding-plan",
            "@ai-sdk/openai-compatible",
            "https://api.z.ai/api/coding/paas/v4",
        ),
        AiProvider::Minimax => (
            "minimax",
            "@ai-sdk/anthropic",
            "https://api.minimax.io/anthropic/v1",
        ),
        AiProvider::Opencode => (
            "opencode",
            "@ai-sdk/openai-compatible",
            "https://opencode.ai/zen/v1",
        ),
        // models.dev `google` provider via the AI SDK Google package; v1beta
        // is the package default base.
        AiProvider::Google => (
            "google",
            "@ai-sdk/google",
            "https://generativelanguage.googleapis.com/v1beta",
        ),
        AiProvider::OpenRouter => (
            "openrouter",
            "@ai-sdk/openai-compatible",
            "https://openrouter.ai/api/v1",
        ),
        // No OpenCode catalog provider exists for Cursor/Meta; a custom
        // provider entry needs verified protocol/base-URL details that are
        // still unknown (Cursor's agent endpoint is proprietary; Meta's API
        // base is unconfirmed). Deferred to the provider-config lane.
        AiProvider::Cursor | AiProvider::Meta => anyhow::bail!(
            "{provider} accounts cannot authenticate OpenCode yet: no catalog provider entry"
        ),
        AiProvider::Amp => anyhow::bail!("Amp accounts cannot authenticate OpenCode"),
    })
}

pub(super) fn opencode_model(provider: AiProvider, model: &str) -> anyhow::Result<String> {
    let (id, _, _) = opencode_provider(provider)?;
    if model.starts_with(&format!("{id}/")) {
        Ok(model.to_owned())
    } else {
        Ok(format!("{id}/{model}"))
    }
}

/// <https://opencode.ai/docs/providers>: provider options and model IDs are paired.
fn configure_opencode(
    root: &Path,
    config: &AppConfig,
    instance: &jackin_config::ResolvedInstance,
    slot: &crate::instance::ProvisionedInstanceAuth,
    model: Option<&str>,
) -> anyhow::Result<()> {
    let account = config
        .accounts
        .get(&instance.account_id)
        .ok_or_else(|| anyhow::anyhow!("unknown account {:?}", instance.account_id))?;
    let AccountCredential::ApiKey { .. } = &account.credential else {
        return Ok(());
    };
    let base_url = instance.base_url.as_deref();
    let (id, npm, default_url) = opencode_provider(account.provider)?;
    let credential_account = account_with_effective_model(account, model);
    let credentials = credential_account.credential_env(Agent::Opencode)?;
    let key = credentials
        .keys()
        .next()
        .context("OpenCode account has no credential variable")?;
    let directory = root.join("home").join(crate::instance::slot_home_rel(
        ".config/opencode",
        slot.slot_suffix.as_deref(),
    ));
    let mut provider = serde_json::json!({
        "name": account.name, "npm": npm,
        "options": { "baseURL": base_url.unwrap_or(default_url), "apiKey": format!("{{env:{key}}}") }
    });
    // Zen chooses a protocol per model; preserve its built-in catalog routing.
    if account.provider == AiProvider::Opencode && base_url.is_none() {
        provider
            .as_object_mut()
            .context("OpenCode provider must be an object")?
            .remove("npm");
        provider["options"]
            .as_object_mut()
            .context("OpenCode options must be an object")?
            .remove("baseURL");
    }
    let mut document = serde_json::json!({
        "$schema": "https://opencode.ai/config.json", "permission": "allow",
        "enabled_providers": [id]
    });
    if let Some(model) = model {
        let full_model = opencode_model(account.provider, model)?;
        let model_id = full_model
            .strip_prefix(&format!("{id}/"))
            .context("OpenCode model provider mismatch")?;
        provider["models"] = serde_json::json!({ model_id: { "name": model_id } });
        document["model"] = full_model.into();
    }
    document["provider"] = serde_json::json!({ id: provider });
    publish_private_config_directory(
        root,
        &directory,
        &[("opencode.json", serde_json::to_vec_pretty(&document)?)],
        &[],
    )
    .context("publish private OpenCode account configuration")
}

/// Provider-published metadata, not guessed for custom model IDs.
/// <https://www.kimi.com/code/docs/en/third-party-tools/codex.html>
/// <https://docs.z.ai/devpack/tool/codex>
/// <https://platform.minimax.io/docs/token-plan/codex>
fn catalog_supports_effort(catalog: &serde_json::Value, effort: &str) -> bool {
    catalog["models"]
        .as_array()
        .and_then(|models| models.first())
        .and_then(|model| model["supported_reasoning_levels"].as_array())
        .is_some_and(|levels| {
            levels
                .iter()
                .any(|level| level["effort"].as_str() == Some(effort))
        })
}

fn model_catalog(provider: AiProvider, model: &str) -> Option<serde_json::Value> {
    let (context, modalities) = match (provider, model) {
        (AiProvider::Moonshot, "k3") => (1_048_576, vec!["text", "image"]),
        (AiProvider::Moonshot, "k3-256k") => (262_144, vec!["text", "image"]),
        (AiProvider::Zai, "glm-5.3") => (1_048_576, vec!["text"]),
        (AiProvider::Minimax, "MiniMax-M3") => (1_000_000, vec!["text", "image"]),
        _ => return None,
    };
    let mut entry = serde_json::json!({
        "slug": model, "display_name": model, "description": model,
        "default_reasoning_level": "high",
        "supported_reasoning_levels": [
            { "effort": "low", "description": "Light reasoning" },
            { "effort": "medium", "description": "Balanced reasoning" },
            { "effort": "high", "description": "Enhanced reasoning" },
            { "effort": "max", "description": "Deep reasoning" }
        ],
        "shell_type": "shell_command", "visibility": "list", "supported_in_api": true,
        "priority": 0, "base_instructions": "", "supports_reasoning_summaries": true,
        "default_reasoning_summary": "none", "support_verbosity": false,
        "truncation_policy": { "mode": "bytes", "limit": 10000 },
        "context_window": context, "max_context_window": context,
        "effective_context_window_percent": 95, "supports_parallel_tool_calls": true,
        "experimental_supported_tools": [], "input_modalities": modalities
    });
    if provider == AiProvider::Minimax {
        entry["supported_reasoning_levels"] = serde_json::json!([
            { "effort": "none", "description": "Thinking disabled" },
            { "effort": "high", "description": "Adaptive thinking" }
        ]);
    }
    if provider == AiProvider::Zai {
        entry["apply_patch_tool_type"] = "freeform".into();
    }
    Some(serde_json::json!({ "models": [entry] }))
}

#[cfg(test)]
mod tests;
