use crate::config::AppConfig;
use crate::derived_image::create_derived_build_context;
use crate::docker::CommandRunner;
use crate::instance::{AgentState, next_container_name};
use crate::paths::JackinPaths;
use crate::repo::{CachedRepo, validate_agent_repo};
use crate::selector::ClassSelector;
use crate::tui;
use owo_colors::OwoColorize;

pub struct LoadOptions {
    pub no_intro: bool,
    pub debug: bool,
    pub rebuild: bool,
}

impl Default for LoadOptions {
    fn default() -> Self {
        Self {
            no_intro: true,
            debug: false,
            rebuild: false,
        }
    }
}

struct StepCounter {
    current: u32,
    quiet: bool,
    agent_name: String,
}

impl StepCounter {
    fn new(quiet: bool, agent_name: &str) -> Self {
        Self {
            current: 0,
            quiet,
            agent_name: agent_name.to_string(),
        }
    }

    fn next(&mut self, text: &str) {
        self.current += 1;
        tui::set_terminal_title(&format!("{} \u{2014} {text}", self.agent_name));
        if self.quiet {
            tui::step_quiet(self.current, text);
        } else {
            tui::step_shimmer(self.current, text);
        }
    }

    fn done(&self) {
        tui::set_terminal_title(&self.agent_name);
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
fn try_capture(runner: &mut impl CommandRunner, program: &str, args: &[&str]) -> Option<String> {
    runner
        .capture(program, args, None)
        .ok()
        .filter(|s| !s.is_empty())
}

fn load_git_identity(runner: &mut impl CommandRunner) -> GitIdentity {
    GitIdentity {
        user_name: try_capture(runner, "git", &["config", "user.name"]).unwrap_or_default(),
        user_email: try_capture(runner, "git", &["config", "user.email"]).unwrap_or_default(),
    }
}

#[cfg(unix)]
fn load_host_identity(runner: &mut impl CommandRunner) -> HostIdentity {
    HostIdentity {
        uid: try_capture(runner, "id", &["-u"]).unwrap_or_else(|| "1000".to_string()),
        gid: try_capture(runner, "id", &["-g"]).unwrap_or_else(|| "1000".to_string()),
    }
}

#[cfg(not(unix))]
fn load_host_identity(_runner: &mut impl CommandRunner) -> HostIdentity {
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
    stripped.rsplit_once(':').map(|(_, p)| p.to_string())
}

fn repo_matches(expected: &str, actual: &str) -> bool {
    match (parse_repo_name(expected), parse_repo_name(actual)) {
        (Some(expected_repo), Some(actual_repo)) => expected_repo == actual_repo,
        _ => expected.trim() == actual.trim(),
    }
}

/// Derive a short repository name from a git remote URL (e.g. `donbeave/jackin`).
fn git_repo_name(dir: &std::path::Path, runner: &mut impl CommandRunner) -> Option<String> {
    let dir_str = dir.display().to_string();
    let url = try_capture(
        runner,
        "git",
        &["-C", &dir_str, "remote", "get-url", "origin"],
    )?;
    parse_repo_name(&url)
}

/// Get the current branch name for a git directory.
fn git_branch(dir: &std::path::Path, runner: &mut impl CommandRunner) -> Option<String> {
    let dir_str = dir.display().to_string();
    try_capture(
        runner,
        "git",
        &["-C", &dir_str, "rev-parse", "--abbrev-ref", "HEAD"],
    )
}

/// Check whether a path is inside a git work tree.
fn is_git_dir(dir: &std::path::Path, runner: &mut impl CommandRunner) -> bool {
    let dir_str = dir.display().to_string();
    try_capture(
        runner,
        "git",
        &["-C", &dir_str, "rev-parse", "--is-inside-work-tree"],
    )
    .is_some()
}

fn build_config_rows(
    agent_display_name: &str,
    container_name: &str,
    workspace: &crate::workspace::ResolvedWorkspace,
    git: &GitIdentity,
    image: &str,
    runner: &mut impl CommandRunner,
) -> Vec<(String, String)> {
    // Who
    let mut rows = vec![("identity".to_string(), agent_display_name.to_string())];
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

    // Where
    let workdir = std::path::Path::new(&workspace.label);
    if workdir.is_absolute() && is_git_dir(workdir, runner) {
        if let Some(repo_name) = git_repo_name(workdir, runner) {
            rows.push(("repository".to_string(), repo_name));
        }
        if let Some(branch) = git_branch(workdir, runner) {
            rows.push(("branch".to_string(), branch));
        }
    } else {
        rows.push(("workspace".to_string(), workspace.label.clone()));
    }

    // Runtime
    rows.push(("container".to_string(), container_name.to_string()));
    rows.push(("image".to_string(), image.to_string()));
    rows
}

/// Resolve the agent repository: clone if missing, pull if already present.
/// Returns the validated repo metadata and cached repo paths.
fn resolve_agent_repo(
    paths: &JackinPaths,
    selector: &ClassSelector,
    git_url: &str,
    runner: &mut impl CommandRunner,
) -> anyhow::Result<(CachedRepo, crate::repo::ValidatedAgentRepo)> {
    let cached_repo = CachedRepo::new(paths, selector);
    let repo_parent = cached_repo.repo_dir.parent().ok_or_else(|| {
        anyhow::anyhow!(
            "agent repo path has no parent: {}",
            cached_repo.repo_dir.display()
        )
    })?;
    std::fs::create_dir_all(repo_parent)?;

    let repo_path = cached_repo.repo_dir.display().to_string();
    if cached_repo.repo_dir.join(".git").is_dir() {
        let remote_url = runner.capture(
            "git",
            &["-C", &repo_path, "remote", "get-url", "origin"],
            None,
        )?;
        anyhow::ensure!(
            repo_matches(git_url, &remote_url),
            "cached agent repo remote does not match configured source: expected {git_url}, found {remote_url}. Remove the cached repo and try again."
        );

        let status = runner.capture(
            "git",
            &[
                "-C",
                &repo_path,
                "status",
                "--porcelain",
                "--ignored=matching",
                "--untracked-files=all",
            ],
            None,
        )?;
        anyhow::ensure!(
            status.is_empty(),
            "cached agent repo contains local changes or extra files: {}. Remove the cached repo or clean it before loading.",
            cached_repo.repo_dir.display()
        );

        runner.capture("git", &["-C", &repo_path, "pull", "--ff-only"], None)?;
    } else {
        runner.capture("git", &["clone", git_url, &repo_path], None)?;
    }

    let validated_repo = validate_agent_repo(&cached_repo.repo_dir)?;
    Ok((cached_repo, validated_repo))
}

/// Build the Docker image for the agent. Returns the image name.
#[allow(clippy::similar_names)]
fn build_agent_image(
    selector: &ClassSelector,
    cached_repo: &CachedRepo,
    validated_repo: &crate::repo::ValidatedAgentRepo,
    host: &HostIdentity,
    rebuild: bool,
    debug: bool,
    runner: &mut impl CommandRunner,
) -> anyhow::Result<String> {
    let build = create_derived_build_context(&cached_repo.repo_dir, validated_repo)?;

    if debug {
        eprintln!(
            "{}",
            format!(
                "[debug] DerivedDockerfile ({}):\n{}",
                build.dockerfile_path.display(),
                std::fs::read_to_string(&build.dockerfile_path).unwrap_or_default()
            )
            .dimmed()
        );
    }
    let image = image_name(selector);

    let build_arg_uid = format!("JACKIN_HOST_UID={}", host.uid);
    let build_arg_gid = format!("JACKIN_HOST_GID={}", host.gid);
    let cache_bust = format!(
        "JACKIN_CACHE_BUST={}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_secs())
    );
    let dockerfile_path = build.dockerfile_path.display().to_string();
    let context_dir = build.context_dir.display().to_string();

    let mut build_args: Vec<&str> = vec![
        "build",
        "--build-arg",
        &build_arg_uid,
        "--build-arg",
        &build_arg_gid,
    ];
    if rebuild {
        build_args.extend(["--build-arg", &cache_bust]);
    }
    build_args.extend(["-t", &image, "-f", &dockerfile_path, &context_dir]);
    runner.run("docker", &build_args, None)?;

    // Extract and display the Claude version from the built image
    if let Ok(version) = runner.capture(
        "docker",
        &["run", "--rm", "--entrypoint", "claude", &image, "--version"],
        None,
    ) {
        let version = version.trim();
        if !version.is_empty() {
            eprintln!("        Claude {version}");
        }
    }

    Ok(image)
}

struct LaunchContext<'a> {
    container_name: &'a str,
    image: &'a str,
    network: &'a str,
    dind: &'a str,
    selector: &'a ClassSelector,
    agent_display_name: &'a str,
    workspace: &'a crate::workspace::ResolvedWorkspace,
    state: &'a AgentState,
    git: &'a GitIdentity,
    debug: bool,
    resolved_env: &'a crate::env_resolver::ResolvedEnv,
}

/// Create the Docker network, start `DinD`, and launch the agent container.
#[allow(clippy::too_many_lines)]
fn launch_agent_runtime(
    ctx: &LaunchContext<'_>,
    steps: &mut StepCounter,
    runner: &mut impl CommandRunner,
) -> anyhow::Result<()> {
    let LaunchContext {
        container_name,
        image,
        network,
        dind,
        selector,
        agent_display_name,
        workspace,
        state,
        git,
        debug,
        resolved_env,
    } = ctx;
    // Clean up stale resources from a previous run that wasn't cleaned up
    // (e.g. terminal closed, process killed, Ctrl+C during docker run)
    let _ = run_cleanup_command(runner, &["rm", "-f", container_name]);
    let _ = run_cleanup_command(runner, &["rm", "-f", dind]);
    let _ = run_cleanup_command(runner, &["network", "rm", network]);

    // Create Docker network
    runner.capture("docker", &["network", "create", network], None)?;

    // Start Docker-in-Docker
    let dind_args: Vec<&str> = vec![
        "run",
        "-d",
        "--name",
        dind,
        "--network",
        network,
        "--privileged",
        "-e",
        "DOCKER_TLS_CERTDIR=",
        "docker:dind",
    ];
    runner.capture("docker", &dind_args, None)?;

    wait_for_dind(dind, runner, *debug)?;

    // Step 4: Mount volumes and launch
    steps.next("Launching agent");
    steps.done();

    tui::print_deploying(agent_display_name);

    let class_label = format!("jackin.class={}", selector.key());
    let display_label = format!("jackin.display_name={agent_display_name}");
    let docker_host = format!("DOCKER_HOST=tcp://{dind}:2375");
    let git_author_name = format!("GIT_AUTHOR_NAME={}", git.user_name);
    let git_author_email = format!("GIT_AUTHOR_EMAIL={}", git.user_email);
    let claude_dir_mount = format!("{}:/home/claude/.claude", state.claude_dir.display());
    let claude_json_mount = format!("{}:/home/claude/.claude.json", state.claude_json.display());
    let gh_config_mount = format!("{}:/home/claude/.config/gh", state.gh_config_dir.display());
    let plugins_mount = format!(
        "{}:/home/claude/.jackin/plugins.json:ro",
        state.plugins_json.display()
    );

    let mut run_args: Vec<&str> = vec![
        "run",
        "-it",
        "--name",
        container_name,
        "--hostname",
        container_name,
        "--network",
        network,
        "--label",
        "jackin.managed=true",
        "--label",
        &class_label,
        "--label",
        &display_label,
        "--workdir",
        &workspace.workdir,
        "-e",
        &docker_host,
        "-e",
        &git_author_name,
        "-e",
        &git_author_email,
    ];
    if *debug {
        run_args.extend_from_slice(&["-e", "CLAUDE_DEBUG=1"]);
    }
    let mut env_strings: Vec<String> = Vec::new();
    for (key, value) in &resolved_env.vars {
        env_strings.push(format!("{key}={value}"));
    }
    for env_str in &env_strings {
        run_args.push("-e");
        run_args.push(env_str);
    }
    run_args.extend_from_slice(&[
        "-v",
        &claude_dir_mount,
        "-v",
        &claude_json_mount,
        "-v",
        &gh_config_mount,
        "-v",
        &plugins_mount,
    ]);

    let mut mount_strings: Vec<String> = Vec::new();
    for mount in &workspace.mounts {
        let suffix = if mount.readonly { ":ro" } else { "" };
        mount_strings.push(format!("{}:{}{}", mount.src, mount.dst, suffix));
    }
    for ms in &mount_strings {
        run_args.push("-v");
        run_args.push(ms);
    }
    run_args.push(image);
    runner.run("docker", &run_args, None)?;

    Ok(())
}

pub fn load_agent(
    paths: &JackinPaths,
    config: &mut AppConfig,
    selector: &ClassSelector,
    workspace: &crate::workspace::ResolvedWorkspace,
    runner: &mut impl CommandRunner,
    opts: &LoadOptions,
) -> anyhow::Result<()> {
    let git = load_git_identity(runner);
    let host = load_host_identity(runner);

    // Matrix intro
    if !opts.no_intro {
        let intro_name = if git.user_name.is_empty() {
            "Neo"
        } else {
            &git.user_name
        };
        tui::matrix_intro(intro_name);
    }

    let (source, is_new) = config.resolve_agent_source(selector)?;

    let mut steps = StepCounter::new(opts.no_intro, &selector.name);

    // Step 1: Resolve agent identity (clone or update repo)
    steps.next("Resolving agent identity");

    let (cached_repo, validated_repo) = resolve_agent_repo(paths, selector, &source.git, runner)?;

    // Persist config only when the agent was newly registered
    if is_new {
        config.save(paths)?;
    }

    let existing = list_managed_agent_names(runner)?;
    let container_name = next_container_name(selector, &existing);
    let state = AgentState::prepare(paths, &container_name, &validated_repo.manifest)?;

    let network = format!("{container_name}-net");
    let dind = format!("{container_name}-dind");

    let agent_display_name = validated_repo.manifest.display_name(&selector.name);
    steps.agent_name.clone_from(&agent_display_name);

    // Logo (if present in agent repo)
    tui::print_logo(&cached_repo.repo_dir.join("logo.txt"));

    // Configuration summary
    let image = image_name(selector);
    let config_rows = build_config_rows(
        &agent_display_name,
        &container_name,
        workspace,
        &git,
        &image,
        runner,
    );
    eprintln!();
    tui::print_config_table(&config_rows);
    eprintln!();

    // Resolve env vars (interactive prompts happen here, before build)
    let resolved_env = if validated_repo.manifest.env.is_empty() {
        crate::env_resolver::ResolvedEnv { vars: vec![] }
    } else {
        let prompter = crate::terminal_prompter::TerminalPrompter;
        crate::env_resolver::resolve_env(&validated_repo.manifest.env, &prompter)?
    };

    let mut cleanup = LoadCleanup::new(container_name.clone(), dind.clone(), network.clone());
    let load_result = (|| -> anyhow::Result<()> {
        // Step 2: Build Docker image
        steps.next("Building Docker image");
        let image = build_agent_image(
            selector,
            &cached_repo,
            &validated_repo,
            &host,
            opts.rebuild,
            opts.debug,
            runner,
        )?;

        // Step 3: Create network and start Docker-in-Docker
        steps.next("Starting Docker-in-Docker");

        let ctx = LaunchContext {
            container_name: &container_name,
            image: &image,
            network: &network,
            dind: &dind,
            selector,
            agent_display_name: &agent_display_name,
            workspace,
            state: &state,
            git: &git,
            debug: opts.debug,
            resolved_env: &resolved_env,
        };
        launch_agent_runtime(&ctx, &mut steps, runner)?;

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
    runner.run("docker", &["attach", container_name], None)
}

fn wait_for_dind(
    dind_name: &str,
    runner: &mut impl CommandRunner,
    _debug: bool,
) -> anyhow::Result<()> {
    tui::spin_wait(
        "Waiting for Docker-in-Docker to be ready",
        30,
        std::time::Duration::from_secs(1),
        || {
            runner
                .capture("docker", &["exec", dind_name, "docker", "info"], None)
                .map(|_| ())
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
    let output = if include_stopped {
        runner.capture(
            "docker",
            &[
                "ps",
                "-a",
                "--filter",
                "label=jackin.managed=true",
                "--format",
                "{{.Names}}",
            ],
            None,
        )?
    } else {
        runner.capture(
            "docker",
            &[
                "ps",
                "--filter",
                "label=jackin.managed=true",
                "--format",
                "{{.Names}}",
            ],
            None,
        )?
    };

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
            "ps",
            "--filter",
            "label=jackin.managed=true",
            "--format",
            "{{.Names}}\t{{.Label \"jackin.display_name\"}}",
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

    run_cleanup_command(runner, &["rm", "-f", container_name])?;
    run_cleanup_command(runner, &["rm", "-f", &dind])?;
    run_cleanup_command(runner, &["network", "rm", &network])?;

    Ok(())
}

fn run_cleanup_command(runner: &mut impl CommandRunner, args: &[&str]) -> anyhow::Result<()> {
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
    format!("jackin-{}", crate::instance::runtime_slug(selector))
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

        if let Err(e) = run_cleanup_command(runner, &["rm", "-f", &self.container_name]) {
            tui::step_fail(&format!("cleanup failed (container): {e}"));
        }
        if let Err(e) = run_cleanup_command(runner, &["rm", "-f", &self.dind]) {
            tui::step_fail(&format!("cleanup failed (dind): {e}"));
        }
        if let Err(e) = run_cleanup_command(runner, &["network", "rm", &self.network]) {
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

    /// Prefixes the capture queue with empty responses for the identity lookups
    /// (`git config user.name`, `git config user.email`, `id -u`, `id -g`)
    /// that `load_agent` performs before any docker commands.
    fn for_load_agent<const N: usize>(outputs: [String; N]) -> Self {
        let mut queue = VecDeque::with_capacity(4 + N);
        for _ in 0..4 {
            queue.push_back(String::new());
        }
        queue.extend(outputs);
        Self {
            capture_queue: queue,
            ..Default::default()
        }
    }
}

#[cfg(test)]
impl FakeRunner {
    fn check_command(&self, command: &str) -> anyhow::Result<()> {
        if let Some((_, message)) = self
            .fail_with
            .iter()
            .find(|(pattern, _)| command.contains(pattern))
        {
            anyhow::bail!("{message}");
        }
        if self.fail_on.iter().any(|pattern| command.contains(pattern)) {
            anyhow::bail!("command failed: {command}");
        }
        Ok(())
    }
}

#[cfg(test)]
impl CommandRunner for FakeRunner {
    fn run(
        &mut self,
        program: &str,
        args: &[&str],
        _cwd: Option<&std::path::Path>,
    ) -> anyhow::Result<()> {
        let command = format!("{} {}", program, args.join(" "));
        self.run_recorded.push(command.clone());
        self.recorded.push(command.clone());
        self.check_command(&command)
    }

    fn capture(
        &mut self,
        program: &str,
        args: &[&str],
        _cwd: Option<&std::path::Path>,
    ) -> anyhow::Result<String> {
        let command = format!("{} {}", program, args.join(" "));
        self.recorded.push(command.clone());
        self.check_command(&command)?;
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
        let mut runner = FakeRunner::for_load_agent([
            String::new(),
            "jackin-chainargos__the-architect".to_string(),
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
            call.contains("docker build ") && call.contains("-t jackin-chainargos__the-architect")
        }));
        assert!(runner.recorded.iter().any(|call| {
            call == "docker ps -a --filter label=jackin.managed=true --format {{.Names}}"
        }));
        assert!(
            runner
                .recorded
                .iter()
                .any(|call| call.contains("docker run -it --name jackin-chainargos__the-architect"))
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
        let mut runner = FakeRunner::for_load_agent([
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
            FakeRunner::for_load_agent([String::new(), "jackin-agent-smith".to_string()]);

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
        let mut runner = FakeRunner::for_load_agent([
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
        let mut runner = FakeRunner::for_load_agent([
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
            capture_queue: VecDeque::from(vec![
                String::new(),
                String::new(),
                String::new(),
                String::new(), // identity
                String::new(), // git pull
            ]),
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
        let mut runner = FakeRunner::for_load_agent([
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
    fn image_name_distinguishes_namespaced_and_flat_classes() {
        let namespaced = ClassSelector::new(Some("chainargos"), "the-architect");
        let flat = ClassSelector::new(None, "chainargos-the-architect");

        assert_ne!(image_name(&namespaced), image_name(&flat));
    }

    #[test]
    fn resolve_agent_repo_rejects_cached_repo_with_wrong_remote() {
        let temp = tempdir().unwrap();
        let paths = JackinPaths::for_tests(temp.path());
        let selector = ClassSelector::new(None, "agent-smith");
        let repo_dir = paths.agents_dir.join("agent-smith");
        std::fs::create_dir_all(repo_dir.join(".git")).unwrap();
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

        let mut runner =
            FakeRunner::with_capture_queue(["git@github.com:evil/agent-smith.git".to_string()]);
        let error = resolve_agent_repo(
            &paths,
            &selector,
            "https://github.com/donbeave/jackin-agent-smith.git",
            &mut runner,
        )
        .unwrap_err();

        assert!(
            error
                .to_string()
                .contains("cached agent repo remote does not match")
        );
    }

    #[test]
    fn resolve_agent_repo_rejects_cached_repo_with_local_changes() {
        let temp = tempdir().unwrap();
        let paths = JackinPaths::for_tests(temp.path());
        let selector = ClassSelector::new(None, "agent-smith");
        let repo_dir = paths.agents_dir.join("agent-smith");
        std::fs::create_dir_all(repo_dir.join(".git")).unwrap();
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

        let mut runner = FakeRunner::with_capture_queue([
            "git@github.com:donbeave/jackin-agent-smith.git".to_string(),
            "?? scratch.txt".to_string(),
        ]);
        let error = resolve_agent_repo(
            &paths,
            &selector,
            "https://github.com/donbeave/jackin-agent-smith.git",
            &mut runner,
        )
        .unwrap_err();

        assert!(error.to_string().contains("contains local changes"));
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

        let rows = build_config_rows(
            "Agent",
            "jackin-agent",
            &workspace,
            &git,
            "img",
            &mut crate::docker::ShellRunner::default(),
        );

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

        let rows = build_config_rows(
            "Agent",
            "jackin-agent",
            &workspace,
            &git,
            "img",
            &mut crate::docker::ShellRunner::default(),
        );

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

        let rows = build_config_rows(
            "Agent",
            "jackin-agent",
            &workspace,
            &git,
            "img",
            &mut crate::docker::ShellRunner::default(),
        );

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
        let mut runner = FakeRunner::for_load_agent([
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
