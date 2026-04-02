use crate::config::AppConfig;
use crate::derived_image::create_derived_build_context;
use crate::docker::CommandRunner;
use crate::instance::{next_container_name, AgentState};
use crate::paths::JackinPaths;
use crate::repo::{validate_agent_repo, CachedRepo};
use crate::selector::ClassSelector;
use crate::tui;

pub struct LoadOptions {
    pub no_intro: bool,
    pub debug: bool,
}

impl Default for LoadOptions {
    fn default() -> Self {
        Self {
            no_intro: true,
            debug: false,
        }
    }
}

struct GitIdentity {
    user_name: String,
    user_email: String,
}

fn load_git_identity() -> GitIdentity {
    let user_name = std::process::Command::new("git")
        .args(["config", "user.name"])
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .unwrap_or_default();
    let user_email = std::process::Command::new("git")
        .args(["config", "user.email"])
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .unwrap_or_default();
    GitIdentity {
        user_name,
        user_email,
    }
}

fn build_config_rows(
    agent_display_name: &str,
    container_name: &str,
    repo_dir: &std::path::Path,
    git: &GitIdentity,
    image: &str,
    dind: &str,
) -> Vec<(String, String)> {
    let mut rows = vec![
        ("identity".to_string(), agent_display_name.to_string()),
        ("container".to_string(), container_name.to_string()),
        ("repository".to_string(), repo_dir.display().to_string()),
    ];

    if !git.user_name.is_empty() {
        rows.push((
            "operator".to_string(),
            if git.user_email.is_empty() {
                git.user_name.clone()
            } else {
                format!("{} <{}>", git.user_name, git.user_email)
            },
        ));
    }

    rows.push(("image".to_string(), image.to_string()));
    rows.push(("dind".to_string(), dind.to_string()));
    rows
}

