//! Test helpers: `FakeRunner` for subprocess injection and minimal role-repo seed utilities.
//!
//! Not responsible for: asserting test outcomes — callers inspect `FakeRunner::recorded`
//! and `FakeRunner::run_options` directly after the call under test.

#![expect(
    clippy::expect_used,
    clippy::unwrap_used,
    reason = "test support fixture setup should fail immediately with source location"
)]

use jackin_core::{CommandRunner, RunOptions};
use std::collections::VecDeque;

/// Pre-install all binary test stubs (agent binaries + jackin-capsule) so that
/// `load_role` calls in tests never fall through to network downloads regardless
/// of how the `cfg!(test)` flag is resolved in each dependency compilation unit.
///
/// Call this once per test that calls `load_role` or any function that internally
/// invokes `ensure_available`.
#[cfg(any(test, feature = "test-support"))]
pub fn install_all_test_stubs(paths: &jackin_core::paths::JackinPaths) {
    use jackin_core::agent::Agent;
    for agent in &[
        Agent::Claude,
        Agent::Codex,
        Agent::Amp,
        Agent::Kimi,
        Agent::Opencode,
    ] {
        jackin_image::agent_binary::install_test_stub(paths, *agent)
            .expect("install agent binary test stub");
    }
    jackin_image::capsule_binary::install_test_stub(paths).expect("install capsule test stub");
}

#[expect(
    missing_debug_implementations,
    reason = "FakeRunner stores one-shot side-effect closures that cannot be formatted."
)]
#[derive(Default)]
pub struct FakeRunner {
    pub recorded: Vec<String>,
    pub run_recorded: Vec<String>,
    pub run_options: Vec<RunOptions>,
    pub fail_on: Vec<String>,
    pub fail_with: Vec<(String, String)>,
    pub capture_queue: VecDeque<String>,
    pub side_effects: Vec<(String, Box<dyn FnOnce()>)>,
}

#[cfg_attr(
    not(test),
    expect(dead_code, reason = "test helper impl is consumed by test targets")
)]
impl FakeRunner {
    pub(super) fn with_capture_queue<const N: usize>(outputs: [String; N]) -> Self {
        Self {
            capture_queue: VecDeque::from(outputs),
            ..Default::default()
        }
    }

    /// Number of capture calls `load_role` makes before reaching role-
    /// specific logic: 2 identity lookups (`git config user.name`,
    /// `git config user.email`).
    /// GC now uses `DockerApi`, not `CommandRunner`, so it no longer counts.
    const LOAD_PREAMBLE_CAPTURES: usize = 2;

    pub(super) fn for_load_agent<const N: usize>(outputs: [String; N]) -> Self {
        let mut queue = VecDeque::with_capacity(Self::LOAD_PREAMBLE_CAPTURES + N);
        for _ in 0..Self::LOAD_PREAMBLE_CAPTURES {
            queue.push_back(String::new());
        }
        queue.extend(outputs);
        Self {
            capture_queue: queue,
            ..Default::default()
        }
    }
}

impl FakeRunner {
    fn check_command(&mut self, command: &str) -> anyhow::Result<()> {
        if let Some((_, message)) = self
            .fail_with
            .iter()
            .find(|(pattern, _)| command.contains(pattern))
        {
            let message = message.clone();
            anyhow::bail!("{message}");
        }
        if self.fail_on.iter().any(|pattern| command.contains(pattern)) {
            anyhow::bail!("command failed: {command}");
        }
        if let Some(pos) = self
            .side_effects
            .iter()
            .position(|(pattern, _)| command.contains(pattern))
        {
            let (_, callback) = self.side_effects.remove(pos);
            callback();
        }
        Ok(())
    }
}

impl CommandRunner for FakeRunner {
    async fn run(
        &mut self,
        program: &str,
        args: &[&str],
        _cwd: Option<&std::path::Path>,
        opts: &RunOptions,
    ) -> anyhow::Result<()> {
        let command = format!("{} {}", program, args.join(" "));
        self.run_options.push(opts.clone());
        self.run_recorded.push(command.clone());
        self.recorded.push(command.clone());
        self.check_command(&command)
    }

