// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin_core::{
    CommitId, DirEntry, FileIdentity, FileKind, FileLock, FileMode, FileSystem, FsError, HostPath,
    LinkProblem, ProcessRunner, RegularFile, RegularFileProblem, RepoPath, RoleName, Sha256,
    StdFileSystem,
};
use jackin_image::{
    BuildxEngine, BuiltImage, ImageEngine, ImageError, ImageEvent, ImagePlan, ImageRequest,
    ImageSource, Rebuild, RoleRevision, ensure_held, ensure_image,
};
use jackin_testkit::{DockerBoundary, DockerBuildReply, guard};
use std::error::Error;
use std::future::Future;
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use tokio::sync::mpsc;
use tokio::time::{Instant, timeout};
use tokio_util::sync::CancellationToken;

use crate::support::{Fixture, TestResult};

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
    docker: Arc<DockerBoundary>,
    contended: mpsc::UnboundedReceiver<()>,
    contended_tx: mpsc::UnboundedSender<()>,
}

impl Rig {
    fn new() -> Result<Rig, Box<dyn Error>> {
        let fixture = Fixture::new()?;
        let docker = Arc::new(DockerBoundary::new());
        let (contended_tx, contended) = mpsc::unbounded_channel();
        Ok(Rig {
            fixture,
            docker,
            contended,
            contended_tx,
        })
    }

    fn engine(&self) -> BuildxEngine {
        let runner: Arc<dyn ProcessRunner> = Arc::<DockerBoundary>::clone(&self.docker);
        BuildxEngine::new(
            runner,
            Arc::new(SignalFs {
                inner: StdFileSystem::default(),
                contended: self.contended_tx.clone(),
            }),
            self.fixture.work.clone(),
        )
    }
}

