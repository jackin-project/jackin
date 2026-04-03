use crate::config::AppConfig;
use crate::derived_image::create_derived_build_context;
use crate::docker::CommandRunner;
use crate::instance::{AgentState, next_container_name};
use crate::paths::JackinPaths;
use crate::repo::{CachedRepo, validate_agent_repo};
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

impl LoadOptions {
    fn step(&self, n: u32, text: &str) {
        if self.no_intro {
            tui::step_quiet(n, text);
        } else {
            tui::step_shimmer(n, text);
        }
    }
}

struct GitIdentity {
    user_name: String,
    user_email: String,
}

struct HostIdentity {
    uid: String,
    gid: String,
}

/// Run a command and return its trimmed stdout, or `None` on failure.
fn capture_stdout(program: &str, args: &[&str]) -> Option<String> {
    let output = std::process::Command::new(program)
        .args(args)
        .output()
        .ok()
        .filter(|o| o.status.success())?;
    let value = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if value.is_empty() { None } else { Some(value) }
}

fn load_git_identity() -> GitIdentity {
    GitIdentity {
        user_name: capture_stdout("git", &["config", "user.name"]).unwrap_or_default(),
        user_email: capture_stdout("git", &["config", "user.email"]).unwrap_or_default(),
    }
}

#[cfg(unix)]
fn load_host_identity() -> HostIdentity {
    HostIdentity {
        uid: capture_stdout("id", &["-u"]).unwrap_or_else(|| "1000".to_string()),
        gid: capture_stdout("id", &["-g"]).unwrap_or_else(|| "1000".to_string()),
    }
}

#[cfg(not(unix))]
fn load_host_identity() -> HostIdentity {
    HostIdentity {
        uid: "1000".to_string(),
        gid: "1000".to_string(),
    }
}

/// Extract `owner/repo` from a git remote URL.
fn parse_repo_name(url: &str) -> Option<String> {
    let url = url.trim();
    let stripped = url.strip_suffix(".git").unwrap_or(url);
    // HTTPS: https://github.com/owner/repo
    if let Some(rest) = stripped
        .strip_prefix("https://")
        .or_else(|| stripped.strip_prefix("http://"))
    {
        return rest.find('/').map(|i| rest[i + 1..].to_string());
    }
    // SSH: git@github.com:owner/repo
    stripped
        .rsplit_once(':')
        .map(|(_, p)| p.to_string())
}

/// Derive a short repository name from a git remote URL (e.g. `donbeave/jackin`).
fn git_repo_name(dir: &std::path::Path) -> Option<String> {
    let dir_str = dir.display().to_string();
    let url = capture_stdout("git", &["-C", &dir_str, "remote", "get-url", "origin"])?;
    parse_repo_name(&url)
}

/// Get the current branch name for a git directory.
fn git_branch(dir: &std::path::Path) -> Option<String> {
    let dir_str = dir.display().to_string();
    capture_stdout("git", &["-C", &dir_str, "rev-parse", "--abbrev-ref", "HEAD"])
}

/// Check whether a path is inside a git work tree.
fn is_git_dir(dir: &std::path::Path) -> bool {
    let dir_str = dir.display().to_string();
    capture_stdout("git", &["-C", &dir_str, "rev-parse", "--is-inside-work-tree"]).is_some()
}

fn build_config_rows(
    agent_display_name: &str,
    container_name: &str,
    workspace: &crate::workspace::ResolvedWorkspace,
    git: &GitIdentity,
    image: &str,
) -> Vec<(String, String)> {
    let mut rows = vec![
        ("identity".to_string(), agent_display_name.to_string()),
        ("container".to_string(), container_name.to_string()),
    ];

    // Show repository/branch for git directories, or workspace name for saved workspaces
    let workdir = std::path::Path::new(&workspace.label);
    if workdir.is_absolute() && is_git_dir(workdir) {
        if let Some(repo_name) = git_repo_name(workdir) {
            rows.push(("repository".to_string(), repo_name));
        }
        if let Some(branch) = git_branch(workdir) {
            rows.push(("branch".to_string(), branch));
        }
    } else {
        rows.push(("workspace".to_string(), workspace.label.clone()));
    }

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
    rows
}

