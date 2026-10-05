// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Publish selected API account settings in host-only provider authority.
//! Only exact generated files cross into the capsule as read-only overlays.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use anyhow::Context as _;
use jackin_config::{AccountCredential, AiProvider, AppConfig};
use jackin_core::Agent;

#[cfg(unix)]
#[path = "account_config/private_config_bounds.rs"]
mod private_config_bounds;

#[cfg(unix)]
mod private_config_fs {
    use std::fs::File;
    use std::io::Write as _;
    use std::path::{Component, Path, PathBuf};
    use std::sync::atomic::{AtomicU64, Ordering};

    #[cfg(target_os = "macos")]
    use std::ffi::OsString;

    #[cfg(target_os = "macos")]
    use std::os::unix::ffi::{OsStrExt as _, OsStringExt as _};

    use anyhow::Context as _;
    use nix::errno::Errno;
    use nix::fcntl::{AtFlags, OFlag, open, openat, renameat};
    use nix::sys::stat::{Mode, SFlag, fchmod, fstat, fstatat, mkdirat};
    use nix::unistd::{UnlinkatFlags, linkat, unlinkat};

    use super::private_config_bounds;

    static TEMP_SEQUENCE: AtomicU64 = AtomicU64::new(0);
    const LOCK_FILE: &str = ".jackin-private-provider-config.lock";
    const PRIVATE_DIR_MODE: Mode = Mode::S_IRWXU;
    const PRIVATE_FILE_MODE: Mode = Mode::from_bits_truncate(0o600);

