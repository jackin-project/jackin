// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::io::ErrorKind;
use std::path::Path;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

use async_trait::async_trait;
use jackin_core::{
    Account, AccountName, Clock, Config, ConfigEdit, ConfigError, ConfigStore, CredentialSource,
    DirEntry, EnvName, FileIdentity, FileKind, FileLock, FileMode, FileSystem, FsError,
    GatewayEntry, GatewayName, HostPath, LinkProblem, Platform, Provider, RegularFile,
    RegularFileProblem, RepoPath, RoleName, RoleSource, StdFileSystem, Timestamp, Workspace,
    WorkspaceName,
};
use tokio::sync::{Semaphore, oneshot};

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

#[derive(Debug, Default)]
struct StepClock {
    slept: Mutex<Vec<Duration>>,
}

impl StepClock {
    fn sleeps(&self) -> Vec<Duration> {
        self.slept
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }
}

#[async_trait]
impl Clock for StepClock {
    fn now(&self) -> Timestamp {
        Timestamp(0)
    }

    fn utc_offset_secs(&self) -> i32 {
        0
    }

    async fn sleep(&self, duration: Duration) {
        self.slept
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push(duration);
    }
}

fn plain_with(path: &HostPath, clock: &Arc<StepClock>) -> ConfigStore {
    let shared = Arc::clone(clock);
    let clock: Arc<dyn Clock> = shared;
    ConfigStore::new(Arc::new(StdFileSystem::default()), clock, path.clone())
}