#[allow(clippy::too_many_lines)]
pub fn load_agent(
    paths: &JackinPaths,
    config: &mut AppConfig,
    selector: &ClassSelector,
    workspace: &crate::workspace::ResolvedWorkspace,
    runner: &mut impl CommandRunner,
    opts: &LoadOptions,
) -> anyhow::Result<()> {
    let git = load_git_identity();
    let host = load_host_identity();

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

    let mut step = 1u32;

    // Step 1: Resolve agent identity (clone or update repo)
    let cached_repo = CachedRepo::new(paths, selector);
    let repo_parent = cached_repo
        .repo_dir
        .parent()
        .ok_or_else(|| anyhow::anyhow!("agent repo path has no parent: {}", cached_repo.repo_dir.display()))?;
    std::fs::create_dir_all(repo_parent)?;

    opts.step(step, "Resolving agent identity");
    step += 1;

    if cached_repo.repo_dir.exists() {
        runner.capture(
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
        runner.capture(
            "git",
            &[
                "clone".into(),
                source.git,
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
        workspace,
        &git,
        &image,
    );
    eprintln!();
    tui::print_config_table(&config_rows);
    eprintln!();

    let mut cleanup = LoadCleanup::new(container_name.clone(), dind.clone(), network.clone());
    let load_result = (|| -> anyhow::Result<()> {
        // Step 2: Build Docker image
        opts.step(step, "Building Docker image");
        step += 1;
        let build_args = [
            "build".into(),
            "--build-arg".into(),
            format!("JACKIN_HOST_UID={}", host.uid),
            "--build-arg".into(),
            format!("JACKIN_HOST_GID={}", host.gid),
            "-t".into(),
            image.clone(),
            "-f".into(),
            build.dockerfile_path.display().to_string(),
            build.context_dir.display().to_string(),
        ];
        runner.run("docker", &build_args, None)?;

        // Step 3: Create Docker network
        opts.step(step, "Creating Docker network");
        step += 1;
        let network_args = ["network".into(), "create".into(), network.clone()];
        if opts.debug {
            runner.run("docker", &network_args, None)?;
        } else {
            runner.capture("docker", &network_args, None)?;
        }

        // Step 4: Start Docker-in-Docker
        opts.step(step, "Starting Docker-in-Docker container");
        step += 1;
        let dind_args = [
            "run".into(),
            "-d".into(),
            "--name".into(),
            dind.clone(),
            "--network".into(),
            network.clone(),
            "--privileged".into(),
            "-e".into(),
            "DOCKER_TLS_CERTDIR=".into(),
            "docker:dind".into(),
        ];
        if opts.debug {
            runner.run("docker", &dind_args, None)?;
        } else {
            runner.capture("docker", &dind_args, None)?;
        }

        wait_for_dind(&dind, runner, opts.debug)?;

        // Step 5: Launch agent
        opts.step(step, "Mounting volumes");

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
            "--label".into(),
            format!("jackin.display_name={agent_display_name}"),
            "--workdir".into(),
            workspace.workdir.clone(),
            "-e".into(),
            format!("DOCKER_HOST=tcp://{dind}:2375"),
            "-v".into(),
            format!("{}:/home/claude/.claude", state.claude_dir.display()),
            "-v".into(),
            format!("{}:/home/claude/.claude.json", state.claude_json.display()),
            "-v".into(),
            format!(
                "{}:/home/claude/.jackin/plugins.json:ro",
                state.plugins_json.display()
            ),
        ];
        for mount in &workspace.mounts {
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
            } else {
                cleanup.run(runner);
                render_exit(&agent_display_name, runner, opts);
            }
            Ok(())
        }
        Err(error) => {
            cleanup.run(runner);
            render_exit(&agent_display_name, runner, opts);
            Err(error)
        }
    }
}

fn render_exit(agent_display_name: &str, runner: &mut impl CommandRunner, opts: &LoadOptions) {
    tui::clear_screen();
    let remaining = list_running_agent_display_names(runner).unwrap_or_default();
    if opts.no_intro {
        tui::simple_outro(agent_display_name, &remaining);
    } else {
        tui::matrix_outro(agent_display_name, &remaining);
    }
}

pub fn hardline_agent(container_name: &str, runner: &mut impl CommandRunner) -> anyhow::Result<()> {
    runner.run(
        "docker",
        &["attach".into(), container_name.to_string()],
        None,
    )
}

fn wait_for_dind(
    dind_name: &str,
    runner: &mut impl CommandRunner,
    debug: bool,
) -> anyhow::Result<()> {
    tui::spin_wait(
        "Waiting for Docker-in-Docker to be ready",
        30,
        std::time::Duration::from_secs(1),
        || {
            let result = runner.capture(
                "docker",
                &[
                    "exec".into(),
                    dind_name.to_string(),
                    "docker".into(),
                    "info".into(),
                ],
                None,
            );
            if debug && let Err(ref e) = result {
                eprintln!("  DinD not ready: {e}");
            }
            result.map(|_| ())
        },
    )
    .map_err(|_| anyhow::anyhow!("timed out waiting for Docker-in-Docker sidecar {dind_name}"))
}

pub fn list_running_agent_names(runner: &mut impl CommandRunner) -> anyhow::Result<Vec<String>> {
    list_agent_names(runner, false)
}

pub fn list_managed_agent_names(runner: &mut impl CommandRunner) -> anyhow::Result<Vec<String>> {
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

    let output = runner.capture("docker", &args, None)?;

    Ok(output
        .lines()
        .filter(|line| !line.is_empty())
        .map(String::from)
        .collect())
}

/// List running agents with human-friendly display names.
///
/// Returns display names like "The Architect" or "The Architect (Clone 2)".
/// Falls back to the raw container name if no display label is present.
pub fn list_running_agent_display_names(
    runner: &mut impl CommandRunner,
) -> anyhow::Result<Vec<String>> {
    let output = runner.capture(
        "docker",
        &[
            "ps".into(),
            "--filter".into(),
            "label=jackin.managed=true".into(),
            "--format".into(),
            "{{.Names}}\t{{.Label \"jackin.display_name\"}}".into(),
        ],
        None,
    )?;

    Ok(output
        .lines()
        .filter(|line| !line.is_empty())
        .map(|line| {
            let parts: Vec<&str> = line.splitn(2, '\t').collect();
            let container_name = parts[0];
            let display_name = parts.get(1).unwrap_or(&"");
            format_agent_display(container_name, display_name)
        })
        .collect())
}

/// Format a human-friendly agent name from a container name and its display label.
///
/// Examples:
///   - `("jackin-the-architect", "The Architect")` → `"The Architect"`
///   - `("jackin-the-architect-clone-2", "The Architect")` → `"The Architect (Clone 2)"`
///   - `("jackin-the-architect", "")` → `"jackin-the-architect"`
fn format_agent_display(container_name: &str, display_name: &str) -> String {
    if display_name.is_empty() {
        return container_name.to_string();
    }

    container_name.rsplit_once("-clone-").map_or_else(
        || display_name.to_string(),
        |suffix| format!("{display_name} (Clone {})", suffix.1),
    )
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

pub fn eject_agent(container_name: &str, runner: &mut impl CommandRunner) -> anyhow::Result<()> {
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
    const fn new(container_name: String, dind: String, network: String) -> Self {
        Self {
            container_name,
            dind,
            network,
            armed: true,
        }
    }

    const fn disarm(&mut self) {
        self.armed = false;
    }

    fn run(&self, runner: &mut impl CommandRunner) {
        if !self.armed {
            return;
        }

        if let Err(e) = run_cleanup_command(
            runner,
            &["rm".into(), "-f".into(), self.container_name.clone()],
        ) {
            tui::step_fail(&format!("cleanup failed (container): {e}"));
        }
        if let Err(e) = run_cleanup_command(
            runner,
            &["rm".into(), "-f".into(), self.dind.clone()],
        ) {
            tui::step_fail(&format!("cleanup failed (dind): {e}"));
        }
        if let Err(e) = run_cleanup_command(
            runner,
            &["network".into(), "rm".into(), self.network.clone()],
        ) {
            tui::step_fail(&format!("cleanup failed (network): {e}"));
        }
    }
}

#[cfg(test)]
use std::collections::VecDeque;

#[cfg(test)]
#[derive(Default)]
pub struct FakeRunner {
    pub recorded: Vec<String>,
    pub run_recorded: Vec<String>,
    pub fail_on: Vec<String>,
    pub fail_with: Vec<(String, String)>,
    pub capture_queue: VecDeque<String>,
}

#[cfg(test)]
impl FakeRunner {
    fn with_capture_queue<const N: usize>(outputs: [String; N]) -> Self {
        Self {
            capture_queue: VecDeque::from(outputs),
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
        self.run_recorded.push(command.clone());
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

    fn repo_workspace(repo_dir: &std::path::Path) -> crate::workspace::ResolvedWorkspace {
        crate::workspace::ResolvedWorkspace {
            label: repo_dir.display().to_string(),
            workdir: "/workspace".to_string(),
            mounts: vec![crate::workspace::MountConfig {
                src: repo_dir.display().to_string(),
                dst: "/workspace".to_string(),
                readonly: false,
            }],
        }
    }

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
        std::fs::write(
            repo_dir.join("Dockerfile"),
            "FROM donbeave/jackin-construct:trixie\n",
        )
        .unwrap();
        std::fs::write(
            repo_dir.join("jackin.agent.toml"),
            "dockerfile = \"Dockerfile\"\n\n[claude]\nplugins = [\"code-review@claude-plugins-official\"]\n",
        )
        .unwrap();

        let workspace = repo_workspace(&repo_dir);
        load_agent(
            &paths,
            &mut config,
            &selector,
            &workspace,
            &mut runner,
            &LoadOptions::default(),
        )
        .unwrap();

        assert!(
            std::fs::read_to_string(&paths.config_file)
                .unwrap()
                .contains("chainargos/the-architect")
        );
        assert!(
            runner
                .recorded
                .iter()
                .any(|call| call.contains("git -C") || call.contains("git clone"))
        );
        assert!(runner.recorded.iter().any(|call| {
            call.contains("docker build ") && call.contains("-t jackin-chainargos-the-architect")
        }));
        assert!(runner.recorded.iter().any(|call| {
            call == "docker ps -a --filter label=jackin.managed=true --format {{.Names}}"
        }));
        assert!(
            runner
                .recorded
                .iter()
                .any(|call| call.contains("docker run -it --name jackin-chainargos-the-architect"))
        );
        assert!(
            runner
                .recorded
                .iter()
                .any(|call| call.contains("/home/claude/.jackin/plugins.json:ro"))
        );
        assert!(
            !runner
                .recorded
                .iter()
                .any(|call| call.contains("claude plugin install"))
        );
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

        assert_eq!(
            matched,
            vec!["jackin-agent-smith", "jackin-agent-smith-clone-1"]
        );
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
        assert!(
            paths
                .data_dir
                .join("jackin-chainargos-the-architect")
                .exists()
        );
    }

    #[test]
    fn eject_agent_removes_container_dind_and_network() {
        let mut runner = FakeRunner::default();

        eject_agent("jackin-agent-smith", &mut runner).unwrap();

        assert_eq!(
            runner.recorded,
            vec![
                "docker rm -f jackin-agent-smith",
                "docker rm -f jackin-agent-smith-dind",
                "docker network rm jackin-agent-smith-net",
            ]
        );
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
                    "Error response from daemon: No such container: jackin-agent-smith-dind"
                        .to_string(),
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

        assert_eq!(
            runner.recorded,
            vec![
                "docker rm -f jackin-agent-smith",
                "docker rm -f jackin-agent-smith-dind",
                "docker network rm jackin-agent-smith-net",
            ]
        );
    }

    #[test]
    fn exile_all_ejects_all_managed_agents() {
        let mut runner = FakeRunner::with_capture_queue([
            "jackin-agent-smith\njackin-agent-smith-clone-1".to_string(),
        ]);

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
            capture_queue: VecDeque::from(vec![
                "jackin-agent-smith\njackin-agent-smith-clone-1".to_string(),
            ]),
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
        std::fs::write(
            repo_dir.join("Dockerfile"),
            "FROM donbeave/jackin-construct:trixie\n",
        )
        .unwrap();
        std::fs::write(
            repo_dir.join("jackin.agent.toml"),
            "dockerfile = \"Dockerfile\"\n\n[claude]\nplugins = []\n",
        )
        .unwrap();

        let mount_src = temp.path().join("test-mount");
        std::fs::create_dir_all(&mount_src).unwrap();
        std::fs::create_dir_all(&paths.config_dir).unwrap();

        let config_content = r#"[agents."chainargos/agent-brown"]
git = "git@github.com:chainargos/jackin-agent-brown.git"
"#;
        std::fs::write(&paths.config_file, config_content).unwrap();
        let mut config = AppConfig::load_or_init(&paths).unwrap();

        let workspace = crate::workspace::ResolvedWorkspace {
            label: "/workspace".to_string(),
            workdir: "/workspace".to_string(),
            mounts: vec![
                crate::workspace::MountConfig {
                    src: repo_dir.display().to_string(),
                    dst: "/workspace".to_string(),
                    readonly: false,
                },
                crate::workspace::MountConfig {
                    src: mount_src.display().to_string(),
                    dst: "/test-data".to_string(),
                    readonly: true,
                },
            ],
        };

        load_agent(
            &paths,
            &mut config,
            &selector,
            &workspace,
            &mut runner,
            &LoadOptions::default(),
        )
        .unwrap();

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

        assert_eq!(
            runner.recorded.last().unwrap(),
            "docker attach jackin-agent-smith"
        );
    }

    #[test]
    fn load_agent_runs_attached_with_plugins_mount() {
        let temp = tempdir().unwrap();
        let paths = JackinPaths::for_tests(temp.path());
        let mut config = AppConfig::load_or_init(&paths).unwrap();
        let selector = ClassSelector::new(None, "agent-smith");
        let mut runner =
            FakeRunner::with_capture_queue([String::new(), "jackin-agent-smith".to_string()]);

        let repo_dir = paths.agents_dir.join("agent-smith");
        std::fs::create_dir_all(&repo_dir).unwrap();
        std::fs::write(
            repo_dir.join("Dockerfile"),
            "FROM donbeave/jackin-construct:trixie\n",
        )
        .unwrap();
        std::fs::write(
            repo_dir.join("jackin.agent.toml"),
            "dockerfile = \"Dockerfile\"\n\n[claude]\nplugins = [\"code-review@claude-plugins-official\"]\n",
        )
        .unwrap();

        let workspace = repo_workspace(&repo_dir);
        load_agent(
            &paths,
            &mut config,
            &selector,
            &workspace,
            &mut runner,
            &LoadOptions::default(),
        )
        .unwrap();

        assert!(
            runner.recorded.iter().any(
                |call| call.contains("docker build ") && call.contains("-t jackin-agent-smith")
            )
        );
        // Docker build always streams output via run (not capture)
        assert!(
            runner
                .run_recorded
                .iter()
                .any(|call| call.contains("docker build "))
        );
        assert!(runner.recorded.iter().any(|call| {
            call == "docker ps -a --filter label=jackin.managed=true --format {{.Names}}"
        }));
        assert!(
            runner
                .recorded
                .iter()
                .any(|call| call.contains("docker run -it --name jackin-agent-smith"))
        );
        assert!(
            runner
                .recorded
                .iter()
                .any(|call| call.contains("/home/claude/.jackin/plugins.json:ro"))
        );
        assert!(
            !runner
                .recorded
                .iter()
                .any(|call| call == "docker rm -f jackin-agent-smith")
        );
        assert!(
            !runner
                .recorded
                .iter()
                .any(|call| call.contains("claude plugin install"))
        );
    }

    #[test]
    fn load_agent_uses_resolved_workspace_mounts_and_workdir() {
        let temp = tempfile::tempdir().unwrap();
        let paths = JackinPaths::for_tests(temp.path());
        let mut config = AppConfig::load_or_init(&paths).unwrap();
        let selector = ClassSelector::new(None, "agent-smith");
        let mut runner = FakeRunner::with_capture_queue([
            String::new(),
            String::new(),
            String::new(),
            String::new(),
            String::new(),
            "jackin-agent-smith".to_string(),
        ]);

        let repo_dir = paths.agents_dir.join("agent-smith");
        std::fs::create_dir_all(&repo_dir).unwrap();
        std::fs::write(
            repo_dir.join("Dockerfile"),
            "FROM donbeave/jackin-construct:trixie\n",
        )
        .unwrap();
        std::fs::write(
            repo_dir.join("jackin.agent.toml"),
            "dockerfile = \"Dockerfile\"\n\n[claude]\nplugins = []\n",
        )
        .unwrap();

        let workspace_dir = temp.path().join("workspace");
        std::fs::create_dir_all(&workspace_dir).unwrap();
        let workspace = crate::workspace::ResolvedWorkspace {
            label: workspace_dir.display().to_string(),
            workdir: workspace_dir.display().to_string(),
            mounts: vec![crate::workspace::MountConfig {
                src: workspace_dir.display().to_string(),
                dst: workspace_dir.display().to_string(),
                readonly: false,
            }],
        };

        load_agent(
            &paths,
            &mut config,
            &selector,
            &workspace,
            &mut runner,
            &LoadOptions::default(),
        )
        .unwrap();

        let run_call = runner
            .recorded
            .iter()
            .find(|call| call.contains("docker run -it"))
            .unwrap();
        assert!(run_call.contains(&format!("--workdir {}", workspace.workdir)));
        assert!(run_call.contains(&format!(
            "{}:{}",
            workspace_dir.display(),
            workspace_dir.display()
        )));
        assert!(!run_call.contains(&format!("{}:/workspace", repo_dir.display())));
    }

    #[test]
    fn load_agent_passes_host_uid_and_gid_to_docker_build() {
        let temp = tempfile::tempdir().unwrap();
        let paths = JackinPaths::for_tests(temp.path());
        let mut config = AppConfig::load_or_init(&paths).unwrap();
        let selector = ClassSelector::new(None, "agent-smith");
        let mut runner = FakeRunner::with_capture_queue([
            String::new(),
            String::new(),
            String::new(),
            String::new(),
            String::new(),
            "jackin-agent-smith".to_string(),
        ]);

        let repo_dir = paths.agents_dir.join("agent-smith");
        std::fs::create_dir_all(&repo_dir).unwrap();
        std::fs::write(
            repo_dir.join("Dockerfile"),
            "FROM donbeave/jackin-construct:trixie\n",
        )
        .unwrap();
        std::fs::write(
            repo_dir.join("jackin.agent.toml"),
            "dockerfile = \"Dockerfile\"\n\n[claude]\nplugins = []\n",
        )
        .unwrap();

        let workspace_dir = temp.path().join("workspace");
        std::fs::create_dir_all(&workspace_dir).unwrap();
        let workspace = crate::workspace::ResolvedWorkspace {
            label: workspace_dir.display().to_string(),
            workdir: workspace_dir.display().to_string(),
            mounts: vec![crate::workspace::MountConfig {
                src: workspace_dir.display().to_string(),
                dst: workspace_dir.display().to_string(),
                readonly: false,
            }],
        };

        load_agent(
            &paths,
            &mut config,
            &selector,
            &workspace,
            &mut runner,
            &LoadOptions::default(),
        )
        .unwrap();

        let build_call = runner
            .recorded
            .iter()
            .find(|call| call.contains("docker build ") && call.contains("-t jackin-agent-smith"))
            .unwrap();
        assert!(build_call.contains("--build-arg JACKIN_HOST_UID="));
        assert!(build_call.contains("--build-arg JACKIN_HOST_GID="));

        // Docker build always streams output via run (not capture)
        assert!(
            runner
                .run_recorded
                .iter()
                .any(|call| call.contains("docker build "))
        );
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
        std::fs::write(
            repo_dir.join("Dockerfile"),
            "FROM donbeave/jackin-construct:trixie\n",
        )
        .unwrap();
        std::fs::write(
            repo_dir.join("jackin.agent.toml"),
            "dockerfile = \"Dockerfile\"\n\n[claude]\nplugins = [\"code-review@claude-plugins-official\"]\n",
        )
        .unwrap();

        let workspace = repo_workspace(&repo_dir);
        let error = load_agent(
            &paths,
            &mut config,
            &selector,
            &workspace,
            &mut runner,
            &LoadOptions::default(),
        )
        .unwrap_err();

        assert!(
            error
                .to_string()
                .contains("docker run -it --name jackin-agent-smith")
        );
        assert!(
            runner
                .recorded
                .iter()
                .any(|call| call == "docker rm -f jackin-agent-smith")
        );
        assert!(
            runner
                .recorded
                .iter()
                .any(|call| call == "docker rm -f jackin-agent-smith-dind")
        );
        assert!(
            runner
                .recorded
                .iter()
                .any(|call| call == "docker network rm jackin-agent-smith-net")
        );
    }

    #[test]
    fn load_agent_checks_dind_readiness() {
        let temp = tempdir().unwrap();
        let paths = JackinPaths::for_tests(temp.path());
        let mut config = AppConfig::load_or_init(&paths).unwrap();
        let selector = ClassSelector::new(None, "agent-smith");
        let mut runner = FakeRunner::with_capture_queue([
            String::new(),
            String::new(),
            String::new(),
            String::new(),
            String::new(),
            "jackin-agent-smith".to_string(),
        ]);

        let repo_dir = paths.agents_dir.join("agent-smith");
        std::fs::create_dir_all(&repo_dir).unwrap();
        std::fs::write(
            repo_dir.join("Dockerfile"),
            "FROM donbeave/jackin-construct:trixie\n",
        )
        .unwrap();
        std::fs::write(
            repo_dir.join("jackin.agent.toml"),
            "dockerfile = \"Dockerfile\"\n\n[claude]\nplugins = []\n",
        )
        .unwrap();

        let workspace = repo_workspace(&repo_dir);
        load_agent(
            &paths,
            &mut config,
            &selector,
            &workspace,
            &mut runner,
            &LoadOptions::default(),
        )
        .unwrap();

        // DinD readiness check polls via docker exec
        assert!(
            runner
                .recorded
                .iter()
                .any(|call| call.contains("docker exec jackin-agent-smith-dind docker info"))
        );

        // DinD container is started before the readiness check
        let dind_start = runner
            .recorded
            .iter()
            .position(|call| call.contains("docker run -d --name jackin-agent-smith-dind"))
            .unwrap();
        let dind_check = runner
            .recorded
            .iter()
            .position(|call| call.contains("docker exec jackin-agent-smith-dind docker info"))
            .unwrap();
        assert!(dind_start < dind_check);
    }

    #[test]
    fn parse_repo_name_extracts_owner_repo_from_ssh_url() {
        assert_eq!(
            parse_repo_name("git@github.com:donbeave/jackin.git"),
            Some("donbeave/jackin".to_string())
        );
    }

    #[test]
    fn parse_repo_name_extracts_owner_repo_from_https_url() {
        assert_eq!(
            parse_repo_name("https://github.com/donbeave/jackin.git"),
            Some("donbeave/jackin".to_string())
        );
    }

    #[test]
    fn parse_repo_name_handles_url_without_git_suffix() {
        assert_eq!(
            parse_repo_name("https://github.com/donbeave/jackin"),
            Some("donbeave/jackin".to_string())
        );
        assert_eq!(
            parse_repo_name("git@github.com:donbeave/jackin"),
            Some("donbeave/jackin".to_string())
        );
    }

    #[test]
    fn config_rows_show_repo_and_branch_for_git_directory() {
        // Use the jackin repo itself as a known git directory
        let cwd = std::env::current_dir().unwrap();
        let workspace = crate::workspace::ResolvedWorkspace {
            label: cwd.display().to_string(),
            workdir: cwd.display().to_string(),
            mounts: vec![],
        };
        let git = GitIdentity {
            user_name: String::new(),
            user_email: String::new(),
        };

        let rows = build_config_rows("Agent", "jackin-agent", &workspace, &git, "img");

        let labels: Vec<&str> = rows.iter().map(|(l, _)| l.as_str()).collect();
        assert!(labels.contains(&"repository"));
        assert!(labels.contains(&"branch"));
        assert!(!labels.contains(&"workspace"));
        assert!(!labels.contains(&"dind"));
    }

    #[test]
    fn config_rows_show_workspace_for_saved_workspace() {
        let workspace = crate::workspace::ResolvedWorkspace {
            label: "big-monorepo".to_string(),
            workdir: "/workspace/project".to_string(),
            mounts: vec![],
        };
        let git = GitIdentity {
            user_name: "Neo".to_string(),
            user_email: "neo@matrix.org".to_string(),
        };

        let rows = build_config_rows("Agent", "jackin-agent", &workspace, &git, "img");

        let labels: Vec<&str> = rows.iter().map(|(l, _)| l.as_str()).collect();
        assert!(labels.contains(&"workspace"));
        assert!(!labels.contains(&"repository"));
        assert!(!labels.contains(&"branch"));
        assert!(!labels.contains(&"dind"));

        let ws_value = rows.iter().find(|(l, _)| l == "workspace").unwrap();
        assert_eq!(ws_value.1, "big-monorepo");
    }

    #[test]
    fn config_rows_omit_dind() {
        let workspace = crate::workspace::ResolvedWorkspace {
            label: "test".to_string(),
            workdir: "/workspace".to_string(),
            mounts: vec![],
        };
        let git = GitIdentity {
            user_name: String::new(),
            user_email: String::new(),
        };

        let rows = build_config_rows("Agent", "jackin-agent", &workspace, &git, "img");

        let labels: Vec<&str> = rows.iter().map(|(l, _)| l.as_str()).collect();
        assert!(!labels.contains(&"dind"));
    }

    #[test]
    fn format_agent_display_uses_display_name_for_primary() {
        assert_eq!(
            format_agent_display("jackin-the-architect", "The Architect"),
            "The Architect"
        );
    }

    #[test]
    fn format_agent_display_appends_clone_index() {
        assert_eq!(
            format_agent_display("jackin-the-architect-clone-2", "The Architect"),
            "The Architect (Clone 2)"
        );
    }

    #[test]
    fn format_agent_display_falls_back_to_container_name() {
        assert_eq!(
            format_agent_display("jackin-the-architect", ""),
            "jackin-the-architect"
        );
    }

    #[test]
    fn load_agent_sets_display_name_label() {
        let temp = tempdir().unwrap();
        let paths = JackinPaths::for_tests(temp.path());
        let mut config = AppConfig::load_or_init(&paths).unwrap();
        let selector = ClassSelector::new(None, "agent-smith");
        let mut runner = FakeRunner::with_capture_queue([
            String::new(),
            String::new(),
            String::new(),
            String::new(),
            String::new(),
            "jackin-agent-smith".to_string(),
        ]);

        let repo_dir = paths.agents_dir.join("agent-smith");
        std::fs::create_dir_all(&repo_dir).unwrap();
        std::fs::write(
            repo_dir.join("Dockerfile"),
            "FROM donbeave/jackin-construct:trixie\n",
        )
        .unwrap();
        std::fs::write(
            repo_dir.join("jackin.agent.toml"),
            "dockerfile = \"Dockerfile\"\n\n[identity]\nname = \"Agent Smith\"\n\n[claude]\nplugins = []\n",
        )
        .unwrap();

        let workspace = repo_workspace(&repo_dir);
        load_agent(
            &paths,
            &mut config,
            &selector,
            &workspace,
            &mut runner,
            &LoadOptions::default(),
        )
        .unwrap();

        let run_cmd = runner
            .recorded
            .iter()
            .find(|call| call.contains("docker run -it"))
            .unwrap();
        assert!(run_cmd.contains("jackin.display_name=Agent Smith"));
    }
}