    #[derive(Clone, Copy, Debug)]
    struct LeafName<'a>(&'a str);

    impl<'a> LeafName<'a> {
        fn parse(name: &'a str) -> anyhow::Result<Self> {
            anyhow::ensure!(!name.is_empty(), "private provider config name is empty");
            anyhow::ensure!(
                !name.as_bytes().contains(&0),
                "private provider config name contains NUL"
            );
            anyhow::ensure!(
                !name.contains('/'),
                "private provider config name must not contain path separators"
            );
            let mut components = Path::new(name).components();
            anyhow::ensure!(
                matches!(components.next(), Some(Component::Normal(_)))
                    && components.next().is_none(),
                "private provider config name must be one normal path component"
            );
            Ok(Self(name))
        }

        fn as_str(self) -> &'a str {
            self.0
        }
    }

    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    pub(super) enum Artifact {
        CodexCatalog,
        CodexConfig,
        OpenCodeConfig,
        CodexGenerationConfig,
        OpenCodeGenerationConfig,
    }

    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    pub(super) enum PublishPoint {
        TempCreated(Artifact),
        TempWritten(Artifact),
        TempSynced(Artifact),
        BeforeInstall(Artifact),
        Installed(Artifact),
        DirectorySynced(Artifact),
    }

    pub(super) fn open_directory(root: &Path, home_relative: &Path) -> anyhow::Result<File> {
        let root = normalize_root(root)?;
        let components = root.components().collect::<Vec<_>>();
        let traversal_root = if root.is_absolute() {
            Path::new("/")
        } else {
            Path::new(".")
        };
        let mut directory = File::from(
            open(
                traversal_root,
                OFlag::O_RDONLY | OFlag::O_DIRECTORY | OFlag::O_CLOEXEC | OFlag::O_NOFOLLOW,
                Mode::empty(),
            )
            .context("open private account config traversal root")?,
        );
        let mut saw_root = false;
        for (index, component) in components.iter().enumerate() {
            match component {
                Component::RootDir => {}
                Component::Normal(name) => {
                    saw_root = true;
                    let final_component = !components[index + 1..]
                        .iter()
                        .any(|component| matches!(component, Component::Normal(_)));
                    directory = if final_component {
                        open_child_directory(&directory, name, false)?
                    } else {
                        open_unchecked_child_directory(&directory, name)?
                    };
                }
                Component::CurDir => {}
                Component::ParentDir | Component::Prefix(_) => {
                    anyhow::bail!("private account config root must not contain parent paths")
                }
            }
        }
        anyhow::ensure!(saw_root, "private account config root is empty");

        // The instance root is never mounted. Keeping authority outside every
        // home/state/auth bind removes the agent's ability to exchange staged
        // names, replace locks, or mutate published config/catalog bytes.
        let directory =
            open_child_directory(&directory, std::ffi::OsStr::new("provider-config"), true)?;
        let mut directory = open_child_directory(&directory, std::ffi::OsStr::new("home"), true)?;
        let mut found_component = false;
        for component in home_relative.components() {
            let Component::Normal(name) = component else {
                anyhow::bail!("private account config directory must be a relative normal path")
            };
            found_component = true;
            directory = open_child_directory(&directory, name, true)?;
        }
        anyhow::ensure!(found_component, "private account config directory is empty");
        Ok(directory)
    }

    /// Normalize only lexical aliases. Symlinks remain rejected by the
    /// descriptor-relative `O_NOFOLLOW` walk below.
    pub(super) fn normalize_root(path: &Path) -> anyhow::Result<PathBuf> {
        let mut normalized = if path.is_absolute() {
            PathBuf::new()
        } else {
            std::env::current_dir().context("finding private account config root base")?
        };
        let mut saw_root = false;
        for component in path.components() {
            match component {
                Component::Prefix(prefix) => normalized.push(prefix.as_os_str()),
                Component::RootDir => normalized.push(Path::new("/")),
                Component::CurDir => {}
                Component::Normal(component) => {
                    saw_root = true;
                    normalized.push(component);
                }
                Component::ParentDir => {
                    anyhow::bail!(
                        "private account config root contains parent traversal: {}",
                        path.display()
                    )
                }
            }
        }
        anyhow::ensure!(saw_root, "private account config root is empty");

        #[cfg(target_os = "macos")]
        {
            let bytes = normalized.as_os_str().as_bytes();
            for alias in [b"/var".as_slice(), b"/tmp".as_slice(), b"/etc".as_slice()] {
                if bytes == alias
                    || bytes
                        .strip_prefix(alias)
                        .is_some_and(|rest| rest.starts_with(b"/"))
                {
                    let mut aliased = b"/private".to_vec();
                    aliased.extend_from_slice(bytes);
                    return Ok(PathBuf::from(OsString::from_vec(aliased)));
                }
            }
        }
        Ok(normalized)
    }

    fn open_child_directory(
        parent: &File,
        name: &std::ffi::OsStr,
        create: bool,
    ) -> anyhow::Result<File> {
        let flags = OFlag::O_RDONLY | OFlag::O_DIRECTORY | OFlag::O_CLOEXEC | OFlag::O_NOFOLLOW;
        match openat(parent, name, flags, Mode::empty()) {
            Ok(fd) => {
                let directory = File::from(fd);
                ensure_private_directory(&directory, name)?;
                Ok(directory)
            }
            Err(Errno::ENOENT) if create => {
                match mkdirat(parent, name, PRIVATE_DIR_MODE) {
                    Ok(()) => parent
                        .sync_all()
                        .context("sync private account config parent")?,
                    Err(Errno::EEXIST) => {}
                    Err(error) => {
                        return Err(error).context("create private account config directory");
                    }
                }
                let directory = openat(parent, name, flags, Mode::empty())
                    .map(File::from)
                    .context("open private account config directory")?;
                ensure_private_directory(&directory, name)?;
                Ok(directory)
            }
            Err(error) => Err(error).context("open private account config directory"),
        }
    }

    fn open_unchecked_child_directory(
        parent: &File,
        name: &std::ffi::OsStr,
    ) -> anyhow::Result<File> {
        openat(
            parent,
            name,
            OFlag::O_RDONLY | OFlag::O_DIRECTORY | OFlag::O_CLOEXEC | OFlag::O_NOFOLLOW,
            Mode::empty(),
        )
        .map(File::from)
        .context("open private account config path component")
    }

    pub(super) fn lock(directory: &File) -> anyhow::Result<File> {
        let lock_name = LeafName::parse(LOCK_FILE)?;
        let fd = openat(
            directory,
            lock_name.as_str(),
            OFlag::O_RDWR
                | OFlag::O_CREAT
                | OFlag::O_CLOEXEC
                | OFlag::O_NOFOLLOW
                | OFlag::O_NONBLOCK,
            PRIVATE_FILE_MODE,
        )
        .context("open private provider config lock")?;
        let lock = File::from(fd);
        ensure_regular(&lock, lock_name.as_str())?;
        lock.sync_all()
            .context("sync private provider config lock")?;
        directory
            .sync_all()
            .context("sync private provider config directory")?;
        private_config_bounds::lock(&lock)?;
        Ok(lock)
    }

    pub(super) fn read_optional(directory: &File, name: &str) -> anyhow::Result<Option<Vec<u8>>> {
        read_optional_bounded(directory, name, private_config_bounds::MAX_CONFIG_BYTES)
    }

    fn read_optional_bounded(
        directory: &File,
        name: &str,
        limit: usize,
    ) -> anyhow::Result<Option<Vec<u8>>> {
        let name = LeafName::parse(name)?;
        let Some(mut file) = open_existing_regular(directory, name)? else {
            return Ok(None);
        };
        private_config_bounds::read(&mut file, name.as_str(), limit).map(Some)
    }

    /// Move a malformed config beside the replacement without resolving any
    /// path component or following a leaf symlink. The reserved name is
    /// created with `O_EXCL`; `renameat` then moves the original inode into
    /// that descriptor-relative namespace.
    pub(super) fn quarantine(directory: &File, name: &str, reason: &str) -> anyhow::Result<()> {
        let name = LeafName::parse(name)?;
        // Verify the source before reserving a quarantine name. This keeps a
        // disappearing or replaced config from being silently treated as the
        // malformed payload we just inspected.
        drop(
            open_existing_regular(directory, name)?.with_context(|| {
                format!("private provider config {} disappeared", name.as_str())
            })?,
        );
        let unix_secs = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |elapsed| elapsed.as_secs());
        for suffix in 0..128u32 {
            let target = if suffix == 0 {
                format!(
                    "{}.corrupt-{unix_secs}-{}",
                    name.as_str(),
                    std::process::id()
                )
            } else {
                format!(
                    "{}.corrupt-{unix_secs}-{}-{suffix}",
                    name.as_str(),
                    std::process::id()
                )
            };
            let target = LeafName::parse(&target)?;
            let reserved = match openat(
                directory,
                target.as_str(),
                OFlag::O_WRONLY
                    | OFlag::O_CREAT
                    | OFlag::O_EXCL
                    | OFlag::O_CLOEXEC
                    | OFlag::O_NOFOLLOW,
                PRIVATE_FILE_MODE,
            ) {
                Ok(file) => File::from(file),
                Err(Errno::EEXIST) => continue,
                Err(error) => {
                    return Err(error).context("reserve private provider config quarantine name");
                }
            };
            ensure_regular(&reserved, target.as_str())?;
            drop(reserved);
            unlinkat(directory, target.as_str(), UnlinkatFlags::NoRemoveDir).with_context(
                || {
                    format!(
                        "release private provider config quarantine name {}",
                        target.as_str()
                    )
                },
            )?;
            renameat(directory, name.as_str(), directory, target.as_str())
                .with_context(|| format!("quarantine private provider config {}", name.as_str()))?;
            directory
                .sync_all()
                .context("sync private provider config quarantine")?;
            eprintln!(
                "[jackin] warning: private Codex configuration {} is corrupt ({reason}); moved to {} and regenerating",
                name.as_str(),
                target.as_str()
            );
            return Ok(());
        }
        anyhow::bail!("could not allocate a private provider config quarantine name")
    }

    fn open_existing_regular(directory: &File, name: LeafName<'_>) -> anyhow::Result<Option<File>> {
        match openat(
            directory,
            name.as_str(),
            OFlag::O_RDONLY | OFlag::O_CLOEXEC | OFlag::O_NOFOLLOW | OFlag::O_NONBLOCK,
            Mode::empty(),
        ) {
            Ok(fd) => {
                let file = File::from(fd);
                ensure_regular(&file, name.as_str())?;
                Ok(Some(file))
            }
            Err(Errno::ENOENT) => Ok(None),
            Err(error) => Err(error)
                .with_context(|| format!("open private provider config {}", name.as_str())),
        }
    }

    fn ensure_regular(file: &File, name: &str) -> anyhow::Result<()> {
        let stat = fstat(file).with_context(|| format!("stat private provider config {name}"))?;
        anyhow::ensure!(
            SFlag::from_bits_truncate(stat.st_mode) == SFlag::S_IFREG,
            "private provider config {name} is not a regular file"
        );
        anyhow::ensure!(
            stat.st_uid == nix::unistd::geteuid().as_raw(),
            "private provider config {name} is not owned by the current user"
        );
        ensure_mode(file, name, PRIVATE_FILE_MODE, stat.st_mode)?;
        Ok(())
    }

    fn ensure_private_directory(file: &File, name: &std::ffi::OsStr) -> anyhow::Result<()> {
        let stat = fstat(file).context("stat private account config directory")?;
        anyhow::ensure!(
            SFlag::from_bits_truncate(stat.st_mode) == SFlag::S_IFDIR,
            "private account config path component {name:?} is not a directory"
        );
        anyhow::ensure!(
            stat.st_uid == nix::unistd::geteuid().as_raw(),
            "private account config directory {name:?} is not owned by the current user"
        );
        ensure_mode(
            file,
            &name.to_string_lossy(),
            PRIVATE_DIR_MODE,
            stat.st_mode,
        )
    }

    fn ensure_mode(
        file: &File,
        name: &str,
        expected: Mode,
        current: nix::libc::mode_t,
    ) -> anyhow::Result<()> {
        if Mode::from_bits_truncate(current) != expected {
            fchmod(file, expected)
                .with_context(|| format!("chmod private provider config {name}"))?;
            let verified =
                fstat(file).with_context(|| format!("restat private provider config {name}"))?;
            anyhow::ensure!(
                Mode::from_bits_truncate(verified.st_mode) == expected,
                "private provider config {name} has unsafe mode"
            );
        }
        Ok(())
    }

    pub(super) fn publish_catalog<F>(
        directory: &File,
        name: &str,
        contents: &[u8],
        hook: F,
    ) -> anyhow::Result<()>
    where
        F: FnMut(PublishPoint) -> anyhow::Result<()>,
    {
        publish_immutable(directory, name, contents, Artifact::CodexCatalog, hook)
    }

    pub(super) fn publish_immutable<F>(
        directory: &File,
        name: &str,
        contents: &[u8],
        artifact: Artifact,
        mut hook: F,
    ) -> anyhow::Result<()>
    where
        F: FnMut(PublishPoint) -> anyhow::Result<()>,
    {
        let name = LeafName::parse(name)?;
        match read_optional_bounded(directory, name.as_str(), contents.len())? {
            Some(existing) if existing == contents => return Ok(()),
            Some(_) => {
                anyhow::bail!(
                    "content-addressed provider artifact {} has different contents",
                    name.as_str()
                )
            }
            None => {}
        }

        let (temp_name, mut temp_file) = create_temp_file(directory)?;
        let result = (|| {
            hook(PublishPoint::TempCreated(artifact))?;
            temp_file
                .write_all(contents)
                .context("write staged provider artifact")?;
            hook(PublishPoint::TempWritten(artifact))?;
            temp_file
                .sync_all()
                .context("sync staged provider artifact")?;
            hook(PublishPoint::TempSynced(artifact))?;
            hook(PublishPoint::BeforeInstall(artifact))?;
            match linkat(
                directory,
                temp_name.as_str(),
                directory,
                name.as_str(),
                AtFlags::empty(),
            ) {
                Ok(()) => {}
                Err(Errno::EEXIST) => {
                    let existing = read_optional_bounded(directory, name.as_str(), contents.len())?;
                    anyhow::ensure!(
                        existing.as_deref() == Some(contents),
                        "content-addressed provider artifact {} changed during publication",
                        name.as_str()
                    );
                    return Ok(());
                }
                Err(error) => return Err(error).context("install immutable provider artifact"),
            }
            directory
                .sync_all()
                .context("sync installed provider artifact")?;
            hook(PublishPoint::Installed(artifact))?;
            directory
                .sync_all()
                .context("sync installed provider artifact")?;
            hook(PublishPoint::DirectorySynced(artifact))?;
            Ok(())
        })();

        cleanup_owned_temp(directory, &temp_name, &temp_file, result)
    }

    pub(super) fn publish_atomic<F>(
        directory: &File,
        name: &str,
        contents: &[u8],
        artifact: Artifact,
        mut hook: F,
    ) -> anyhow::Result<()>
    where
        F: FnMut(PublishPoint) -> anyhow::Result<()>,
    {
        let name = LeafName::parse(name)?;
        validate_config_contents(contents, name.as_str())?;
        // Check the current entry without following it. Renameat below cannot
        // follow a leaf symlink, but rejecting non-regular entries also keeps
        // malformed capsule state from being silently taken over.
        drop(open_existing_regular(directory, name)?);
        let (temp_name, mut temp_file) = create_temp_file(directory)?;
        let result = (|| {
            hook(PublishPoint::TempCreated(artifact))?;
            temp_file.write_all(contents).with_context(|| {
                format!("write staged private provider config {}", name.as_str())
            })?;
            hook(PublishPoint::TempWritten(artifact))?;
            temp_file.sync_all().with_context(|| {
                format!("sync staged private provider config {}", name.as_str())
            })?;
            hook(PublishPoint::TempSynced(artifact))?;
            hook(PublishPoint::BeforeInstall(artifact))?;
            renameat(directory, temp_name.as_str(), directory, name.as_str()).with_context(
                || {
                    format!(
                        "atomically install private provider config {}",
                        name.as_str()
                    )
                },
            )?;
            directory.sync_all().with_context(|| {
                format!(
                    "sync private provider config directory for {}",
                    name.as_str()
                )
            })?;
            hook(PublishPoint::Installed(artifact))?;
            directory.sync_all().with_context(|| {
                format!(
                    "sync private provider config directory for {}",
                    name.as_str()
                )
            })?;
            hook(PublishPoint::DirectorySynced(artifact))?;
            Ok(())
        })();

        cleanup_owned_temp(directory, &temp_name, &temp_file, result)
    }

    pub(super) fn validate_config_contents(contents: &[u8], name: &str) -> anyhow::Result<()> {
        private_config_bounds::ensure_size(
            contents.len() as u64,
            name,
            private_config_bounds::MAX_CONFIG_BYTES,
        )
    }

    pub(super) fn create_temp_file(directory: &File) -> anyhow::Result<(String, File)> {
        for _ in 0..128 {
            let sequence = TEMP_SEQUENCE.fetch_add(1, Ordering::Relaxed);
            let name = format!(
                ".jackin-private-provider-config-{}-{sequence}.tmp",
                std::process::id()
            );
            let name = LeafName::parse(&name)?;
            match openat(
                directory,
                name.as_str(),
                OFlag::O_WRONLY
                    | OFlag::O_CREAT
                    | OFlag::O_EXCL
                    | OFlag::O_CLOEXEC
                    | OFlag::O_NOFOLLOW,
                PRIVATE_FILE_MODE,
            ) {
                Ok(fd) => {
                    let file = File::from(fd);
                    ensure_regular(&file, name.as_str())?;
                    return Ok((name.as_str().to_owned(), file));
                }
                Err(Errno::EEXIST) => {}
                Err(error) => {
                    return Err(error).context("create private provider config staging file");
                }
            }
        }
        anyhow::bail!("could not allocate a private provider config staging name")
    }

    pub(super) fn cleanup_owned_temp(
        directory: &File,
        temp_name: &str,
        temp_file: &File,
        result: anyhow::Result<()>,
    ) -> anyhow::Result<()> {
        let temp_name = LeafName::parse(temp_name)?;
        let temp_stat = fstat(temp_file).context("stat owned provider config staging file")?;
        anyhow::ensure!(
            temp_stat.st_uid == nix::unistd::geteuid().as_raw(),
            "provider config staging file is not owned by the current user"
        );
        let mut removed = false;
        match fstatat(directory, temp_name.as_str(), AtFlags::AT_SYMLINK_NOFOLLOW) {
            Ok(path_stat)
                if path_stat.st_dev == temp_stat.st_dev && path_stat.st_ino == temp_stat.st_ino =>
            {
                match unlinkat(directory, temp_name.as_str(), UnlinkatFlags::NoRemoveDir) {
                    Ok(()) => removed = true,
                    Err(Errno::ENOENT) => {}
                    Err(error) => {
                        return Err(error).context("remove owned provider config staging file");
                    }
                }
            }
            Err(Errno::ENOENT) => {}
            Ok(_) => {
                anyhow::bail!("provider config staging name changed ownership; left untouched")
            }
            Err(error) => return Err(error).context("verify owned provider config staging file"),
        }
        if removed {
            directory
                .sync_all()
                .context("sync private provider config staging cleanup")?;
        }
        result
    }
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
) -> anyhow::Result<Vec<(PathBuf, String)>> {
    let mut mounts = Vec::new();
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
            Agent::Codex => mounts.extend(configure_codex(
                root,
                config,
                instance,
                slot,
                model,
                efforts.get(&instance.config_id).map(String::as_str),
            )?),
            Agent::Opencode => {
                mounts.extend(configure_opencode(root, config, instance, slot, model)?)
            }
            _ => {}
        }
    }
    Ok(mounts)
}

