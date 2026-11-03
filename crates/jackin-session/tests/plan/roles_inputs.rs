// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::ffi::OsStr;
use std::os::unix::ffi::OsStrExt;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use async_trait::async_trait;
use jackin_core::{
    Config, DirEntry, FileKind, FileLock, FileMode, FileSystem, FsError, HookKind, HostPath,
    ImageId, LinkProblem, ProcessRunner, RegularFile, RegularFileProblem, RepoPath, RoleName,
    StdFileSystem, WorkspaceName,
};
use jackin_image::{BuildxEngine, ImageEngine, ImagePlan, Rebuild};
use jackin_session::{RoleCatalog, plan_image};
use jackin_testkit::{
    CapturedFile, FakeClock, ProcessEffect, ProcessReply, RoleFixture, ScriptedProcessRunner,
    ScriptedProgram, TestRunner,
};
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

use super::roles_origin::{Origin, cache, git};
use super::support::{Spec, TestResult, catalog, guard};

const FIFO_HOLD: Duration = Duration::from_secs(1);

fn release_later(path: &Path) -> Arc<AtomicBool> {
    let released = Arc::new(AtomicBool::new(false));
    let flag = Arc::clone(&released);
    let path = path.to_path_buf();
    tokio::spawn(async move {
        tokio::time::sleep(FIFO_HOLD).await;
        for _ in 0..20 {
            let opened = rustix::fs::open(
                &path,
                rustix::fs::OFlags::WRONLY | rustix::fs::OFlags::NONBLOCK,
                rustix::fs::Mode::empty(),
            );
            if opened.is_ok() {
                flag.store(true, Ordering::SeqCst);
                return;
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    });
    released
}

async fn mkfifo(path: &Path) -> TestResult {
    let status = tokio::process::Command::new("mkfifo")
        .arg(path)
        .status()
        .await?;
    assert!(status.success());
    Ok(())
}

async fn refuses_without_blocking(
    catalog: &RoleCatalog,
    role: &RoleFixture,
    fifo: &str,
) -> TestResult {
    let target = role.root().as_path().join(fifo);
    let released = release_later(&target);
    let found = catalog.load_path(role.root()).await;
    assert!(
        !released.load(Ordering::SeqCst),
        "{fifo}: the load blocked on the FIFO until a writer opened it"
    );
    assert!(found.is_err(), "{fifo}: {found:?}");
    Ok(())
}

#[tokio::test]
async fn a_fifo_as_a_role_input_is_refused_at_once() -> TestResult {
    guard(async {
        let (cache, _keep) = cache()?;
        let (catalog, _runner) = catalog(&cache);

        let policy = RoleFixture::new("sentinel")?;
        policy.commit().await?;
        policy.remove("policy.yaml").await?;
        mkfifo(&policy.root().as_path().join("policy.yaml")).await?;
        refuses_without_blocking(&catalog, &policy, "policy.yaml").await?;

        let hook = RoleFixture::new("sentinel")?.hook(HookKind::Setup, b"#!/bin/sh\n");
        hook.commit().await?;
        hook.remove("hooks/setup.sh").await?;
        mkfifo(&hook.root().as_path().join("hooks/setup.sh")).await?;
        refuses_without_blocking(&catalog, &hook, "hooks/setup.sh").await?;

        let listed = RoleFixture::new("sentinel")?;
        listed.commit().await?;
        mkfifo(&listed.root().as_path().join("pipe.txt")).await?;
        refuses_without_blocking(&catalog, &listed, "pipe.txt").await?;
        Ok(())
    })
    .await
}

#[derive(Debug)]
struct SwapFs {
    inner: StdFileSystem,
    leaf: HostPath,
    secret: std::path::PathBuf,
    swapped: AtomicBool,
}

impl SwapFs {
    fn swap_after(&self, path: &HostPath) -> Result<(), FsError> {
        if *path == self.leaf && !self.swapped.swap(true, Ordering::SeqCst) {
            let failure = |source: std::io::Error| FsError::Io {
                path: self.leaf.clone(),
                kind: source.kind(),
                message: source.to_string(),
            };
            std::fs::remove_file(self.leaf.as_path()).map_err(failure)?;
            std::os::unix::fs::symlink(&self.secret, self.leaf.as_path()).map_err(failure)?;
        }
        Ok(())
    }
}

#[async_trait]
impl FileSystem for SwapFs {
    async fn read(&self, p: &HostPath) -> Result<Option<Vec<u8>>, FsError> {
        let found = self.inner.read(p).await;
        self.swap_after(p)?;
        found
    }
    async fn stat(&self, p: &HostPath) -> Result<Option<FileKind>, FsError> {
        let found = self.inner.stat(p).await;
        self.swap_after(p)?;
        found
    }
    async fn stat_following(&self, p: &HostPath) -> Result<Option<FileKind>, FsError> {
        self.inner.stat_following(p).await
    }
    async fn file_len(&self, p: &HostPath) -> Result<Option<u64>, FsError> {
        self.inner.file_len(p).await
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
        self.inner.lock(p).await
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
        let found = self.inner.read_regular(root, file, max_bytes).await;
        let leaf =
            HostPath::parse(&root.as_path().join(file.as_str())).map_err(|error| FsError::Io {
                path: root.clone(),
                kind: std::io::ErrorKind::InvalidInput,
                message: error.to_string(),
            })?;
        self.swap_after(&leaf)?;
        found
    }
}

#[tokio::test]
async fn a_role_input_swapped_for_a_symlink_after_it_was_checked_never_yields_outside_bytes()
-> TestResult {
    guard(async {
        let role = RoleFixture::new("sentinel")?;
        role.commit().await?;
        let outside = tempfile::tempdir()?;
        let secret = outside.path().join("policy.yaml");
        std::fs::write(
            &secret,
            super::roles::rules(&[("evil", "evil.example", 443, "/usr/bin/curl")]),
        )?;
        let swapping = Arc::new(SwapFs {
            inner: StdFileSystem::default(),
            leaf: HostPath::parse(&role.root().as_path().join("policy.yaml"))?,
            secret,
            swapped: AtomicBool::new(false),
        });
        let (cache, _keep) = cache()?;
        let catalog = RoleCatalog::new(
            Arc::<SwapFs>::clone(&swapping),
            Arc::new(TestRunner::new()),
            Arc::new(FakeClock::new()),
            cache,
        );
        let found = catalog.load_path(role.root()).await;
        assert!(
            swapping.swapped.load(Ordering::SeqCst),
            "the swap never ran"
        );
        if let Ok(loaded) = found {
            let keys: Vec<String> = loaded
                .policy()
                .network_rules()
                .iter()
                .map(|rule| rule.key.to_string())
                .collect();
            assert!(!keys.iter().any(|key| key == "evil"), "{keys:?}");
        }
        Ok(())
    })
    .await
}

const FILE_LIMIT: u64 = 16 * 1024 * 1024;

fn sparse(path: &Path, len: u64) -> TestResult {
    std::fs::File::create(path)?.set_len(len)?;
    Ok(())
}

#[tokio::test]
async fn a_role_file_over_the_size_limit_is_refused() -> TestResult {
    guard(async {
        let (cache, _keep) = cache()?;
        let (catalog, _runner) = catalog(&cache);

        let single = RoleFixture::new("sentinel")?;
        single.commit().await?;
        sparse(&single.root().as_path().join("big.bin"), FILE_LIMIT + 1)?;
        let found = catalog.load_path(single.root()).await;
        assert!(
            found
                .as_ref()
                .is_err_and(|error| error.to_string().contains("big.bin")),
            "{found:?}"
        );

        let many = RoleFixture::new("sentinel")?;
        many.commit().await?;
        for index in 0..5 {
            sparse(
                &many.root().as_path().join(format!("part-{index}.bin")),
                FILE_LIMIT - 1024,
            )?;
        }
        let found = catalog.load_path(many.root()).await;
        assert!(found.is_err(), "the sum of the files is unbounded");

        let fine = RoleFixture::new("sentinel")?;
        fine.commit().await?;
        sparse(&fine.root().as_path().join("ok.bin"), FILE_LIMIT)?;
        catalog.load_path(fine.root()).await?;
        Ok(())
    })
    .await
}

#[tokio::test]
async fn the_bytes_the_image_build_stages_are_the_bytes_the_image_key_covers() -> TestResult {
    guard(async {
        let role = RoleFixture::new("sentinel")?;
        role.file("README.md", b"one\n").await?;
        let setup = Spec::new(role).build().await?;
        let loaded = setup.role;
        setup.role_fixture.file("README.md", b"two\n").await?;

        let request = plan_image(&setup.config, &loaded, &setup.gateway, None, Rebuild::No)?;
        let construct = ImageId::parse(&format!("sha256:{}", "1".repeat(64)))?;
        let build = ImagePlan::new(&request)?.final_builds(&construct).role;

        let runner = Arc::new(ScriptedProcessRunner::new());
        runner.expect_fn(ScriptedProgram::Docker, &[], |_call| {
            ProcessReply::exited(0, b"", b"")
                .with_effect(ProcessEffect::WriteArgFile {
                    prefix: "--iidfile=".to_owned(),
                    bytes: format!("sha256:{}\n", "2".repeat(64)).into_bytes(),
                })
                .with_effect(ProcessEffect::CaptureContext)
        });
        let work = tempfile::tempdir()?;
        let shared: Arc<dyn ProcessRunner> = Arc::<ScriptedProcessRunner>::clone(&runner);
        let engine = BuildxEngine::new(
            shared,
            Arc::new(StdFileSystem::default()),
            HostPath::parse(&std::fs::canonicalize(work.path())?)?,
        );
        let (tx, _rx) = mpsc::channel(8);
        engine.build(&build, &tx, &CancellationToken::new()).await?;
        let captured = runner.captured_contexts();
        assert_eq!(
            captured[0].files.get("README.md"),
            Some(&CapturedFile::file(b"one\n", 0o644))
        );
        Ok(())
    })
    .await
}

#[tokio::test]
async fn a_manifest_name_that_differs_from_the_configured_role_is_refused() -> TestResult {
    guard(async {
        let role = RoleFixture::new("sentinel")?;
        role.commit().await?;
        let (cache, _keep) = cache()?;
        let (catalog, _runner) = catalog(&cache);
        let config = Config::parse(&format!(
            "[roles.foo]\nsource = {{ path = \"{}\" }}\n\n[workspaces.w]\npath = \"{}\"\n",
            role.root().as_path().display(),
            role.root().as_path().display()
        ))?;
        let workspace = config
            .workspace(&WorkspaceName::parse("w")?)
            .ok_or("no workspace")?;
        let foo = RoleName::parse("foo")?;
        let found = catalog.load(&config, Some(&foo), workspace).await;
        assert!(
            found.as_ref().is_err_and(|error| {
                let text = error.to_string();
                text.contains("foo") && text.contains("sentinel")
            }),
            "{found:?}"
        );
        Ok(())
    })
    .await
}

#[tokio::test]
async fn a_tracked_file_whose_name_is_not_utf8_is_refused() -> TestResult {
    guard(async {
        let role = RoleFixture::new("sentinel")?;
        let root = role.root().as_path();
        role.commit().await?;
        let blob = git(root, &["hash-object", "-w", "--stdin"]).await?;
        let name = OsStr::from_bytes(b"bad\xff.txt");
        let mut spec = std::ffi::OsString::from(format!("100644,{blob},"));
        spec.push(name);
        let status = tokio::process::Command::new("/usr/bin/git")
            .arg("update-index")
            .arg("--add")
            .arg("--cacheinfo")
            .arg(spec)
            .current_dir(root)
            .status()
            .await?;
        assert!(status.success());
        let (cache, _keep) = cache()?;
        let (catalog, _runner) = catalog(&cache);
        let found = catalog.load_path(role.root()).await;
        assert!(found.is_err(), "{found:?}");
        Ok(())
    })
    .await
}

#[tokio::test]
async fn a_cached_role_follows_the_default_branch_when_the_origin_renames_it() -> TestResult {
    guard(async {
        let origin = Origin::new().await?;
        let (cache, _keep) = cache()?;
        origin
            .load(&cache, None)
            .await?
            .map_err(|e| format!("{e:?}"))?;
        git(
            origin.role.root().as_path(),
            &["branch", "-m", "main", "trunk"],
        )
        .await?;
        let moved = origin.advance("model-trunk").await?;
        let (commit, model) = origin
            .load(&cache, None)
            .await?
            .map_err(|e| format!("{e:?}"))?;
        assert_eq!((commit, model.as_deref()), (moved, Some("model-trunk")));
        Ok(())
    })
    .await
}

#[tokio::test]
async fn the_role_cache_is_private_to_the_operator() -> TestResult {
    guard(async {
        let origin = Origin::new().await?;
        let (cache, _keep) = cache()?;
        origin
            .load(&cache, None)
            .await?
            .map_err(|e| format!("{e:?}"))?;
        let mode = std::fs::metadata(cache.as_path().join("roles"))?
            .permissions()
            .mode();
        assert_eq!(mode & 0o077, 0, "{mode:o}");
        Ok(())
    })
    .await
}
