// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin_core::Sha256;
use std::error::Error;
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

use async_trait::async_trait;
use jackin_core::{
    CommitId, DirEntry, DockerSubcommand, FileIdentity, FileKind, FileLock, FileMode, FileSystem,
    FsError, HostPath, LineSink, LinkProblem, OutputStream, ProcessError, ProcessOutput,
    ProcessRunner, ProcessSpec, ProcessStatus, Program, RegularFile, RegularFileProblem, RepoPath,
    RoleName, SecretOutput, StdFileSystem,
};
use jackin_image::{
    BuildStage, BuildxEngine, BuiltImage, ImageEngine, ImageError, ImageEvent, ImagePlan,
    ImageRequest, ImageSource, Rebuild, RoleRevision, ensure_held, ensure_image,
};
use jackin_testkit::ScriptedProcessRunner;
use tokio::sync::{mpsc, oneshot, watch};
use tokio::time::{Instant, timeout};
use tokio_util::sync::CancellationToken;

use crate::support::{
    DockerModel, Fixture, Outcome, Script, TestResult, guarded, tag_of, until_role_line,
};

#[derive(Debug)]
struct Gate {
    prefix: &'static str,
    inner: Arc<ScriptedProcessRunner>,
    open: watch::Sender<bool>,
    held: Mutex<Option<String>>,
    arrivals: mpsc::UnboundedSender<String>,
}

impl Gate {
    fn holds(&self, spec: &ProcessSpec) -> Option<String> {
        if spec.program != Program::Docker(DockerSubcommand::BuildxBuild) || *self.open.borrow() {
            return None;
        }
        let tag = spec
            .args
            .iter()
            .find_map(|arg| arg.strip_prefix("--tag="))?
            .to_owned();
        if !tag.starts_with(self.prefix) {
            return None;
        }
        let mut held = self.held.lock().unwrap_or_else(PoisonError::into_inner);
        match held.as_deref() {
            None => *held = Some(tag.clone()),
            Some(first) if first != tag => return None,
            Some(_) => {}
        }
        Some(tag)
    }
}

#[async_trait]
impl ProcessRunner for Gate {
    async fn run(
        &self,
        spec: &ProcessSpec,
        lines: Option<&LineSink>,
        cancel: &CancellationToken,
    ) -> Result<ProcessOutput, ProcessError> {
        if let Some(tag) = self.holds(spec) {
            let mut open = self.open.subscribe();
            drop(self.arrivals.send(tag));
            drop(open.wait_for(|open| *open).await);
        }
        self.inner.run(spec, lines, cancel).await
    }

    async fn run_secret(
        &self,
        spec: &ProcessSpec,
        cancel: &CancellationToken,
    ) -> Result<SecretOutput, ProcessError> {
        self.inner.run_secret(spec, cancel).await
    }
}

#[derive(Debug)]
struct SignalFs {
    inner: StdFileSystem,
    contended: mpsc::UnboundedSender<()>,
}

#[async_trait]
impl FileSystem for SignalFs {
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
    async fn write_new(&self, p: &HostPath, b: &[u8], m: FileMode) -> Result<(), FsError> {
        self.inner.write_new(p, b, m).await
    }
    async fn rename(&self, f: &HostPath, t: &HostPath) -> Result<(), FsError> {
        self.inner.rename(f, t).await
    }
    async fn remove_file(&self, p: &HostPath) -> Result<(), FsError> {
        self.inner.remove_file(p).await
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
    async fn lock(&self, p: &HostPath) -> Result<FileLock, FsError> {
        let result = self.inner.lock(p).await;
        if matches!(result, Err(FsError::LockHeld(_))) {
            match self.contended.send(()) {
                Ok(()) | Err(_) => {}
            }
        }
        result
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
        root: &HostPath,
        file: &RepoPath,
        max_bytes: usize,
    ) -> Result<Result<RegularFile, RegularFileProblem>, FsError> {
        self.inner.read_regular(root, file, max_bytes).await
    }
}

struct Rig {
    fixture: Fixture,
    scripted: Arc<ScriptedProcessRunner>,
    model: DockerModel,
    gate: Arc<Gate>,
    arrivals: mpsc::UnboundedReceiver<String>,
    contended: mpsc::UnboundedReceiver<()>,
    contended_tx: mpsc::UnboundedSender<()>,
}

const ROLE_TAGS: &str = "jackin.invalid/role/";
const CONSTRUCT_TAGS: &str = "jackin.invalid/construct:";

impl Rig {
    fn new() -> Result<Rig, Box<dyn Error>> {
        Rig::holding(ROLE_TAGS)
    }