    async fn capture(
        &mut self,
        program: &str,
        args: &[&str],
        _cwd: Option<&std::path::Path>,
    ) -> anyhow::Result<String> {
        let command = format!("{} {}", program, args.join(" "));
        self.recorded.push(command.clone());
        self.check_command(&command)?;
        // Empty queue returns "" — safe for most captures (git SHA, id outputs), but
        // dangerous for assess_cleanup captures: `rev-list` returning "" maps to
        // "0 commits ahead, safe to delete" and `symbolic-ref HEAD` returning ""
        // silently skips the detached-HEAD guard. Pre-fill the queue in tests that
        // exercise those code paths.
        Ok(self.capture_queue.pop_front().unwrap_or_default())
    }

    async fn capture_secret(
        &mut self,
        program: &str,
        args: &[&str],
        cwd: Option<&std::path::Path>,
    ) -> anyhow::Result<String> {
        self.capture(program, args, cwd).await
    }
}

/// Minimal Dockerfile content used in test role repos. Passes `validate_agent_dockerfile`.
pub(crate) const TEST_DOCKERFILE_FROM: &str = jackin_manifest::repo_contract::BASE_DOCKERFILE_FROM;

/// Minimal `jackin.role.toml` content used in test role repos. Parses as a valid manifest.
pub(crate) const TEST_MANIFEST_TOML: &str = r#"version = "v1alpha3"
dockerfile = "Dockerfile"

[claude]
plugins = []
"#;

/// Seed a minimal but valid role repo at `repo_dir`.
///
/// Creates `.git/`, `Dockerfile`, and `jackin.role.toml`. All three are
/// required for `validate_role_repo` to succeed.
pub fn seed_valid_role_repo(repo_dir: &std::path::Path) {
    std::fs::create_dir_all(repo_dir.join(".git")).unwrap();
    std::fs::write(repo_dir.join("Dockerfile"), TEST_DOCKERFILE_FROM).unwrap();
    std::fs::write(repo_dir.join("jackin.role.toml"), TEST_MANIFEST_TOML).unwrap();
}

/// Find the `repo` subdir under the first `role-resolve-*` temp dir that
/// `register_agent_repo` creates inside `data_dir`.
pub fn first_temp_role_repo(data_dir: &std::path::Path) -> std::path::PathBuf {
    std::fs::read_dir(data_dir)
        .unwrap()
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .find(|path| {
            path.is_dir()
                && path
                    .file_name()
                    .and_then(|name| name.to_str())
                    .is_some_and(|name| name.starts_with("role-resolve-"))
        })
        .expect("role registration temp dir should exist before git clone side-effect")
        .join("repo")
}

#[cfg(any(test, feature = "test-support"))]
pub use fake_docker::FakeDockerClient;

#[cfg(any(test, feature = "test-support"))]
pub mod fake_docker {
    use std::collections::HashMap;

    use jackin_core::{
        ContainerRow, ContainerSpec, ContainerState, DockerApi, NetworkRow, RemoveImageOutcome,
    };

