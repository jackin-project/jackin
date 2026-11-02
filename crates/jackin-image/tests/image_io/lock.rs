// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::error::Error;
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

use async_trait::async_trait;
use jackin_core::{
    CommitId, DirEntry, DockerSubcommand, FileKind, FileLock, FileMode, FileSystem, FsError,
    HostPath, LineSink, OutputStream, ProcessError, ProcessOutput, ProcessRunner, ProcessSpec,
    ProcessStatus, Program, RegularFile, RegularFileProblem, RepoPath, SecretOutput, StdFileSystem,
};
use jackin_image::{
    BuildStage, BuildxEngine, BuiltImage, ImageError, ImageEvent, ImageRequest, ImageSource,
    RoleRevision, ensure_image,
};
use jackin_testkit::ScriptedProcessRunner;
use tokio::sync::{mpsc, oneshot, watch};
use tokio::time::timeout;
use tokio_util::sync::CancellationToken;

use crate::support::{DockerModel, Fixture, Outcome, Script, TestResult, guarded, until_role_line};

#[derive(Debug)]
struct Gate {
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
        if !tag.starts_with("jackin.invalid/role/") {
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
    async fn list(&self, d: &HostPath) -> Result<Vec<DirEntry>, FsError> {
        self.inner.list(d).await
    }
    async fn create_dir_all(&self, d: &HostPath) -> Result<(), FsError> {
        self.inner.create_dir_all(d).await
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
    async fn read_regular(
        &self,
        root: &HostPath,
        file: &RepoPath,
    ) -> Result<Result<RegularFile, RegularFileProblem>, FsError> {
        self.inner.read_regular(root, file).await
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

impl Rig {
    fn new() -> Result<Rig, Box<dyn Error>> {
        let fixture = Fixture::new()?;
        let scripted = Arc::new(ScriptedProcessRunner::new());
        let model = DockerModel::new();
        model.install(&scripted);
        model.fresh_ids();
        let (arrivals_tx, arrivals) = mpsc::unbounded_channel();
        let (contended_tx, contended) = mpsc::unbounded_channel();
        let gate = Arc::new(Gate {
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
        self.scripted
            .seen()
            .iter()
            .filter(|call| call.program == Program::Docker(DockerSubcommand::BuildxBuild))
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
async fn different_final_keys_do_not_wait_for_each_other() -> TestResult {
    guarded(async {
        let mut rig = Rig::new()?;
        rig.close();
        let first_request = rig.fixture.request()?;
        let mut second_request = first_request.clone();
        second_request.revision = RoleRevision::Clean {
            commit: CommitId::parse(&"2".repeat(40))?,
        };
        let (first, second) = (rig.engine(), rig.engine());
        let cancel = CancellationToken::new();
        let a = call(&first, &first_request, &cancel);
        let driver = async {
            rig.arrivals
                .recv()
                .await
                .ok_or("the first call never built")?;
            let b = call(&second, &second_request, &cancel);
            let waited = tokio::select! {
                result = b => Ok(result),
                _held = rig.contended.recv() => Err("the second call waited for the first"),
            };
            rig.release();
            Ok::<_, Box<dyn Error>>(waited?)
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
            matches!(cancelled, Err(ImageError::Cancelled)),
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
    assert!(matches!(result, Err(ImageError::Cancelled)), "{result:?}");
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
