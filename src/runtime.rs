use crate::config::AppConfig;
use crate::derived_image::create_derived_build_context;
use crate::docker::{CommandRunner, RunOptions};
use crate::instance::{AgentState, next_container_name};
use crate::paths::JackinPaths;
use crate::repo::{CachedRepo, validate_agent_repo};
use crate::selector::ClassSelector;
use crate::tui;
use crate::version_check;
use owo_colors::OwoColorize;
use std::io::IsTerminal;

// ── Docker label keys ─────────────────────────────────────────────────────
//
// Used to tag and filter jackin-managed containers and networks.

/// Applied to agent containers, `DinD` sidecars, and networks.
const LABEL_MANAGED: &str = "jackin.managed=true";
/// Agent containers only — distinguishes them from `DinD` sidecars.
const LABEL_ROLE_AGENT: &str = "jackin.role=agent";
/// `DinD` sidecars only — distinguishes them from agent containers.
const LABEL_ROLE_DIND: &str = "jackin.role=dind";
/// Filter expression for `docker ps --filter` to find managed containers.
const FILTER_MANAGED: &str = "label=jackin.managed=true";
/// Filter expression for `docker ps --filter` to find agent containers.
const FILTER_ROLE_AGENT: &str = "label=jackin.role=agent";
/// Filter expression for `docker ps --filter` to find `DinD` sidecars.
const FILTER_ROLE_DIND: &str = "label=jackin.role=dind";

/// Environment variables owned by the jackin runtime that must not be
/// overridden by agent manifests.  These are injected as `-e` flags in
/// `launch_agent_runtime` and are silently skipped if a manifest declares them.
/// The corresponding manifest-time validation lives in
/// `manifest::RESERVED_RUNTIME_ENV_VARS`.
const RUNTIME_OWNED_ENV_VARS: &[&str] = &["DOCKER_HOST", "DOCKER_TLS_VERIFY", "DOCKER_CERT_PATH"];

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

/// Derive a short repository name from a git remote URL (e.g. `jackin-project/jackin`).
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

// ── Agent source trust ───────────────────────────────────────────────────

/// Display an untrusted-agent warning and ask the operator to confirm.
/// Aborts when stdin is not a terminal or the operator declines.
fn confirm_agent_trust(
    selector: &ClassSelector,
    source: &crate::config::AgentSource,
) -> anyhow::Result<()> {
    if !std::io::stdin().is_terminal() {
        anyhow::bail!(
            "untrusted agent source \"{selector}\" from {}\n\
             Trust it first: `jackin config trust grant {selector}`, or add `trusted = true` in config.toml.",
            source.git,
        );
    }

    eprintln!();
    eprintln!("{}", "!! Untrusted agent source !!".red().bold());
    eprintln!();
    eprintln!("  agent:  {}", selector.to_string().bold());
    eprintln!("  source: {}", source.git.yellow());
    eprintln!();
    eprintln!(
        "  {}",
        "jackin' has never loaded this agent before. Trusting it means:".bold()
    );
    eprintln!(
        "    {} Its {} will be executed during the image build",
        "-".dimmed(),
        "Dockerfile".bold()
    );
    eprintln!(
        "    {} Arbitrary commands in that Dockerfile will run {}",
        "-".dimmed(),
        "on your machine".bold()
    );
    eprintln!(
        "    {} The agent will have access to your {}",
        "-".dimmed(),
        "mounted workspace files".bold()
    );
    eprintln!();
    eprintln!("  {}", "Review the repository before trusting it.".dimmed());
    eprintln!();

    let confirmed = dialoguer::Confirm::new()
        .with_prompt("Do you trust this agent source and want to proceed?")
        .default(false)
        .interact()?;

    if !confirmed {
        anyhow::bail!(
            "agent source \"{selector}\" not trusted — aborting.\n\
             To trust it later, run `jackin config trust grant {selector}` or try loading again."
        );
    }

    Ok(())
}

/// Resolve the agent repository: clone if missing, pull if already present.
/// Returns the validated repo metadata and cached repo paths.
/// Prompt the user to confirm cached-repo removal when running in an
/// interactive terminal.  Returns `true` when the user accepts.
fn confirm_repo_removal_interactive() -> anyhow::Result<bool> {
    if !std::io::stdin().is_terminal() {
        return Ok(false);
    }
    Ok(dialoguer::Confirm::new()
        .with_prompt("Remove the cached repo and re-clone from the configured source?")
        .default(false)
        .interact()?)
}

fn resolve_agent_repo(
    paths: &JackinPaths,
    selector: &ClassSelector,
    git_url: &str,
    runner: &mut impl CommandRunner,
) -> anyhow::Result<(CachedRepo, crate::repo::ValidatedAgentRepo)> {
    resolve_agent_repo_with(
        paths,
        selector,
        git_url,
        runner,
        confirm_repo_removal_interactive,
    )
}

fn resolve_agent_repo_with(
    paths: &JackinPaths,
    selector: &ClassSelector,
    git_url: &str,
    runner: &mut impl CommandRunner,
    confirm_removal: impl FnOnce() -> anyhow::Result<bool>,
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
        if !repo_matches(git_url, &remote_url) {
            let repo_display = cached_repo.repo_dir.display();
            eprintln!(
                "{} cached agent repo remote does not match configured source",
                "error:".red().bold()
            );
            eprintln!("  expected: {}", git_url.green());
            eprintln!("  found:    {}", remote_url.yellow());
            eprintln!();
            eprintln!("To fix this, remove the cached repo and try again:");
            eprintln!("  rm -rf {repo_display}");
            eprintln!();

            if confirm_removal()? {
                std::fs::remove_dir_all(&cached_repo.repo_dir)?;
                runner.run(
                    "git",
                    &["clone", git_url, &repo_path],
                    None,
                    &RunOptions::default(),
                )?;
                let validated_repo = validate_agent_repo(&cached_repo.repo_dir)?;
                return Ok((cached_repo, validated_repo));
            }

            anyhow::bail!("cached agent repo remote mismatch — aborting");
        }

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

        runner.run(
            "git",
            &["-C", &repo_path, "pull", "--ff-only"],
            None,
            &RunOptions::default(),
        )?;
    } else {
        runner.run(
            "git",
            &["clone", git_url, &repo_path],
            None,
            &RunOptions::default(),
        )?;
    }

    let validated_repo = validate_agent_repo(&cached_repo.repo_dir)?;
    Ok((cached_repo, validated_repo))
}