fn plain(path: &HostPath) -> ConfigStore {
    plain_with(path, &Arc::new(StepClock::default()))
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
        ConfigError::DuplicatePath { path, workspaces } => format!(
            "duplicate-path:{}:{}",
            path.as_path().display(),
            workspaces
                .iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>()
                .join(",")
        ),
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
    let clock = Arc::new(StepClock::default());
    let store = plain_with(&path, &clock);
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
    assert!(clock.sleeps().is_empty());
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

    let limit = 1 << 20;
    let oversized = format!("#{}", "x".repeat(limit));
    for (bytes, expected) in [
        ("bogus = 1\n".to_owned(), "unknown:bogus"),
        ("[gateways.g\n".to_owned(), "syntax@1"),
        (oversized, "fs"),
    ] {
        std::fs::write(path.as_path(), &bytes)?;
        let loaded = store.load().await.err().ok_or("load")?;
        let edited = edit_settled(&store, vec![role("x")?])
            .await?
            .err()
            .ok_or("edit")?;
        assert_eq!(label(&loaded), expected);
        assert_eq!(label(&edited), expected);
        assert_eq!(std::fs::read(path.as_path())?, bytes.as_bytes());
    }
    std::fs::write(path.as_path(), format!("#{}", "x".repeat(limit - 1)))?;
    assert_eq!(store.load().await?, Config::empty());

    std::fs::remove_file(path.as_path())?;
    assert!(
        std::process::Command::new("mkfifo")
            .arg(path.as_path())
            .status()?
            .success()
    );
    let loaded = settle(tokio::spawn({
        let store = store.clone();
        async move { store.load().await }
    }))
    .await?;
    assert_eq!(label(&loaded.err().ok_or("fifo load")?), "fs");
    let edited = edit_settled(&store, vec![role("x")?]).await?;
    assert_eq!(label(&edited.err().ok_or("fifo edit")?), "fs");
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
    async fn stat_following(&self, p: &HostPath) -> Result<Option<FileKind>, FsError> {
        self.gate(None, p)?;
        self.inner.stat_following(p).await
    }
    async fn canonicalize(&self, p: &HostPath) -> Result<Option<HostPath>, FsError> {
        self.inner.canonicalize(p).await
    }
    async fn file_len(&self, p: &HostPath) -> Result<Option<u64>, FsError> {
        self.inner.file_len(p).await
    }
    async fn identity(&self, p: &HostPath) -> Result<Option<FileIdentity>, FsError> {
        self.inner.identity(p).await
    }
    async fn list(&self, d: &HostPath) -> Result<Vec<DirEntry>, FsError> {
        self.gate(None, d)?;
        self.inner.list(d).await
    }
    async fn create_dir_all(&self, d: &HostPath) -> Result<(), FsError> {
        self.gate(None, d)?;
        self.inner.create_dir_all(d).await
    }
    async fn create_private_dir_all(&self, d: &HostPath) -> Result<(), FsError> {
        self.gate(None, d)?;
        self.inner.create_private_dir_all(d).await
    }
    async fn remove_file(&self, p: &HostPath) -> Result<(), FsError> {
        self.gate(None, p)?;
        self.inner.remove_file(p).await
    }
    async fn read_at(&self, p: &HostPath, offset: u64, len: usize) -> Result<Vec<u8>, FsError> {
        self.inner.read_at(p, offset, len).await
    }
    async fn read_link(
        &self,
        root: &HostPath,
        file: &RepoPath,
    ) -> Result<Result<String, LinkProblem>, FsError> {
        self.inner.read_link(root, file).await
    }
    async fn write_symlink(&self, path: &HostPath, target: &str) -> Result<(), FsError> {
        self.inner.write_symlink(path, target).await
    }
    async fn read_regular(
        &self,
        r: &HostPath,
        f: &RepoPath,
        max_bytes: usize,
    ) -> Result<Result<RegularFile, RegularFileProblem>, FsError> {
        self.inner.read_regular(r, f, max_bytes).await
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
    async fn remove_dir_all(&self, p: &HostPath) -> Result<(), FsError> {
        self.inner.remove_dir_all(p).await
    }
    async fn mode(&self, p: &HostPath) -> Result<Option<FileMode>, FsError> {
        self.inner.mode(p).await
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
    let error = ConfigStore::new(crashing, Arc::new(StepClock::default()), path.clone())
        .edit(vec![account("new")?])
        .await
        .err()
        .ok_or("crash")?;
    let expected = match fault {
        Fault::SyncDir => matches!(error, ConfigError::Fs(FsError::Unsynced { .. })),
        _ => matches!(
            error,
            ConfigError::Fs(FsError::Io {
                kind: ErrorKind::StorageFull,
                ..
            })
        ),
    };
    assert!(expected, "{fault:?} {existing}: {error:?}");
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
    reads: AtomicUsize,
    locks: AtomicUsize,
    gate: Mutex<Option<(oneshot::Sender<()>, oneshot::Receiver<()>)>>,
}

#[async_trait]
impl FileSystem for GateFs {
    async fn read(&self, p: &HostPath) -> Result<Option<Vec<u8>>, FsError> {
        self.inner.read(p).await
    }
    async fn stat(&self, p: &HostPath) -> Result<Option<FileKind>, FsError> {
        self.inner.stat(p).await
    }
    async fn stat_following(&self, p: &HostPath) -> Result<Option<FileKind>, FsError> {
        self.inner.stat_following(p).await
    }
    async fn canonicalize(&self, p: &HostPath) -> Result<Option<HostPath>, FsError> {
        self.inner.canonicalize(p).await
    }
    async fn file_len(&self, p: &HostPath) -> Result<Option<u64>, FsError> {
        self.inner.file_len(p).await
    }
    async fn identity(&self, p: &HostPath) -> Result<Option<FileIdentity>, FsError> {
        self.inner.identity(p).await
    }
    async fn list(&self, d: &HostPath) -> Result<Vec<DirEntry>, FsError> {
        self.inner.list(d).await
    }
    async fn create_dir_all(&self, d: &HostPath) -> Result<(), FsError> {
        self.inner.create_dir_all(d).await
    }
    async fn create_private_dir_all(&self, d: &HostPath) -> Result<(), FsError> {
        self.inner.create_private_dir_all(d).await
    }
    async fn remove_file(&self, p: &HostPath) -> Result<(), FsError> {
        self.inner.remove_file(p).await
    }
    async fn read_at(&self, p: &HostPath, offset: u64, len: usize) -> Result<Vec<u8>, FsError> {
        self.inner.read_at(p, offset, len).await
    }
    async fn read_link(
        &self,
        root: &HostPath,
        file: &RepoPath,
    ) -> Result<Result<String, LinkProblem>, FsError> {
        self.inner.read_link(root, file).await
    }
    async fn write_symlink(&self, path: &HostPath, target: &str) -> Result<(), FsError> {
        self.inner.write_symlink(path, target).await
    }
    async fn read_regular(
        &self,
        r: &HostPath,
        f: &RepoPath,
        max_bytes: usize,
    ) -> Result<Result<RegularFile, RegularFileProblem>, FsError> {
        let target = &self.target;
        let parked = if r.as_path().join(f.as_str()) == target.as_path() {
            self.reads.fetch_add(1, Ordering::SeqCst);
            self.gate
                .lock()
                .map_err(|_poisoned| gate_error(target))?
                .take()
        } else {
            None
        };
        let read = self.inner.read_regular(r, f, max_bytes).await?;
        if let Some((entered, release)) = parked {
            entered.send(()).map_err(|()| gate_error(target))?;
            release.await.map_err(|_closed| gate_error(target))?;
        }
        Ok(read)
    }
    async fn lock(&self, p: &HostPath) -> Result<FileLock, FsError> {
        self.locks.fetch_add(1, Ordering::SeqCst);
        self.inner.lock(p).await
    }
    async fn write_new(&self, p: &HostPath, b: &[u8], m: FileMode) -> Result<(), FsError> {
        self.inner.write_new(p, b, m).await
    }
    async fn rename(&self, f: &HostPath, t: &HostPath) -> Result<(), FsError> {
        self.inner.rename(f, t).await
    }
    async fn remove_dir_all(&self, p: &HostPath) -> Result<(), FsError> {
        self.inner.remove_dir_all(p).await
    }
    async fn mode(&self, p: &HostPath) -> Result<Option<FileMode>, FsError> {
        self.inner.mode(p).await
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

#[derive(Debug)]
struct TickClock {
    slept: Mutex<Vec<Duration>>,
    permits: Semaphore,
}

impl TickClock {
    fn new() -> TickClock {
        TickClock {
            slept: Mutex::new(Vec::new()),
            permits: Semaphore::new(0),
        }
    }

    fn sleep_count(&self) -> usize {
        self.slept
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .len()
    }
}

#[async_trait]
impl Clock for TickClock {
    fn now(&self) -> Timestamp {
        Timestamp(0)
    }

    fn utc_offset_secs(&self) -> i32 {
        0
    }

    async fn sleep(&self, duration: Duration) {
        self.slept
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push(duration);
        if let Ok(permit) = self.permits.acquire().await {
            permit.forget();
        }
    }
}

async fn until(what: &str, mut ready: impl FnMut() -> bool) -> TestResult {
    for _ in 0..SETTLE_ROUNDS {
        if ready() {
            return Ok(());
        }
        tokio::task::spawn_blocking(|| ()).await?;
        tokio::task::yield_now().await;
    }
    Err(format!("timed out waiting for {what}").into())
}

#[tokio::test]
async fn concurrent_edits_serialise_on_the_lock_and_lose_no_update() -> TestResult {
    let dir = tempfile::tempdir()?;
    let path = hp(&dir.path().join("config.toml"))?;
    plain(&path).edit(vec![gateway("g1")?]).await?;
    let old_bytes = std::fs::read(path.as_path())?;
    let (entered_tx, entered_rx) = oneshot::channel();
    let (release_tx, release_rx) = oneshot::channel();
    let gate = Arc::new(GateFs {
        inner: StdFileSystem::default(),
        target: path.clone(),
        reads: AtomicUsize::new(0),
        locks: AtomicUsize::new(0),
        gate: Mutex::new(Some((entered_tx, release_rx))),
    });
    let clock = Arc::new(TickClock::new());
    let (gate_copy, clock_copy) = (Arc::clone(&gate), Arc::clone(&clock));
    let fs: Arc<dyn FileSystem> = gate_copy;
    let tick: Arc<dyn Clock> = clock_copy;
    let first = ConfigStore::new(Arc::clone(&fs), Arc::clone(&tick), path.clone());
    let second = ConfigStore::new(fs, tick, path.clone());
    let edit_a = vec![role("a")?];
    let task_a = tokio::spawn(async move { first.edit(edit_a).await });
    entered_rx.await?;
    let edit_b = vec![role("b")?];
    let task_b = tokio::spawn(async move { second.edit(edit_b).await });
    until("the first failed lock attempt of B", || {
        clock.sleep_count() >= 1 || gate.reads.load(Ordering::SeqCst) >= 2
    })
    .await?;
    for round in 0..4 {
        if round > 0 {
            let seen = clock.sleep_count();
            clock.permits.add_permits(1);
            until("the next lock retry of B", || clock.sleep_count() > seen).await?;
        }
        assert_eq!(gate.reads.load(Ordering::SeqCst), 1, "round {round}");
        assert!(!task_b.is_finished(), "round {round}");
        assert_eq!(std::fs::read(path.as_path())?, old_bytes, "round {round}");
    }
    release_tx.send(()).map_err(|()| "release")?;
    settle(task_a).await??;
    clock.permits.add_permits(1000);
    settle(task_b).await??;
    assert_eq!(gate.reads.load(Ordering::SeqCst), 2);
    let merged = plain(&path).load().await?;
    assert!(has_role(&merged, "a")? && has_role(&merged, "b")?);
    assert!(merged.gateway(&GatewayName::parse("g1")?).is_some());
    Ok(())
}

#[tokio::test]
async fn a_held_lock_is_retried_on_a_fixed_interval_then_edit_gives_up_with_lock_held() -> TestResult
{
    let dir = tempfile::tempdir()?;
    let path = hp(&dir.path().join("config.toml"))?;
    let lock_path = hp(&dir.path().join("config.toml.lock"))?;
    let held = StdFileSystem::default().lock(&lock_path).await?;
    let clock = Arc::new(StepClock::default());
    let error = plain_with(&path, &clock)
        .edit(vec![role("a")?])
        .await
        .err()
        .ok_or("edit under a held lock")?;
    assert!(
        matches!(&error, ConfigError::Fs(FsError::LockHeld(p)) if *p == lock_path),
        "{error:?}"
    );
    assert!(!path.as_path().exists());
    let sleeps = clock.sleeps();
    assert!(sleeps.len() >= 50, "{}", sleeps.len());
    assert!(sleeps.iter().all(|d| *d == sleeps[0]), "{sleeps:?}");
    assert!(
        (Duration::from_millis(10)..=Duration::from_millis(50)).contains(&sleeps[0]),
        "{:?}",
        sleeps[0]
    );
    let total: Duration = sleeps.iter().sum();
    assert!(
        (Duration::from_secs(5)..=Duration::from_secs(20)).contains(&total),
        "{total:?}"
    );
    drop(held);
    let fresh = Arc::new(StepClock::default());
    let config = plain_with(&path, &fresh).edit(vec![role("a")?]).await?;
    assert!(has_role(&config, "a")?);
    assert!(fresh.sleeps().is_empty());
    Ok(())
}

#[derive(Debug)]
struct HoldingClock {
    held: Mutex<Option<FileLock>>,
    slept: Mutex<usize>,
}

#[async_trait]
impl Clock for HoldingClock {
    fn now(&self) -> Timestamp {
        Timestamp(0)
    }

    fn utc_offset_secs(&self) -> i32 {
        0
    }

    async fn sleep(&self, _duration: Duration) {
        let count = {
            let mut slept = self.slept.lock().unwrap_or_else(PoisonError::into_inner);
            *slept += 1;
            *slept
        };
        if count == 3 {
            self.held
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .take();
        }
    }
}

#[tokio::test]
async fn a_lock_released_during_the_wait_lets_the_edit_succeed() -> TestResult {
    let dir = tempfile::tempdir()?;
    let path = hp(&dir.path().join("config.toml"))?;
    let lock_path = hp(&dir.path().join("config.toml.lock"))?;
    let held = StdFileSystem::default().lock(&lock_path).await?;
    let clock = Arc::new(HoldingClock {
        held: Mutex::new(Some(held)),
        slept: Mutex::new(0),
    });
    let counting = Arc::new(GateFs {
        inner: StdFileSystem::default(),
        target: path.clone(),
        reads: AtomicUsize::new(0),
        locks: AtomicUsize::new(0),
        gate: Mutex::new(None),
    });
    let (fs_copy, clock_copy) = (Arc::clone(&counting), Arc::clone(&clock));
    let fs: Arc<dyn FileSystem> = fs_copy;
    let ticking: Arc<dyn Clock> = clock_copy;
    let store = ConfigStore::new(fs, ticking, path.clone());
    let config = store.edit(vec![role("a")?]).await?;
    assert!(has_role(&config, "a")?);
    assert_eq!(
        *clock.slept.lock().unwrap_or_else(PoisonError::into_inner),
        3
    );
    assert_eq!(counting.locks.load(Ordering::SeqCst), 4);
    assert_eq!(std::fs::read(path.as_path())?, config.render().as_bytes());
    StdFileSystem::default().lock(&lock_path).await?;
    Ok(())
}

#[tokio::test]
async fn edit_through_a_symlinked_config_is_refused_and_leaves_the_target_untouched() -> TestResult
{
    let dir = tempfile::tempdir()?;
    let real_dir = dir.path().join("real");
    std::fs::create_dir(&real_dir)?;
    let real = hp(&real_dir.join("config.toml"))?;
    plain(&real).edit(vec![gateway("g1")?]).await?;
    let before = std::fs::read(real.as_path())?;
    let link = dir.path().join("config.toml");
    std::os::unix::fs::symlink(real.as_path(), &link)?;
    let error = plain(&hp(&link)?)
        .edit(vec![role("a")?])
        .await
        .err()
        .ok_or("edit through a link")?;
    assert!(
        matches!(&error, ConfigError::Fs(FsError::SymlinkTarget(_))),
        "{error:?}"
    );
    assert_eq!(std::fs::read(real.as_path())?, before);
    assert_eq!(std::fs::read_link(&link)?, real.as_path());
    assert!(!dir.path().join(".config.toml.tmp").exists());
    assert!(!real_dir.join(".config.toml.tmp").exists());
    let config = plain(&real).edit(vec![role("a")?]).await?;
    assert!(has_role(&config, "a")?);

    let dangling = dir.path().join("dangling.toml");
    let missing = dir.path().join("missing.toml");
    std::os::unix::fs::symlink(&missing, &dangling)?;
    let error = plain(&hp(&dangling)?)
        .edit(vec![role("a")?])
        .await
        .err()
        .ok_or("edit through a dangling link")?;
    assert!(
        matches!(&error, ConfigError::Fs(FsError::SymlinkTarget(_))),
        "{error:?}"
    );
    assert!(!missing.exists());
    assert_eq!(std::fs::read_link(&dangling)?, missing);
    assert!(!dir.path().join(".dangling.toml.tmp").exists());
    Ok(())
}

#[tokio::test]
async fn an_edit_sweeps_the_crash_leftovers_of_its_file_and_keeps_every_other_file() -> TestResult {
    let dir = tempfile::tempdir()?;
    let path = hp(&dir.path().join("config.toml"))?;
    let leftovers = [".config.toml.4242-0.tmp", ".config.toml.7-31.tmp"];
    let others = [
        ".config.toml.tmp",
        ".config.toml.4242-0.tmp.bak",
        ".config.toml.x-0.tmp",
        ".config.toml.4242-.tmp",
        ".config.toml.4242-0.5-6.tmp",
        "config.toml.4242-0.tmp",
        ".other.toml.4242-0.tmp",
        ".config.toml.lock.4242-0.tmp",
    ];
    for name in leftovers.iter().chain(&others) {
        std::fs::write(dir.path().join(name), b"leftover")?;
    }
    std::fs::create_dir(dir.path().join(".config.toml.9-9.tmp"))?;
    plain(&path).edit(vec![role("a")?]).await?;
    let mut expected: Vec<String> = others.iter().map(ToString::to_string).collect();
    expected.extend([".config.toml.9-9.tmp", "config.toml", "config.toml.lock"].map(str::to_owned));
    expected.sort();
    let mut found: Vec<String> = std::fs::read_dir(dir.path())?
        .map(|entry| Ok(entry?.file_name().to_string_lossy().into_owned()))
        .collect::<Result<_, std::io::Error>>()?;
    found.sort();
    assert_eq!(found, expected);
    Ok(())
}

#[tokio::test]
async fn a_rename_keeps_what_another_handle_wrote_after_the_caller_read() -> TestResult {
    let dir = tempfile::tempdir()?;
    let path = hp(&dir.path().join("config.toml"))?;
    let store = plain(&path);
    let other = plain(&path);
    let from = WorkspaceName::parse("proj")?;
    let to = WorkspaceName::parse("app")?;
    store
        .edit(vec![ConfigEdit::AddWorkspace {
            name: from.clone(),
            workspace: Workspace::new(hp(Path::new("/p/proj"))?, false),
        }])
        .await?;
    let stale = store.load().await?;
    other
        .edit(vec![ConfigEdit::SetWorkspaceDocker {
            workspace: from.clone(),
            docker: true,
        }])
        .await?;
    assert!(stale.workspace(&from).is_some_and(|saved| !saved.docker));
    let saved = store
        .edit(vec![ConfigEdit::RenameWorkspace {
            from: from.clone(),
            to: to.clone(),
        }])
        .await?;
    assert!(saved.workspace(&from).is_none());
    assert!(saved.workspace(&to).is_some_and(|moved| moved.docker));
    assert_eq!(store.load().await?, saved);
    Ok(())
}