#[cfg(unix)]
fn configure_codex(
    root: &Path,
    config: &AppConfig,
    instance: &jackin_config::ResolvedInstance,
    slot: &crate::instance::ProvisionedInstanceAuth,
    model: Option<&str>,
    effort: Option<&str>,
) -> anyhow::Result<Vec<(PathBuf, String)>> {
    configure_codex_with_publish_hook(root, config, instance, slot, model, effort, |_| Ok(()))
}

#[cfg(not(unix))]
fn configure_codex(
    _root: &Path,
    _config: &AppConfig,
    _instance: &jackin_config::ResolvedInstance,
    _slot: &crate::instance::ProvisionedInstanceAuth,
    _model: Option<&str>,
    _effort: Option<&str>,
) -> anyhow::Result<Vec<(PathBuf, String)>> {
    anyhow::bail!(
        "private Codex config publication requires Unix descriptor-relative file operations"
    )
}

#[cfg(unix)]
fn configure_codex_with_publish_hook<F>(
    root: &Path,
    config: &AppConfig,
    instance: &jackin_config::ResolvedInstance,
    slot: &crate::instance::ProvisionedInstanceAuth,
    model: Option<&str>,
    effort: Option<&str>,
    mut hook: F,
) -> anyhow::Result<Vec<(PathBuf, String)>>
where
    F: FnMut(private_config_fs::PublishPoint) -> anyhow::Result<()>,
{
    let account = config
        .accounts
        .get(&instance.account_id)
        .ok_or_else(|| anyhow::anyhow!("unknown account {:?}", instance.account_id))?;
    let AccountCredential::ApiKey { .. } = &account.credential else {
        return Ok(Vec::new());
    };
    let base_url = instance.base_url.as_deref();
    let cross_provider = account.provider != AiProvider::OpenAi;
    anyhow::ensure!(
        !cross_provider || model.is_some(),
        "a model is required for a Codex provider account"
    );
    let default_url = match account.provider {
        AiProvider::Moonshot => "https://api.kimi.com/coding/v1",
        AiProvider::Zai => "https://api.z.ai/api/v1",
        AiProvider::Minimax => "https://api.minimax.io/v1",
        AiProvider::OpenAi => "https://api.openai.com/v1",
        _ => anyhow::bail!("selected provider cannot authenticate Codex"),
    };
    let key = account
        .resolved_credential_descriptor(Agent::Codex)?
        .env_name;
    // `container_home_rel` is computed by the auth provisioner from the same
    // slot layout used by mounts and the Capsule's CODEX_HOME value. Never
    // collapse multiple admitted Codex instances onto the primary home.
    let target_home = format!("/home/agent/{}", slot.container_home_rel);
    anyhow::ensure!(
        slot.folder_target == target_home,
        "Codex config slot folder target disagrees with its mounted home"
    );
    let authority = root
        .join("provider-config/home")
        .join(&slot.container_home_rel);
    let mut mounts = Vec::new();
    let directory = private_config_fs::open_directory(root, Path::new(&slot.container_home_rel))?;
    let _lock = private_config_fs::lock(&directory)?;
    let mut document: toml::Table =
        match private_config_fs::read_optional(&directory, "config.toml")
            .context("read private Codex configuration")?
        {
            Some(bytes) => {
                if let Ok(contents) = String::from_utf8(bytes) {
                    if let Ok(document) = toml::from_str(&contents) {
                        document
                    } else {
                        private_config_fs::quarantine(&directory, "config.toml", "invalid TOML")?;
                        toml::Table::new()
                    }
                } else {
                    private_config_fs::quarantine(&directory, "config.toml", "non-UTF8 bytes")?;
                    toml::Table::new()
                }
            }
            None => toml::Table::new(),
        };
    let mut provider = toml::Table::new();
    provider.insert("name".into(), account.provider.slug().into());
    provider.insert("base_url".into(), base_url.unwrap_or(default_url).into());
    provider.insert("env_key".into(), key.into());
    provider.insert("wire_api".into(), "responses".into());
    provider.insert("requires_openai_auth".into(), false.into());
    // Same bricking shape as a parse failure: an existing file whose
    // `model_providers` key is not a table. Only reachable with prior bytes
    // (a fresh table has no such key), so quarantine and regenerate.
    if document
        .get("model_providers")
        .is_some_and(|value| !value.is_table())
    {
        private_config_fs::quarantine(&directory, "config.toml", "model_providers is not a table")?;
        document = toml::Table::new();
    }
    let providers = document
        .entry("model_providers")
        .or_insert_with(|| toml::Value::Table(toml::Table::new()));
    let providers = providers
        .as_table_mut()
        .context("Codex model_providers must be a table")?;
    providers.insert("jackin_account".into(), provider.into());
    document.insert("model_provider".into(), "jackin_account".into());
    let mut catalog_to_publish = None;
    if let Some(model) = model {
        document.insert("model".into(), model.into());
        if let Some(catalog) = model_catalog(account.provider, model) {
            if let Some(effort) = effort {
                anyhow::ensure!(
                    catalog_supports_effort(&catalog, effort),
                    "Codex model {model:?} does not support reasoning effort {effort:?}"
                );
            }
            let catalog_contents = serde_json::to_vec_pretty(&catalog)?;
            let catalog_name = codex_catalog_filename(&catalog_contents);
            let catalog_target = Path::new(&slot.folder_target)
                .join(&catalog_name)
                .to_string_lossy()
                .into_owned();
            document.insert("model_catalog_json".into(), catalog_target.into());
            catalog_to_publish = Some((catalog_name, catalog_contents));
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
    let config_contents = toml::to_string_pretty(&document)?.into_bytes();
    // Apply the same bound as the reader before publishing even an immutable
    // catalog: rejection must leave the complete previous pair untouched.
    private_config_fs::validate_config_contents(&config_contents, "config.toml")?;
    if let Some((catalog_name, catalog_contents)) = catalog_to_publish {
        // The catalog name is content-addressed and immutable. Sync it before
        // atomically changing config.toml, which is the pair's commit point.
        private_config_fs::publish_catalog(&directory, &catalog_name, &catalog_contents, &mut hook)
            .context("publish private Codex model metadata")?;
        mounts.push((
            authority.join(&catalog_name),
            format!("{target_home}/{catalog_name}"),
        ));
    }
    // Docker resolves bind sources after this function returns. Bind the
    // immutable content-addressed config, so another host launch cannot
    // substitute a later config/catalog generation during container creation.
    let generation_name = provider_config_filename(&config_contents, "toml");
    private_config_fs::publish_immutable(
        &directory,
        &generation_name,
        &config_contents,
        private_config_fs::Artifact::CodexGenerationConfig,
        &mut hook,
    )?;
    mounts.push((
        authority.join(&generation_name),
        format!("{target_home}/config.toml"),
    ));
    private_config_fs::publish_atomic(
        &directory,
        "config.toml",
        &config_contents,
        private_config_fs::Artifact::CodexConfig,
        &mut hook,
    )
    .context("publish private Codex account configuration")?;
    Ok(mounts)
}

fn provider_config_filename(contents: &[u8], extension: &str) -> String {
    use sha2::{Digest as _, Sha256};
    format!(
        "account-config-{}.{extension}",
        hex::encode(Sha256::digest(contents))
    )
}

fn codex_catalog_filename(contents: &[u8]) -> String {
    use sha2::{Digest as _, Sha256};

    format!(
        "account-models-{}.json",
        hex::encode(Sha256::digest(contents))
    )
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
#[cfg(unix)]
fn configure_opencode(
    root: &Path,
    config: &AppConfig,
    instance: &jackin_config::ResolvedInstance,
    slot: &crate::instance::ProvisionedInstanceAuth,
    model: Option<&str>,
) -> anyhow::Result<Vec<(PathBuf, String)>> {
    let account = config
        .accounts
        .get(&instance.account_id)
        .ok_or_else(|| anyhow::anyhow!("unknown account {:?}", instance.account_id))?;
    let AccountCredential::ApiKey { .. } = &account.credential else {
        return Ok(Vec::new());
    };
    let base_url = instance.base_url.as_deref();
    let (id, npm, default_url) = opencode_provider(account.provider)?;
    let credential_account = account_with_effective_model(account, model);
    credential_account.credential_env(Agent::Opencode)?;
    let key = account
        .resolved_credential_descriptor(Agent::Opencode)?
        .env_name;
    let home_rel = crate::instance::slot_home_rel(".config/opencode", slot.slot_suffix.as_deref());
    let directory = private_config_fs::open_directory(root, Path::new(&home_rel))?;
    let _lock = private_config_fs::lock(&directory)?;
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
    let contents = serde_json::to_string_pretty(&document)?;
    private_config_fs::validate_config_contents(contents.as_bytes(), "opencode.json")?;
    let generation_name = provider_config_filename(contents.as_bytes(), "json");
    private_config_fs::publish_immutable(
        &directory,
        &generation_name,
        contents.as_bytes(),
        private_config_fs::Artifact::OpenCodeGenerationConfig,
        |_| Ok(()),
    )?;
    private_config_fs::publish_atomic(
        &directory,
        "opencode.json",
        contents.as_bytes(),
        private_config_fs::Artifact::OpenCodeConfig,
        |_| Ok(()),
    )
    .context("write private OpenCode account configuration")?;
    Ok(vec![(
        root.join("provider-config/home")
            .join(home_rel)
            .join(generation_name),
        format!(
            "/home/agent/{}/opencode.json",
            crate::instance::slot_home_rel(".config/opencode", slot.slot_suffix.as_deref())
        ),
    )])
}

#[cfg(not(unix))]
fn configure_opencode(
    _root: &Path,
    _config: &AppConfig,
    _instance: &jackin_config::ResolvedInstance,
    _slot: &crate::instance::ProvisionedInstanceAuth,
    _model: Option<&str>,
) -> anyhow::Result<Vec<(PathBuf, String)>> {
    anyhow::bail!(
        "private OpenCode config publication requires Unix descriptor-relative file operations"
    )
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

#[cfg(all(test, unix))]
mod tests;

#[cfg(all(test, unix))]
#[path = "account_config/authority_tests.rs"]
mod authority_tests;