/// Build the Docker image for the agent. Returns the image name.
#[allow(clippy::similar_names, clippy::too_many_arguments)]
fn build_agent_image(
    paths: &JackinPaths,
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
                r"[debug] DerivedDockerfile ({}):
{}",
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
    runner.run(
        "docker",
        &build_args,
        None,
        &RunOptions {
            capture_stderr: true,
            timeout: Some(crate::docker::DOCKER_BUILD_TIMEOUT),
        },
    )?;

    // Extract and display the Claude version from the built image
    if let Ok(version) = runner.capture(
        "docker",
        &["run", "--rm", "--entrypoint", "claude", &image, "--version"],
        None,
    ) {
        let version = version.trim();
        if !version.is_empty() {
            eprintln!("        Claude {version}");
            if let Some(semver) = version_check::parse_claude_version(version) {
                version_check::store_image_version(paths, &image, semver);
            } else {
                eprintln!("warning: unexpected claude --version output: {version:?}");
            }
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
    let certs_volume = dind_certs_volume(container_name);
    let _ = run_cleanup_command(runner, &["rm", "-f", container_name]);
    let _ = run_cleanup_command(runner, &["rm", "-f", dind]);
    let _ = run_cleanup_command(runner, &["volume", "rm", &certs_volume]);
    let _ = run_cleanup_command(runner, &["network", "rm", network]);

    // Create Docker network
    let agent_label = format!("jackin.agent={container_name}");
    runner.run(
        "docker",
        &[
            "network",
            "create",
            "--label",
            LABEL_MANAGED,
            "--label",
            &agent_label,
            network,
        ],
        None,
        &RunOptions::default(),
    )?;

    // Start Docker-in-Docker with TLS
    let certs_dind_mount = format!("{certs_volume}:/certs/client");
    let dind_args: Vec<&str> = vec![
        "run",
        "-d",
        "--name",
        dind,
        "--network",
        network,
        "--privileged",
        "--label",
        LABEL_MANAGED,
        "--label",
        LABEL_ROLE_DIND,
        "--label",
        &agent_label,
        "-e",
        "DOCKER_TLS_CERTDIR=/certs",
        "-v",
        &certs_dind_mount,
        "docker:dind",
    ];
    runner.run("docker", &dind_args, None, &RunOptions::default())?;

    wait_for_dind(dind, &certs_volume, runner, *debug)?;

    // Step 4: Mount volumes and launch
    steps.next("Launching agent");
    steps.done();

    tui::print_deploying(agent_display_name);

    let class_label = format!("jackin.class={}", selector.key());
    let display_label = format!("jackin.display_name={agent_display_name}");
    let docker_host = format!("DOCKER_HOST=tcp://{dind}:2376");
    let dind_hostname = format!("{}={dind}", crate::manifest::JACKIN_DIND_HOSTNAME_ENV_NAME);
    let git_author_name = format!("GIT_AUTHOR_NAME={}", git.user_name);
    let git_author_email = format!("GIT_AUTHOR_EMAIL={}", git.user_email);
    let claude_dir_mount = format!("{}:/home/claude/.claude", state.claude_dir.display());
    let claude_json_mount = format!("{}:/home/claude/.claude.json", state.claude_json.display());
    let gh_config_mount = format!("{}:/home/claude/.config/gh", state.gh_config_dir.display());
    let plugins_mount = format!(
        "{}:/home/claude/.jackin/plugins.json:ro",
        state.plugins_json.display()
    );
    let certs_agent_mount = format!("{certs_volume}:/certs/client:ro");

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
        LABEL_MANAGED,
        "--label",
        LABEL_ROLE_AGENT,
        "--label",
        &class_label,
        "--label",
        &display_label,
        "--workdir",
        &workspace.workdir,
        // JACKIN_* runtime metadata is injected by jackin, not declared in agent manifests.
        "-e",
        &docker_host,
        "-e",
        "DOCKER_TLS_VERIFY=1",
        "-e",
        "DOCKER_CERT_PATH=/certs/client",
        "-e",
        &dind_hostname,
        "-e",
        &git_author_name,
        "-e",
        &git_author_email,
    ];
    if *debug {
        run_args.extend_from_slice(&["-e", "JACKIN_DEBUG=1"]);
    }
    let mut env_strings: Vec<String> = Vec::new();
    env_strings.push(format!(
        "{}={}",
        crate::manifest::JACKIN_RUNTIME_ENV_NAME,
        crate::manifest::JACKIN_RUNTIME_ENV_VALUE
    ));
    for (key, value) in &resolved_env.vars {
        if key == crate::manifest::JACKIN_RUNTIME_ENV_NAME
            || key == crate::manifest::JACKIN_DIND_HOSTNAME_ENV_NAME
            || RUNTIME_OWNED_ENV_VARS.contains(&key.as_str())
        {
            continue;
        }
        env_strings.push(format!("{key}={value}"));
    }
    for env_str in &env_strings {
        run_args.push("-e");
        run_args.push(env_str);
    }
    run_args.extend_from_slice(&[
        "-v",
        &certs_agent_mount,
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
    let result = runner.run("docker", &run_args, None, &RunOptions::default());
    // Ensure cleanup debug logs start on a fresh line after the interactive session
    eprintln!();
    result?;

    Ok(())
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
    load_agent_with(
        paths,
        config,
        selector,
        workspace,
        runner,
        opts,
        confirm_agent_trust,
    )
}

#[allow(clippy::too_many_lines)]
fn load_agent_with(
    paths: &JackinPaths,
    config: &mut AppConfig,
    selector: &ClassSelector,
    workspace: &crate::workspace::ResolvedWorkspace,
    runner: &mut impl CommandRunner,
    opts: &LoadOptions,
    confirm_trust: impl FnOnce(&ClassSelector, &crate::config::AgentSource) -> anyhow::Result<()>,
) -> anyhow::Result<()> {
    // Pre-launch garbage collection: remove orphaned DinD containers and
    // networks left behind by hard kills, terminal closures, or startup
    // failures.  Best-effort — errors are silently ignored.
    gc_orphaned_resources(runner);

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

    // Trust gate: prompt the operator before running an untrusted third-party agent
    let newly_trusted = if source.trusted {
        false
    } else {
        confirm_trust(selector, &source)?;
        config.trust_agent(&selector.key());
        true
    };

    // Persist config when the agent was newly registered or newly trusted
    if is_new || newly_trusted {
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

    let certs_volume = dind_certs_volume(&container_name);
    let mut cleanup = LoadCleanup::new(
        container_name.clone(),
        dind.clone(),
        certs_volume,
        network.clone(),
    );
    let load_result = (|| -> anyhow::Result<()> {
        // Step 2: Build Docker image
        let rebuild = opts.rebuild || {
            let img = image_name(selector);
            let needs_update = version_check::needs_claude_update(paths, &img, runner);
            if needs_update {
                eprintln!("        Claude update available — rebuilding image");
            }
            needs_update
        };
        steps.next("Building Docker image");
        let image = build_agent_image(
            paths,
            selector,
            &cached_repo,
            &validated_repo,
            &host,
            rebuild,
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
    if opts.no_intro {
        return;
    }
    tui::matrix_outro(
        agent_display_name,
        &list_running_agent_display_names(runner).unwrap_or_default(),
    );
}

pub fn hardline_agent(container_name: &str, runner: &mut impl CommandRunner) -> anyhow::Result<()> {
    runner.run(
        "docker",
        &["attach", container_name],
        None,
        &RunOptions::default(),
    )
}

fn wait_for_dind(
    dind_name: &str,
    certs_volume: &str,
    runner: &mut impl CommandRunner,
    _debug: bool,
) -> anyhow::Result<()> {
    // Wait for the DinD daemon to become ready (TLS handshake included).
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
    .map_err(|_| anyhow::anyhow!("timed out waiting for Docker-in-Docker sidecar {dind_name}"))?;

    // Verify TLS client certificates were generated on the shared volume.
    // The DinD entrypoint writes certs before starting dockerd, so this
    // should succeed immediately after `docker info` passes.
    runner
        .capture(
            "docker",
            &["exec", dind_name, "test", "-f", "/certs/client/ca.pem"],
            None,
        )
        .map_err(|_| {
            anyhow::anyhow!(
                "DinD TLS client certificates not found on volume {certs_volume} — \
                 the DinD sidecar may have started without generating certificates"
            )
        })?;

    Ok(())
}

pub fn list_running_agent_names(runner: &mut impl CommandRunner) -> anyhow::Result<Vec<String>> {
    list_agent_names(runner, false)
}

pub fn list_managed_agent_names(runner: &mut impl CommandRunner) -> anyhow::Result<Vec<String>> {
    list_agent_names(runner, true)
}

fn capture_managed_container_rows(
    runner: &mut impl CommandRunner,
    include_stopped: bool,
    format: &str,
) -> anyhow::Result<String> {
    if include_stopped {
        runner.capture(
            "docker",
            &["ps", "-a", "--filter", FILTER_MANAGED, "--format", format],
            None,
        )
    } else {
        runner.capture(
            "docker",
            &["ps", "--filter", FILTER_MANAGED, "--format", format],
            None,
        )
    }
}

fn list_legacy_managed_agent_names(
    runner: &mut impl CommandRunner,
    include_stopped: bool,
) -> anyhow::Result<Vec<String>> {
    let output = capture_managed_container_rows(
        runner,
        include_stopped,
        "{{.Names}}\t{{.Label \"jackin.agent\"}}\t{{.Label \"jackin.role\"}}",
    )?;

    Ok(output
        .lines()
        .filter_map(|line| {
            let mut parts = line.splitn(3, '\t');
            let name = parts.next()?;
            let agent = parts.next().unwrap_or("");
            let role = parts.next().unwrap_or("");
            if name.is_empty() || !agent.is_empty() || !role.is_empty() {
                return None;
            }
            Some(name.to_string())
        })
        .collect())
}

fn list_agent_names(
    runner: &mut impl CommandRunner,
    include_stopped: bool,
) -> anyhow::Result<Vec<String>> {
    let role_output = if include_stopped {
        runner.capture(
            "docker",
            &[
                "ps",
                "-a",
                "--filter",
                FILTER_ROLE_AGENT,
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
                FILTER_ROLE_AGENT,
                "--format",
                "{{.Names}}",
            ],
            None,
        )?
    };

    let mut names: Vec<String> = role_output
        .lines()
        .filter(|line| !line.is_empty())
        .map(String::from)
        .collect();
    names.extend(list_legacy_managed_agent_names(runner, include_stopped)?);
    Ok(names)
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
            FILTER_ROLE_AGENT,
            "--format",
            "{{.Names}}\t{{.Label \"jackin.display_name\"}}",
        ],
        None,
    )?;

    let mut names: Vec<String> = output
        .lines()
        .filter(|line| !line.is_empty())
        .map(|line| {
            let parts: Vec<&str> = line.splitn(2, '\t').collect();
            let container_name = parts[0];
            let display_name = parts.get(1).unwrap_or(&"");
            format_agent_display(container_name, display_name)
        })
        .collect();

    let legacy_output = capture_managed_container_rows(
        runner,
        false,
        "{{.Names}}\t{{.Label \"jackin.display_name\"}}\t{{.Label \"jackin.agent\"}}\t{{.Label \"jackin.role\"}}",
    )?;
    names.extend(legacy_output.lines().filter_map(|line| {
        let mut parts = line.splitn(4, '\t');
        let container_name = parts.next()?;
        let display_name = parts.next().unwrap_or("");
        let agent = parts.next().unwrap_or("");
        let role = parts.next().unwrap_or("");
        if container_name.is_empty() || !agent.is_empty() || !role.is_empty() {
            return None;
        }
        Some(format_agent_display(container_name, display_name))
    }));

    Ok(names)
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
    let certs_volume = dind_certs_volume(container_name);
    let network = format!("{container_name}-net");

    run_cleanup_command(runner, &["rm", "-f", container_name])?;
    run_cleanup_command(runner, &["rm", "-f", &dind])?;
    run_cleanup_command(runner, &["volume", "rm", &certs_volume])?;
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
    message.contains("No such container")
        || message.contains("No such volume")
        || message.contains("No such network")
}

// ── Orphaned resource garbage collection ─────────────────────────────────

/// Parsed row from `docker ps` for a `DinD` sidecar.
struct DindInfo {
    name: String,
    agent: String,
}

fn collect_labeled_dind(runner: &mut impl CommandRunner) -> anyhow::Result<Vec<DindInfo>> {
    let dind_output = runner.capture(
        "docker",
        &[
            "ps",
            "-a",
            "--filter",
            FILTER_ROLE_DIND,
            "--format",
            "{{.Names}}\t{{.Label \"jackin.agent\"}}",
        ],
        None,
    )?;

    Ok(dind_output
        .lines()
        .filter(|line| !line.is_empty())
        .filter_map(|line| {
            let (name, agent) = line.split_once('\t')?;
            if agent.is_empty() {
                return None;
            }
            Some(DindInfo {
                name: name.to_string(),
                agent: agent.to_string(),
            })
        })
        .collect())
}

fn collect_legacy_dind(runner: &mut impl CommandRunner) -> anyhow::Result<Vec<DindInfo>> {
    let output = capture_managed_container_rows(
        runner,
        true,
        "{{.Names}}\t{{.Label \"jackin.agent\"}}\t{{.Label \"jackin.role\"}}",
    )?;

    Ok(output
        .lines()
        .filter(|line| !line.is_empty())
        .filter_map(|line| {
            let mut parts = line.splitn(3, '\t');
            let name = parts.next()?;
            let agent = parts.next().unwrap_or("");
            let role = parts.next().unwrap_or("");
            if name.is_empty() || agent.is_empty() || !role.is_empty() {
                return None;
            }
            Some(DindInfo {
                name: name.to_string(),
                agent: agent.to_string(),
            })
        })
        .collect())
}

/// Return `DinD` sidecar containers whose corresponding agent container is no
/// longer running.  These are leftovers from hard kills, terminal closures,
/// or startup failures.
fn collect_orphaned_dind(runner: &mut impl CommandRunner) -> anyhow::Result<Vec<DindInfo>> {
    let mut sidecars = collect_labeled_dind(runner)?;
    sidecars.extend(collect_legacy_dind(runner)?);

    if sidecars.is_empty() {
        return Ok(vec![]);
    }

    // Running agent containers (label filter excludes DinD sidecars).
    let running = list_agent_names(runner, false)?;

    Ok(sidecars
        .into_iter()
        .filter(|info| !running.contains(&info.agent))
        .collect())
}

/// Remove orphaned `DinD` containers, their associated agent containers, cert
/// volumes, and networks.  Errors are logged but do not abort the launch — GC
/// is best-effort.
fn gc_orphaned_resources(runner: &mut impl CommandRunner) {
    let Ok(orphaned) = collect_orphaned_dind(runner) else {
        return;
    };

    for info in &orphaned {
        let certs_volume = dind_certs_volume(&info.agent);
        let network = format!("{}-net", info.agent);

        // Remove stopped agent container, DinD sidecar, cert volume, and network.
        let _ = run_cleanup_command(runner, &["rm", "-f", &info.agent]);
        let _ = run_cleanup_command(runner, &["rm", "-f", &info.name]);
        let _ = run_cleanup_command(runner, &["volume", "rm", &certs_volume]);
        let _ = run_cleanup_command(runner, &["network", "rm", &network]);

        eprintln!(
            "        {} orphaned resources for {}",
            "cleaned up".dimmed(),
            info.agent
        );
    }

    // Clean up any orphaned networks that survived without a DinD container
    // (e.g. the DinD container was manually removed but the network lingers).
    gc_orphaned_networks(runner);
}

/// Remove jackin-managed Docker networks whose owning agent container no
/// longer exists.
fn gc_orphaned_networks(runner: &mut impl CommandRunner) {
    let Ok(net_output) = runner.capture(
        "docker",
        &[
            "network",
            "ls",
            "--filter",
            FILTER_MANAGED,
            "--format",
            "{{.Name}}\t{{.Label \"jackin.agent\"}}",
        ],
        None,
    ) else {
        return;
    };

    let networks: Vec<(&str, &str)> = net_output
        .lines()
        .filter(|l| !l.is_empty())
        .filter_map(|l| l.split_once('\t'))
        .filter(|(_, agent)| !agent.is_empty())
        .collect();

    if networks.is_empty() {
        return;
    }

    let Ok(running) = list_agent_names(runner, false) else {
        return;
    };

    for (net_name, agent) in networks {
        if running.iter().any(|r| r == agent) {
            continue;
        }
        let _ = run_cleanup_command(runner, &["network", "rm", net_name]);
    }
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

/// Docker volume name for the TLS client certificates shared between the
/// `DinD` sidecar (writer) and the agent container (reader).
fn dind_certs_volume(container_name: &str) -> String {
    format!("{container_name}-dind-certs")
}

struct LoadCleanup {
    container_name: String,
    dind: String,
    certs_volume: String,
    network: String,
    armed: bool,
}

impl LoadCleanup {
    const fn new(
        container_name: String,
        dind: String,
        certs_volume: String,
        network: String,
    ) -> Self {
        Self {
            container_name,
            dind,
            certs_volume,
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
        if let Err(e) = run_cleanup_command(runner, &["volume", "rm", &self.certs_volume]) {
            tui::step_fail(&format!("cleanup failed (certs volume): {e}"));
        }
        if let Err(e) = run_cleanup_command(runner, &["network", "rm", &self.network]) {
            tui::step_fail(&format!("cleanup failed (network): {e}"));
        }
    }
}

#[cfg(test)]
use std::collections::VecDeque;

#[cfg(test)]
pub struct FakeRunner {
    pub recorded: Vec<String>,
    pub run_recorded: Vec<String>,
    pub fail_on: Vec<String>,
    pub fail_with: Vec<(String, String)>,
    pub capture_queue: VecDeque<String>,
    /// Optional callbacks keyed by a substring of the command.  When a
    /// captured command matches the key, the callback is invoked before the
    /// output is returned.  This is useful for simulating filesystem
    /// side-effects (e.g. `git clone` creating repo files on disk).
    pub side_effects: Vec<(String, Box<dyn FnOnce()>)>,
}

#[cfg(test)]
impl Default for FakeRunner {
    fn default() -> Self {
        Self {
            recorded: Vec::new(),
            run_recorded: Vec::new(),
            fail_on: Vec::new(),
            fail_with: Vec::new(),
            capture_queue: VecDeque::new(),
            side_effects: Vec::new(),
        }
    }
}

#[cfg(test)]
impl FakeRunner {
    fn with_capture_queue<const N: usize>(outputs: [String; N]) -> Self {
        Self {
            capture_queue: VecDeque::from(outputs),
            ..Default::default()
        }
    }

    /// Number of capture calls `load_agent` makes before reaching agent-
    /// specific logic: 2 GC queries (orphaned DinD scan + orphaned network
    /// scan) + 4 identity lookups (`git config user.name`, `git config
    /// user.email`, `id -u`, `id -g`).
    const LOAD_PREAMBLE_CAPTURES: usize = 6;

    /// Prefixes the capture queue with empty responses for the `load_agent`
    /// preamble queries so tests can focus on the agent-specific output.
    fn for_load_agent<const N: usize>(outputs: [String; N]) -> Self {
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

#[cfg(test)]
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

#[cfg(test)]
impl CommandRunner for FakeRunner {
    fn run(
        &mut self,
        program: &str,
        args: &[&str],
        _cwd: Option<&std::path::Path>,
        _opts: &RunOptions,
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
    fn trust_gate_rejects_untrusted_agent_in_non_interactive_context() {
        let selector = ClassSelector::new(Some("evil-org"), "backdoor");
        let source = crate::config::AgentSource {
            git: "https://github.com/evil-org/jackin-backdoor.git".to_string(),
            trusted: false,
        };

        let error = confirm_agent_trust(&selector, &source).unwrap_err();
        let message = error.to_string();

        assert!(
            message.contains("untrusted agent source"),
            "expected 'untrusted agent source' in: {message}"
        );
        assert!(
            message.contains("evil-org/backdoor"),
            "expected agent selector in error: {message}"
        );
        assert!(
            message.contains("evil-org/jackin-backdoor.git"),
            "expected git URL in error: {message}"
        );
    }

    /// Helper: trust callback that always accepts.
    fn auto_trust(_: &ClassSelector, _: &crate::config::AgentSource) -> anyhow::Result<()> {
        Ok(())
    }

    /// Helper: trust callback that always declines.
    fn deny_trust(_: &ClassSelector, _: &crate::config::AgentSource) -> anyhow::Result<()> {
        anyhow::bail!("agent source not trusted — aborting")
    }

    #[test]
    fn load_namespaced_agent_registers_source_and_trusts_on_accept() {
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
            "FROM projectjackin/construct:trixie\n",
        )
        .unwrap();
        std::fs::write(
            repo_dir.join("jackin.agent.toml"),
            r#"dockerfile = "Dockerfile"

[claude]
plugins = ["code-review@claude-plugins-official"]
"#,
        )
        .unwrap();

        let workspace = repo_workspace(&repo_dir);
        load_agent_with(
            &paths,
            &mut config,
            &selector,
            &workspace,
            &mut runner,
            &LoadOptions::default(),
            auto_trust,
        )
        .unwrap();

        // Source was auto-registered and persisted with trust
        let persisted = std::fs::read_to_string(&paths.config_file).unwrap();
        assert!(persisted.contains("chainargos/the-architect"));
        assert!(persisted.contains("trusted = true"));
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
            call == "docker ps -a --filter label=jackin.role=agent --format {{.Names}}"
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
    fn load_namespaced_agent_aborts_when_trust_declined() {
        let temp = tempdir().unwrap();
        let paths = JackinPaths::for_tests(temp.path());
        let mut config = AppConfig::load_or_init(&paths).unwrap();
        let selector = ClassSelector::new(Some("evil-org"), "backdoor");
        let mut runner = FakeRunner::for_load_agent([String::new(), String::new()]);

        let repo_dir = paths.agents_dir.join("evil-org").join("backdoor");
        std::fs::create_dir_all(&repo_dir).unwrap();
        std::fs::write(
            repo_dir.join("Dockerfile"),
            "FROM projectjackin/construct:trixie\n",
        )
        .unwrap();
        std::fs::write(
            repo_dir.join("jackin.agent.toml"),
            r#"dockerfile = "Dockerfile"

[claude]
plugins = []
"#,
        )
        .unwrap();

        let workspace = repo_workspace(&repo_dir);
        let error = load_agent_with(
            &paths,
            &mut config,
            &selector,
            &workspace,
            &mut runner,
            &LoadOptions::default(),
            deny_trust,
        )
        .unwrap_err();

        assert!(error.to_string().contains("not trusted"));

        // Source was NOT persisted when trust was declined
        let persisted = std::fs::read_to_string(&paths.config_file).unwrap();
        assert!(!persisted.contains("evil-org/backdoor"));

        // No Docker build or run commands were issued
        assert!(
            !runner
                .recorded
                .iter()
                .any(|call| call.contains("docker build") || call.contains("docker run"))
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
                "docker volume rm jackin-agent-smith-dind-certs",
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
                    "docker volume rm jackin-agent-smith-dind-certs".to_string(),
                    "Error response from daemon: No such volume: jackin-agent-smith-dind-certs"
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
                "docker volume rm jackin-agent-smith-dind-certs",
                "docker network rm jackin-agent-smith-net",
            ]
        );
    }

    #[test]
    fn exile_all_ejects_all_managed_agents() {
        let mut runner = FakeRunner::with_capture_queue([
            r#"jackin-agent-smith
jackin-agent-smith-clone-1"#
                .to_string(),
            String::new(),
        ]);

        exile_all(&mut runner).unwrap();

        assert_eq!(
            runner.recorded,
            vec![
                "docker ps -a --filter label=jackin.role=agent --format {{.Names}}",
                "docker ps -a --filter label=jackin.managed=true --format {{.Names}}\t{{.Label \"jackin.agent\"}}\t{{.Label \"jackin.role\"}}",
                "docker rm -f jackin-agent-smith",
                "docker rm -f jackin-agent-smith-dind",
                "docker volume rm jackin-agent-smith-dind-certs",
                "docker network rm jackin-agent-smith-net",
                "docker rm -f jackin-agent-smith-clone-1",
                "docker rm -f jackin-agent-smith-clone-1-dind",
                "docker volume rm jackin-agent-smith-clone-1-dind-certs",
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
                r#"jackin-agent-smith
jackin-agent-smith-clone-1"#
                    .to_string(),
                String::new(),
            ]),
            ..Default::default()
        };

        exile_all(&mut runner).unwrap();

        assert_eq!(
            runner.recorded,
            vec![
                "docker ps -a --filter label=jackin.role=agent --format {{.Names}}",
                "docker ps -a --filter label=jackin.managed=true --format {{.Names}}\t{{.Label \"jackin.agent\"}}\t{{.Label \"jackin.role\"}}",
                "docker rm -f jackin-agent-smith",
                "docker rm -f jackin-agent-smith-dind",
                "docker volume rm jackin-agent-smith-dind-certs",
                "docker network rm jackin-agent-smith-net",
                "docker rm -f jackin-agent-smith-clone-1",
                "docker rm -f jackin-agent-smith-clone-1-dind",
                "docker volume rm jackin-agent-smith-clone-1-dind-certs",
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
            "FROM projectjackin/construct:trixie\n",
        )
        .unwrap();
        std::fs::write(
            repo_dir.join("jackin.agent.toml"),
            r#"dockerfile = "Dockerfile"

[claude]
plugins = []
"#,
        )
        .unwrap();

        let mount_src = temp.path().join("test-mount");
        std::fs::create_dir_all(&mount_src).unwrap();
        std::fs::create_dir_all(&paths.config_dir).unwrap();

        let config_content = r#"[agents."chainargos/agent-brown"]
git = "git@github.com:chainargos/jackin-agent-brown.git"
trusted = true
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
            "FROM projectjackin/construct:trixie\n",
        )
        .unwrap();
        std::fs::write(
            repo_dir.join("jackin.agent.toml"),
            r#"dockerfile = "Dockerfile"

[claude]
plugins = ["code-review@claude-plugins-official"]
"#,
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
        assert!(
            runner
                .run_recorded
                .iter()
                .any(|call| call.contains("docker build "))
        );
        assert!(runner.recorded.iter().any(|call| {
            call == "docker ps -a --filter label=jackin.role=agent --format {{.Names}}"
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
            "FROM projectjackin/construct:trixie\n",
        )
        .unwrap();
        std::fs::write(
            repo_dir.join("jackin.agent.toml"),
            r#"dockerfile = "Dockerfile"

[claude]
plugins = []
"#,
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
            "FROM projectjackin/construct:trixie\n",
        )
        .unwrap();
        std::fs::write(
            repo_dir.join("jackin.agent.toml"),
            r#"dockerfile = "Dockerfile"

[claude]
plugins = []
"#,
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
            "FROM projectjackin/construct:trixie\n",
        )
        .unwrap();
        std::fs::write(
            repo_dir.join("jackin.agent.toml"),
            r#"dockerfile = "Dockerfile"

[claude]
plugins = ["code-review@claude-plugins-official"]
"#,
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
                .any(|call| call == "docker volume rm jackin-agent-smith-dind-certs")
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
            "FROM projectjackin/construct:trixie\n",
        )
        .unwrap();
        std::fs::write(
            repo_dir.join("jackin.agent.toml"),
            r#"dockerfile = "Dockerfile"

[claude]
plugins = []
"#,
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

        // TLS cert verification runs after docker info check
        assert!(runner.recorded.iter().any(|call| {
            call.contains("docker exec jackin-agent-smith-dind test -f /certs/client/ca.pem")
        }));
    }

    #[test]
    fn dind_certs_volume_derives_from_container_name() {
        assert_eq!(
            dind_certs_volume("jackin-agent-smith"),
            "jackin-agent-smith-dind-certs"
        );
        assert_eq!(
            dind_certs_volume("jackin-chainargos__the-architect-clone-2"),
            "jackin-chainargos__the-architect-clone-2-dind-certs"
        );
    }

    #[test]
    fn is_missing_cleanup_error_tolerates_all_resource_types() {
        let container_err =
            anyhow::anyhow!("Error response from daemon: No such container: jackin-agent-smith");
        let volume_err = anyhow::anyhow!(
            "Error response from daemon: No such volume: jackin-agent-smith-dind-certs"
        );
        let network_err =
            anyhow::anyhow!("Error response from daemon: No such network: jackin-agent-smith-net");
        let real_err = anyhow::anyhow!("Error response from daemon: permission denied");

        assert!(is_missing_cleanup_error(&container_err));
        assert!(is_missing_cleanup_error(&volume_err));
        assert!(is_missing_cleanup_error(&network_err));
        assert!(!is_missing_cleanup_error(&real_err));
    }

    #[test]
    fn load_agent_configures_dind_with_tls() {
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
            "FROM projectjackin/construct:trixie\n",
        )
        .unwrap();
        std::fs::write(
            repo_dir.join("jackin.agent.toml"),
            r#"dockerfile = "Dockerfile"

[claude]
plugins = []
"#,
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

        // DinD sidecar: TLS enabled with cert volume
        let dind_cmd = runner
            .recorded
            .iter()
            .find(|call| call.contains("docker run -d --name jackin-agent-smith-dind"))
            .unwrap();
        assert!(
            dind_cmd.contains("DOCKER_TLS_CERTDIR=/certs"),
            "DinD must enable TLS cert generation"
        );
        assert!(
            dind_cmd.contains("jackin-agent-smith-dind-certs:/certs/client"),
            "DinD must mount cert volume"
        );

        // Agent container: TLS client config
        let run_cmd = runner
            .recorded
            .iter()
            .find(|call| call.contains("docker run -it"))
            .unwrap();
        assert!(
            run_cmd.contains("DOCKER_HOST=tcp://jackin-agent-smith-dind:2376"),
            "agent must use TLS port 2376"
        );
        assert!(
            run_cmd.contains("DOCKER_TLS_VERIFY=1"),
            "agent must verify TLS"
        );
        assert!(
            run_cmd.contains("DOCKER_CERT_PATH=/certs/client"),
            "agent must know cert path"
        );
        assert!(
            run_cmd.contains("jackin-agent-smith-dind-certs:/certs/client:ro"),
            "agent must mount cert volume read-only"
        );
    }

    #[test]
    fn parse_repo_name_extracts_owner_repo_from_ssh_url() {
        assert_eq!(
            parse_repo_name("git@github.com:jackin-project/jackin.git"),
            Some("jackin-project/jackin".to_string())
        );
    }

    #[test]
    fn parse_repo_name_extracts_owner_repo_from_https_url() {
        assert_eq!(
            parse_repo_name("https://github.com/jackin-project/jackin.git"),
            Some("jackin-project/jackin".to_string())
        );
    }

    #[test]
    fn parse_repo_name_handles_url_without_git_suffix() {
        assert_eq!(
            parse_repo_name("https://github.com/jackin-project/jackin"),
            Some("jackin-project/jackin".to_string())
        );
        assert_eq!(
            parse_repo_name("git@github.com:jackin-project/jackin"),
            Some("jackin-project/jackin".to_string())
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
            "FROM projectjackin/construct:trixie\n",
        )
        .unwrap();
        std::fs::write(
            repo_dir.join("jackin.agent.toml"),
            r#"dockerfile = "Dockerfile"

[claude]
plugins = []
"#,
        )
        .unwrap();

        let mut runner =
            FakeRunner::with_capture_queue(["git@github.com:evil/agent-smith.git".to_string()]);
        let error = resolve_agent_repo(
            &paths,
            &selector,
            "https://github.com/jackin-project/jackin-agent-smith.git",
            &mut runner,
        )
        .unwrap_err();

        assert!(
            error
                .to_string()
                .contains("cached agent repo remote mismatch")
        );
    }

    #[test]
    fn resolve_agent_repo_recovers_when_user_confirms_removal() {
        let temp = tempdir().unwrap();
        let paths = JackinPaths::for_tests(temp.path());
        let selector = ClassSelector::new(None, "agent-smith");
        let repo_dir = paths.agents_dir.join("agent-smith");
        std::fs::create_dir_all(repo_dir.join(".git")).unwrap();
        std::fs::write(
            repo_dir.join("Dockerfile"),
            "FROM projectjackin/construct:trixie\n",
        )
        .unwrap();
        std::fs::write(
            repo_dir.join("jackin.agent.toml"),
            r#"dockerfile = "Dockerfile"

[claude]
plugins = []
"#,
        )
        .unwrap();

        // The capture queue provides: 1) the wrong remote URL, then 2) a
        // successful clone response (empty output).  After the user confirms,
        // the function removes the stale dir and re-clones.
        let mut runner = FakeRunner::with_capture_queue([
            "git@github.com:evil/agent-smith.git".to_string(),
            String::new(), // clone output
        ]);

        // Simulate what `git clone` would produce on disk: recreate the repo
        // files when the clone command is captured by FakeRunner.
        let repo_dir_clone = repo_dir.clone();
        runner.side_effects.push((
            "clone".to_string(),
            Box::new(move || {
                std::fs::create_dir_all(repo_dir_clone.join(".git")).unwrap();
                std::fs::write(
                    repo_dir_clone.join("Dockerfile"),
                    "FROM projectjackin/construct:trixie\n",
                )
                .unwrap();
                std::fs::write(
                    repo_dir_clone.join("jackin.agent.toml"),
                    r#"dockerfile = "Dockerfile"

[claude]
plugins = []
"#,
                )
                .unwrap();
            }),
        ));

        let result = resolve_agent_repo_with(
            &paths,
            &selector,
            "https://github.com/jackin-project/jackin-agent-smith.git",
            &mut runner,
            || Ok(true), // user confirms removal
        );

        assert!(result.is_ok(), "expected recovery to succeed: {result:?}");
        assert!(
            runner.recorded.iter().any(|c| c.contains("clone")),
            "expected a git clone after removal"
        );
    }

    #[test]
    fn resolve_agent_repo_aborts_when_user_declines_removal() {
        let temp = tempdir().unwrap();
        let paths = JackinPaths::for_tests(temp.path());
        let selector = ClassSelector::new(None, "agent-smith");
        let repo_dir = paths.agents_dir.join("agent-smith");
        std::fs::create_dir_all(repo_dir.join(".git")).unwrap();
        std::fs::write(
            repo_dir.join("Dockerfile"),
            "FROM projectjackin/construct:trixie\n",
        )
        .unwrap();
        std::fs::write(
            repo_dir.join("jackin.agent.toml"),
            r#"dockerfile = "Dockerfile"

[claude]
plugins = []
"#,
        )
        .unwrap();

        let mut runner =
            FakeRunner::with_capture_queue(["git@github.com:evil/agent-smith.git".to_string()]);
        let error = resolve_agent_repo_with(
            &paths,
            &selector,
            "https://github.com/jackin-project/jackin-agent-smith.git",
            &mut runner,
            || Ok(false), // user declines
        )
        .unwrap_err();

        assert!(
            error
                .to_string()
                .contains("cached agent repo remote mismatch")
        );
        // The cached repo directory should still exist
        assert!(repo_dir.join(".git").is_dir());
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
            "FROM projectjackin/construct:trixie\n",
        )
        .unwrap();
        std::fs::write(
            repo_dir.join("jackin.agent.toml"),
            r#"dockerfile = "Dockerfile"

[claude]
plugins = []
"#,
        )
        .unwrap();

        let mut runner = FakeRunner::with_capture_queue([
            "git@github.com:jackin-project/jackin-agent-smith.git".to_string(),
            "?? scratch.txt".to_string(),
        ]);
        let error = resolve_agent_repo(
            &paths,
            &selector,
            "https://github.com/jackin-project/jackin-agent-smith.git",
            &mut runner,
        )
        .unwrap_err();

        assert!(error.to_string().contains("contains local changes"));
    }

    #[test]
    fn resolve_agent_repo_uses_run_for_clone_after_recovery() {
        let temp = tempdir().unwrap();
        let paths = JackinPaths::for_tests(temp.path());
        let selector = ClassSelector::new(None, "agent-smith");
        let repo_dir = paths.agents_dir.join("agent-smith");
        std::fs::create_dir_all(repo_dir.join(".git")).unwrap();
        std::fs::write(
            repo_dir.join("Dockerfile"),
            "FROM projectjackin/construct:trixie\n",
        )
        .unwrap();
        std::fs::write(
            repo_dir.join("jackin.agent.toml"),
            r#"dockerfile = "Dockerfile"

[claude]
plugins = []
"#,
        )
        .unwrap();

        let mut runner =
            FakeRunner::with_capture_queue(["git@github.com:evil/agent-smith.git".to_string()]);
        let repo_dir_clone = repo_dir.clone();
        runner.side_effects.push((
            "clone".to_string(),
            Box::new(move || {
                std::fs::create_dir_all(repo_dir_clone.join(".git")).unwrap();
                std::fs::write(
                    repo_dir_clone.join("Dockerfile"),
                    "FROM projectjackin/construct:trixie\n",
                )
                .unwrap();
                std::fs::write(
                    repo_dir_clone.join("jackin.agent.toml"),
                    r#"dockerfile = "Dockerfile"

[claude]
plugins = []
"#,
                )
                .unwrap();
            }),
        ));

        let result = resolve_agent_repo_with(
            &paths,
            &selector,
            "https://github.com/jackin-project/jackin-agent-smith.git",
            &mut runner,
            || Ok(true),
        );

        assert!(result.is_ok(), "expected recovery to succeed: {result:?}");
        assert!(runner.run_recorded.iter().any(|call| {
            call.contains("git clone https://github.com/jackin-project/jackin-agent-smith.git")
        }));
    }

    #[test]
    fn resolve_agent_repo_uses_run_for_pull_on_clean_repo() {
        let temp = tempdir().unwrap();
        let paths = JackinPaths::for_tests(temp.path());
        let selector = ClassSelector::new(None, "agent-smith");
        let repo_dir = paths.agents_dir.join("agent-smith");
        std::fs::create_dir_all(repo_dir.join(".git")).unwrap();
        std::fs::write(
            repo_dir.join("Dockerfile"),
            "FROM projectjackin/construct:trixie\n",
        )
        .unwrap();
        std::fs::write(
            repo_dir.join("jackin.agent.toml"),
            r#"dockerfile = "Dockerfile"

[claude]
plugins = []
"#,
        )
        .unwrap();

        let mut runner = FakeRunner::with_capture_queue([
            "git@github.com:jackin-project/jackin-agent-smith.git".to_string(),
            String::new(),
        ]);

        let result = resolve_agent_repo(
            &paths,
            &selector,
            "https://github.com/jackin-project/jackin-agent-smith.git",
            &mut runner,
        );

        assert!(
            result.is_ok(),
            "expected clean repo update to succeed: {result:?}"
        );
        assert!(
            runner
                .run_recorded
                .iter()
                .any(|call| call.contains("git -C") && call.contains("pull --ff-only"))
        );
    }

    #[test]
    fn list_managed_agent_names_excludes_dind_sidecars() {
        let mut runner = FakeRunner::with_capture_queue(["jackin-agent-smith".to_string()]);

        let names = list_managed_agent_names(&mut runner).unwrap();

        assert_eq!(names, vec!["jackin-agent-smith"]);
        assert!(runner.recorded.iter().any(|call| {
            call == "docker ps -a --filter label=jackin.role=agent --format {{.Names}}"
        }));
    }

    #[test]
    fn list_managed_agent_names_includes_legacy_agents_without_role_label() {
        let mut runner =
            FakeRunner::with_capture_queue([String::new(), "jackin-agent-smith\t\t".to_string()]);

        let names = list_managed_agent_names(&mut runner).unwrap();

        assert_eq!(names, vec!["jackin-agent-smith"]);
        assert!(runner.recorded.iter().any(|call| {
            call == "docker ps -a --filter label=jackin.managed=true --format {{.Names}}\t{{.Label \"jackin.agent\"}}\t{{.Label \"jackin.role\"}}"
        }));
    }

    #[test]
    fn list_running_agent_display_names_excludes_dind_sidecars() {
        let mut runner =
            FakeRunner::with_capture_queue(["jackin-agent-smith\tAgent Smith".to_string()]);

        let names = list_running_agent_display_names(&mut runner).unwrap();

        assert_eq!(names, vec!["Agent Smith"]);
        assert!(runner.recorded.iter().any(|call| {
            call == "docker ps --filter label=jackin.role=agent --format {{.Names}}\t{{.Label \"jackin.display_name\"}}"
        }));
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
            "FROM projectjackin/construct:trixie\n",
        )
        .unwrap();
        std::fs::write(
            repo_dir.join("jackin.agent.toml"),
            r#"dockerfile = "Dockerfile"

[identity]
name = "Agent Smith"

[claude]
plugins = []
"#,
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

    #[test]
    fn load_agent_sets_claude_env_to_jackin() {
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
            "FROM projectjackin/construct:trixie\n",
        )
        .unwrap();
        std::fs::write(
            repo_dir.join("jackin.agent.toml"),
            r#"dockerfile = "Dockerfile"

[claude]
plugins = []
"#,
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
        assert!(run_cmd.contains("-e JACKIN_CLAUDE_ENV=jackin"));
        assert!(run_cmd.contains("-e JACKIN_DIND_HOSTNAME=jackin-agent-smith-dind"));
        assert!(!run_cmd.contains("JACKIN_DEBUG"));
    }

    #[test]
    fn load_agent_passes_debug_flag_when_enabled() {
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
            "FROM projectjackin/construct:trixie\n",
        )
        .unwrap();
        std::fs::write(
            repo_dir.join("jackin.agent.toml"),
            r#"dockerfile = "Dockerfile"

[claude]
plugins = []
"#,
        )
        .unwrap();

        let workspace = repo_workspace(&repo_dir);
        let opts = LoadOptions {
            debug: true,
            ..LoadOptions::default()
        };
        load_agent(
            &paths,
            &mut config,
            &selector,
            &workspace,
            &mut runner,
            &opts,
        )
        .unwrap();

        let run_cmd = runner
            .recorded
            .iter()
            .find(|call| call.contains("docker run -it"))
            .unwrap();
        assert!(run_cmd.contains("-e JACKIN_DEBUG=1"));
    }

    // -- orphaned resource GC -------------------------------------------------

    #[test]
    fn gc_removes_orphaned_dind_and_network() {
        let mut runner = FakeRunner::with_capture_queue([
            // collect_orphaned_dind: docker ps -a --filter label=jackin.role=dind
            "jackin-agent-smith-dind\tjackin-agent-smith".to_string(),
            // collect_orphaned_dind: list_agent_names (running)
            String::new(),
            // gc_orphaned_networks: docker network ls
            String::new(),
        ]);

        gc_orphaned_resources(&mut runner);

        assert!(
            runner
                .recorded
                .iter()
                .any(|c| c.contains("docker rm -f jackin-agent-smith-dind"))
        );
        assert!(
            runner
                .recorded
                .iter()
                .any(|c| c.contains("docker rm -f jackin-agent-smith"))
        );
        assert!(
            runner
                .recorded
                .iter()
                .any(|c| c.contains("docker volume rm jackin-agent-smith-dind-certs"))
        );
        assert!(
            runner
                .recorded
                .iter()
                .any(|c| c.contains("docker network rm jackin-agent-smith-net"))
        );
    }

    #[test]
    fn gc_skips_dind_when_agent_is_running() {
        let mut runner = FakeRunner::with_capture_queue([
            // collect_orphaned_dind: docker ps -a --filter label=jackin.role=dind
            "jackin-agent-smith-dind\tjackin-agent-smith".to_string(),
            // collect_orphaned_dind: legacy managed sidecars without role labels
            String::new(),
            // collect_orphaned_dind: running role-labeled agents — agent IS running
            "jackin-agent-smith".to_string(),
            // collect_orphaned_dind: running legacy agents without role labels
            String::new(),
            // gc_orphaned_networks: docker network ls
            String::new(),
        ]);

        gc_orphaned_resources(&mut runner);

        assert!(
            !runner
                .recorded
                .iter()
                .any(|c| c.contains("docker rm -f jackin-agent-smith-dind"))
        );
    }

    #[test]
    fn gc_does_nothing_when_no_orphans() {
        let mut runner = FakeRunner::with_capture_queue([
            // collect_orphaned_dind: no DinD sidecars
            String::new(),
            // gc_orphaned_networks: no networks
            String::new(),
        ]);

        gc_orphaned_resources(&mut runner);

        assert!(!runner.recorded.iter().any(|c| c.contains("docker rm")));
    }

    #[test]
    fn gc_removes_orphaned_network_without_dind() {
        let mut runner = FakeRunner::with_capture_queue([
            // collect_orphaned_dind: no DinD sidecars
            String::new(),
            // gc_orphaned_networks: docker network ls — has a network
            "jackin-agent-smith-net\tjackin-agent-smith".to_string(),
            // gc_orphaned_networks: list_agent_names (running) — agent not running
            String::new(),
        ]);

        gc_orphaned_resources(&mut runner);

        assert!(
            runner
                .recorded
                .iter()
                .any(|c| c.contains("docker network rm jackin-agent-smith-net"))
        );
    }

    #[test]
    fn gc_cleans_multiple_orphans() {
        let mut runner = FakeRunner::with_capture_queue([
            // collect_orphaned_dind: two orphaned DinD sidecars
            "jackin-agent-smith-dind\tjackin-agent-smith\njackin-neo-dind\tjackin-neo".to_string(),
            // collect_orphaned_dind: list_agent_names (running)
            String::new(),
            // gc_orphaned_networks: no additional networks
            String::new(),
        ]);

        gc_orphaned_resources(&mut runner);

        assert!(
            runner
                .recorded
                .iter()
                .any(|c| c.contains("docker rm -f jackin-agent-smith-dind"))
        );
        assert!(
            runner
                .recorded
                .iter()
                .any(|c| c.contains("docker volume rm jackin-agent-smith-dind-certs"))
        );
        assert!(
            runner
                .recorded
                .iter()
                .any(|c| c.contains("docker rm -f jackin-neo-dind"))
        );
        assert!(
            runner
                .recorded
                .iter()
                .any(|c| c.contains("docker volume rm jackin-neo-dind-certs"))
        );
        assert!(
            runner
                .recorded
                .iter()
                .any(|c| c.contains("docker network rm jackin-neo-net"))
        );
    }

    #[test]
    fn gc_removes_legacy_orphaned_dind_without_role_label() {
        let mut runner = FakeRunner::with_capture_queue([
            // collect_orphaned_dind: role-labeled DinD sidecars
            String::new(),
            // collect_orphaned_dind: legacy managed sidecars without role labels
            "jackin-agent-smith-dind\tjackin-agent-smith\t".to_string(),
            // collect_orphaned_dind: running role-labeled agents
            String::new(),
            // collect_orphaned_dind: running legacy agents without role labels
            String::new(),
            // gc_orphaned_networks: no additional networks
            String::new(),
        ]);

        gc_orphaned_resources(&mut runner);

        assert!(
            runner
                .recorded
                .iter()
                .any(|c| c.contains("docker rm -f jackin-agent-smith-dind"))
        );
        assert!(
            runner
                .recorded
                .iter()
                .any(|c| c.contains("docker rm -f jackin-agent-smith"))
        );
    }
}