    fn holding(prefix: &'static str) -> Result<Rig, Box<dyn Error>> {
        let fixture = Fixture::new()?;
        let scripted = Arc::new(ScriptedProcessRunner::new());
        let model = DockerModel::new();
        model.install(&scripted);
        model.fresh_ids();
        let (arrivals_tx, arrivals) = mpsc::unbounded_channel();
        let (contended_tx, contended) = mpsc::unbounded_channel();
        let gate = Arc::new(Gate {
            prefix,
            inner: Arc::clone(&scripted),
            open: watch::channel(true).0,
            held: Mutex::new(None),
            arrivals: arrivals_tx,
        });
        Ok(Rig {
            fixture,
            scripted,
            model,
            gate,
            arrivals,
            contended,
            contended_tx,
        })
    }

    fn close(&self) {
        self.gate.open.send_replace(false);
    }

    fn release(&self) {
        self.gate.open.send_replace(true);
    }

    fn engine(&self) -> BuildxEngine {
        let runner: Arc<dyn ProcessRunner> = Arc::<Gate>::clone(&self.gate);
        BuildxEngine::new(
            runner,
            Arc::new(SignalFs {
                inner: StdFileSystem::default(),
                contended: self.contended_tx.clone(),
            }),
            self.fixture.work.clone(),
        )
    }

    fn builds(&self) -> usize {
        self.builds_of("")
    }