async fn prepared(
    rig: &Rig,
    request: &ImageRequest,
) -> Result<jackin_image::FinalBuilds, Box<dyn Error>> {
    let engine = rig.engine();
    let plan = ImagePlan::new(request)?;
    let cancel = CancellationToken::new();
    let construct = if let Some(id) = engine.find(&plan.construct_build().tag, &cancel).await? {
        id
    } else {
        let (tx, rx) = mpsc::channel(8);
        drop(rx);
        engine.build(plan.construct_build(), &tx, &cancel).await?
    };
    engine
        .attest(&plan.construct_build().tag, &construct, &cancel)
        .await?;
    Ok(plan.final_builds(&construct))
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

async fn expect_contention(
    rig: &mut Rig,
    pending: &mut std::pin::Pin<Box<impl Future<Output = Result<BuiltImage, ImageError>>>>,
    case: &str,
) -> TestResult {
    match timeout(Duration::from_secs(3), async {
        tokio::select! {
            result = pending.as_mut() => Err(result),
            event = rig.contended.recv() => Ok(event),
        }
    })
    .await
    .map_err(|_| format!("{case} did not reach filesystem lock contention"))?
    {
        Ok(Some(())) => Ok(()),
        Ok(None) => Err(format!("{case}: lock signal closed").into()),
        Err(result) => {
            Err(format!("{case} completed before its peer released the lock: {result:?}").into())
        }
    }
}

#[tokio::test]
async fn two_calls_for_one_final_key_build_once_and_the_second_is_served_from_cache() -> TestResult
{
    guard(async {
        let mut rig = Rig::new()?;
        let request = rig.fixture.request()?;
        let finals = prepared(&rig, &request).await?;
        let gate = rig.docker.hold_build(finals.role.tag.clone());
        let (first, second) = (rig.engine(), rig.engine());
        let cancel = CancellationToken::new();
        let first_request = request.clone();
        let first_cancel = cancel.clone();
        let first_task =
            tokio::spawn(async move { call(&first, &first_request, &first_cancel).await });
        gate.wait_arrived().await;
        let mut b = Box::pin(call(&second, &request, &cancel));
        expect_contention(&mut rig, &mut b, "same-key peer").await?;
        gate.release();
        let (a, b) = tokio::join!(first_task, b);
        let (a, b) = (a??, b?);
        assert_eq!(a.source, ImageSource::Built);
        assert_eq!(b.source, ImageSource::Cache);
        assert_eq!((&a.tag, &a.id), (&b.tag, &b.id));
        assert_eq!(rig.docker.image_id(&a.tag), Some(a.id.clone()));
        assert!(rig.fixture.work_entries().is_empty());
        Ok(())
    })
    .await
}

#[tokio::test]
async fn a_peer_rebuild_waits_for_construct_readers_then_reuses_the_same_final_key() -> TestResult {
    guard(async {
        let mut rig = Rig::new()?;
        let request = rig.fixture.request()?;
        let finals = prepared(&rig, &request).await?;
        let gate = rig.docker.hold_build(finals.role.tag.clone());
        let mut rebuild = request.clone();
        rebuild.rebuild = Rebuild::Yes;
        let first = rig.engine();
        let second = rig.engine();
        let cancel = CancellationToken::new();
        let first_request = request.clone();
        let first_cancel = cancel.clone();
        let first_task =
            tokio::spawn(async move { call(&first, &first_request, &first_cancel).await });
        gate.wait_arrived().await;
        let mut b = Box::pin(call(&second, &rebuild, &cancel));
        expect_contention(&mut rig, &mut b, "construct reader").await?;
        gate.release();
        let (a, b) = tokio::join!(first_task, b);
        let (a, b) = (a??, b?);
        assert_eq!(a.source, ImageSource::Built);
        assert_eq!(b.source, ImageSource::Built);
        assert_eq!((&a.tag, &a.id), (&b.tag, &b.id));
        assert_eq!(rig.docker.image_id(&a.tag), Some(a.id));
        Ok(())
    })
    .await
}

#[tokio::test]
async fn a_forced_rebuild_waits_for_the_held_final_key_then_reuses_its_resolved_id() -> TestResult {
    guard(async {
        let mut rig = Rig::new()?;
        let request = rig.fixture.request()?;
        let engine = rig.engine();
        let cancel = CancellationToken::new();
        let (tx, mut rx) = mpsc::channel::<ImageEvent>(256);
        let held = ensure_held(&engine, &request, &tx, &cancel).await?;
        rx.close();
        assert_eq!(held.image.source, ImageSource::Built);
        let mut rebuild = request.clone();
        rebuild.rebuild = Rebuild::Yes;
        let second = rig.engine();
        let mut pending = Box::pin(call(&second, &rebuild, &cancel));
        expect_contention(&mut rig, &mut pending, "forced rebuild final key").await?;
        let original = held.image.clone();
        drop(held);
        let rebuilt = pending.await?;
        assert_eq!(rebuilt.source, ImageSource::Built);
        assert_eq!((&rebuilt.tag, &rebuilt.id), (&original.tag, &original.id));
        assert_eq!(rig.docker.image_id(&rebuilt.tag), Some(rebuilt.id));
        Ok(())
    })
    .await
}

#[tokio::test]
async fn a_cache_hit_keeps_the_final_tag_held_until_the_caller_drops_the_returned_hold()
-> TestResult {
    guard(async {
        let mut rig = Rig::new()?;
        let request = rig.fixture.request()?;
        let engine = rig.engine();
        let warm = call(&engine, &request, &CancellationToken::new()).await?;
        assert_eq!(warm.source, ImageSource::Built);
        let (tx, mut rx) = mpsc::channel::<ImageEvent>(256);
        let held = ensure_held(&engine, &request, &tx, &CancellationToken::new()).await?;
        rx.close();
        assert_eq!(held.image.source, ImageSource::Cache);
        assert_eq!(held.image.id, warm.id);
        let second = rig.engine();
        let rebuild_cancel = CancellationToken::new();
        let mut again = Box::pin(call(&second, &request, &rebuild_cancel));
        expect_contention(&mut rig, &mut again, "cache-hit final key").await?;
        drop(held);
        let cached = again.await?;
        assert_eq!(cached.source, ImageSource::Cache);
        assert_eq!(cached.id, warm.id);
        assert_eq!(rig.docker.image_id(&cached.tag), Some(cached.id));
        Ok(())
    })
    .await
}

#[tokio::test]
async fn different_final_keys_wait_for_the_shared_construct_read_hold() -> TestResult {
    guard(async {
        let mut rig = Rig::new()?;
        let first_request = rig.fixture.request()?;
        let first_finals = prepared(&rig, &first_request).await?;
        let gate = rig.docker.hold_build(first_finals.role.tag.clone());
        let mut second_request = first_request.clone();
        second_request.revision = RoleRevision {
            commit: CommitId::parse(&"2".repeat(40))?,
            digest: Sha256::of(b"clean"),
        };
        let (first, second) = (rig.engine(), rig.engine());
        let cancel = CancellationToken::new();
        let first_request_for_task = first_request.clone();
        let first_cancel = cancel.clone();
        let first_task =
            tokio::spawn(async move { call(&first, &first_request_for_task, &first_cancel).await });
        gate.wait_arrived().await;
        let mut b = Box::pin(call(&second, &second_request, &cancel));
        expect_contention(&mut rig, &mut b, "second final key").await?;
        gate.release();
        let (a, b) = tokio::join!(first_task, b);
        let (a, b) = (a??, b?);
        assert_eq!(a.source, ImageSource::Built);
        assert_eq!(b.source, ImageSource::Built);
        assert_ne!(a.tag, b.tag);
        assert_eq!(rig.docker.image_id(&a.tag), Some(a.id));
        assert_eq!(rig.docker.image_id(&b.tag), Some(b.id));
        Ok(())
    })
    .await
}

#[tokio::test]
async fn a_cached_role_is_served_without_waiting_for_a_peers_role_build() -> TestResult {
    guard(async {
        let rig = Rig::new()?;
        let cached_request = rig.fixture.request()?;
        let warm = call(&rig.engine(), &cached_request, &CancellationToken::new()).await?;
        let mut building_request = cached_request.clone();
        building_request.revision = RoleRevision {
            commit: CommitId::parse(&"2".repeat(40))?,
            digest: Sha256::of(b"clean"),
        };
        let finals = prepared(&rig, &building_request).await?;
        let gate = rig.docker.hold_build(finals.role.tag.clone());
        let first = rig.engine();
        let first_cancel = CancellationToken::new();
        let building_request_for_task = building_request.clone();
        let first_task =
            tokio::spawn(
                async move { call(&first, &building_request_for_task, &first_cancel).await },
            );
        gate.wait_arrived().await;
        let second = rig.engine();
        let cached = timeout(
            Duration::from_secs(3),
            call(&second, &cached_request, &CancellationToken::new()),
        )
        .await
        .map_err(|_| "cached image waited behind another role build")??;
        assert_eq!(cached.source, ImageSource::Cache);
        assert_eq!(cached.id, warm.id);
        gate.release();
        let built = first_task.await??;
        assert_eq!(built.source, ImageSource::Built);
        assert_ne!(built.tag, cached.tag);
        Ok(())
    })
    .await
}

async fn following_call_completes(rig: &Rig, request: &ImageRequest, case: &str) -> TestResult {
    let engine = rig.engine();
    let finished = timeout(
        Duration::from_secs(3),
        call(&engine, request, &CancellationToken::new()),
    )
    .await
    .map_err(|_| format!("key lock not released after {case}"))??;
    assert_eq!(rig.docker.image_id(&finished.tag), Some(finished.id));
    Ok(())
}

#[tokio::test]
async fn the_key_lock_is_released_on_failure_cancel_and_drop_and_waiting_honors_cancel()
-> TestResult {
    guard(async {
        let mut rig = Rig::new()?;
        let request = rig.fixture.request()?;
        let finals = prepared(&rig, &request).await?;
        rig.docker.script_build(
            finals.role.tag.clone(),
            DockerBuildReply::failed(b"role failed"),
        );
        let failed = call(&rig.engine(), &request, &CancellationToken::new()).await;
        assert!(matches!(failed, Err(ImageError::Engine(_))), "{failed:?}");
        assert!(rig.docker.image_id(&finals.role.tag).is_none());
        assert!(rig.docker.image_id(&finals.derived.tag).is_none());
        assert!(rig.fixture.work_entries().is_empty());
        following_call_completes(&rig, &request, "a failed build").await?;

        for cancel_it in [true, false] {
            let mut next_request = rig.fixture.request()?;
            let commit = if cancel_it { "3" } else { "4" }.repeat(40);
            next_request.revision = RoleRevision {
                commit: CommitId::parse(&commit)?,
                digest: Sha256::of(b"clean"),
            };
            let next_finals = prepared(&rig, &next_request).await?;
            let gate = rig.docker.hold_build(next_finals.role.tag.clone());
            let engine = rig.engine();
            let cancel = CancellationToken::new();
            let cancellation = cancel.clone();
            let request_for_task = next_request.clone();
            let (tx, mut rx) = mpsc::channel::<ImageEvent>(256);
            let running = tokio::spawn(async move {
                ensure_image(&engine, &request_for_task, &tx, &cancellation).await
            });
            gate.wait_arrived().await;
            if cancel_it {
                cancel.cancel();
                let result = running.await?;
                assert!(
                    matches!(result, Err(ImageError::Cancelled { .. })),
                    "{result:?}"
                );
            } else {
                running.abort();
                assert!(running.await.unwrap_err().is_cancelled());
            }
            gate.release();
            rx.close();
            assert!(rig.docker.image_id(&next_finals.role.tag).is_none());
            assert!(rig.docker.image_id(&next_finals.derived.tag).is_none());
            assert!(rig.fixture.work_entries().is_empty());
            following_call_completes(
                &rig,
                &next_request,
                if cancel_it {
                    "a cancel"
                } else {
                    "a dropped call"
                },
            )
            .await?;
        }

        let mut wait_request = rig.fixture.request()?;
        wait_request.revision = RoleRevision {
            commit: CommitId::parse(&"5".repeat(40))?,
            digest: Sha256::of(b"clean"),
        };
        let wait_finals = prepared(&rig, &wait_request).await?;
        let gate = rig.docker.hold_build(wait_finals.role.tag.clone());
        let first = rig.engine();
        let second = rig.engine();
        let holder_cancel = CancellationToken::new();
        let waiter_cancel = CancellationToken::new();
        let holder_request = wait_request.clone();
        let holder_task =
            tokio::spawn(async move { call(&first, &holder_request, &holder_cancel).await });
        gate.wait_arrived().await;
        let mut waiter = Box::pin(call(&second, &wait_request, &waiter_cancel));
        expect_contention(&mut rig, &mut waiter, "cancelled waiter").await?;
        waiter_cancel.cancel();
        let cancelled = waiter.await;
        assert!(
            matches!(cancelled, Err(ImageError::Cancelled { .. })),
            "{cancelled:?}"
        );
        assert!(rig.docker.image_id(&wait_finals.role.tag).is_none());
        gate.release();
        let built = holder_task.await??;
        assert_eq!(built.source, ImageSource::Built);
        assert_eq!(rig.docker.image_id(&built.tag), Some(built.id));
        Ok(())
    })
    .await
}

#[tokio::test]
async fn two_roles_that_miss_the_construct_wait_for_its_real_filesystem_lock() -> TestResult {
    guard(async {
        let mut rig = Rig::new()?;
        let first_request = rig.fixture.request()?;
        let construct_tag = ImagePlan::new(&first_request)?
            .construct_build()
            .tag
            .clone();
        let gate = rig.docker.hold_build(construct_tag.clone());
        let mut second_request = first_request.clone();
        second_request.role = RoleName::parse("other")?;
        let first = rig.engine();
        let second = rig.engine();
        let cancel = CancellationToken::new();
        let first_request_for_task = first_request.clone();
        let first_cancel = cancel.clone();
        let first_task =
            tokio::spawn(async move { call(&first, &first_request_for_task, &first_cancel).await });
        gate.wait_arrived().await;
        let mut b = Box::pin(call(&second, &second_request, &cancel));
        expect_contention(&mut rig, &mut b, "shared construct").await?;
        gate.release();
        let (a, b) = tokio::join!(first_task, b);
        let (a, b) = (a??, b?);
        assert_eq!(a.source, ImageSource::Built);
        assert_eq!(b.source, ImageSource::Built);
        assert_ne!(a.tag, b.tag);
        assert_eq!(
            rig.docker.image_id(&construct_tag),
            Some(
                rig.engine()
                    .find(&construct_tag, &cancel)
                    .await?
                    .ok_or("construct tag missing")?
            )
        );
        assert_eq!(rig.docker.image_id(&a.tag), Some(a.id));
        assert_eq!(rig.docker.image_id(&b.tag), Some(b.id));
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
    assert!(rig.docker.images().is_empty());
    drop(held);
    Ok(())
}