pub fn load_agent(
    paths: &JackinPaths,
    config: &mut AppConfig,
    selector: &ClassSelector,
    runner: &mut impl CommandRunner,
    opts: &LoadOptions,
) -> anyhow::Result<()> {
    let git = load_git_identity();

    // Matrix intro
    if !opts.no_intro {
        let intro_name = if git.user_name.is_empty() {
            "Neo"
        } else {
            &git.user_name
        };
        tui::matrix_intro(intro_name);
    }

    let source = config.resolve_or_register(selector, paths)?;

    let cached_repo = CachedRepo::new(paths, selector);
    std::fs::create_dir_all(cached_repo.repo_dir.parent().unwrap())?;

    if cached_repo.repo_dir.exists() {
        runner.run(
            "git",
            &[
                "-C".into(),
                cached_repo.repo_dir.display().to_string(),
                "pull".into(),
                "--ff-only".into(),
            ],
            None,
        )?;
    } else {
        runner.run(
            "git",
            &[
                "clone".into(),
                source.git.clone(),
                cached_repo.repo_dir.display().to_string(),
            ],
            None,
        )?;
    }

    let validated_repo = validate_agent_repo(&cached_repo.repo_dir)?;
    let existing = list_managed_agent_names(runner)?;
    let container_name = next_container_name(selector, &existing);
    let state = AgentState::prepare(paths, &container_name, &validated_repo.manifest)?;
    let build = create_derived_build_context(&cached_repo.repo_dir, &validated_repo)?;

    let resolved_mounts = config.resolve_mounts(selector);
    let validated_mounts = AppConfig::validate_mounts(&resolved_mounts)?;

    let image = image_name(selector);
    let network = format!("{container_name}-net");
    let dind = format!("{container_name}-dind");

    let agent_display_name = validated_repo.manifest.display_name(&selector.name);

    // Set terminal title
    tui::set_terminal_title(&agent_display_name);

    // Logo (if present in agent repo)
    tui::print_logo(&cached_repo.repo_dir.join("logo.txt"));

    // Configuration summary
    let config_rows = build_config_rows(
        &agent_display_name,
        &container_name,
        &cached_repo.repo_dir,
        &git,
        &image,
        &dind,
    );
    eprintln!();
    tui::print_config_table(&config_rows);
    eprintln!();

    let mut step = 1u32;

    let mut cleanup = LoadCleanup::new(container_name.clone(), dind.clone(), network.clone());
    let load_result = (|| -> anyhow::Result<()> {
        // Step 1: Build Docker image
        tui::step_shimmer(step, "Building Docker image");
        step += 1;
        runner.run(
            "docker",
            &[
                "build".into(),
                "-t".into(),
                image.clone(),
                "-f".into(),
                build.dockerfile_path.display().to_string(),
                build.context_dir.display().to_string(),
            ],
            None,
        )?;

        // Step 2: Create Docker network
        tui::step_shimmer(step, "Creating Docker network");
        step += 1;
        runner.run(
            "docker",
            &["network".into(), "create".into(), network.clone()],
            None,
        )?;

        // Step 3: Start Docker-in-Docker
        tui::step_shimmer(step, "Starting Docker-in-Docker container");
        step += 1;
        runner.run(
            "docker",
            &[
                "run".into(),
                "-d".into(),
                "--name".into(),
                dind.clone(),
                "--network".into(),
                network.clone(),
                "--privileged".into(),
                "docker:dind".into(),
            ],
            None,
        )?;

        wait_for_dind(&dind, runner)?;

        // Step 4: Launch agent
        tui::step_shimmer(step, "Mounting volumes");

        tui::print_deploying(&agent_display_name);

        let mut run_args: Vec<String> = vec![
                "run".into(),
                "-it".into(),
                "--name".into(),
                container_name.clone(),
                "--hostname".into(),
                container_name.clone(),
                "--network".into(),
                network.clone(),
                "--label".into(),
                "jackin.managed=true".into(),
                "--label".into(),
                format!("jackin.class={}", selector.key()),
                "-e".into(),
                format!("DOCKER_HOST=tcp://{dind}:2375"),
                "-v".into(),
                format!("{}:/workspace", cached_repo.repo_dir.display()),
                "-v".into(),
                format!("{}:/home/claude/.claude", state.claude_dir.display()),
                "-v".into(),
                format!("{}:/home/claude/.claude.json", state.claude_json.display()),
                "-v".into(),
                format!("{}:/home/claude/.jackin/plugins.json:ro", state.plugins_json.display()),
        ];
        // User-configured mounts
        for mount in &validated_mounts {
            let suffix = if mount.readonly { ":ro" } else { "" };
            run_args.extend([
                "-v".into(),
                format!("{}:{}{}", mount.src, mount.dst, suffix),
            ]);
        }
        run_args.push(image.clone());
        runner.run("docker", &run_args, None)?;

        Ok(())
    })();

    match load_result {
        Ok(()) => {
            if list_running_agent_names(runner)?
                .iter()
                .any(|name| name == &container_name)
            {
                cleanup.disarm();
                Ok(())
            } else {
                cleanup.run(runner);
                render_exit(&agent_display_name, runner, opts);
                Ok(())
            }
        }
        Err(error) => {
            cleanup.run(runner);
            render_exit(&agent_display_name, runner, opts);
            Err(error)
        }
    }
}

fn render_exit(
    agent_display_name: &str,
    runner: &mut impl CommandRunner,
    opts: &LoadOptions,
) {
    tui::clear_screen();
    let remaining = list_running_agent_names(runner).unwrap_or_default();
    if !opts.no_intro {
        tui::matrix_outro(agent_display_name, &remaining);
    } else {
        tui::simple_outro(agent_display_name, &remaining);
    }
}

pub fn hardline_agent(
    container_name: &str,
    runner: &mut impl CommandRunner,
) -> anyhow::Result<()> {
    runner.run("docker", &["attach".into(), container_name.to_string()], None)
}

fn wait_for_dind(dind_name: &str, runner: &mut impl CommandRunner) -> anyhow::Result<()> {
    for _ in 0..30 {
        if runner
            .run(
                "docker",
                &[
                    "exec".into(),
                    dind_name.to_string(),
                    "docker".into(),
                    "info".into(),
                ],
                None,
            )
            .is_ok()
        {
            return Ok(());
        }
        std::thread::sleep(std::time::Duration::from_secs(1));
    }

    anyhow::bail!("timed out waiting for Docker-in-Docker sidecar {dind_name}")
}

pub fn list_running_agent_names(
    runner: &mut impl CommandRunner,
) -> anyhow::Result<Vec<String>> {
    list_agent_names(runner, false)
}

pub fn list_managed_agent_names(
    runner: &mut impl CommandRunner,
) -> anyhow::Result<Vec<String>> {
    list_agent_names(runner, true)
}

