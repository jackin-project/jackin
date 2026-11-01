// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::io::ErrorKind;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use jackin_core::{
    Account, AccountName, Config, ConfigEdit, ConfigError, ConfigStore, CredentialSource, DirEntry,
    EnvName, FileKind, FileLock, FileMode, FileSystem, FsError, GatewayEntry, GatewayName,
    HostPath, Platform, Provider, RoleName, RoleSource, StdFileSystem,
};
use tokio::sync::oneshot;

type Fallible<T> = Result<T, Box<dyn std::error::Error>>;
type TestResult = Fallible<()>;

const SETTLE_ROUNDS: usize = 100_000;

fn hp(path: &Path) -> Fallible<HostPath> {
    Ok(HostPath::parse(path)?)
}

fn gateway(name: &str) -> Fallible<ConfigEdit> {
    Ok(ConfigEdit::AddGateway {
        name: GatewayName::parse(name)?,
        entry: GatewayEntry {
            openshell_workspace: "default".parse()?,
            builder: "default".parse()?,
            platform: Platform::LinuxArm64,
            docker_engine: None,
        },
    })
}

fn account(name: &str) -> Fallible<ConfigEdit> {
    Ok(ConfigEdit::AddAccount(Account::new(
        AccountName::parse(name)?,
        Provider::Codex,
        CredentialSource::Env {
            var: EnvName::parse("K")?,
        },
    )?))
}

fn role(name: &str) -> Fallible<ConfigEdit> {
    Ok(ConfigEdit::AddRole {
        name: RoleName::parse(name)?,
        source: RoleSource::Path(hp(Path::new("/r"))?),
    })
}

fn default_gateway(name: &str) -> Fallible<ConfigEdit> {
    Ok(ConfigEdit::SetDefaultGateway(Some(GatewayName::parse(
        name,
    )?)))
}

fn has_role(config: &Config, name: &str) -> Fallible<bool> {
    Ok(config.role(&RoleName::parse(name)?).is_some())
}

fn plain(path: &HostPath) -> ConfigStore {
    ConfigStore::new(Arc::new(StdFileSystem::default()), path.clone())
}

fn label(error: &ConfigError) -> String {
    match error {
        ConfigError::Syntax { line, .. } => format!("syntax@{line}"),
        ConfigError::UnknownField { key } => format!("unknown:{key}"),
        ConfigError::MissingField { key } => format!("missing:{key}"),
        ConfigError::InvalidValue { key, .. } => format!("invalid:{key}"),
        ConfigError::DuplicateName { kind, name } => format!("duplicate:{kind}:{name}"),
        ConfigError::MissingReference { kind, name } => format!("reference:{kind}:{name}"),
        ConfigError::ProviderMismatch { .. } => "mismatch".to_owned(),
        ConfigError::CredentialNotAllowed { .. } => "credential".to_owned(),
        ConfigError::AccountInUse { .. } => "account-in-use".to_owned(),
        ConfigError::RoleInUse { .. } => "role-in-use".to_owned(),
        ConfigError::NoGateway => "no-gateway".to_owned(),
        ConfigError::AmbiguousGateway { .. } => "ambiguous".to_owned(),
        ConfigError::UnknownGateway(_) => "unknown-gateway".to_owned(),
        ConfigError::NoWorkspaceForPath(_) => "no-workspace".to_owned(),
        ConfigError::Fs(_) => "fs".to_owned(),
    }
}

async fn settle<T>(handle: tokio::task::JoinHandle<T>) -> Fallible<T> {
    for _ in 0..SETTLE_ROUNDS {
        if handle.is_finished() {
            return Ok(handle.await?);
        }
        tokio::task::spawn_blocking(|| ()).await?;
        tokio::task::yield_now().await;
    }
    handle.abort();
    Err("the task did not finish".into())
}

async fn edit_settled(
    store: &ConfigStore,
    edits: Vec<ConfigEdit>,
) -> Fallible<Result<Config, ConfigError>> {
    let store = store.clone();
    settle(tokio::spawn(async move { store.edit(edits).await })).await
}

#[tokio::test]
async fn store_creates_the_file_applies_edits_in_order_and_reads_them_back() -> TestResult {
    let dir = tempfile::tempdir()?;
    let path = hp(&dir.path().join("jackin").join("config.toml"))?;
    let store = plain(&path);
    assert_eq!(store.load().await?, Config::empty());
    let saved = store
        .edit(vec![gateway("g1")?, default_gateway("g1")?, account("a")?])
        .await?;
    assert_eq!(std::fs::read(path.as_path())?, saved.render().as_bytes());
    assert_eq!(store.load().await?, saved);
    let before = std::fs::read(path.as_path())?;
    let error = store
        .edit(vec![default_gateway("g2")?, gateway("g2")?])
        .await
        .err()
        .ok_or("reverse order")?;
    assert_eq!(label(&error), "reference:gateway:g2");
    assert_eq!(std::fs::read(path.as_path())?, before);
    let config = store
        .edit(vec![gateway("g2")?, default_gateway("g2")?])
        .await?;
    assert_eq!(
        config.default_gateway().map(ToString::to_string),
        Some("g2".to_owned())
    );
    assert_eq!(
        ConfigStore::default_path(&hp(Path::new("/home/u"))?).as_path(),
        Path::new("/home/u/.config/jackin/config.toml")
    );
    Ok(())
}