    fn builds_of(&self, prefix: &str) -> usize {
        self.scripted
            .seen()
            .iter()
            .filter(|call| call.program == Program::Docker(DockerSubcommand::BuildxBuild))
            .filter(|call| tag_of(call).is_some_and(|tag| tag.starts_with(prefix)))
            .count()
    }
}

async fn call(
    engine: &BuildxEngine,
    request: &ImageRequest,
    cancel: &CancellationToken,
) -> Result<BuiltImage, ImageError> {
    let (tx, mut rx) = mpsc::channel::<ImageEvent>(256);
    let result = ensure_image(engine, request, &tx, cancel).await;
    rx.close();
    result
}

#[tokio::test]
async fn two_calls_for_one_final_key_build_once_and_the_second_is_served_from_cache() -> TestResult
{
    guarded(async {
        let mut rig = Rig::new()?;
        rig.close();
        let request = rig.fixture.request()?;
        let (first, second) = (rig.engine(), rig.engine());
        let (start_second, start) = oneshot::channel::<()>();
        let cancel = CancellationToken::new();
        let a = call(&first, &request, &cancel);
        let b = async {
            start.await?;
            Ok::<_, Box<dyn Error>>(call(&second, &request, &cancel).await)
        };
        let driver = async {
            rig.arrivals
                .recv()
                .await
                .ok_or("the first call never built")?;
            start_second
                .send(())
                .map_err(|()| "the second call is gone")?;
            let verdict = tokio::select! {
                held = rig.contended.recv() => held.map(|()| None).ok_or("the signal closed"),
                again = rig.arrivals.recv() => Ok(again),
            };
            rig.release();
            Ok::<_, Box<dyn Error>>(verdict?)
        };
        let (a, b, driver) = tokio::join!(a, b, driver);
        if let Some(tag) = driver? {
            return Err(format!("the second call also reached the role build {tag}").into());
        }
        let (a, b) = (a?, b??);
        assert_eq!(a.source, ImageSource::Built);
        assert_eq!(b.source, ImageSource::Cache);
        assert_eq!((&a.tag, &a.id), (&b.tag, &b.id));
        assert_eq!(rig.model.images().len(), 3, "{:?}", rig.model.images());
        assert_eq!(rig.builds(), 3, "builds the runner saw");
        assert!(
            rig.fixture.work_entries().is_empty(),
            "per-call directories"
        );
        Ok(())
    })
    .await
}

#[tokio::test]
async fn a_peer_rebuild_waits_while_the_role_and_derived_builds_read_the_construct() -> TestResult {
    guarded(async {
        let mut rig = Rig::new()?;
        rig.close();
        let request = rig.fixture.request()?;
        let mut rebuild = request.clone();
        rebuild.rebuild = Rebuild::Yes;
        let (first, second) = (rig.engine(), rig.engine());
        let cancel = CancellationToken::new();
        let a = call(&first, &request, &cancel);
        let driver = async {
            rig.arrivals
                .recv()
                .await
                .ok_or("the first call never reached the role build")?;
            let mut b = Box::pin(call(&second, &rebuild, &cancel));
            let waited = tokio::select! {
                finished = b.as_mut() => Err(finished),
                held = rig.contended.recv() => Ok(held),
            };
            let constructs = rig.builds_of(CONSTRUCT_TAGS);
            rig.release();
            match waited {
                Err(finished) => Err(format!(
                    "the rebuild did not wait for the role build: {:?} after {constructs} construct builds",
                    finished.map(|built| built.source)
                )
                .into()),
                Ok(held) => {
                    held.ok_or("the signal closed")?;
                    Ok::<_, Box<dyn Error>>((constructs, b.await))
                }
            }
        };
        let (a, driver) = tokio::join!(a, driver);
        let (constructs, b) = driver?;
        assert_eq!(constructs, 1, "construct builds while the role build ran");
        assert_eq!(a?.source, ImageSource::Built);
        assert_eq!(b?.source, ImageSource::Built);
        Ok(())
    })
    .await
}

#[tokio::test]
async fn a_rebuild_of_the_same_key_waits_until_the_caller_drops_the_returned_hold() -> TestResult {
    guarded(async {
        let mut rig = Rig::new()?;
        rig.model.stable_ids();
        let request = rig.fixture.request()?;
        let mut rebuild = request.clone();
        rebuild.rebuild = Rebuild::Yes;
        let (first, second) = (rig.engine(), rig.engine());
        let cancel = CancellationToken::new();
        let (tx, mut rx) = mpsc::channel::<ImageEvent>(256);
        let held = ensure_held(&first, &request, &tx, &cancel).await?;
        rx.close();
        assert_eq!(held.image.source, ImageSource::Built);
        assert_eq!(rig.builds_of(ROLE_TAGS), 1);

        let mut again = Box::pin(call(&second, &rebuild, &cancel));
        let waited = tokio::select! {
            result = again.as_mut() => Err(result),
            contended = rig.contended.recv() => Ok(contended),
        };
        let during = rig.builds_of(ROLE_TAGS);
        drop(held);
        match waited {
            Err(result) => {
                return Err(format!(
                    "the rebuild did not wait for the hold: {:?} after {during} role builds",
                    result.map(|built| built.source)
                )
                .into());
            }
            Ok(contended) => contended.ok_or("the signal closed")?,
        }
        assert_eq!(during, 1, "role builds while the first caller held the key");
        let rebuilt = again.await?;
        assert_eq!(rebuilt.source, ImageSource::Built);
        assert_eq!(rig.builds_of(ROLE_TAGS), 2);
        Ok(())
    })
    .await
}

#[tokio::test]
async fn a_cache_hit_keeps_the_final_tag_held_until_the_caller_drops_the_returned_hold()
-> TestResult {
    guarded(async {
        let mut rig = Rig::new()?;
        rig.model.stable_ids();
        let request = rig.fixture.request()?;
        let mut rebuild = request.clone();
        rebuild.rebuild = Rebuild::Yes;
        let (first, second) = (rig.engine(), rig.engine());
        let cancel = CancellationToken::new();
        let built = call(&first, &request, &cancel).await?;
        assert_eq!(built.source, ImageSource::Built);
        assert_eq!(rig.builds_of(ROLE_TAGS), 1);

        let (tx, mut rx) = mpsc::channel::<ImageEvent>(256);
        let held = ensure_held(&first, &request, &tx, &cancel).await?;
        rx.close();
        assert_eq!(held.image.source, ImageSource::Cache);
        assert_eq!(rig.builds_of(ROLE_TAGS), 1);

        let mut again = Box::pin(call(&second, &rebuild, &cancel));
        let waited = tokio::select! {
            result = again.as_mut() => Err(result),
            contended = rig.contended.recv() => Ok(contended),
        };
        let during = rig.builds_of(ROLE_TAGS);
        drop(held);
        match waited {
            Err(result) => {
                return Err(format!(
                    "the rebuild did not wait for the cache hit's hold: {:?} after {during} role builds",
                    result.map(|built| built.source)
                )
                .into());
            }
            Ok(contended) => contended.ok_or("the signal closed")?,
        }
        assert_eq!(during, 1, "role builds while the cache hit was held");
        assert_eq!(again.await?.source, ImageSource::Built);
        assert_eq!(rig.builds_of(ROLE_TAGS), 2);
        Ok(())
    })
    .await
}

#[tokio::test]
async fn different_final_keys_share_only_the_construct_lock_and_both_build() -> TestResult {
    guarded(async {
        let mut rig = Rig::new()?;
        rig.close();
        let first_request = rig.fixture.request()?;
        let mut second_request = first_request.clone();
        second_request.revision = RoleRevision {
            commit: CommitId::parse(&"2".repeat(40))?,
            digest: Sha256::of(b"clean"),
        };
        let (first, second) = (rig.engine(), rig.engine());
        let cancel = CancellationToken::new();
        let a = call(&first, &first_request, &cancel);
        let driver = async {
            rig.arrivals
                .recv()
                .await
                .ok_or("the first call never built")?;
            let mut b = Box::pin(call(&second, &second_request, &cancel));
            let waited = tokio::select! {
                result = b.as_mut() => Err(result),
                held = rig.contended.recv() => Ok(held),
            };
            rig.release();
            match waited {
                Err(result) => Err(format!(
                    "the second call did not wait for the construct: {:?}",
                    result.map(|built| built.source)
                )
                .into()),
                Ok(held) => {
                    held.ok_or("the signal closed")?;
                    Ok::<_, Box<dyn Error>>(b.await)
                }
            }
        };
        let (a, b) = tokio::join!(a, driver);
        let (a, b) = (a?, b??);
        assert_eq!(a.source, ImageSource::Built);
        assert_eq!(b.source, ImageSource::Built);
        assert_ne!(a.tag, b.tag);
        Ok(())
    })
    .await
}

#[tokio::test]
async fn a_cached_role_is_served_without_waiting_for_a_peers_role_build() -> TestResult {
    guarded(async {
        let mut rig = Rig::new()?;
        let cached_request = rig.fixture.request()?;
        let mut building_request = cached_request.clone();
        building_request.revision = RoleRevision {
            commit: CommitId::parse(&"2".repeat(40))?,
            digest: Sha256::of(b"clean"),
        };
        let cancel = CancellationToken::new();
        let warm = call(&rig.engine(), &cached_request, &cancel).await?;
        assert_eq!(warm.source, ImageSource::Built);
        rig.close();
        let (first, second) = (rig.engine(), rig.engine());
        let a = call(&first, &building_request, &cancel);
        let driver = async {
            rig.arrivals
                .recv()
                .await
                .ok_or("the first call never reached the role build")?;
            let mut b = Box::pin(call(&second, &cached_request, &cancel));
            let verdict = tokio::select! {
                finished = b.as_mut() => Ok(finished),
                held = rig.contended.recv() => Err(held),
            };
            let role_builds = rig.builds_of(ROLE_TAGS);
            rig.release();
            match verdict {
                Ok(finished) => Ok::<_, Box<dyn Error>>((role_builds, finished)),
                Err(_) => Err(format!(
                    "the cached role waited on a lock while the peer built; {:?}",
                    b.await.map(|built| built.source)
                )
                .into()),
            }
        };
        let (a, driver) = tokio::join!(a, driver);
        let (role_builds, b) = driver?;
        assert_eq!(a?.source, ImageSource::Built);
        let b = b?;
        assert_eq!(b.source, ImageSource::Cache);
        assert_eq!(b.id, warm.id);
        assert_eq!(
            role_builds, 1,
            "role builds begun when the cached call finished"
        );
        Ok(())
    })
    .await
}

async fn following_call_completes(rig: &Rig, case: &str) -> TestResult {
    let request = rig.fixture.request()?;
    let engine = rig.engine();
    let finished = timeout(
        Duration::from_secs(3),
        call(&engine, &request, &CancellationToken::new()),
    )
    .await
    .map_err(|_| format!("key lock not released after {case}"))?;
    finished?;
    Ok(())
}

fn hung_role_build(rig: &Rig) {
    rig.model.script(
        BuildStage::Role,
        Script {
            lines: vec![(OutputStream::Stdout, "building".to_owned())],
            outcome: Outcome::UntilCancelled,
            ..Script::default()
        },
    );
}

async fn failed_case(request: &ImageRequest) -> TestResult {
    let rig = Rig::new()?;
    rig.model.script(
        BuildStage::Role,
        Script {
            outcome: Outcome::Exit(ProcessStatus::Exited(1)),
            ..Script::default()
        },
    );
    let failed = call(&rig.engine(), request, &CancellationToken::new()).await;
    assert!(matches!(failed, Err(ImageError::Engine(_))), "{failed:?}");
    rig.model.script(BuildStage::Role, Script::default());
    following_call_completes(&rig, "a failed build").await
}

async fn interrupted_case(request: &ImageRequest, cancel_it: bool) -> TestResult {
    let rig = Rig::new()?;
    hung_role_build(&rig);
    let engine = rig.engine();
    let (tx, mut rx) = mpsc::channel::<ImageEvent>(256);
    let cancel = CancellationToken::new();
    let mut running = Box::pin(ensure_image(&engine, request, &tx, &cancel));
    until_role_line(&mut running, &mut rx).await?;
    if cancel_it {
        cancel.cancel();
        let cancelled = running.await;
        assert!(
            matches!(cancelled, Err(ImageError::Cancelled { .. })),
            "{cancelled:?}"
        );
    } else {
        drop(running);
    }
    rig.model.script(BuildStage::Role, Script::default());
    let case = if cancel_it {
        "a cancel"
    } else {
        "a dropped call"
    };
    following_call_completes(&rig, case).await
}

async fn waiting_case(request: &ImageRequest) -> TestResult {
    let mut rig = Rig::new()?;
    rig.close();
    let (first, second) = (rig.engine(), rig.engine());
    let waiter = CancellationToken::new();
    let holder = CancellationToken::new();
    let a = call(&first, request, &holder);
    let driver = async {
        rig.arrivals
            .recv()
            .await
            .ok_or("the first call never built")?;
        let mut b = Box::pin(call(&second, request, &waiter));
        tokio::select! {
            finished = b.as_mut() => {
                return Err(format!("the second call did not wait: {finished:?}").into());
            }
            held = rig.contended.recv() => held.ok_or("the signal closed")?,
        }
        waiter.cancel();
        let result = b.await;
        let report = (rig.builds(), rig.model.images().len());
        rig.release();
        Ok::<_, Box<dyn Error>>((result, report))
    };
    let (a, driver) = tokio::join!(a, driver);
    let (result, (builds, images)) = driver?;
    assert!(
        matches!(result, Err(ImageError::Cancelled { .. })),
        "{result:?}"
    );
    assert_eq!((builds, images), (1, 1), "the waiter built or tagged");
    assert_eq!(a?.source, ImageSource::Built);
    Ok(())
}

#[tokio::test]
async fn the_key_lock_is_released_on_failure_cancel_and_drop_and_waiting_honors_cancel()
-> TestResult {
    guarded(async {
        let fixture = Fixture::new()?;
        let request = fixture.request()?;
        failed_case(&request).await?;
        interrupted_case(&request, true).await?;
        interrupted_case(&request, false).await?;
        waiting_case(&request).await
    })
    .await
}

#[tokio::test]
async fn two_roles_that_both_miss_the_construct_build_it_once_and_neither_sees_a_moved_source()
-> TestResult {
    guarded(async {
        let mut rig = Rig::holding(CONSTRUCT_TAGS)?;
        rig.close();
        let first_request = rig.fixture.request()?;
        let mut second_request = first_request.clone();
        second_request.role = RoleName::parse("other")?;
        let (first, second) = (rig.engine(), rig.engine());
        let (start_second, start) = oneshot::channel::<()>();
        let cancel = CancellationToken::new();
        let a = call(&first, &first_request, &cancel);
        let b = async {
            start.await?;
            Ok::<_, Box<dyn Error>>(call(&second, &second_request, &cancel).await)
        };
        let driver = async {
            rig.arrivals
                .recv()
                .await
                .ok_or("the first call never built the construct")?;
            start_second
                .send(())
                .map_err(|()| "the second call is gone")?;
            let verdict = tokio::select! {
                held = rig.contended.recv() => held.map(|()| None).ok_or("the signal closed"),
                again = rig.arrivals.recv() => Ok(again),
            };
            rig.release();
            Ok::<_, Box<dyn Error>>(verdict?)
        };
        let (a, b, driver) = tokio::join!(a, b, driver);
        if let Some(tag) = driver? {
            return Err(format!("the second call also reached the construct build {tag}").into());
        }
        let (a, b) = (a?, b??);
        assert_eq!(a.source, ImageSource::Built);
        assert_eq!(b.source, ImageSource::Built);
        assert_ne!(a.tag, b.tag);
        assert_eq!(rig.builds_of(CONSTRUCT_TAGS), 1, "construct builds");
        assert_eq!(rig.builds(), 5, "builds the runner saw");
        assert_eq!(rig.model.images().len(), 5, "{:?}", rig.model.images());
        Ok(())
    })
    .await
}

#[tokio::test(start_paused = true)]
async fn waiting_behind_a_live_peer_gives_up_after_3600_seconds_with_the_construct_key()
-> TestResult {
    let rig = Rig::new()?;
    let request = rig.fixture.request()?;
    let construct = ImagePlan::new(&request)?.construct_build().tag.clone();
    let peer = rig.engine();
    let held = peer
        .hold(&construct, &CancellationToken::new())
        .await?
        .ok_or("the peer got no lock")?;
    let engine = rig.engine();
    let started = Instant::now();
    let result = timeout(
        Duration::from_secs(3700),
        call(&engine, &request, &CancellationToken::new()),
    )
    .await
    .map_err(|_| "no LockTimeout within 3700 s of paused time")?;
    let waited = started.elapsed();
    match result {
        Err(ImageError::LockTimeout { key }) => assert_eq!(key, construct),
        other => return Err(format!("expected LockTimeout, got {other:?}").into()),
    }
    assert!(
        waited >= Duration::from_secs(3600) && waited < Duration::from_secs(3700),
        "waited {waited:?}"
    );
    assert_eq!(rig.builds(), 0, "builds the runner saw");
    drop(held);
    Ok(())
}
