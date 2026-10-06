// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;
use jackin_core::RoleSelector;
use jackin_manifest::repo::CachedRepo;
#[cfg(unix)]
pub(super) const BUILD_TOKEN_TEST_CHILD: &str = "JACKIN_BUILD_TOKEN_TEST_CHILD";

#[cfg(unix)]
pub(super) const BUILD_TOKEN_TEST_MARKER: &str = "JACKIN_BUILD_TOKEN_TEST_MARKER";

#[cfg(unix)]
pub(super) const BUILD_TOKEN_TEST_GITHUB_TOKEN: &str = "FAKE_GITHUB_TOKEN_CANARY";

#[cfg(unix)]
pub(super) const BUILD_TOKEN_TEST_GH_TOKEN: &str = "FAKE_GH_TOKEN_CANARY";

#[cfg(unix)]
pub(super) const BUILD_TOKEN_TEST_GH_CLI: &str = "FAKE_GH_CLI_TOKEN_CANARY";

#[cfg(unix)]
#[derive(Default)]
pub(super) struct BuildSecurityRunner {
    pub(super) commands: Vec<String>,
    pub(super) secret_commands: Vec<String>,
    pub(super) docker_build_options: Vec<RunOptions>,
    pub(super) docker_build_count: usize,
    pub(super) fail_docker_build_at: Option<usize>,
    pub(super) execute_fake_gh: bool,
}

#[cfg(unix)]
impl CommandRunner for BuildSecurityRunner {
    #[expect(
        clippy::manual_async_fn,
        reason = "desugared so the sync-bodied fake satisfies 1.98's \
         `unused_async_trait_impl`; the trait still requires a `Future`"
    )]
    fn run(
        &mut self,
        program: &str,
        args: &[&str],
        _cwd: Option<&Path>,
        opts: &RunOptions,
    ) -> impl Future<Output = anyhow::Result<()>> {
        async move {
            self.commands.push(format!("{program} {}", args.join(" ")));
            if program == "docker" && args.first() == Some(&"build") {
                self.docker_build_count += 1;
                self.docker_build_options.push(opts.clone());
                if self.fail_docker_build_at == Some(self.docker_build_count) {
                    anyhow::bail!("simulated Docker BuildKit failure");
                }
            }
            Ok(())
        }
    }

    #[expect(
        clippy::manual_async_fn,
        reason = "desugared so the sync-bodied fake satisfies 1.98's \
         `unused_async_trait_impl`; the trait still requires a `Future`"
    )]
    fn capture(
        &mut self,
        program: &str,
        args: &[&str],
        _cwd: Option<&Path>,
    ) -> impl Future<Output = anyhow::Result<String>> {
        async move {
            self.commands.push(format!("{program} {}", args.join(" ")));
            if program == "git" && args.contains(&"remote") {
                return Ok("https://github.com/example/agent-smith.git".to_owned());
            }
            if program == "git" && args.contains(&"rev-parse") {
                return Ok("main".to_owned());
            }
            Ok(String::new())
        }
    }

    #[expect(
        clippy::manual_async_fn,
        reason = "desugared so the sync-bodied fake satisfies 1.98's \
         `unused_async_trait_impl`; the trait still requires a `Future`"
    )]
    fn capture_secret(
        &mut self,
        program: &str,
        args: &[&str],
        _cwd: Option<&Path>,
    ) -> impl Future<Output = anyhow::Result<String>> {
        async move {
            self.secret_commands
                .push(format!("{program} {}", args.join(" ")));
            if !self.execute_fake_gh {
                return Ok(BUILD_TOKEN_TEST_GH_CLI.to_owned());
            }
            #[expect(
                clippy::disallowed_methods,
                reason = "test fake executes a fixture credential command off runtime threads"
            )]
            let output = ProcessCommand::new(program).args(args).output()?;
            anyhow::ensure!(output.status.success(), "fake credential command failed");
            Ok(String::from_utf8(output.stdout)?)
        }
    }
}