fn list_agent_names(
    runner: &mut impl CommandRunner,
    include_stopped: bool,
) -> anyhow::Result<Vec<String>> {
    let mut args = vec!["ps".into()];
    if include_stopped {
        args.push("-a".into());
    }
    args.extend([
        "--filter".into(),
        "label=jackin.managed=true".into(),
        "--format".into(),
        "{{.Names}}".into(),
    ]);

    let output = runner.capture(
        "docker",
        &args,
        None,
    )?;

    Ok(output
        .lines()
        .filter(|line| !line.is_empty())
        .map(|line| line.to_string())
        .collect())
}

pub fn matching_family(selector: &ClassSelector, names: &[String]) -> Vec<String> {
    names
        .iter()
        .filter(|name| crate::instance::class_family_matches(selector, name))
        .cloned()
        .collect()
}

pub fn purge_class_data(paths: &JackinPaths, selector: &ClassSelector) -> anyhow::Result<()> {
    if !paths.data_dir.exists() {
        return Ok(());
    }

    for entry in std::fs::read_dir(&paths.data_dir)? {
        let entry = entry?;
        let file_name = entry.file_name().to_string_lossy().to_string();
        if crate::instance::class_family_matches(selector, &file_name) {
            std::fs::remove_dir_all(entry.path())?;
        }
    }

    Ok(())
}

pub fn eject_agent(
    container_name: &str,
    runner: &mut impl CommandRunner,
) -> anyhow::Result<()> {
    let dind = format!("{container_name}-dind");
    let network = format!("{container_name}-net");

    run_cleanup_command(
        runner,
        &["rm".into(), "-f".into(), container_name.to_string()],
    )?;
    run_cleanup_command(runner, &["rm".into(), "-f".into(), dind])?;
    run_cleanup_command(runner, &["network".into(), "rm".into(), network])?;

    Ok(())
}

fn run_cleanup_command(runner: &mut impl CommandRunner, args: &[String]) -> anyhow::Result<()> {
    match runner.capture("docker", args, None) {
        Ok(_) => Ok(()),
        Err(error) if is_missing_cleanup_error(&error) => Ok(()),
        Err(error) => Err(error),
    }
}

fn is_missing_cleanup_error(error: &anyhow::Error) -> bool {
    let message = error.to_string();
    message.contains("No such container") || message.contains("No such network")
}

pub fn exile_all(runner: &mut impl CommandRunner) -> anyhow::Result<()> {
    let names = list_managed_agent_names(runner)?;
    for name in names {
        eject_agent(&name, runner)?;
    }
    Ok(())
}

fn image_name(selector: &ClassSelector) -> String {
    format!("jackin-{}", selector.key().replace('/', "-"))
}

struct LoadCleanup {
    container_name: String,
    dind: String,
    network: String,
    armed: bool,
}

impl LoadCleanup {
    fn new(container_name: String, dind: String, network: String) -> Self {
        Self {
            container_name,
            dind,
            network,
            armed: true,
        }
    }

    fn disarm(&mut self) {
        self.armed = false;
    }

    fn run(&self, runner: &mut impl CommandRunner) {
        if !self.armed {
            return;
        }

        let _ = runner.run(
            "docker",
            &["rm".into(), "-f".into(), self.container_name.clone()],
            None,
        );
        let _ = runner.run(
            "docker",
            &["rm".into(), "-f".into(), self.dind.clone()],
            None,
        );
        let _ = runner.run(
            "docker",
            &["network".into(), "rm".into(), self.network.clone()],
            None,
        );
    }
}

#[cfg(test)]
use std::collections::VecDeque;

#[cfg(test)]
#[derive(Default)]
pub struct FakeRunner {
    pub recorded: Vec<String>,
    pub fail_on: Vec<String>,
    pub fail_with: Vec<(String, String)>,
    pub capture_queue: VecDeque<String>,
}

#[cfg(test)]
impl FakeRunner {
    fn with_capture_queue<const N: usize>(outputs: [String; N]) -> Self {
        Self {
            capture_queue: VecDeque::from(outputs.to_vec()),
            ..Default::default()
        }
    }
}

#[cfg(test)]
impl CommandRunner for FakeRunner {
    fn run(
        &mut self,
        program: &str,
        args: &[String],
        _cwd: Option<&std::path::Path>,
    ) -> anyhow::Result<()> {
        let command = format!("{} {}", program, args.join(" "));
        self.recorded.push(command.clone());
        if let Some((_, message)) = self
            .fail_with
            .iter()
            .find(|(pattern, _)| command.contains(pattern))
        {
            anyhow::bail!(message.clone());
        }
        if self.fail_on.iter().any(|pattern| command.contains(pattern)) {
            anyhow::bail!("command failed: {command}");
        }
        Ok(())
    }