#[tokio::test]
async fn a_failing_edit_or_a_corrupt_file_leaves_the_file_untouched_and_the_lock_free() -> TestResult
{
    let dir = tempfile::tempdir()?;
    let path = hp(&dir.path().join("config.toml"))?;
    let store = plain(&path);
    store.edit(vec![gateway("g1")?]).await?;
    let before = std::fs::read(path.as_path())?;
    let error = edit_settled(&store, vec![role("ok")?, default_gateway("ghost")?])
        .await?
        .err()
        .ok_or("failing edit")?;
    assert_eq!(label(&error), "reference:gateway:ghost");
    assert_eq!(std::fs::read(path.as_path())?, before);
    let next = edit_settled(&store, vec![role("ok")?]).await??;
    assert!(has_role(&next, "ok")?);

    let fresh = hp(&dir.path().join("fresh").join("config.toml"))?;
    let error = plain(&fresh)
        .edit(vec![default_gateway("ghost")?])
        .await
        .err()
        .ok_or("fresh")?;
    assert_eq!(label(&error), "reference:gateway:ghost");
    assert!(!fresh.as_path().exists());

    for (bytes, expected) in [
        ("bogus = 1\n", "unknown:bogus"),
        ("[gateways.g\n", "syntax@1"),
    ] {
        std::fs::write(path.as_path(), bytes)?;
        let loaded = store.load().await.err().ok_or("load")?;
        let edited = edit_settled(&store, vec![role("x")?])
            .await?
            .err()
            .ok_or("edit")?;
        assert_eq!(label(&loaded), expected);
        assert_eq!(label(&edited), expected);
        assert_eq!(std::fs::read(path.as_path())?, bytes.as_bytes());
    }
    Ok(())
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum Fault {
    WriteNew,
    Rename,
    SyncDir,
}

#[derive(Debug)]
struct CrashFs {
    inner: StdFileSystem,
    fault: Fault,
    fired: AtomicBool,
}

impl CrashFs {
    fn gate(&self, at: Option<Fault>, path: &HostPath) -> Result<(), FsError> {
        let trips = at.is_some_and(|at| at == self.fault);
        if trips {
            self.fired.store(true, Ordering::SeqCst);
        }
        if trips || self.fired.load(Ordering::SeqCst) {
            return Err(FsError::Io {
                path: path.clone(),
                kind: ErrorKind::StorageFull,
                message: "injected".to_owned(),
            });
        }
        Ok(())
    }
}

#[async_trait]
impl FileSystem for CrashFs {
    async fn read(&self, p: &HostPath) -> Result<Option<Vec<u8>>, FsError> {
        self.gate(None, p)?;
        self.inner.read(p).await
    }
    async fn stat(&self, p: &HostPath) -> Result<Option<FileKind>, FsError> {
        self.gate(None, p)?;
        self.inner.stat(p).await
    }
    async fn list(&self, d: &HostPath) -> Result<Vec<DirEntry>, FsError> {
        self.gate(None, d)?;
        self.inner.list(d).await
    }
    async fn create_dir_all(&self, d: &HostPath) -> Result<(), FsError> {
        self.gate(None, d)?;
        self.inner.create_dir_all(d).await
    }
    async fn remove_file(&self, p: &HostPath) -> Result<(), FsError> {
        self.gate(None, p)?;
        self.inner.remove_file(p).await
    }
    async fn lock(&self, p: &HostPath) -> Result<FileLock, FsError> {
        self.gate(None, p)?;
        self.inner.lock(p).await
    }
    async fn write_new(&self, p: &HostPath, b: &[u8], m: FileMode) -> Result<(), FsError> {
        self.gate(None, p)?;
        if self.fault == Fault::WriteNew {
            self.inner.write_new(p, &b[..b.len() / 2], m).await?;
        }
        self.gate(Some(Fault::WriteNew), p)?;
        self.inner.write_new(p, b, m).await
    }
    async fn rename(&self, f: &HostPath, t: &HostPath) -> Result<(), FsError> {
        self.gate(Some(Fault::Rename), f)?;
        self.inner.rename(f, t).await
    }
    async fn sync_dir(&self, d: &HostPath) -> Result<(), FsError> {
        self.gate(None, d)?;
        self.inner.sync_dir(d).await?;
        self.gate(Some(Fault::SyncDir), d)
    }
}

async fn crash_case(fault: Fault, existing: bool) -> TestResult {
    let dir = tempfile::tempdir()?;
    let path = hp(&dir.path().join("config.toml"))?;
    let fresh = plain(&path);
    let mut old = Config::empty();
    if existing {
        old = fresh.edit(vec![gateway("g1")?]).await?;
    }
    let before = std::fs::read(path.as_path()).ok();
    let mut with_new = old.clone();
    with_new.apply(account("new")?)?;
    let crashing = Arc::new(CrashFs {
        inner: StdFileSystem::default(),
        fault,
        fired: AtomicBool::new(false),
    });
    let error = ConfigStore::new(crashing, path.clone())
        .edit(vec![account("new")?])
        .await
        .err()
        .ok_or("crash")?;
    assert!(
        matches!(
            error,
            ConfigError::Fs(FsError::Io {
                kind: ErrorKind::StorageFull,
                ..
            })
        ),
        "{fault:?} {existing}: {error:?}"
    );
    let loaded = fresh.load().await?;
    if fault == Fault::SyncDir {
        assert!(loaded == old || loaded == with_new, "{fault:?} {existing}");
    } else {
        assert_eq!(loaded, old, "{fault:?} {existing}");
        assert_eq!(
            std::fs::read(path.as_path()).ok(),
            before,
            "{fault:?} {existing}"
        );
    }
    let recovered = edit_settled(&fresh, vec![role("r")?]).await??;
    assert!(has_role(&recovered, "r")?, "{fault:?} {existing}");
    Ok(())
}

#[tokio::test]
async fn a_crash_during_the_write_leaves_the_old_file_and_the_next_edit_succeeds() -> TestResult {
    for fault in [Fault::WriteNew, Fault::Rename, Fault::SyncDir] {
        for existing in [true, false] {
            crash_case(fault, existing).await?;
        }
    }
    Ok(())
}

#[derive(Debug)]
struct GateFs {
    inner: StdFileSystem,
    target: HostPath,
    gate: Mutex<Option<(oneshot::Sender<()>, oneshot::Receiver<()>)>>,
}

#[async_trait]
impl FileSystem for GateFs {
    async fn read(&self, p: &HostPath) -> Result<Option<Vec<u8>>, FsError> {
        let parked = if *p == self.target {
            self.gate.lock().map_err(|_poisoned| gate_error(p))?.take()
        } else {
            None
        };
        let bytes = self.inner.read(p).await?;
        if let Some((entered, release)) = parked {
            entered.send(()).map_err(|()| gate_error(p))?;
            release.await.map_err(|_closed| gate_error(p))?;
        }
        Ok(bytes)
    }
    async fn stat(&self, p: &HostPath) -> Result<Option<FileKind>, FsError> {
        self.inner.stat(p).await
    }
    async fn list(&self, d: &HostPath) -> Result<Vec<DirEntry>, FsError> {
        self.inner.list(d).await
    }
    async fn create_dir_all(&self, d: &HostPath) -> Result<(), FsError> {
        self.inner.create_dir_all(d).await
    }
    async fn remove_file(&self, p: &HostPath) -> Result<(), FsError> {
        self.inner.remove_file(p).await
    }
    async fn lock(&self, p: &HostPath) -> Result<FileLock, FsError> {
        self.inner.lock(p).await
    }
    async fn write_new(&self, p: &HostPath, b: &[u8], m: FileMode) -> Result<(), FsError> {
        self.inner.write_new(p, b, m).await
    }
    async fn rename(&self, f: &HostPath, t: &HostPath) -> Result<(), FsError> {
        self.inner.rename(f, t).await
    }
    async fn sync_dir(&self, d: &HostPath) -> Result<(), FsError> {
        self.inner.sync_dir(d).await
    }
}

fn gate_error(path: &HostPath) -> FsError {
    FsError::Io {
        path: path.clone(),
        kind: ErrorKind::Other,
        message: "gate".to_owned(),
    }
}

#[tokio::test]
async fn concurrent_edits_serialise_on_the_lock_and_lose_no_update() -> TestResult {
    let dir = tempfile::tempdir()?;
    let path = hp(&dir.path().join("config.toml"))?;
    plain(&path).edit(vec![gateway("g1")?]).await?;
    let old_bytes = std::fs::read(path.as_path())?;
    let (entered_tx, entered_rx) = oneshot::channel();
    let (release_tx, release_rx) = oneshot::channel();
    let fs: Arc<dyn FileSystem> = Arc::new(GateFs {
        inner: StdFileSystem::default(),
        target: path.clone(),
        gate: Mutex::new(Some((entered_tx, release_rx))),
    });
    let first = ConfigStore::new(Arc::clone(&fs), path.clone());
    let second = ConfigStore::new(fs, path.clone());
    let edit_a = vec![role("a")?];
    let task_a = tokio::spawn(async move { first.edit(edit_a).await });
    entered_rx.await?;
    let edit_b = vec![role("b")?];
    let task_b = tokio::spawn(async move { second.edit(edit_b).await });
    for _ in 0..50 {
        tokio::task::yield_now().await;
    }
    assert!(!task_b.is_finished());
    assert_eq!(std::fs::read(path.as_path())?, old_bytes);
    release_tx.send(()).map_err(|()| "release")?;
    settle(task_a).await??;
    settle(task_b).await??;
    let merged = plain(&path).load().await?;
    assert!(has_role(&merged, "a")? && has_role(&merged, "b")?);
    Ok(())
}