    #[derive(Debug)]
    pub struct FakeDockerClient {
        pub recorded: std::cell::RefCell<Vec<String>>,
        pub inspect_queue: std::cell::RefCell<std::collections::VecDeque<ContainerState>>,
        /// Per-container-name inspect overrides, checked before `inspect_queue`.
        /// Lets a test pin one container's state by name regardless of how many
        /// other (queue-order-dependent) inspects run first.
        pub inspect_state_by_name: std::cell::RefCell<HashMap<String, ContainerState>>,
        pub list_containers_queue:
            std::cell::RefCell<std::collections::VecDeque<Vec<ContainerRow>>>,
        pub list_networks_queue: std::cell::RefCell<std::collections::VecDeque<Vec<NetworkRow>>>,
        pub list_image_tags_queue: std::cell::RefCell<std::collections::VecDeque<Vec<String>>>,
        pub remove_image_queue: std::cell::RefCell<std::collections::VecDeque<RemoveImageOutcome>>,
        pub exec_capture_queue: std::cell::RefCell<std::collections::VecDeque<String>>,
        pub inspect_image_labels_queue:
            std::cell::RefCell<std::collections::VecDeque<HashMap<String, String>>>,
        pub inspect_network_queue:
            std::cell::RefCell<std::collections::VecDeque<Option<NetworkRow>>>,
        pub fail_with: Vec<(String, String)>,
        pub created_containers: std::cell::RefCell<Vec<(String, ContainerSpec)>>,
        /// `(name, labels, internal)` — tracks networks created via `DockerApi::create_network`.
        #[expect(
            clippy::type_complexity,
            reason = "test record tuple mirrors the API signature; factoring adds indirection without clarity"
        )]
        pub created_networks: std::cell::RefCell<Vec<(String, HashMap<String, String>, bool)>>,
    }

    impl Default for FakeDockerClient {
        fn default() -> Self {
            Self {
                recorded: std::cell::RefCell::new(Vec::new()),
                inspect_queue: std::cell::RefCell::new(std::collections::VecDeque::new()),
                inspect_state_by_name: std::cell::RefCell::new(HashMap::new()),
                list_containers_queue: std::cell::RefCell::new(std::collections::VecDeque::new()),
                list_networks_queue: std::cell::RefCell::new(std::collections::VecDeque::new()),
                list_image_tags_queue: std::cell::RefCell::new(std::collections::VecDeque::new()),
                remove_image_queue: std::cell::RefCell::new(std::collections::VecDeque::new()),
                exec_capture_queue: std::cell::RefCell::new(std::collections::VecDeque::new()),
                inspect_image_labels_queue: std::cell::RefCell::new(
                    std::collections::VecDeque::new(),
                ),
                inspect_network_queue: std::cell::RefCell::new(std::collections::VecDeque::new()),
                fail_with: Vec::new(),
                created_containers: std::cell::RefCell::new(Vec::new()),
                created_networks: std::cell::RefCell::new(Vec::new()),
            }
        }
    }

    impl FakeDockerClient {
        fn check_fail(&self, op: &str) -> anyhow::Result<()> {
            if let Some((_, msg)) = self
                .fail_with
                .iter()
                .find(|(pat, _)| op.contains(pat.as_str()))
            {
                anyhow::bail!("{msg}");
            }
            Ok(())
        }

        fn record(&self, entry: &str) {
            self.recorded.borrow_mut().push(entry.to_owned());
        }

        fn ignore_if_missing(result: anyhow::Result<()>) -> anyhow::Result<()> {
            result.or_else(|e| {
                if e.to_string().to_ascii_lowercase().contains("no such") {
                    Ok(())
                } else {
                    Err(e)
                }
            })
        }

        fn pop_inspect(&self) -> ContainerState {
            self.inspect_queue
                .borrow_mut()
                .pop_front()
                .unwrap_or(ContainerState::NotFound)
        }

        fn pop_list_containers(&self) -> Vec<ContainerRow> {
            self.list_containers_queue
                .borrow_mut()
                .pop_front()
                .unwrap_or_default()
        }

        fn pop_list_networks(&self) -> Vec<NetworkRow> {
            self.list_networks_queue
                .borrow_mut()
                .pop_front()
                .unwrap_or_default()
        }

        fn pop_list_image_tags(&self) -> Vec<String> {
            self.list_image_tags_queue
                .borrow_mut()
                .pop_front()
                .unwrap_or_default()
        }

        fn pop_remove_image(&self) -> RemoveImageOutcome {
            self.remove_image_queue
                .borrow_mut()
                .pop_front()
                .expect("remove_image called but remove_image_queue is empty")
        }

        fn pop_exec_capture(&self) -> String {
            self.exec_capture_queue
                .borrow_mut()
                .pop_front()
                .unwrap_or_default()
        }

        fn pop_inspect_image_labels(&self) -> HashMap<String, String> {
            self.inspect_image_labels_queue
                .borrow_mut()
                .pop_front()
                .unwrap_or_default()
        }

        fn pop_inspect_network(&self) -> Option<NetworkRow> {
            self.inspect_network_queue
                .borrow_mut()
                .pop_front()
                .flatten()
        }
    }

    impl DockerApi for FakeDockerClient {
        async fn inspect_container_state(&self, name: &str) -> ContainerState {
            let op = format!("docker inspect {name}");
            self.record(&op);
            if let Some((_, msg)) = self
                .fail_with
                .iter()
                .find(|(pat, _)| op.contains(pat.as_str()))
            {
                let msg = msg.clone();
                let lower = msg.to_ascii_lowercase();
                if lower.contains("no such object")
                    || lower.contains("no such container")
                    || lower.contains("no such image")
                {
                    return ContainerState::NotFound;
                }
                return ContainerState::InspectUnavailable(msg);
            }
            if let Some(state) = self.inspect_state_by_name.borrow().get(name) {
                return state.clone();
            }
            self.pop_inspect()
        }

        async fn remove_container(&self, name: &str) -> anyhow::Result<()> {
            let op = format!("docker rm -f {name}");
            self.record(&op);
            Self::ignore_if_missing(self.check_fail(&op))
        }

        async fn list_containers(
            &self,
            label_filters: &[&str],
            all: bool,
        ) -> anyhow::Result<Vec<ContainerRow>> {
            let filter_str = label_filters.join(" --filter ");
            let op = if all {
                format!("docker ps -a --filter {filter_str}")
            } else {
                format!("docker ps --filter {filter_str}")
            };
            self.record(&op);
            self.check_fail(&op)?;
            Ok(self.pop_list_containers())
        }

        async fn create_container(&self, name: &str, spec: ContainerSpec) -> anyhow::Result<()> {
            let op = format!("create_container:{name}");
            self.record(&op);
            self.check_fail(&op)?;
            self.created_containers
                .borrow_mut()
                .push((name.to_owned(), spec));
            Ok(())
        }

        async fn start_container(&self, name: &str) -> anyhow::Result<()> {
            let op = format!("start_container:{name}");
            self.record(&op);
            self.check_fail(&op)
        }

        async fn remove_volume(&self, name: &str) -> anyhow::Result<()> {
            let op = format!("docker volume rm {name}");
            self.record(&op);
            Self::ignore_if_missing(self.check_fail(&op))
        }

        async fn create_network(
            &self,
            name: &str,
            labels: HashMap<String, String>,
            internal: bool,
        ) -> anyhow::Result<()> {
            let op = format!("docker network create {name}");
            self.record(&op);
            self.created_networks
                .borrow_mut()
                .push((name.to_owned(), labels, internal));
            self.check_fail(&op)
        }

        async fn remove_network(&self, name: &str) -> anyhow::Result<()> {
            let op = format!("docker network rm {name}");
            self.record(&op);
            Self::ignore_if_missing(self.check_fail(&op))
        }

        async fn list_networks(&self, label_filters: &[&str]) -> anyhow::Result<Vec<NetworkRow>> {
            let filter_str = label_filters.join(" --filter ");
            let op = format!("docker network ls --filter {filter_str}");
            self.record(&op);
            self.check_fail(&op)?;
            Ok(self.pop_list_networks())
        }

        async fn inspect_network(&self, name: &str) -> anyhow::Result<Option<NetworkRow>> {
            let op = format!("docker network inspect {name}");
            self.record(&op);
            self.check_fail(&op)?;
            Ok(self.pop_inspect_network())
        }

        async fn list_image_tags(&self, reference_filter: &str) -> anyhow::Result<Vec<String>> {
            let op = format!("docker images --filter reference={reference_filter}");
            self.record(&op);
            self.check_fail(&op)?;
            Ok(self.pop_list_image_tags())
        }

        async fn remove_image(&self, name: &str) -> anyhow::Result<RemoveImageOutcome> {
            let op = format!("docker rmi {name}");
            self.record(&op);
            self.check_fail(&op)?;
            Ok(self.pop_remove_image())
        }

        async fn inspect_image_labels(
            &self,
            image: &str,
        ) -> anyhow::Result<HashMap<String, String>> {
            let op = format!("docker inspect image:{image}");
            self.record(&op);
            self.check_fail(&op)?;
            Ok(self.pop_inspect_image_labels())
        }

        async fn pull_image(&self, image: &str) -> anyhow::Result<()> {
            let op = format!("docker pull {image}");
            self.record(&op);
            self.check_fail(&op)
        }

        async fn exec_capture(&self, container: &str, cmd: &[&str]) -> anyhow::Result<String> {
            let op = format!("docker exec {} {}", container, cmd.join(" "));
            self.record(&op);
            self.check_fail(&op)?;
            Ok(self.pop_exec_capture())
        }
    }
}