    fn capture(
        &mut self,
        program: &str,
        args: &[String],
        _cwd: Option<&std::path::Path>,
    ) -> anyhow::Result<String> {
        let command = format!("{} {}", program, args.join(" "));
        self.recorded.push(command.clone());
        if let Some((_, message)) = self
            .fail_with
            .iter()
            .find(|(pattern, _)| command.contains(pattern))
        {
            anyhow::bail!(message.clone());
        }
        if self.fail_on.iter().any(|pattern| command.contains(pattern)) {
            anyhow::bail!("command failed: {command}");
        }
        Ok(self.capture_queue.pop_front().unwrap_or_default())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::AppConfig;
    use crate::paths::JackinPaths;
    use crate::selector::ClassSelector;
    use tempfile::tempdir;

    #[test]
    fn load_owner_repo_registers_source_and_builds_commands() {
        let temp = tempdir().unwrap();
        let paths = JackinPaths::for_tests(temp.path());
        let mut config = AppConfig::load_or_init(&paths).unwrap();
        let selector = ClassSelector::new(Some("chainargos"), "the-architect");
        let mut runner = FakeRunner::with_capture_queue([
            String::new(),
            "jackin-chainargos-the-architect".to_string(),
        ]);

        let repo_dir = paths.agents_dir.join("chainargos").join("the-architect");
        std::fs::create_dir_all(&repo_dir).unwrap();
        std::fs::write(repo_dir.join("Dockerfile"), "FROM donbeave/jackin-construct:trixie\n").unwrap();
        std::fs::write(
            repo_dir.join("jackin.agent.toml"),
            "dockerfile = \"Dockerfile\"\n\n[claude]\nplugins = [\"code-review@claude-plugins-official\"]\n",
        )
        .unwrap();

        load_agent(&paths, &mut config, &selector, &mut runner, &LoadOptions::default()).unwrap();

        assert!(std::fs::read_to_string(&paths.config_file)
            .unwrap()
            .contains("chainargos/the-architect"));
        assert!(runner
            .recorded
            .iter()
            .any(|call| call.contains("git -C") || call.contains("git clone")));
        assert!(runner
            .recorded
            .iter()
            .any(|call| call.contains("docker build -t jackin-chainargos-the-architect -f")));
        assert!(runner.recorded.iter().any(|call| {
            call == "docker ps -a --filter label=jackin.managed=true --format {{.Names}}"
        }));
        assert!(runner
            .recorded
            .iter()
            .any(|call| call.contains("docker run -it --name jackin-chainargos-the-architect")));
        assert!(runner
            .recorded
            .iter()
            .any(|call| call.contains("/home/claude/.jackin/plugins.json:ro")));
        assert!(!runner
            .recorded
            .iter()
            .any(|call| call.contains("claude plugin install")));
    }

    #[test]
    fn eject_all_targets_only_requested_class_family() {
        let selector = ClassSelector::new(None, "agent-smith");
        let names = vec![
            "jackin-agent-smith".to_string(),
            "jackin-agent-smith-clone-1".to_string(),
            "jackin-chainargos-the-architect".to_string(),
        ];

        let matched = matching_family(&selector, &names);

        assert_eq!(matched, vec!["jackin-agent-smith", "jackin-agent-smith-clone-1"]);
    }

    #[test]
    fn purge_all_removes_matching_state_directories() {
        let temp = tempdir().unwrap();
        let paths = JackinPaths::for_tests(temp.path());
        std::fs::create_dir_all(paths.data_dir.join("jackin-agent-smith")).unwrap();
        std::fs::create_dir_all(paths.data_dir.join("jackin-agent-smith-clone-1")).unwrap();
        std::fs::create_dir_all(paths.data_dir.join("jackin-chainargos-the-architect")).unwrap();
        let selector = ClassSelector::new(None, "agent-smith");

        purge_class_data(&paths, &selector).unwrap();

        assert!(!paths.data_dir.join("jackin-agent-smith").exists());
        assert!(!paths.data_dir.join("jackin-agent-smith-clone-1").exists());
        assert!(paths.data_dir.join("jackin-chainargos-the-architect").exists());
    }

    #[test]
    fn eject_agent_removes_container_dind_and_network() {
        let mut runner = FakeRunner::default();

        eject_agent("jackin-agent-smith", &mut runner).unwrap();

        assert_eq!(runner.recorded, vec![
            "docker rm -f jackin-agent-smith",
            "docker rm -f jackin-agent-smith-dind",
            "docker network rm jackin-agent-smith-net",
        ]);
    }

    #[test]
    fn eject_agent_ignores_missing_runtime_resources() {
        let mut runner = FakeRunner {
            fail_with: vec![
                (
                    "docker rm -f jackin-agent-smith".to_string(),
                    "Error response from daemon: No such container: jackin-agent-smith".to_string(),
                ),
                (
                    "docker rm -f jackin-agent-smith-dind".to_string(),
                    "Error response from daemon: No such container: jackin-agent-smith-dind".to_string(),
                ),
                (
                    "docker network rm jackin-agent-smith-net".to_string(),
                    "Error response from daemon: No such network: jackin-agent-smith-net"
                        .to_string(),
                ),
            ],
            ..Default::default()
        };

        eject_agent("jackin-agent-smith", &mut runner).unwrap();

        assert_eq!(runner.recorded, vec![
            "docker rm -f jackin-agent-smith",
            "docker rm -f jackin-agent-smith-dind",
            "docker network rm jackin-agent-smith-net",
        ]);
    }

    #[test]
    fn exile_all_ejects_all_managed_agents() {
        let mut runner = FakeRunner::with_capture_queue(["jackin-agent-smith\njackin-agent-smith-clone-1".to_string()]);

        exile_all(&mut runner).unwrap();

        assert_eq!(
            runner.recorded,
            vec![
                "docker ps -a --filter label=jackin.managed=true --format {{.Names}}",
                "docker rm -f jackin-agent-smith",
                "docker rm -f jackin-agent-smith-dind",
                "docker network rm jackin-agent-smith-net",
                "docker rm -f jackin-agent-smith-clone-1",
                "docker rm -f jackin-agent-smith-clone-1-dind",
                "docker network rm jackin-agent-smith-clone-1-net",
            ]
        );
    }

    #[test]
    fn exile_all_continues_when_some_runtime_resources_are_missing() {
        let mut runner = FakeRunner {
            fail_with: vec![
                (
                    "docker rm -f jackin-agent-smith".to_string(),
                    "Error response from daemon: No such container: jackin-agent-smith".to_string(),
                ),
                (
                    "docker network rm jackin-agent-smith-net".to_string(),
                    "Error response from daemon: No such network: jackin-agent-smith-net"
                        .to_string(),
                ),
            ],
            capture_queue: VecDeque::from(vec!["jackin-agent-smith\njackin-agent-smith-clone-1".to_string()]),
            ..Default::default()
        };

        exile_all(&mut runner).unwrap();

        assert_eq!(
            runner.recorded,
            vec![
                "docker ps -a --filter label=jackin.managed=true --format {{.Names}}",
                "docker rm -f jackin-agent-smith",
                "docker rm -f jackin-agent-smith-dind",
                "docker network rm jackin-agent-smith-net",
                "docker rm -f jackin-agent-smith-clone-1",
                "docker rm -f jackin-agent-smith-clone-1-dind",
                "docker network rm jackin-agent-smith-clone-1-net",
            ]
        );
    }

    #[test]
    fn load_agent_injects_configured_mounts() {
        let temp = tempdir().unwrap();
        let paths = JackinPaths::for_tests(temp.path());
        let selector = ClassSelector::new(Some("chainargos"), "agent-brown");
        let mut runner = FakeRunner::with_capture_queue([
            String::new(),
            "jackin-chainargos-agent-brown".to_string(),
        ]);

        let repo_dir = paths.agents_dir.join("chainargos").join("agent-brown");
        std::fs::create_dir_all(&repo_dir).unwrap();
        std::fs::write(repo_dir.join("Dockerfile"), "FROM donbeave/jackin-construct:trixie\n").unwrap();
        std::fs::write(
            repo_dir.join("jackin.agent.toml"),
            "dockerfile = \"Dockerfile\"\n\n[claude]\nplugins = []\n",
        )
        .unwrap();

        // Create a mount source directory
        let mount_src = temp.path().join("test-mount");
        std::fs::create_dir_all(&mount_src).unwrap();
        std::fs::create_dir_all(&paths.config_dir).unwrap();

        // Write config with a scoped mount
        let config_content = format!(
            r#"[agents."chainargos/agent-brown"]
git = "git@github.com:chainargos/jackin-agent-brown.git"

[docker.mounts."chainargos/*"]
test-mount = {{ src = "{}", dst = "/test-data", readonly = true }}
"#,
            mount_src.display()
        );
        std::fs::write(&paths.config_file, &config_content).unwrap();
        let mut config = AppConfig::load_or_init(&paths).unwrap();

        load_agent(&paths, &mut config, &selector, &mut runner, &LoadOptions::default()).unwrap();

        let run_cmd = runner
            .recorded
            .iter()
            .find(|call| call.contains("docker run -it"))
            .unwrap();
        assert!(run_cmd.contains(&format!("{}:/test-data:ro", mount_src.display())));
    }

    #[test]
    fn hardline_uses_docker_attach() {
        let mut runner = FakeRunner::default();

        hardline_agent("jackin-agent-smith", &mut runner).unwrap();

        assert_eq!(runner.recorded.last().unwrap(), "docker attach jackin-agent-smith");
    }

    #[test]
    fn load_agent_runs_attached_with_plugins_mount() {
        let temp = tempdir().unwrap();
        let paths = JackinPaths::for_tests(temp.path());
        let mut config = AppConfig::load_or_init(&paths).unwrap();
        let selector = ClassSelector::new(None, "agent-smith");
        let mut runner = FakeRunner::with_capture_queue([
            String::new(),
            "jackin-agent-smith".to_string(),
        ]);

        let repo_dir = paths.agents_dir.join("agent-smith");
        std::fs::create_dir_all(&repo_dir).unwrap();
        std::fs::write(repo_dir.join("Dockerfile"), "FROM donbeave/jackin-construct:trixie\n").unwrap();
        std::fs::write(
            repo_dir.join("jackin.agent.toml"),
            "dockerfile = \"Dockerfile\"\n\n[claude]\nplugins = [\"code-review@claude-plugins-official\"]\n",
        )
        .unwrap();

        load_agent(&paths, &mut config, &selector, &mut runner, &LoadOptions::default()).unwrap();

        assert!(runner
            .recorded
            .iter()
            .any(|call| call.contains("docker build -t jackin-agent-smith -f")));
        assert!(runner.recorded.iter().any(|call| {
            call == "docker ps -a --filter label=jackin.managed=true --format {{.Names}}"
        }));
        assert!(runner
            .recorded
            .iter()
            .any(|call| call.contains("docker run -it --name jackin-agent-smith")));
        assert!(runner
            .recorded
            .iter()
            .any(|call| call.contains("/home/claude/.jackin/plugins.json:ro")));
        assert!(!runner.recorded.iter().any(|call| call == "docker rm -f jackin-agent-smith"));
        assert!(!runner
            .recorded
            .iter()
            .any(|call| call.contains("claude plugin install")));
    }

    #[test]
    fn load_agent_rolls_back_runtime_on_attached_run_failure() {
        let temp = tempdir().unwrap();
        let paths = JackinPaths::for_tests(temp.path());
        let mut config = AppConfig::load_or_init(&paths).unwrap();
        let selector = ClassSelector::new(None, "agent-smith");
        let mut runner = FakeRunner {
            fail_on: vec!["docker run -it --name jackin-agent-smith".to_string()],
            capture_queue: VecDeque::from(vec![String::new()]),
            ..Default::default()
        };

        let repo_dir = paths.agents_dir.join("agent-smith");
        std::fs::create_dir_all(&repo_dir).unwrap();
        std::fs::write(repo_dir.join("Dockerfile"), "FROM donbeave/jackin-construct:trixie\n").unwrap();
        std::fs::write(
            repo_dir.join("jackin.agent.toml"),
            "dockerfile = \"Dockerfile\"\n\n[claude]\nplugins = [\"code-review@claude-plugins-official\"]\n",
        )
        .unwrap();

        let error = load_agent(&paths, &mut config, &selector, &mut runner, &LoadOptions::default()).unwrap_err();

        assert!(error.to_string().contains("docker run -it --name jackin-agent-smith"));
        assert!(runner.recorded.iter().any(|call| call == "docker rm -f jackin-agent-smith"));
        assert!(runner.recorded.iter().any(|call| call == "docker rm -f jackin-agent-smith-dind"));
        assert!(runner
            .recorded
            .iter()
            .any(|call| call == "docker network rm jackin-agent-smith-net"));
    }
}