#[cfg(unix)]
pub(super) async fn build_test_agent_image(runner: &mut BuildSecurityRunner) -> anyhow::Result<()> {
    let temp = tempfile::tempdir()?;
    let paths = JackinPaths::for_tests(temp.path());
    let selector = RoleSelector::new(None, "agent-smith");
    let cached_repo = CachedRepo::new(&paths, &selector);
    jackin_test_support::seed_valid_role_repo(&cached_repo.repo_dir);
    std::fs::write(
        cached_repo.repo_dir.join("Dockerfile"),
        format!(
            "{}RUN --mount=type=secret,id=github_token echo private-dependency\n",
            jackin_test_support::TEST_DOCKERFILE_FROM
        ),
    )?;

    let source_url = "https://github.com/example/agent-smith.git";
    let (cached_repo, validated_repo, repo_lock) =
        crate::runtime::repo_cache::resolve_agent_repo_with(
            &paths,
            &selector,
            source_url,
            runner,
            crate::runtime::repo_cache::RepoResolveOptions::interactive(false),
            || Ok(false),
        )
        .await?;
    let capsule = temp.path().join("jackin-capsule");
    std::fs::write(&capsule, b"fake capsule binary")?;
    let runtime_binaries = PreparedRuntimeBinaries {
        agent_installs: BTreeMap::from([(Agent::Claude, AgentInstall::ScriptFallback)]),
        prefetched_agent_versions: BTreeMap::new(),
        jackin_capsule_src: capsule.display().to_string(),
    };
    let docker = FakeDockerClient::default();

    build_agent_image(
        &paths,
        &selector,
        &cached_repo,
        &validated_repo,
        Agent::Claude,
        runtime_binaries,
        false,
        ImageInvalidationReason::LocalImageMissing,
        None,
        false,
        None,
        &docker,
        runner,
        repo_lock,
        Some("abc123"),
        None,
    )
    .await?;
    Ok(())
}

pub(super) static RICH_SURFACE_TEST_LOCK: Mutex<()> = Mutex::new(());

pub(super) const IMAGE_BUILD_SOURCE: &str = include_str!("../build.rs");

pub(super) const IMAGE_VERSION_SOURCE: &str = include_str!("../version.rs");

pub(super) const IMAGE_MODULE_SOURCE: &str = include_str!("../../image.rs");

pub(super) const SHARED_IMAGE_BUILD_SOURCE: &str =
    include_str!("../../../../../../adapters/jackin-image/src/image_build.rs");

pub(super) struct RichSurfaceTestGuard {
    _guard: MutexGuard<'static, ()>,
}

impl Drop for RichSurfaceTestGuard {
    fn drop(&mut self) {
        jackin_diagnostics::set_rich_surface_active(false);
        jackin_diagnostics::set_host_screen_owned(false);
    }
}

pub(super) fn rich_surface_test_guard() -> RichSurfaceTestGuard {
    let guard = RICH_SURFACE_TEST_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    jackin_diagnostics::set_rich_surface_active(false);
    jackin_diagnostics::set_host_screen_owned(false);
    RichSurfaceTestGuard { _guard: guard }
}

pub(super) fn make_docker(labels: HashMap<String, String>) -> FakeDockerClient {
    let docker = FakeDockerClient::default();
    docker
        .inspect_image_labels_queue
        .borrow_mut()
        .push_back(labels);
    docker
}

pub(super) fn validated_test_repo(
    paths: &JackinPaths,
    selector: &RoleSelector,
) -> (CachedRepo, jackin_manifest::repo::ValidatedRoleRepo) {
    let cached_repo = CachedRepo::new(paths, selector);
    jackin_test_support::seed_valid_role_repo(&cached_repo.repo_dir);
    let validated_repo = jackin_manifest::repo::validate_role_repo(&cached_repo.repo_dir).unwrap();
    (cached_repo, validated_repo)
}

pub(super) fn recorded_docker_build(runner: &FakeRunner) -> &str {
    runner
        .run_recorded
        .iter()
        .find(|command| command.contains("docker build "))
        .map(String::as_str)
        .expect("expected docker build command")
}
