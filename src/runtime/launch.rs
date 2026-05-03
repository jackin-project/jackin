use crate::config::AppConfig;
use crate::docker::{CommandRunner, RunOptions};
use crate::instance::{RoleState, primary_container_name};
use crate::paths::JackinPaths;
use crate::selector::RoleSelector;
use crate::tui;
use crate::version_check;
use fs2::FileExt;
use owo_colors::OwoColorize;
use std::io::IsTerminal;

use super::attach::{ContainerState, inspect_container_state, wait_for_dind};
use super::cleanup::{gc_orphaned_resources, run_cleanup_command};
use super::discovery::list_running_agent_display_names;
use super::identity::{GitIdentity, build_config_rows, load_git_identity, load_host_identity};
use super::image::build_agent_image;
use super::naming::{
    LABEL_KEEP_AWAKE, LABEL_MANAGED, LABEL_ROLE_AGENT, LABEL_ROLE_DIND, dind_certs_volume,
    format_role_display, image_name,
};
use super::repo_cache::resolve_agent_repo;

// Four launch-time toggles (no_intro / debug / rebuild / force) all map
// directly to CLI flags; bundling them into nested structs would obscure
// rather than clarify the call sites.
#[allow(clippy::struct_excessive_bools)]
pub struct LoadOptions {
    pub no_intro: bool,
    pub debug: bool,
    pub rebuild: bool,

    /// Bypass interactive preflight gates (e.g. dirty host repo).
    /// Wired through to `PreflightContext.force` during workspace
    /// materialization.
    pub force: bool,

    /// Optional test seam: inject a custom `OpRunner` for `op://`
    /// resolution. `None` (the production default) means
    /// `resolve_operator_env` picks the default `OpCli::new()`.
    pub op_runner: Option<Box<dyn crate::operator_env::OpRunner>>,

    /// Optional test seam: inject a host-env lookup map. `None` (the
    /// production default) means `resolve_operator_env` reads from
    /// `std::env::var`. When `Some(map)`, `$NAME` / `${NAME}`
    /// references are resolved by looking up `name` in `map`.
    pub host_env: Option<std::collections::BTreeMap<String, String>>,

    /// CLI override for the agent. `None` means "use the workspace's
    /// `default_agent` field, falling back to `Agent::Claude` when unset".
    pub agent: Option<crate::agent::Agent>,
}

impl LoadOptions {
    /// Build options for `jackin load`. Debug mode implies `no_intro`.
    pub fn for_load(no_intro: bool, debug: bool, rebuild: bool) -> Self {
        Self {
            no_intro: no_intro || debug,
            debug,
            rebuild,
            force: false,
            op_runner: None,
            host_env: None,
            agent: None,
        }
    }

    /// Build options for the operator console (`jackin console`).
    /// Debug mode implies `no_intro`.
    pub fn for_launch(debug: bool) -> Self {
        Self {
            no_intro: debug,
            debug,
            rebuild: false,
            force: false,
            op_runner: None,
            host_env: None,
            agent: None,
        }
    }
}

impl Default for LoadOptions {
    fn default() -> Self {
        Self {
            no_intro: true,
            debug: false,
            rebuild: false,
            force: false,
            op_runner: None,
            host_env: None,
            agent: None,
        }
    }
}

/// Resolve which agent to launch under. CLI flag wins; falls back
/// to the workspace's `agent` field; otherwise defaults to Claude.
fn resolve_agent(
    cli_override: Option<crate::agent::Agent>,
    workspace_agent: Option<crate::agent::Agent>,
) -> crate::agent::Agent {
    cli_override
        .or(workspace_agent)
        .unwrap_or(crate::agent::Agent::Claude)
}

fn validate_agent_supported(
    selector: &RoleSelector,
    manifest: &crate::manifest::RoleManifest,
    agent: crate::agent::Agent,
) -> anyhow::Result<()> {
    let supported = manifest.supported_agents();
    if supported.contains(&agent) {
        return Ok(());
    }

    let supported_list = supported
        .iter()
        .map(|h| h.slug())
        .collect::<Vec<_>>()
        .join(", ");
    anyhow::bail!(
        "role \"{}\" does not support agent \"{}\"; supported: [{}]",
        selector.key(),
        agent.slug(),
        supported_list
    );
}

fn verify_required_agent_env(
    agent: crate::agent::Agent,
    resolved_env: &crate::env_resolver::ResolvedEnv,
) -> anyhow::Result<()> {
    let profile = crate::agent::profile::profile(agent);
    let missing = profile
        .required_env
        .into_iter()
        .filter(|required| {
            !resolved_env
                .vars
                .iter()
                .any(|(key, value)| key == required && !value.is_empty())
        })
        .collect::<Vec<_>>();

    if missing.is_empty() {
        return Ok(());
    }

    anyhow::bail!(
        "agent \"{}\" requires {} in the resolved launch env; declare it in workspace/global env or role manifest env",
        agent.slug(),
        missing.join(", ")
    );
}

struct StepCounter {
    current: u32,
    quiet: bool,
    role_name: String,
}

impl StepCounter {
    fn new(quiet: bool, role_name: &str) -> Self {
        Self {
            current: 0,
            quiet,
            role_name: role_name.to_string(),
        }
    }

    fn next(&mut self, text: &str) {
        self.current += 1;
        tui::set_terminal_title(&format!("{} \u{2014} {text}", self.role_name));
        if self.quiet {
            tui::step_quiet(self.current, text);
        } else {
            tui::step_shimmer(self.current, text);
        }
    }

    fn done(&self) {
        tui::set_terminal_title(&self.role_name);
    }
}

// ── Terminal / terminfo resolution ────────────────────────────────────────
//
// Modern terminals (Ghostty, Kitty, WezTerm) define custom TERM values
// whose terminfo entries don't ship in Debian's ncurses-base.  Rather
// than falling back to xterm-256color (which loses terminal-specific
// capabilities), we export the host's terminfo entry, compile it into a
// cache directory, and mount it read-only into the container.

/// Terminal types that ship with Debian's `ncurses-base` package and can
/// be forwarded into the container without an extra terminfo mount.
const STANDARD_TERMS: &[&str] = &[
    "ansi",
    "dumb",
    "linux",
    "rxvt",
    "rxvt-unicode",
    "rxvt-unicode-256color",
    "screen",
    "screen-256color",
    "tmux",
    "tmux-256color",
    "vt100",
    "vt220",
    "xterm",
    "xterm-256color",
    "xterm-color",
];

/// Returns the per-agent mount strings in jackin's "src:dst" /
/// "src:dst:ro" idiom, ready to be passed to `docker run -v`.
///
/// The agent variant is read directly off `state.agent_runtime` rather
/// than passed in — the prior shape took a separate `agent` parameter
/// plus an `Option<PathBuf>` field on the state, with a runtime
/// `expect()` enforcing "Some iff agent == Codex" across two
/// functions. The enum variant on `RoleState` makes that invariant
/// compile-checked: an exhaustive match here cannot construct a
/// codex-mounts arm without a `config_toml` path in scope.
fn agent_mounts(state: &crate::instance::RoleState) -> Vec<String> {
    use crate::instance::AgentRuntimeState;

    match &state.agent_runtime {
        AgentRuntimeState::Claude {
            dir,
            json,
            plugins_json,
        } => vec![
            format!("{}:/home/agent/.claude", dir.display()),
            format!("{}:/home/agent/.claude.json", json.display()),
            format!(
                "{}:/home/agent/.jackin/plugins.json:ro",
                plugins_json.display()
            ),
        ],
        AgentRuntimeState::Codex { config_toml } => vec![format!(
            "{}:/home/agent/.codex/config.toml",
            config_toml.display()
        )],
    }
}

/// Translate a [`MaterializedWorkspace`] into the `-v` argument values
/// for `docker run`. Pulled out of `load_role_with` so the mount-flag
/// shape — including the `:ro` placement on worktree-mode override
/// files — can be unit-tested without docker mocks.
///
/// For each mount, the worktree dir / shared bind goes first; when the
/// mount is worktree-mode, three auxiliary entries follow:
///
/// 1. Host's `.git/` at `/jackin/host/<dst-stripped>/.git` (rw).
///    Includes the per-worktree admin dir at `worktrees/<container>/`
///    natively (no separate admin mount).
/// 2. `.git` pointer override at `<dst>/.git` (`:ro`). Redirects gitdir
///    to the admin entry inside the host `.git/` mount.
/// 3. `gitdir` back-pointer override at
///    `/jackin/host/<dst-stripped>/.git/worktrees/<container>/gitdir`
///    (`:ro`). Matches the worktree's `<dst>/.git` location so git's
///    verification check passes inside the container.
///
/// `:ro` on the override files is defensive hardening: git only reads
/// them during normal role work, and a misbehaving role could
/// otherwise rewrite the gitdir pointer to redirect operations at a
/// different repo entirely.
fn build_workspace_mount_strings(
    workspace: &crate::isolation::materialize::MaterializedWorkspace,
) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for mount in crate::isolation::materialize::mount_order_for_docker(workspace) {
        let suffix = if mount.readonly { ":ro" } else { "" };
        out.push(format!("{}:{}{}", mount.bind_src, mount.dst, suffix));
        if let Some(aux) = &mount.worktree_aux {
            out.push(format!("{}:{}", aux.host_git_dir, aux.host_git_target));
            out.push(format!(
                "{}:{}:ro",
                aux.git_file_override, aux.git_file_target
            ));
            out.push(format!(
                "{}:{}:ro",
                aux.gitdir_back_override, aux.gitdir_back_target
            ));
        }
    }
    out
}

/// Resolve the TERM value and an optional terminfo bind-mount for the
/// container.
///
/// Returns `(term_value, Some(mount_string))` when the host's terminfo
/// was exported, or `(term_value, None)` when the TERM is standard or
/// export failed (in which case `term_value` is the safe fallback).
fn resolve_terminal_setup(cache_dir: &std::path::Path) -> (String, Option<String>) {
    let host_term = std::env::var("TERM").unwrap_or_default();

    if host_term.is_empty() {
        return ("xterm-256color".to_string(), None);
    }

    if STANDARD_TERMS.contains(&host_term.as_str()) {
        return (host_term, None);
    }

    // Exotic terminal — try to export and compile the terminfo entry.
    export_host_terminfo(&host_term, cache_dir).map_or_else(
        |_| ("xterm-256color".to_string(), None),
        |terminfo_dir| {
            let mount = format!("{}:/home/agent/.terminfo:ro", terminfo_dir.display());
            (host_term, Some(mount))
        },
    )
}

/// Export the host's terminfo entry for `term` into `cache_dir/terminfo/`.
///
/// Uses `infocmp -x` to dump the source and `tic -x -o` to compile it.
/// The compiled output is a small architecture-independent binary that
/// can be mounted directly into any container.
fn export_host_terminfo(
    term: &str,
    cache_dir: &std::path::Path,
) -> anyhow::Result<std::path::PathBuf> {
    let terminfo_dir = cache_dir.join("terminfo");

    // Check if already cached (first letter dir + entry file).
    let first_char = term.chars().next().unwrap_or('x');
    let entry_path = terminfo_dir.join(first_char.to_string()).join(term);
    if entry_path.exists() {
        return Ok(terminfo_dir);
    }

    // Export the source from the host.
    let infocmp = std::process::Command::new("infocmp")
        .args(["-x", term])
        .output()?;
    anyhow::ensure!(infocmp.status.success(), "infocmp failed for {term}");

    std::fs::create_dir_all(&terminfo_dir)?;

    // Compile into the cache directory.
    // Suppress stderr — tic emits harmless warnings for some terminal
    // entries (e.g. Ghostty's "alias multiply defined" notice).
    let tic = std::process::Command::new("tic")
        .args(["-x", "-o"])
        .arg(&terminfo_dir)
        .arg("-")
        .stdin(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .spawn();
    let mut tic = tic?;
    if let Some(ref mut stdin) = tic.stdin {
        use std::io::Write;
        stdin.write_all(&infocmp.stdout)?;
    }
    let status = tic.wait()?;
    anyhow::ensure!(
        status.success(),
        "tic failed to compile terminfo for {term}"
    );

    Ok(terminfo_dir)
}

// ── Role source trust ───────────────────────────────────────────────────

/// Display an untrusted-role warning and ask the operator to confirm.
/// Aborts when stdin is not a terminal or the operator declines.
fn confirm_agent_trust(
    selector: &RoleSelector,
    source: &crate::config::RoleSource,
) -> anyhow::Result<()> {
    if !std::io::stdin().is_terminal() {
        anyhow::bail!(
            "untrusted role source \"{selector}\" from {}\n\
             Trust it first: `jackin config trust grant {selector}`, or add `trusted = true` in config.toml.",
            source.git,
        );
    }

    eprintln!();
    eprintln!("{}", "!! Untrusted role source !!".red().bold());
    eprintln!();
    eprintln!("  role:  {}", selector.to_string().bold());
    eprintln!("  source: {}", source.git.yellow());
    eprintln!();
    eprintln!(
        "  {}",
        "jackin' has never loaded this role before. Trusting it means:".bold()
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
        "    {} The role will have access to your {}",
        "-".dimmed(),
        "mounted workspace files".bold()
    );
    eprintln!();
    eprintln!("  {}", "Review the repository before trusting it.".dimmed());
    eprintln!();

    let confirmed = dialoguer::Confirm::new()
        .with_prompt("Do you trust this role source and want to proceed?")
        .default(false)
        .interact()?;

    if !confirmed {
        anyhow::bail!(
            "role source \"{selector}\" not trusted — aborting.\n\
             To trust it later, run `jackin config trust grant {selector}` or try loading again."
        );
    }

    Ok(())
}

struct LaunchContext<'a> {
    container_name: &'a str,
    image: &'a str,
    network: &'a str,
    dind: &'a str,
    selector: &'a RoleSelector,
    agent_display_name: &'a str,
    workspace: &'a crate::isolation::materialize::MaterializedWorkspace,
    state: &'a RoleState,
    git: &'a GitIdentity,
    debug: bool,
    agent: crate::agent::Agent,
    resolved_env: &'a crate::env_resolver::ResolvedEnv,
    cache_dir: &'a std::path::Path,
    /// Required so `launch_role_runtime` can fire the `keep_awake`
    /// reconciler between `docker run -d` and the foreground `docker
    /// attach`. Without that mid-flight call, caffeinate would never
    /// spawn for an interactive `jackin load`: the post-launch
    /// reconcile in `app::Command::Load` only runs after attach
    /// returns, by which time the container has stopped and the
    /// `keep_awake` count is back to zero.
    paths: &'a JackinPaths,
}

/// Create the Docker network, start `DinD`, and launch the role container.
#[allow(clippy::too_many_lines)]
fn launch_role_runtime(
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
        agent,
        resolved_env,
        cache_dir,
        paths,
    } = ctx;

    let certs_volume = dind_certs_volume(container_name);

    let docker_run_opts = RunOptions {
        quiet: !debug,
        ..RunOptions::default()
    };

    // Create Docker network
    let role_label = format!("jackin.role={container_name}");
    runner.run(
        "docker",
        &[
            "network",
            "create",
            "--label",
            LABEL_MANAGED,
            "--label",
            &role_label,
            network,
        ],
        None,
        &docker_run_opts,
    )?;

    // Start Docker-in-Docker with TLS.
    //
    // `DOCKER_TLS_SAN` is read by docker:dind's `dockerd-entrypoint.sh` and
    // appended to the auto-generated server cert's Subject Alternative Names.
    // Without it, the cert only covers the short container ID, `docker`, and
    // `localhost` — so roles connecting via `tcp://{dind}:2376` get a TLS
    // hostname-mismatch error. We can't set `--hostname` to the same value
    // because namespaced class keys contain `__`, which is invalid in
    // RFC-1123 hostnames.
    //
    // The entrypoint concatenates `DOCKER_TLS_SAN` into the openssl config
    // verbatim (no type prefix added), so the value must already be in the
    // `DNS:<name>` form that openssl's `subjectAltName` section expects.
    // Without the prefix, openssl aborts with `v2i_GENERAL_NAME_ex: missing
    // value` and DinD never comes up.
    let certs_dind_mount = format!("{certs_volume}:/certs/client");
    let dind_tls_san = format!("DOCKER_TLS_SAN=DNS:{dind}");
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
        &role_label,
        "-e",
        "DOCKER_TLS_CERTDIR=/certs",
        "-e",
        &dind_tls_san,
        "-v",
        &certs_dind_mount,
        "docker:dind",
    ];
    runner.run("docker", &dind_args, None, &docker_run_opts)?;

    wait_for_dind(dind, &certs_volume, runner, *debug)?;

    // Step 4: Mount volumes and launch
    steps.next("Launching role");
    steps.done();

    tui::print_deploying(agent_display_name);

    let class_label = format!("jackin.class={}", selector.key());
    let display_label = format!("jackin.display_name={agent_display_name}");
    let docker_host = format!("DOCKER_HOST=tcp://{dind}:2376");
    let dind_hostname = format!("{}={dind}", crate::env_model::JACKIN_DIND_HOSTNAME_ENV_NAME);
    let git_author_name = format!("GIT_AUTHOR_NAME={}", git.user_name);
    let git_author_email = format!("GIT_AUTHOR_EMAIL={}", git.user_email);
    let agent_specific_mounts = agent_mounts(state);
    let gh_config_mount = format!("{}:/home/agent/.config/gh", state.gh_config_dir.display());
    let certs_agent_mount = format!("{certs_volume}:/certs/client:ro");
    let jackin_agent_env = format!(
        "{}={}",
        crate::env_model::JACKIN_AGENT_ENV_NAME,
        agent.slug()
    );
    let jackin_role_env = format!(
        "{}={}",
        crate::env_model::JACKIN_ROLE_ENV_NAME,
        selector.key()
    );

    // Forward the host TERM so the container's terminal type matches what the
    // terminal emulator actually supports.  Docker defaults to TERM=xterm which
    // can cause input handling issues (e.g. paste not working) in applications
    // that adjust behaviour based on terminal capabilities.
    //
    // For exotic terminals (Ghostty, Kitty, WezTerm, etc.) whose terminfo
    // entries don't ship in Debian's ncurses-base, we export and compile the
    // host's terminfo into a cache directory and mount it into the container.
    let (resolved_term, terminfo_mount) = resolve_terminal_setup(cache_dir);
    let container_term = format!("TERM={resolved_term}");

    // Start detached with a persistent TTY, then attach separately.  This
    // decouples the container's lifetime from the foreground attach, so
    // closing the terminal tab only drops the attach — the container keeps
    // running and `jackin hardline` can reconnect to the same live session.
    //
    // No `--rm` here, intentionally.  Omitting `--rm` lets the container
    // persist after exit so that:
    //   - A crashed container's logs and filesystem remain available for
    //     diagnosis, and `jackin hardline` can restart the exact same
    //     container in place without rebuilding the network stack.
    //   - Clean-exit containers (exit 0, not OOM-killed) are removed by
    //     `cleanup` at the end of a normal session, and the
    //     `claim_container_name` loop reclaims those slots on the next load
    //     by inspecting per-candidate state rather than relying on Docker
    //     auto-removal.
    //
    // Using the naming loop rather than `--rm` as the removal mechanism is
    // the right boundary: the loop can inspect the exit code and OOM flag
    // and make a correct per-container decision (reclaim vs. preserve for
    // hardline), whereas `--rm` would indiscriminately destroy every exited
    // container, making crash recovery impossible.
    let mut run_args: Vec<&str> = vec![
        "run",
        "-d",
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
    ];

    if workspace.keep_awake_enabled {
        run_args.extend_from_slice(&["--label", LABEL_KEEP_AWAKE]);
    }

    run_args.extend_from_slice(&[
        // JACKIN_* runtime metadata is injected by jackin, not declared in role manifests.
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
        "-e",
        &container_term,
    ]);
    if *debug {
        run_args.extend_from_slice(&["-e", "JACKIN_DEBUG=1"]);
    }

    // Forward JACKIN_DISABLE_* env vars from the host so the operator can
    // disable security tools (tirith, shellfirm) without rebuilding the image.
    let mut passthrough_strings: Vec<String> = Vec::new();
    for (key, value) in std::env::vars() {
        if key.starts_with("JACKIN_DISABLE_") {
            passthrough_strings.push(format!("{key}={value}"));
        }
    }
    for env_str in &passthrough_strings {
        run_args.push("-e");
        run_args.push(env_str);
    }

    let mut env_strings: Vec<String> = Vec::new();
    env_strings.push(format!(
        "{}={}",
        crate::env_model::JACKIN_RUNTIME_ENV_NAME,
        crate::env_model::JACKIN_RUNTIME_ENV_VALUE
    ));
    for (key, value) in &resolved_env.vars {
        if crate::env_model::is_reserved(key) {
            continue;
        }
        env_strings.push(format!("{key}={value}"));
    }
    for env_str in &env_strings {
        run_args.push("-e");
        run_args.push(env_str);
    }
    run_args.extend_from_slice(&[
        "-e",
        &jackin_agent_env,
        "-e",
        &jackin_role_env,
        "-v",
        &certs_agent_mount,
        "-v",
        &gh_config_mount,
    ]);
    for mount in &agent_specific_mounts {
        run_args.push("-v");
        run_args.push(mount);
    }

    if let Some(ref ti_mount) = terminfo_mount {
        run_args.extend_from_slice(&["-v", ti_mount]);
    }

    let mount_strings = build_workspace_mount_strings(workspace);
    for ms in &mount_strings {
        run_args.push("-v");
        run_args.push(ms);
    }
    run_args.push(image);
    runner.run("docker", &run_args, None, &docker_run_opts)?;

    // Reconcile keep_awake AFTER the role container is running but
    // BEFORE the foreground attach blocks. This is the only window in
    // which an interactive `jackin load` can spawn caffeinate: the
    // pre-launch reconcile in `app::Command::Load` runs before the
    // container exists (count=0 → no-op), and the post-launch
    // reconcile only runs after attach returns, by which time the
    // container has stopped (count=0 again → no-op). Without this
    // mid-flight call the feature would never hold a power assertion
    // for a single interactive session.
    super::caffeinate::reconcile(paths, runner);

    // Attach with signal forwarding disabled and the default detach shortcut
    // cleared: only an explicit exit from inside (or terminal close) ends the
    // foreground session, and closing the terminal leaves the container
    // running so `jackin hardline` can reconnect.
    let attach_result = runner.run(
        "docker",
        &[
            "attach",
            "--detach-keys=",
            "--sig-proxy=false",
            container_name,
        ],
        None,
        &RunOptions::default(),
    );
    // Ensure cleanup debug logs start on a fresh line after the interactive session
    eprintln!();
    attach_result?;

    Ok(())
}

/// Query a container's post-attach state for use by `finalize_foreground_session`.
///
/// Returns `AttachOutcome::still_running` when the container is still running
/// (terminal closed / detach), `AttachOutcome::oom_killed` when the kernel
/// killed the container OOM, otherwise `AttachOutcome::stopped(exit_code)`.
///
/// Capture failures (docker daemon hiccup, container removed mid-inspect)
/// are mapped to `still_running()` — the **conservative** default. Returning
/// `stopped(0)` here would route the call through `finalize_clean_exit`,
/// which combined with any concurrent git failure inside `assess_cleanup`
/// could auto-delete worktrees of containers that may actually still be
/// running. `still_running()` instead skips the auto-cleanup path entirely
/// and preserves records for `jackin hardline` to recover.
#[allow(clippy::unnecessary_wraps)] // Result preserved so callers' `?` keeps working without a churn-y signature change
pub fn inspect_attach_outcome(
    runner: &mut impl crate::docker::CommandRunner,
    container: &str,
) -> anyhow::Result<crate::isolation::finalize::AttachOutcome> {
    use crate::isolation::finalize::AttachOutcome;
    let state = match runner.capture(
        "docker",
        &[
            "inspect",
            "-f",
            "{{.State.Status}}|{{.State.ExitCode}}|{{.State.OOMKilled}}",
            container,
        ],
        None,
    ) {
        Ok(s) => s,
        Err(e) => {
            crate::debug_log!(
                "isolation",
                "inspect_attach_outcome: docker inspect failed for {container}: {e}; treating as still_running (conservative — finalize_clean_exit's auto-cleanup never fires)",
            );
            return Ok(AttachOutcome::still_running());
        }
    };
    let parts: Vec<&str> = state.trim().split('|').collect();
    let status = parts.first().copied().unwrap_or("");
    let exit_code = parts.get(1).and_then(|s| s.parse::<i32>().ok());
    let oom = parts.get(2).copied().unwrap_or("") == "true";
    // Only `exited` legitimately routes through finalize_clean_exit.
    // `paused | restarting | removing | created` are all states where
    // the container hasn't exited and has no exit code to act on —
    // collapsing them into stopped(0) would let finalize_clean_exit
    // auto-delete worktrees of containers that may resume any moment.
    // OOM is a real exit (the kernel killed the process); we surface
    // it explicitly so finalize preserves the recovery state.
    // Unknown status strings (future Docker versions, exotic runtimes)
    // are treated conservatively as still_running with a debug_log so
    // the issue is debuggable but not data-destructive.
    match status {
        "running" | "paused" | "restarting" | "removing" | "created" => {
            Ok(AttachOutcome::still_running())
        }
        "exited" | "dead" if oom => Ok(AttachOutcome::oom_killed()),
        "exited" => Ok(AttachOutcome::stopped(exit_code.unwrap_or(0))),
        "dead" => {
            // `dead` means the daemon failed to deinitialize the container
            // — rare, indicates trouble. Preserve records so the operator
            // can inspect rather than auto-cleaning.
            crate::debug_log!(
                "isolation",
                "inspect_attach_outcome: container {container} status=dead; treating as still_running to preserve records for inspection",
            );
            Ok(AttachOutcome::still_running())
        }
        other => {
            crate::debug_log!(
                "isolation",
                "inspect_attach_outcome: unknown docker status `{other}` for {container}; treating as still_running (conservative)",
            );
            Ok(AttachOutcome::still_running())
        }
    }
}

#[allow(clippy::too_many_lines)]
pub fn load_role(
    paths: &JackinPaths,
    config: &mut AppConfig,
    selector: &RoleSelector,
    workspace: &crate::workspace::ResolvedWorkspace,
    runner: &mut impl CommandRunner,
    opts: &LoadOptions,
) -> anyhow::Result<()> {
    load_role_with(
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
fn load_role_with(
    paths: &JackinPaths,
    config: &mut AppConfig,
    selector: &RoleSelector,
    workspace: &crate::workspace::ResolvedWorkspace,
    runner: &mut impl CommandRunner,
    opts: &LoadOptions,
    confirm_trust: impl FnOnce(&RoleSelector, &crate::config::RoleSource) -> anyhow::Result<()>,
) -> anyhow::Result<()> {
    // Pre-launch garbage collection: remove orphaned DinD containers and
    // networks left behind by hard kills, terminal closures, or startup
    // failures.  Best-effort — errors are silently ignored.
    gc_orphaned_resources(runner);

    let git = load_git_identity(runner);
    let host = load_host_identity(runner);

    // Intro animation
    if !opts.no_intro {
        let intro_name = if git.user_name.is_empty() {
            "operator"
        } else {
            &git.user_name
        };
        tui::intro_animation(intro_name);
    }

    let (source, is_new) = config.resolve_role_source(selector)?;

    let mut steps = StepCounter::new(opts.no_intro, &selector.name);

    // Step 1: Resolve role identity (clone or update repo)
    steps.next("Resolving role identity");

    let (cached_repo, validated_repo, repo_lock) =
        resolve_agent_repo(paths, selector, &source.git, runner, opts.debug)?;

    // Trust gate: prompt the operator before running an untrusted third-party role
    let newly_trusted = if source.trusted {
        false
    } else {
        confirm_trust(selector, &source)?;
        // Mutate the in-memory copy so callers downstream see the trust
        // without a reload; persist via editor below.
        if let Some(entry) = config.roles.get_mut(&selector.key()) {
            entry.trusted = true;
        }
        true
    };

    if is_new || newly_trusted {
        let mut editor = crate::config::ConfigEditor::open(paths)?;
        if let Some(role_source) = config.roles.get(&selector.key()) {
            editor.upsert_agent_source(&selector.key(), role_source);
        }
        editor.set_agent_trust(&selector.key(), true);
        *config = editor.save()?;
    }

    let agent_display_name = validated_repo.manifest.display_name(&selector.name);
    steps.role_name.clone_from(&agent_display_name);

    let agent = resolve_agent(opts.agent, workspace.default_agent);
    validate_agent_supported(selector, &validated_repo.manifest, agent)?;

    // Logo (if present in role repo)
    tui::print_logo(&cached_repo.repo_dir.join("logo.txt"));

    // Show a preliminary config summary (container name will be
    // confirmed after the image build, right before launch).
    let image_tag = image_name(selector);
    let preliminary_name = primary_container_name(selector);
    let config_rows = build_config_rows(
        &agent_display_name,
        &preliminary_name,
        workspace,
        &git,
        &image_tag,
        runner,
    );
    eprintln!();
    tui::print_config_table(&config_rows);
    eprintln!();

    // Resolve env vars (interactive prompts happen here, before build)
    let manifest_resolved = if validated_repo.manifest.env.is_empty() {
        crate::env_resolver::ResolvedEnv { vars: vec![] }
    } else {
        let prompter = crate::terminal_prompter::TerminalPrompter;
        crate::env_resolver::resolve_env(&validated_repo.manifest.env, &prompter)?
    };

    // Resolve operator env layers (global / role / workspace /
    // workspace × role). op:// refs shell out to `op`; $NAME refs
    // read the host env. Failures are aggregated into a single error.
    //
    // Workspace name: the launch pipeline does not currently pass a
    // workspace *name* down into load_role — only a ResolvedWorkspace
    // (mounts + workdir). Look up the name by scanning config.workspaces
    // for the entry whose workdir matches; this matches the same
    // identification rule used by `jackin workspace show`.
    let workspace_name = config
        .workspaces
        .iter()
        .find(|(_, w)| w.workdir == workspace.workdir)
        .map(|(name, _)| name.clone());

    // The operator env resolver takes two injection seams:
    //   * `op_runner`  — resolves `op://...` references (production:
    //     `OpCli::new()`; tests: a mock `OpRunner` constructed directly).
    //   * `host_env`   — resolves `$NAME` / `${NAME}` references
    //     (production: `|name| std::env::var(name).ok()`; tests: a
    //     closure over a `BTreeMap` seeded by the test).
    //
    // Both seams are carried on `LoadOptions` as optional fields. When
    // unset (the production default), `resolve_operator_env` is called,
    // which wires in the real `OpCli` and the real host env. When set
    // (tests only), `resolve_operator_env_with` is called with the
    // supplied seams, so tests never need to mutate `std::env` and the
    // crate-level `unsafe_code = "forbid"` lint stays intact.
    let operator_env = if opts.op_runner.is_none() && opts.host_env.is_none() {
        crate::operator_env::resolve_operator_env(
            config,
            Some(&selector.key()),
            workspace_name.as_deref(),
        )?
    } else {
        let default_runner = crate::operator_env::OpCli::new();
        let runner: &dyn crate::operator_env::OpRunner =
            opts.op_runner.as_deref().unwrap_or(&default_runner);
        let host_env_fn = |name: &str| -> Result<String, std::env::VarError> {
            opts.host_env.as_ref().map_or_else(
                || std::env::var(name),
                |map| map.get(name).cloned().ok_or(std::env::VarError::NotPresent),
            )
        };
        crate::operator_env::resolve_operator_env_with(
            config,
            Some(&selector.key()),
            workspace_name.as_deref(),
            runner,
            host_env_fn,
        )?
    };

    // Overlay the operator env map on top of the manifest env: operator
    // wins on conflicts (so a workspace-scoped `OPERATOR_TOKEN` overrides
    // a manifest default, which is the whole point of letting operators
    // supply env at launch time). Reserved names are filtered out in
    // the docker-run construction below.
    let mut merged_vars: Vec<(String, String)> = manifest_resolved.vars;
    for (k, v) in &operator_env {
        if let Some(slot) = merged_vars.iter_mut().find(|(mk, _)| mk == k) {
            slot.1.clone_from(v);
        } else {
            merged_vars.push((k.clone(), v.clone()));
        }
    }
    let resolved_env = crate::env_resolver::ResolvedEnv { vars: merged_vars };
    verify_required_agent_env(agent, &resolved_env)?;

    // Launch-time diagnostic: emit a single compact line summarising
    // the operator env that will be injected. In normal mode we show
    // counts only ("3 refs resolved"); in --debug mode we show each
    // key → layer/reference kind ("OPERATOR_TOKEN: op://Personal/...
    // from workspace \"big-monorepo\"") — never values.
    if !operator_env.is_empty() {
        crate::operator_env::print_launch_diagnostic(
            config,
            Some(&selector.key()),
            workspace_name.as_deref(),
            &operator_env,
            opts.debug,
        );
    }

    let load_result = (|| -> anyhow::Result<String> {
        // Step 2: Build Docker image
        let rebuild = opts.rebuild || {
            let img = image_name(selector);
            let needs_update = agent == crate::agent::Agent::Claude
                && version_check::needs_claude_update(paths, &img, runner);
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
            agent,
            rebuild,
            opts.debug,
            runner,
            repo_lock,
        )?;

        // Claim a unique container name using an exclusive lock file.
        // Each candidate name gets a lock file at `~/.jackin/data/<name>.lock`.
        // If `try_lock_exclusive` succeeds, we own the name for this
        // session.  If it fails (another instance holds it), we skip to
        // the next clone name.  The lock is held for the entire run and
        // released on exit (or process crash).
        let (container_name, _name_lock) = claim_container_name(paths, selector, runner)?;

        let auth_mode = config.resolve_auth_forward_mode(&selector.key());

        // Token mode requires CLAUDE_CODE_OAUTH_TOKEN in the resolved
        // operator env; fail fast with an actionable error if it is
        // missing so the operator sees the problem before we spend time
        // starting the network and DinD sidecar.
        if agent == crate::agent::Agent::Claude
            && matches!(auth_mode, crate::config::AuthForwardMode::Token)
        {
            verify_token_env_present(&operator_env)?;
        }

        let (state, auth_outcome) = RoleState::prepare(
            paths,
            &container_name,
            &validated_repo.manifest,
            auth_mode,
            &paths.home_dir,
            agent,
        )?;

        // Diagnostic line: surface the active auth mode and, for token
        // mode, the source reference of CLAUDE_CODE_OAUTH_TOKEN drawn
        // from the operator env config's raw declaration (the op://
        // reference or $NAME ref as written). Resolved values are never
        // printed.
        if agent == crate::agent::Agent::Claude {
            match auth_mode {
                crate::config::AuthForwardMode::Token => {
                    let raw = lookup_operator_env_raw(
                        config,
                        Some(&selector.key()),
                        workspace_name.as_deref(),
                        "CLAUDE_CODE_OAUTH_TOKEN",
                    );
                    let source_ref = auth_token_source_reference(raw.as_deref());
                    tui::auth_mode_notice("token", Some(&source_ref));
                }
                crate::config::AuthForwardMode::Sync => {
                    tui::auth_mode_notice("sync", None);
                }
                crate::config::AuthForwardMode::Ignore => {
                    tui::auth_mode_notice("ignore", None);
                }
            }

            // Verbose outcome notices kept for operator context.
            match auth_outcome {
                crate::instance::AuthProvisionOutcome::Synced => {
                    eprintln!(
                        "[jackin] Synced host Claude Code authentication into role state \
                         (auth_forward=sync)."
                    );
                }
                crate::instance::AuthProvisionOutcome::TokenMode => {
                    eprintln!(
                        "[jackin] auth_forward=token — role will use CLAUDE_CODE_OAUTH_TOKEN \
                         from the resolved env."
                    );
                }
                crate::instance::AuthProvisionOutcome::HostMissing => match auth_mode {
                    crate::config::AuthForwardMode::Sync => {
                        eprintln!(
                            "[jackin] auth_forward=sync but no host credentials found; \
                                 preserving existing container auth if present."
                        );
                    }
                    crate::config::AuthForwardMode::Ignore
                    | crate::config::AuthForwardMode::Token => {}
                },
                crate::instance::AuthProvisionOutcome::Skipped => {}
            }
        } else if !matches!(auth_mode, crate::config::AuthForwardMode::Ignore) {
            // auth_forward is a Claude-only setting today (it gates
            // CLAUDE_CODE_OAUTH_TOKEN forwarding and .credentials.json
            // syncing). Surface a one-line notice when the operator has
            // configured something other than the default and the active
            // agent isn't Claude — otherwise the setting silently no-ops
            // and the operator never finds out their config is dead.
            let mode_label = match auth_mode {
                crate::config::AuthForwardMode::Token => "token",
                crate::config::AuthForwardMode::Sync => "sync",
                crate::config::AuthForwardMode::Ignore => unreachable!(),
            };
            eprintln!(
                "[jackin] auth_forward={mode_label} is a Claude-only setting and is ignored \
                 under agent={}.",
                agent.slug()
            );
        }

        // Materialize workspace mounts: shared mounts pass through;
        // worktree-isolated mounts get a per-container `git worktree`
        // staged on the host. Must run AFTER `RoleState::prepare` (so the
        // per-container state directory exists) and BEFORE the docker run
        // command is assembled (so the docker `-v` flags reflect the
        // per-mount bind sources).
        let interactive = std::io::stdin().is_terminal();
        let workspace_label = workspace.label.as_str();
        let container_state = paths.data_dir.join(&container_name);
        crate::debug_log!(
            "isolation",
            "load_role: invoking materialize_workspace for container {container_name} (interactive={interactive}, force={force})",
            force = opts.force,
        );
        let materialized = crate::isolation::materialize::materialize_workspace(
            workspace,
            &container_state,
            &selector.key(),
            &container_name,
            workspace_label,
            &crate::isolation::materialize::PreflightContext {
                workspace_name: workspace_label.to_string(),
                force: opts.force,
                interactive,
            },
            runner,
        )?;

        let network = format!("{container_name}-net");
        let dind = format!("{container_name}-dind");

        // Step 3: Create network and start Docker-in-Docker
        steps.next("Starting Docker-in-Docker");

        let ctx = LaunchContext {
            container_name: &container_name,
            image: &image,
            network: &network,
            dind: &dind,
            selector,
            agent_display_name: &agent_display_name,
            workspace: &materialized,
            state: &state,
            git: &git,
            debug: opts.debug,
            agent,
            resolved_env: &resolved_env,
            cache_dir: &paths.cache_dir,
            paths,
        };
        let certs_volume = dind_certs_volume(&container_name);
        let mut cleanup = LoadCleanup::new(
            container_name.clone(),
            dind.clone(),
            certs_volume,
            network.clone(),
        );
        let launch_result = launch_role_runtime(&ctx, &mut steps, runner);
        if launch_result.is_err() {
            cleanup.run(runner);
        }
        launch_result?;

        // Finalize per-mount isolation worktrees BEFORE the container teardown
        // decision below: clean exits without dirty/unpushed state get their
        // worktrees swept; dirty state is preserved (with an interactive prompt
        // when stdin is a TTY). A `ReturnToAgent` choice restarts + re-attaches
        // the container exactly once so the operator can address the dirty
        // state inside the role, then the safe cleanup is retried.
        let interactive_finalize = std::io::stdin().is_terminal();
        let mut prompt = crate::isolation::finalize::StdinPrompt;
        let outcome = inspect_attach_outcome(runner, &container_name)?;
        let decision = crate::isolation::finalize::finalize_foreground_session(
            &container_name,
            &paths.data_dir.join(&container_name),
            outcome,
            interactive_finalize,
            &mut prompt,
            runner,
        )?;
        if matches!(
            decision,
            crate::isolation::finalize::FinalizeDecision::ReturnToAgent
        ) {
            // Restart and re-attach the container in one command, then retry
            // the safe cleanup pass once. We do not loop further: if the
            // operator still leaves dirty state, the second pass will fall
            // back to Preserved and exit normally.
            //
            // Reconcile keep_awake BEFORE the restart re-attach, mirroring the
            // mid-flight reconcile in `launch_role_runtime`: between the
            // original exit and this restart, a parallel jackin invocation
            // could observe `docker ps --filter ...` = 0 and kill caffeinate,
            // leaving the restart session unprotected. The lock inside
            // `reconcile` serializes against that race.
            super::caffeinate::reconcile(paths, runner);
            runner.run(
                "docker",
                &["start", "-ai", &container_name],
                None,
                &RunOptions::default(),
            )?;
            let outcome2 = inspect_attach_outcome(runner, &container_name)?;
            let _ = crate::isolation::finalize::finalize_foreground_session(
                &container_name,
                &paths.data_dir.join(&container_name),
                outcome2,
                interactive_finalize,
                &mut prompt,
                runner,
            )?;
        }

        // Classify how the interactive session ended so we know whether to
        // tear the container down or preserve it for `jackin hardline` to
        // restart:
        //  - Running     → terminal was closed (user detached).  Keep it.
        //  - Stopped / 0 → user exited cleanly inside Claude Code.  Tear down.
        //  - Stopped / ≠0 or OOM-killed → crash.  Preserve so `jackin hardline`
        //    can restart the existing container + DinD sidecar.
        #[allow(clippy::match_same_arms)]
        match inspect_container_state(runner, &container_name) {
            ContainerState::Running => cleanup.disarm(),
            ContainerState::Stopped {
                exit_code: 0,
                oom_killed: false,
            } => cleanup.run(runner),
            ContainerState::Stopped { .. } => cleanup.disarm(),
            ContainerState::NotFound => cleanup.run(runner),
        }

        Ok(container_name)
    })();

    // Update display name to include clone index (e.g. "The Architect (Clone 2)")
    let agent_display_name = match &load_result {
        Ok(container_name) => format_role_display(container_name, &agent_display_name),
        Err(_) => agent_display_name,
    };

    match load_result {
        Ok(_) => {
            render_exit(&agent_display_name, runner, opts);
            Ok(())
        }
        Err(error) => {
            render_exit(&agent_display_name, runner, opts);
            Err(error)
        }
    }
}

fn render_exit(agent_display_name: &str, runner: &mut impl CommandRunner, opts: &LoadOptions) {
    if opts.no_intro {
        return;
    }
    tui::outro_animation(
        agent_display_name,
        &list_running_agent_display_names(runner).unwrap_or_default(),
    );
}

/// Claim a unique container name for this role class by acquiring an
/// exclusive lock file.
///
/// Tries the primary name first, then clone-1, clone-2, etc.  For each
/// candidate the container state is inspected individually:
///
/// - `Running`                    → skip (active session owns this slot).
/// - `Stopped` / exit 0, no OOM  → remove the stopped container (best-effort)
///   and reclaim the slot.  The state directory on disk is untouched, so
///   credentials in `~/.jackin/data/<name>/.config/gh/` survive.
/// - `Stopped` / non-zero exit or OOM-killed → skip (`jackin hardline` needs
///   to restart the crashed container in place).
/// - `NotFound`                   → try to claim the slot as usual.
///
/// For the two "free" cases (clean-exit and not-found) the slot is claimed by
/// acquiring an exclusive lock file at `~/.jackin/data/<name>.lock`.  If the
/// lock is already held by a concurrent `jackin load`, the loop advances to
/// the next clone index.
///
/// The returned `File` holds the lock — it must be kept alive for the
/// duration of the role session.  The lock is automatically released
/// when the file is dropped (normal exit or crash).
fn claim_container_name(
    paths: &JackinPaths,
    selector: &RoleSelector,
    runner: &mut impl CommandRunner,
) -> anyhow::Result<(String, std::fs::File)> {
    let primary = primary_container_name(selector);

    std::fs::create_dir_all(&paths.data_dir)?;

    // Try primary name first, then clone-1, clone-2, ... (unbounded).
    let mut clone_index = 0_u32;
    loop {
        let name = if clone_index == 0 {
            primary.clone()
        } else {
            format!("{primary}-clone-{clone_index}")
        };

        let slot_free = match inspect_container_state(runner, &name) {
            // Clean exit: remove the stopped container so the slot is free.
            // Best-effort; ignore errors — the state dir on disk is untouched.
            ContainerState::Stopped {
                exit_code: 0,
                oom_killed: false,
            } => {
                let _ = runner.run("docker", &["rm", &name], None, &RunOptions::default());
                true
            }
            // Active session, or crashed/OOM-killed: do not disturb.
            // Crashed containers are preserved for `jackin hardline` restart.
            ContainerState::Running | ContainerState::Stopped { .. } => false,
            // No container exists — slot is free.
            ContainerState::NotFound => true,
        };

        if slot_free {
            let lock_path = paths.data_dir.join(format!("{name}.lock"));
            let lock_file = std::fs::File::create(&lock_path)?;
            if lock_file.try_lock_exclusive().is_ok() {
                return Ok((name, lock_file));
            }
            // Lock held by another process — try next name
        }

        clone_index += 1;
    }
}

/// Verify that `CLAUDE_CODE_OAUTH_TOKEN` is present in the resolved
/// operator env when `auth_forward == Token`. Returns an actionable
/// error listing both remediation paths (1Password `op://` reference
/// and `$CLAUDE_CODE_OAUTH_TOKEN` host shell forwarding) when the
/// token is missing or empty.
///
/// Kept as a small pure helper over `BTreeMap<String, String>` so it
/// can be unit-tested without faking the workspace env resolver.
fn verify_token_env_present(
    vars: &std::collections::BTreeMap<String, String>,
) -> anyhow::Result<()> {
    if vars
        .get("CLAUDE_CODE_OAUTH_TOKEN")
        .is_some_and(|v| !v.is_empty())
    {
        return Ok(());
    }
    anyhow::bail!(
        "auth_forward = \"token\" but CLAUDE_CODE_OAUTH_TOKEN is not set in the resolved \
         operator env.\n\
         \n\
         Add it in your workspace config under [env]. Either:\n\
         \n\
         - Reference a 1Password secret:\n  \
             [env]\n  \
             CLAUDE_CODE_OAUTH_TOKEN = \"op://vault/claude/token\"\n\
         \n\
         - Forward from the host shell:\n  \
             [env]\n  \
             CLAUDE_CODE_OAUTH_TOKEN = \"$CLAUDE_CODE_OAUTH_TOKEN\"\n\
         \n\
         Generate a token with `claude setup-token`, then either store it in \
         1Password (first form) or export it in your shell (second form)."
    );
}

/// Return a printable source reference for `CLAUDE_CODE_OAUTH_TOKEN`
/// given the raw (unresolved) declaration value from the operator env
/// config (e.g. `"Private/Claude/security/auth token"` or
/// `"$CLAUDE_CODE_OAUTH_TOKEN"`). Produces the `"KEY ← value"` form
/// consumed by `tui::auth_mode_notice`. When `raw` is `None` or the
/// display string is empty, falls back to the bare env-var name.
fn auth_token_source_reference(raw: Option<&str>) -> String {
    match raw {
        None | Some("") => "CLAUDE_CODE_OAUTH_TOKEN".to_string(),
        Some(value) => format!("CLAUDE_CODE_OAUTH_TOKEN \u{2190} {value}"),
    }
}

/// Look up the raw (unresolved) declaration value for `key` in the
/// operator env config layers, using the same precedence as
/// `resolve_operator_env` (global < role < workspace < workspace ×
/// role — later wins).
fn lookup_operator_env_raw(
    config: &crate::config::AppConfig,
    role_selector: Option<&str>,
    workspace_name: Option<&str>,
    key: &str,
) -> Option<String> {
    let ws_opt = workspace_name.and_then(|w| config.workspaces.get(w));

    // Walk layers low → high priority; later assignments win over
    // earlier ones. Assign each layer's `.get(key).cloned()` in turn,
    // `or_else`-chaining lets `None` from a later layer fall back to
    // an earlier layer's value.
    let workspace_role = ws_opt.zip(role_selector).and_then(|(ws, role_name)| {
        ws.roles
            .get(role_name)
            .and_then(|overlay| overlay.env.get(key).map(|v| v.as_display_str().to_string()))
    });
    let workspace = ws_opt.and_then(|ws| ws.env.get(key).map(|v| v.as_display_str().to_string()));
    let role = role_selector
        .and_then(|role_name| config.roles.get(role_name))
        .and_then(|a| a.env.get(key).map(|v| v.as_display_str().to_string()));
    let global = config.env.get(key).map(|v| v.as_display_str().to_string());

    workspace_role.or(workspace).or(role).or(global)
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
mod tests {
    use super::super::test_support::FakeRunner;
    use super::*;
    use crate::config::AppConfig;
    use crate::isolation::MountIsolation;
    use crate::isolation::materialize::{
        MaterializedMount, MaterializedWorkspace, WorktreeAuxMounts,
    };
    use crate::paths::JackinPaths;
    use crate::selector::RoleSelector;
    use std::collections::VecDeque;
    use tempfile::tempdir;

    #[test]
    fn agent_mounts_for_claude_includes_claude_state() {
        use crate::agent::Agent;
        use crate::instance::RoleState;

        let temp = tempdir().unwrap();
        let paths = JackinPaths::for_tests(temp.path());
        let manifest_temp = tempdir().unwrap();
        std::fs::write(
            manifest_temp.path().join("jackin.role.toml"),
            r#"dockerfile = "Dockerfile"

[claude]
plugins = []
"#,
        )
        .unwrap();
        std::fs::write(
            manifest_temp.path().join("Dockerfile"),
            "FROM projectjackin/construct:trixie\n",
        )
        .unwrap();
        let manifest = crate::manifest::RoleManifest::load(manifest_temp.path()).unwrap();

        let (state, _) = RoleState::prepare(
            &paths,
            "jackin-agent-smith",
            &manifest,
            crate::config::AuthForwardMode::Ignore,
            temp.path(),
            Agent::Claude,
        )
        .unwrap();

        let mounts = agent_mounts(&state);
        assert!(
            mounts
                .iter()
                .any(|m| m.contains("/home/agent/.claude") && !m.contains("/.claude.json"))
        );
        assert!(
            mounts
                .iter()
                .any(|m| m.contains("/home/agent/.claude.json"))
        );
        assert!(
            mounts
                .iter()
                .any(|m| m.contains("/home/agent/.jackin/plugins.json:ro"))
        );
    }

    #[test]
    fn agent_mounts_for_codex_only_has_config_toml() {
        use crate::agent::Agent;
        use crate::instance::RoleState;

        let temp = tempdir().unwrap();
        let paths = JackinPaths::for_tests(temp.path());
        let manifest_temp = tempdir().unwrap();
        std::fs::write(
            manifest_temp.path().join("jackin.role.toml"),
            r#"dockerfile = "Dockerfile"
agents = ["codex"]

[codex]
"#,
        )
        .unwrap();
        std::fs::write(
            manifest_temp.path().join("Dockerfile"),
            "FROM projectjackin/construct:trixie\n",
        )
        .unwrap();
        let manifest = crate::manifest::RoleManifest::load(manifest_temp.path()).unwrap();

        let (state, _) = RoleState::prepare(
            &paths,
            "jackin-agent-smith",
            &manifest,
            crate::config::AuthForwardMode::Ignore,
            temp.path(),
            Agent::Codex,
        )
        .unwrap();

        let mounts = agent_mounts(&state);
        assert_eq!(mounts.len(), 1);
        assert!(mounts[0].contains("/home/agent/.codex/config.toml"));
        assert!(!mounts[0].ends_with(":ro"));
    }

    #[test]
    fn build_workspace_mount_strings_marks_overrides_readonly() {
        // One worktree-mode mount with all four bind sources populated.
        // Host `.git/` mount MUST stay rw (git writes refs/objects/
        // HEAD/index/logs all under it on every commit/branch/fetch).
        // Both override files MUST be `:ro`-suppressed.
        let mat = MaterializedWorkspace {
            workdir: "/workspace/jackin".into(),
            mounts: vec![MaterializedMount {
                bind_src:
                    "/data/jackin-the-architect/git/worktree/repo/Users/donbeave/Projects/jackin-project/jackin/jackin-the-architect"
                        .into(),
                dst: "/Users/donbeave/Projects/jackin-project/jackin".into(),
                readonly: false,
                isolation: MountIsolation::Worktree,
                worktree_aux: Some(WorktreeAuxMounts {
                    host_git_dir: "/Users/donbeave/Projects/jackin-project/jackin/.git".into(),
                    host_git_target:
                        "/jackin/host/Users/donbeave/Projects/jackin-project/jackin/.git".into(),
                    git_file_override:
                        "/data/jackin-the-architect/git/overrides/Users/donbeave/Projects/jackin-project/jackin/.git"
                            .into(),
                    git_file_target: "/Users/donbeave/Projects/jackin-project/jackin/.git".into(),
                    gitdir_back_override:
                        "/data/jackin-the-architect/git/overrides/Users/donbeave/Projects/jackin-project/jackin/gitdir"
                            .into(),
                    gitdir_back_target:
                        "/jackin/host/Users/donbeave/Projects/jackin-project/jackin/.git/worktrees/jackin-the-architect/gitdir"
                            .into(),
                }),
            }],
            keep_awake_enabled: false,
        };

        let strings = build_workspace_mount_strings(&mat);
        assert_eq!(strings.len(), 4, "one worktree mount → four bind specs");

        // 1: worktree at <dst>, no :ro (writable).
        assert_eq!(
            strings[0],
            "/data/jackin-the-architect/git/worktree/repo/Users/donbeave/Projects/jackin-project/jackin/jackin-the-architect:/Users/donbeave/Projects/jackin-project/jackin"
        );
        assert!(!strings[0].ends_with(":ro"));

        // 2: host .git/, MUST stay rw — refs/objects/HEAD/index/logs
        // are all written under it. Both ends terminate in `.git`.
        assert_eq!(
            strings[1],
            "/Users/donbeave/Projects/jackin-project/jackin/.git:/jackin/host/Users/donbeave/Projects/jackin-project/jackin/.git"
        );
        assert!(
            !strings[1].ends_with(":ro"),
            "host .git mount must remain rw",
        );

        // 3: .git pointer override at <dst>/.git. :ro hardening.
        assert!(
            strings[2].ends_with(":ro"),
            "git-file override must be ro; got {}",
            strings[2],
        );
        assert!(
            strings[2]
                .contains("/git/overrides/Users/donbeave/Projects/jackin-project/jackin/.git")
        );
        assert!(strings[2].contains(":/Users/donbeave/Projects/jackin-project/jackin/.git:ro"));

        // 4: gitdir back-pointer override at
        // `/jackin/host/<dst-tree>/.git/worktrees/<container>/gitdir`.
        // File-level overlay on top of the host `.git/` mount destination.
        // :ro hardening.
        assert!(
            strings[3].ends_with(":ro"),
            "gitdir-back override must be ro; got {}",
            strings[3],
        );
        assert!(
            strings[3]
                .contains("/git/overrides/Users/donbeave/Projects/jackin-project/jackin/gitdir")
        );
        assert!(
            strings[3].contains(
                ":/jackin/host/Users/donbeave/Projects/jackin-project/jackin/.git/worktrees/jackin-the-architect/gitdir:ro"
            )
        );
    }

    #[test]
    fn build_workspace_mount_strings_passthrough_for_shared_mounts() {
        // Shared mounts produce exactly one bind spec, no aux entries.
        let mat = MaterializedWorkspace {
            workdir: "/workspace".into(),
            mounts: vec![MaterializedMount {
                bind_src: "/host/shared".into(),
                dst: "/workspace/shared".into(),
                readonly: false,
                isolation: MountIsolation::Shared,
                worktree_aux: None,
            }],
            keep_awake_enabled: false,
        };

        let strings = build_workspace_mount_strings(&mat);
        assert_eq!(strings, vec!["/host/shared:/workspace/shared".to_string()]);
    }

    #[test]
    fn build_workspace_mount_strings_two_isolated_mounts_emits_eight_distinct_strings() {
        // A workspace with two isolated mounts on different host repos
        // (allowed by validate_isolation_layout) must emit a clean
        // 4-bind grouping per mount with no path collisions. This is
        // the production multi-mount path; finalize.rs's prompt loop
        // also handles this case (see multi_mount_force_delete_on_each_*).
        let mat = MaterializedWorkspace {
            workdir: "/workspace".into(),
            mounts: vec![
                MaterializedMount {
                    bind_src: "/data/jackin-x/git/worktree/repo/workspace/a/jackin-x".into(),
                    dst: "/workspace/a".into(),
                    readonly: false,
                    isolation: MountIsolation::Worktree,
                    worktree_aux: Some(crate::isolation::materialize::WorktreeAuxMounts {
                        host_git_dir: "/host/repo-a/.git".into(),
                        host_git_target: "/jackin/host/workspace/a/.git".into(),
                        git_file_override: "/data/jackin-x/git/overrides/workspace/a/.git".into(),
                        git_file_target: "/workspace/a/.git".into(),
                        gitdir_back_override: "/data/jackin-x/git/overrides/workspace/a/gitdir"
                            .into(),
                        gitdir_back_target:
                            "/jackin/host/workspace/a/.git/worktrees/jackin-x/gitdir".into(),
                    }),
                },
                MaterializedMount {
                    bind_src: "/data/jackin-x/git/worktree/repo/workspace/b/jackin-x".into(),
                    dst: "/workspace/b".into(),
                    readonly: false,
                    isolation: MountIsolation::Worktree,
                    worktree_aux: Some(crate::isolation::materialize::WorktreeAuxMounts {
                        host_git_dir: "/host/repo-b/.git".into(),
                        host_git_target: "/jackin/host/workspace/b/.git".into(),
                        git_file_override: "/data/jackin-x/git/overrides/workspace/b/.git".into(),
                        git_file_target: "/workspace/b/.git".into(),
                        gitdir_back_override: "/data/jackin-x/git/overrides/workspace/b/gitdir"
                            .into(),
                        gitdir_back_target:
                            "/jackin/host/workspace/b/.git/worktrees/jackin-x/gitdir".into(),
                    }),
                },
            ],
            keep_awake_enabled: false,
        };

        let strings = build_workspace_mount_strings(&mat);
        assert_eq!(
            strings.len(),
            8,
            "two isolated mounts → eight bind specs (4 per mount); got {strings:?}"
        );

        // No two emitted strings may be identical — distinct dsts
        // throughout, which is the disambiguation guarantee under
        // /jackin/host/<dst-tree>/.
        let mut sorted = strings.clone();
        sorted.sort();
        sorted.dedup();
        assert_eq!(
            sorted.len(),
            strings.len(),
            "no duplicate bind specs across mounts; got {strings:?}"
        );

        // Each mount's 4 bind specs reference its own dst tree.
        let first_mount_count = strings
            .iter()
            .filter(|s| s.contains("/workspace/a") || s.contains("/jackin/host/workspace/a/"))
            .count();
        let second_mount_count = strings
            .iter()
            .filter(|s| s.contains("/workspace/b") || s.contains("/jackin/host/workspace/b/"))
            .count();
        assert_eq!(first_mount_count, 4, "mount A should have 4 bind specs");
        assert_eq!(second_mount_count, 4, "mount B should have 4 bind specs");

        // Both override files for both mounts must remain :ro.
        let ro_count = strings.iter().filter(|s| s.ends_with(":ro")).count();
        assert_eq!(
            ro_count, 4,
            ":ro hardening must apply to both override files of both mounts; got {strings:?}"
        );
    }

    #[test]
    fn build_workspace_mount_strings_preserves_readonly_on_user_facing_mount() {
        // A user-configured `readonly = true` mount still gets `:ro` on
        // the user-facing dst — this is independent of the override
        // hardening.
        let mat = MaterializedWorkspace {
            workdir: "/workspace".into(),
            mounts: vec![MaterializedMount {
                bind_src: "/host/cache".into(),
                dst: "/workspace/cache".into(),
                readonly: true,
                isolation: MountIsolation::Shared,
                worktree_aux: None,
            }],
            keep_awake_enabled: false,
        };

        let strings = build_workspace_mount_strings(&mat);
        assert_eq!(strings, vec!["/host/cache:/workspace/cache:ro".to_string()]);
    }

    fn repo_workspace(repo_dir: &std::path::Path) -> crate::workspace::ResolvedWorkspace {
        crate::workspace::ResolvedWorkspace {
            label: repo_dir.display().to_string(),
            workdir: "/workspace".to_string(),
            mounts: vec![crate::workspace::MountConfig {
                src: repo_dir.display().to_string(),
                dst: "/workspace".to_string(),
                readonly: false,
                isolation: crate::isolation::MountIsolation::Shared,
            }],
            default_agent: None,
            keep_awake_enabled: false,
        }
    }

    #[test]
    fn resolve_agent_cli_override_wins() {
        assert_eq!(
            resolve_agent(
                Some(crate::agent::Agent::Codex),
                Some(crate::agent::Agent::Claude),
            ),
            crate::agent::Agent::Codex
        );
    }

    #[test]
    fn resolve_agent_uses_workspace_when_cli_absent() {
        assert_eq!(
            resolve_agent(None, Some(crate::agent::Agent::Codex)),
            crate::agent::Agent::Codex
        );
    }

    #[test]
    fn resolve_agent_defaults_to_claude() {
        assert_eq!(resolve_agent(None, None), crate::agent::Agent::Claude);
    }

    #[test]
    fn validate_agent_supported_rejects_unsupported_choice() {
        let temp = tempdir().unwrap();
        std::fs::write(
            temp.path().join("jackin.role.toml"),
            r#"dockerfile = "Dockerfile"

[claude]
plugins = []
"#,
        )
        .unwrap();
        let manifest = crate::manifest::RoleManifest::load(temp.path()).unwrap();
        let selector = RoleSelector::new(None, "agent-smith");

        let err =
            validate_agent_supported(&selector, &manifest, crate::agent::Agent::Codex).unwrap_err();
        let message = err.to_string();
        assert!(message.contains("role \"agent-smith\""));
        assert!(message.contains("agent \"codex\""));
        assert!(message.contains("supported: [claude]"));
    }

    #[test]
    fn verify_required_agent_env_rejects_missing_codex_key() {
        let env = crate::env_resolver::ResolvedEnv { vars: vec![] };
        let err = verify_required_agent_env(crate::agent::Agent::Codex, &env).unwrap_err();
        assert!(err.to_string().contains("OPENAI_API_KEY"));
    }

    #[test]
    fn verify_required_agent_env_accepts_codex_key() {
        let env = crate::env_resolver::ResolvedEnv {
            vars: vec![("OPENAI_API_KEY".to_string(), "test-key".to_string())],
        };
        verify_required_agent_env(crate::agent::Agent::Codex, &env).unwrap();
    }

    #[test]
    fn trust_gate_rejects_untrusted_agent_in_non_interactive_context() {
        let selector = RoleSelector::new(Some("evil-org"), "backdoor");
        let source = crate::config::RoleSource {
            git: "https://github.com/evil-org/jackin-backdoor.git".to_string(),
            trusted: false,
            claude: None,
            env: std::collections::BTreeMap::new(),
        };

        let error = confirm_agent_trust(&selector, &source).unwrap_err();
        let message = error.to_string();

        assert!(
            message.contains("untrusted role source"),
            "expected 'untrusted role source' in: {message}"
        );
        assert!(
            message.contains("evil-org/backdoor"),
            "expected role selector in error: {message}"
        );
        assert!(
            message.contains("evil-org/jackin-backdoor.git"),
            "expected git URL in error: {message}"
        );
    }

    /// Helper: trust callback that always accepts.
    ///
    /// Signature matches `deny_trust` so both can be passed as the same
    /// function-pointer type to the trust prompt; the `Ok(())` is therefore
    /// load-bearing even though clippy flags it.
    #[allow(clippy::unnecessary_wraps)]
    fn auto_trust(_: &RoleSelector, _: &crate::config::RoleSource) -> anyhow::Result<()> {
        Ok(())
    }

    /// Helper: trust callback that always declines.
    fn deny_trust(_: &RoleSelector, _: &crate::config::RoleSource) -> anyhow::Result<()> {
        anyhow::bail!("role source not trusted — aborting")
    }

    #[test]
    fn load_namespaced_agent_registers_source_and_trusts_on_accept() {
        let temp = tempdir().unwrap();
        let paths = JackinPaths::for_tests(temp.path());
        let mut config = AppConfig::load_or_init(&paths).unwrap();
        let selector = RoleSelector::new(Some("chainargos"), "the-architect");
        let mut runner = FakeRunner::for_load_agent([String::new()]);

        let repo_dir = paths.roles_dir.join("chainargos").join("the-architect");
        std::fs::create_dir_all(&repo_dir).unwrap();
        std::fs::write(
            repo_dir.join("Dockerfile"),
            "FROM projectjackin/construct:trixie\n",
        )
        .unwrap();
        std::fs::write(
            repo_dir.join("jackin.role.toml"),
            r#"dockerfile = "Dockerfile"

[claude]
plugins = ["code-review@claude-plugins-official"]
"#,
        )
        .unwrap();

        let workspace = repo_workspace(&repo_dir);
        load_role_with(
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
            call.contains(
                "docker inspect --format {{.State.Running}} {{.State.ExitCode}} {{.State.OOMKilled}} jackin-chainargos__the-architect",
            )
        }));
        assert!(runner.recorded.iter().any(|call| {
            call.contains("docker run -d -it --name jackin-chainargos__the-architect")
        }));
        assert!(
            runner
                .recorded
                .iter()
                .any(|call| call.contains("/home/agent/.jackin/plugins.json:ro"))
        );
        assert!(
            !runner
                .recorded
                .iter()
                .any(|call| call.contains("claude plugin install"))
        );

        // Regression guard: namespaced class keys contain `__`, which is invalid
        // in RFC-1123 hostnames. The DinD SAN must still carry the full
        // container name so roles can connect via
        // tcp://jackin-chainargos__the-architect-dind:2376 without TLS errors.
        let dind_cmd = runner
            .recorded
            .iter()
            .find(|call| {
                call.contains("docker run -d --name jackin-chainargos__the-architect-dind")
            })
            .expect("expected DinD startup command");
        assert!(
            dind_cmd.contains("DOCKER_TLS_SAN=DNS:jackin-chainargos__the-architect-dind"),
            "DinD SAN must include the namespaced container name with a DNS: prefix"
        );
    }

    #[test]
    fn load_namespaced_agent_aborts_when_trust_declined() {
        let temp = tempdir().unwrap();
        let paths = JackinPaths::for_tests(temp.path());
        let mut config = AppConfig::load_or_init(&paths).unwrap();
        let selector = RoleSelector::new(Some("evil-org"), "backdoor");
        let mut runner = FakeRunner::for_load_agent([String::new(), String::new()]);

        let repo_dir = paths.roles_dir.join("evil-org").join("backdoor");
        std::fs::create_dir_all(&repo_dir).unwrap();
        std::fs::write(
            repo_dir.join("Dockerfile"),
            "FROM projectjackin/construct:trixie\n",
        )
        .unwrap();
        std::fs::write(
            repo_dir.join("jackin.role.toml"),
            r#"dockerfile = "Dockerfile"

[claude]
plugins = []
"#,
        )
        .unwrap();

        let workspace = repo_workspace(&repo_dir);
        let error = load_role_with(
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
    fn load_agent_injects_configured_mounts() {
        let temp = tempdir().unwrap();
        let paths = JackinPaths::for_tests(temp.path());
        let selector = RoleSelector::new(Some("chainargos"), "agent-brown");
        let mut runner = FakeRunner::for_load_agent([String::new()]);

        let repo_dir = paths.roles_dir.join("chainargos").join("agent-brown");
        std::fs::create_dir_all(&repo_dir).unwrap();
        std::fs::write(
            repo_dir.join("Dockerfile"),
            "FROM projectjackin/construct:trixie\n",
        )
        .unwrap();
        std::fs::write(
            repo_dir.join("jackin.role.toml"),
            r#"dockerfile = "Dockerfile"

[claude]
plugins = []
"#,
        )
        .unwrap();

        let mount_src = temp.path().join("test-mount");
        std::fs::create_dir_all(&mount_src).unwrap();
        std::fs::create_dir_all(&paths.config_dir).unwrap();

        let config_content = r#"[roles."chainargos/agent-brown"]
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
                    isolation: crate::isolation::MountIsolation::Shared,
                },
                crate::workspace::MountConfig {
                    src: mount_src.display().to_string(),
                    dst: "/test-data".to_string(),
                    readonly: true,
                    isolation: crate::isolation::MountIsolation::Shared,
                },
            ],
            default_agent: None,
            keep_awake_enabled: false,
        };

        load_role(
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
            .find(|call| call.contains("docker run -d -it"))
            .unwrap();
        assert!(run_cmd.contains(&format!("{}:/test-data:ro", mount_src.display())));
    }

    #[test]
    fn load_agent_runs_attached_with_plugins_mount() {
        let temp = tempdir().unwrap();
        let paths = JackinPaths::for_tests(temp.path());
        let mut config = AppConfig::load_or_init(&paths).unwrap();
        let selector = RoleSelector::new(None, "agent-smith");
        let mut runner = FakeRunner::for_load_agent([String::new()]);

        let repo_dir = paths.roles_dir.join("agent-smith");
        std::fs::create_dir_all(&repo_dir).unwrap();
        std::fs::write(
            repo_dir.join("Dockerfile"),
            "FROM projectjackin/construct:trixie\n",
        )
        .unwrap();
        std::fs::write(
            repo_dir.join("jackin.role.toml"),
            r#"dockerfile = "Dockerfile"

[claude]
plugins = ["code-review@claude-plugins-official"]
"#,
        )
        .unwrap();

        let workspace = repo_workspace(&repo_dir);
        load_role(
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
            call.contains(
                "docker inspect --format {{.State.Running}} {{.State.ExitCode}} {{.State.OOMKilled}} jackin-agent-smith",
            )
        }));
        assert!(
            runner
                .recorded
                .iter()
                .any(|call| call.contains("docker run -d -it --name jackin-agent-smith"))
        );
        assert!(
            runner
                .recorded
                .iter()
                .any(|call| call.contains("/home/agent/.jackin/plugins.json:ro"))
        );
        assert!(
            !runner
                .recorded
                .iter()
                .any(|call| call.contains("claude plugin install"))
        );
    }

    #[test]
    fn load_agent_launches_codex_from_workspace_agent() {
        let temp = tempdir().unwrap();
        let paths = JackinPaths::for_tests(temp.path());
        paths.ensure_base_dirs().unwrap();
        std::fs::write(
            &paths.config_file,
            r#"[env]
OPENAI_API_KEY = "test-openai-key"

[roles.agent-smith]
git = "https://github.com/jackin-project/jackin-agent-smith.git"
trusted = true
"#,
        )
        .unwrap();
        let mut config = AppConfig::load_or_init(&paths).unwrap();
        let selector = RoleSelector::new(None, "agent-smith");
        let mut runner = FakeRunner::for_load_agent([String::new()]);

        let repo_dir = paths.roles_dir.join("agent-smith");
        std::fs::create_dir_all(&repo_dir).unwrap();
        std::fs::write(
            repo_dir.join("Dockerfile"),
            "FROM projectjackin/construct:trixie\n",
        )
        .unwrap();
        std::fs::write(
            repo_dir.join("jackin.role.toml"),
            r#"dockerfile = "Dockerfile"
agents = ["claude", "codex"]

[claude]
plugins = ["code-review@claude-plugins-official"]

[codex]
model = "gpt-5"
"#,
        )
        .unwrap();

        let mut workspace = repo_workspace(&repo_dir);
        workspace.default_agent = Some(crate::agent::Agent::Codex);
        load_role(
            &paths,
            &mut config,
            &selector,
            &workspace,
            &mut runner,
            &LoadOptions::default(),
        )
        .unwrap();

        let build_cmd = runner
            .recorded
            .iter()
            .find(|call| call.contains("docker build "))
            .unwrap();
        assert!(build_cmd.contains("--pull"));

        let run_cmd = runner
            .recorded
            .iter()
            .find(|call| call.contains("docker run -d -it"))
            .unwrap();
        assert!(run_cmd.contains("-e JACKIN_AGENT=codex"));
        assert!(run_cmd.contains("-e OPENAI_API_KEY=test-openai-key"));
        assert!(run_cmd.contains("/home/agent/.codex/config.toml"));
        assert!(!run_cmd.contains("/home/agent/.claude"));
        assert!(!run_cmd.contains("/home/agent/.jackin/plugins.json"));
        assert!(
            paths
                .data_dir
                .join("jackin-agent-smith")
                .join("config.toml")
                .is_file()
        );
    }

    #[test]
    fn load_agent_rejects_codex_when_openai_key_missing() {
        let temp = tempdir().unwrap();
        let paths = JackinPaths::for_tests(temp.path());
        paths.ensure_base_dirs().unwrap();
        std::fs::write(
            &paths.config_file,
            r#"[roles.agent-smith]
git = "https://github.com/jackin-project/jackin-agent-smith.git"
trusted = true
"#,
        )
        .unwrap();
        let mut config = AppConfig::load_or_init(&paths).unwrap();
        let selector = RoleSelector::new(None, "agent-smith");
        let mut runner = FakeRunner::for_load_agent([String::new()]);

        let repo_dir = paths.roles_dir.join("agent-smith");
        std::fs::create_dir_all(&repo_dir).unwrap();
        std::fs::write(
            repo_dir.join("Dockerfile"),
            "FROM projectjackin/construct:trixie\n",
        )
        .unwrap();
        std::fs::write(
            repo_dir.join("jackin.role.toml"),
            r#"dockerfile = "Dockerfile"
agents = ["codex"]

[codex]
"#,
        )
        .unwrap();

        let mut workspace = repo_workspace(&repo_dir);
        workspace.default_agent = Some(crate::agent::Agent::Codex);
        let err = load_role(
            &paths,
            &mut config,
            &selector,
            &workspace,
            &mut runner,
            &LoadOptions::default(),
        )
        .unwrap_err();

        assert!(err.to_string().contains("OPENAI_API_KEY"));
        assert!(
            !runner
                .recorded
                .iter()
                .any(|call| call.contains("docker build") || call.contains("docker run"))
        );
    }

    #[test]
    fn load_agent_uses_resolved_workspace_mounts_and_workdir() {
        let temp = tempfile::tempdir().unwrap();
        let paths = JackinPaths::for_tests(temp.path());
        let mut config = AppConfig::load_or_init(&paths).unwrap();
        let selector = RoleSelector::new(None, "agent-smith");
        let mut runner = FakeRunner::for_load_agent([
            String::new(),
            String::new(),
            String::new(),
            String::new(),
            "jackin-agent-smith".to_string(),
        ]);

        let repo_dir = paths.roles_dir.join("agent-smith");
        std::fs::create_dir_all(&repo_dir).unwrap();
        std::fs::write(
            repo_dir.join("Dockerfile"),
            "FROM projectjackin/construct:trixie\n",
        )
        .unwrap();
        std::fs::write(
            repo_dir.join("jackin.role.toml"),
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
                isolation: crate::isolation::MountIsolation::Shared,
            }],
            default_agent: None,
            keep_awake_enabled: false,
        };

        load_role(
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
            .find(|call| call.contains("docker run -d -it"))
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
        let selector = RoleSelector::new(None, "agent-smith");
        let mut runner = FakeRunner::for_load_agent([
            String::new(),
            String::new(),
            String::new(),
            String::new(),
            "jackin-agent-smith".to_string(),
        ]);

        let repo_dir = paths.roles_dir.join("agent-smith");
        std::fs::create_dir_all(&repo_dir).unwrap();
        std::fs::write(
            repo_dir.join("Dockerfile"),
            "FROM projectjackin/construct:trixie\n",
        )
        .unwrap();
        std::fs::write(
            repo_dir.join("jackin.role.toml"),
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
                isolation: crate::isolation::MountIsolation::Shared,
            }],
            default_agent: None,
            keep_awake_enabled: false,
        };

        load_role(
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
        let selector = RoleSelector::new(None, "agent-smith");
        let mut runner = FakeRunner {
            fail_on: vec!["docker run -d -it --name jackin-agent-smith".to_string()],
            capture_queue: VecDeque::from(vec![
                String::new(),
                String::new(),
                String::new(),
                String::new(), // identity
                String::new(), // git pull
            ]),
            ..Default::default()
        };

        let repo_dir = paths.roles_dir.join("agent-smith");
        std::fs::create_dir_all(&repo_dir).unwrap();
        std::fs::write(
            repo_dir.join("Dockerfile"),
            "FROM projectjackin/construct:trixie\n",
        )
        .unwrap();
        std::fs::write(
            repo_dir.join("jackin.role.toml"),
            r#"dockerfile = "Dockerfile"

[claude]
plugins = ["code-review@claude-plugins-official"]
"#,
        )
        .unwrap();

        let workspace = repo_workspace(&repo_dir);
        let error = load_role(
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
                .contains("docker run -d -it --name jackin-agent-smith")
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
        let selector = RoleSelector::new(None, "agent-smith");
        let mut runner = FakeRunner::for_load_agent([
            String::new(),
            String::new(),
            String::new(),
            String::new(),
            "jackin-agent-smith".to_string(),
        ]);

        let repo_dir = paths.roles_dir.join("agent-smith");
        std::fs::create_dir_all(&repo_dir).unwrap();
        std::fs::write(
            repo_dir.join("Dockerfile"),
            "FROM projectjackin/construct:trixie\n",
        )
        .unwrap();
        std::fs::write(
            repo_dir.join("jackin.role.toml"),
            r#"dockerfile = "Dockerfile"

[claude]
plugins = []
"#,
        )
        .unwrap();

        let workspace = repo_workspace(&repo_dir);
        load_role(
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
    fn load_agent_configures_dind_with_tls() {
        let temp = tempdir().unwrap();
        let paths = JackinPaths::for_tests(temp.path());
        let mut config = AppConfig::load_or_init(&paths).unwrap();
        let selector = RoleSelector::new(None, "agent-smith");
        let mut runner = FakeRunner::for_load_agent([
            String::new(),
            String::new(),
            String::new(),
            String::new(),
            "jackin-agent-smith".to_string(),
        ]);

        let repo_dir = paths.roles_dir.join("agent-smith");
        std::fs::create_dir_all(&repo_dir).unwrap();
        std::fs::write(
            repo_dir.join("Dockerfile"),
            "FROM projectjackin/construct:trixie\n",
        )
        .unwrap();
        std::fs::write(
            repo_dir.join("jackin.role.toml"),
            r#"dockerfile = "Dockerfile"

[claude]
plugins = []
"#,
        )
        .unwrap();

        let workspace = repo_workspace(&repo_dir);
        load_role(
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
        // DinD's auto-generated server cert must include the container name as a
        // Subject Alternative Name, because the role connects via
        // DOCKER_HOST=tcp://jackin-agent-smith-dind:2376. Without this, the TLS
        // handshake fails because the default SANs only cover the short
        // container ID, `docker`, and `localhost`.
        //
        // The `DNS:` prefix is mandatory: `dockerd-entrypoint.sh` passes
        // `DOCKER_TLS_SAN` through to openssl verbatim (without adding a type
        // prefix), and openssl rejects SAN entries that lack a type tag with
        // `v2i_GENERAL_NAME_ex: missing value`.
        assert!(
            dind_cmd.contains("DOCKER_TLS_SAN=DNS:jackin-agent-smith-dind"),
            "DinD SAN value must be prefixed with `DNS:` so openssl accepts it"
        );

        // Role container: TLS client config
        let run_cmd = runner
            .recorded
            .iter()
            .find(|call| call.contains("docker run -d -it"))
            .unwrap();
        assert!(
            run_cmd.contains("DOCKER_HOST=tcp://jackin-agent-smith-dind:2376"),
            "role must use TLS port 2376"
        );
        assert!(
            run_cmd.contains("DOCKER_TLS_VERIFY=1"),
            "role must verify TLS"
        );
        assert!(
            run_cmd.contains("DOCKER_CERT_PATH=/certs/client"),
            "role must know cert path"
        );
        assert!(
            run_cmd.contains("jackin-agent-smith-dind-certs:/certs/client:ro"),
            "role must mount cert volume read-only"
        );
    }

    #[test]
    fn load_agent_sets_display_name_label() {
        let temp = tempdir().unwrap();
        let paths = JackinPaths::for_tests(temp.path());
        let mut config = AppConfig::load_or_init(&paths).unwrap();
        let selector = RoleSelector::new(None, "agent-smith");
        let mut runner = FakeRunner::for_load_agent([
            String::new(),
            String::new(),
            String::new(),
            String::new(),
            "jackin-agent-smith".to_string(),
        ]);

        let repo_dir = paths.roles_dir.join("agent-smith");
        std::fs::create_dir_all(&repo_dir).unwrap();
        std::fs::write(
            repo_dir.join("Dockerfile"),
            "FROM projectjackin/construct:trixie\n",
        )
        .unwrap();
        std::fs::write(
            repo_dir.join("jackin.role.toml"),
            r#"dockerfile = "Dockerfile"

[identity]
name = "Agent Smith"

[claude]
plugins = []
"#,
        )
        .unwrap();

        let workspace = repo_workspace(&repo_dir);
        load_role(
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
            .find(|call| call.contains("docker run -d -it"))
            .unwrap();
        assert!(run_cmd.contains("jackin.display_name=Agent Smith"));
    }

    #[test]
    fn load_agent_emits_keep_awake_label_when_workspace_opted_in() {
        let temp = tempdir().unwrap();
        let paths = JackinPaths::for_tests(temp.path());
        let mut config = AppConfig::load_or_init(&paths).unwrap();
        let selector = RoleSelector::new(None, "agent-smith");
        let mut runner = FakeRunner::for_load_agent([
            String::new(),
            String::new(),
            String::new(),
            String::new(),
            "jackin-agent-smith".to_string(),
        ]);

        let repo_dir = paths.roles_dir.join("agent-smith");
        std::fs::create_dir_all(&repo_dir).unwrap();
        std::fs::write(
            repo_dir.join("Dockerfile"),
            "FROM projectjackin/construct:trixie\n",
        )
        .unwrap();
        std::fs::write(
            repo_dir.join("jackin.role.toml"),
            r#"dockerfile = "Dockerfile"

[identity]
name = "Agent Smith"

[claude]
plugins = []
"#,
        )
        .unwrap();

        let mut workspace = repo_workspace(&repo_dir);
        workspace.keep_awake_enabled = true;
        load_role(
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
            .find(|call| call.contains("docker run -d -it"))
            .unwrap();
        assert!(
            run_cmd.contains("--label jackin.keep_awake=true"),
            "role container with keep_awake_enabled must carry the keep_awake label, \
             so runtime::caffeinate::reconcile can detect it via docker ps --filter; \
             actual run command: {run_cmd}"
        );
    }

    #[test]
    fn load_agent_omits_keep_awake_label_when_workspace_opted_out() {
        let temp = tempdir().unwrap();
        let paths = JackinPaths::for_tests(temp.path());
        let mut config = AppConfig::load_or_init(&paths).unwrap();
        let selector = RoleSelector::new(None, "agent-smith");
        let mut runner = FakeRunner::for_load_agent([
            String::new(),
            String::new(),
            String::new(),
            String::new(),
            "jackin-agent-smith".to_string(),
        ]);

        let repo_dir = paths.roles_dir.join("agent-smith");
        std::fs::create_dir_all(&repo_dir).unwrap();
        std::fs::write(
            repo_dir.join("Dockerfile"),
            "FROM projectjackin/construct:trixie\n",
        )
        .unwrap();
        std::fs::write(
            repo_dir.join("jackin.role.toml"),
            r#"dockerfile = "Dockerfile"

[identity]
name = "Agent Smith"

[claude]
plugins = []
"#,
        )
        .unwrap();

        let workspace = repo_workspace(&repo_dir); // keep_awake_enabled defaults false
        load_role(
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
            .find(|call| call.contains("docker run -d -it"))
            .unwrap();
        assert!(
            !run_cmd.contains("jackin.keep_awake"),
            "role container without keep_awake_enabled must not carry the label, \
             else the reconciler would hold caffeinate for opted-out workspaces; \
             actual run command: {run_cmd}"
        );
    }

    #[test]
    fn load_agent_sets_claude_env_to_jackin() {
        let temp = tempdir().unwrap();
        let paths = JackinPaths::for_tests(temp.path());
        let mut config = AppConfig::load_or_init(&paths).unwrap();
        let selector = RoleSelector::new(None, "agent-smith");
        let mut runner = FakeRunner::for_load_agent([
            String::new(),
            String::new(),
            String::new(),
            String::new(),
            "jackin-agent-smith".to_string(),
        ]);

        let repo_dir = paths.roles_dir.join("agent-smith");
        std::fs::create_dir_all(&repo_dir).unwrap();
        std::fs::write(
            repo_dir.join("Dockerfile"),
            "FROM projectjackin/construct:trixie\n",
        )
        .unwrap();
        std::fs::write(
            repo_dir.join("jackin.role.toml"),
            r#"dockerfile = "Dockerfile"

[claude]
plugins = []
"#,
        )
        .unwrap();

        let workspace = repo_workspace(&repo_dir);
        load_role(
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
            .find(|call| call.contains("docker run -d -it"))
            .unwrap();
        assert!(run_cmd.contains("-e JACKIN=1"));
        assert!(run_cmd.contains("-e JACKIN_DIND_HOSTNAME=jackin-agent-smith-dind"));
        assert!(!run_cmd.contains("JACKIN_DEBUG"));
    }

    #[test]
    fn load_agent_passes_debug_flag_when_enabled() {
        let temp = tempdir().unwrap();
        let paths = JackinPaths::for_tests(temp.path());
        let mut config = AppConfig::load_or_init(&paths).unwrap();
        let selector = RoleSelector::new(None, "agent-smith");
        let mut runner = FakeRunner::for_load_agent([
            String::new(),
            String::new(),
            String::new(),
            String::new(),
            "jackin-agent-smith".to_string(),
        ]);

        let repo_dir = paths.roles_dir.join("agent-smith");
        std::fs::create_dir_all(&repo_dir).unwrap();
        std::fs::write(
            repo_dir.join("Dockerfile"),
            "FROM projectjackin/construct:trixie\n",
        )
        .unwrap();
        std::fs::write(
            repo_dir.join("jackin.role.toml"),
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
        load_role(
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
            .find(|call| call.contains("docker run -d -it"))
            .unwrap();
        assert!(run_cmd.contains("-e JACKIN_DEBUG=1"));
    }

    #[test]
    fn load_options_debug_disables_intro_for_load() {
        let opts = LoadOptions::for_load(false, true, false);
        assert!(opts.no_intro, "debug mode must disable intro for load");
        assert!(opts.debug);
    }

    #[test]
    fn load_options_no_intro_flag_for_load() {
        let opts = LoadOptions::for_load(true, false, false);
        assert!(opts.no_intro, "explicit no_intro must be respected");
        assert!(!opts.debug);
    }

    #[test]
    fn load_options_intro_plays_when_no_flags_for_load() {
        let opts = LoadOptions::for_load(false, false, false);
        assert!(!opts.no_intro, "intro should play when no flags set");
    }

    #[test]
    fn load_options_debug_disables_intro_for_launch() {
        let opts = LoadOptions::for_launch(true);
        assert!(opts.no_intro, "debug mode must disable intro for launch");
        assert!(opts.debug);
    }

    #[test]
    fn load_options_intro_plays_when_no_debug_for_launch() {
        let opts = LoadOptions::for_launch(false);
        assert!(!opts.no_intro, "intro should play when debug is off");
        assert!(!opts.debug);
    }

    #[test]
    fn load_agent_injects_global_operator_env_literal() {
        let temp = tempdir().unwrap();
        let paths = JackinPaths::for_tests(temp.path());
        paths.ensure_base_dirs().unwrap();

        // Seed a config.toml with a global operator env map.
        std::fs::write(
            &paths.config_file,
            r#"[env]
OPERATOR_SMOKE = "smoke-literal"

[roles.agent-smith]
git = "https://github.com/jackin-project/jackin-agent-smith.git"
trusted = true
"#,
        )
        .unwrap();

        let mut config = AppConfig::load_or_init(&paths).unwrap();
        let selector = RoleSelector::new(None, "agent-smith");
        let mut runner = FakeRunner::for_load_agent([
            String::new(),
            String::new(),
            String::new(),
            String::new(),
            "jackin-agent-smith".to_string(),
        ]);

        let repo_dir = paths.roles_dir.join("agent-smith");
        std::fs::create_dir_all(&repo_dir).unwrap();
        std::fs::write(
            repo_dir.join("Dockerfile"),
            "FROM projectjackin/construct:trixie\n",
        )
        .unwrap();
        std::fs::write(
            repo_dir.join("jackin.role.toml"),
            r#"dockerfile = "Dockerfile"

[claude]
plugins = []
"#,
        )
        .unwrap();

        let workspace = repo_workspace(&repo_dir);
        load_role(
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
            .find(|call| call.contains("docker run -d -it"))
            .unwrap();
        assert!(
            run_cmd.contains("-e OPERATOR_SMOKE=smoke-literal"),
            "docker run must inject operator env; got: {run_cmd}"
        );
    }

    #[test]
    fn load_agent_operator_env_overrides_manifest_env() {
        // Spec: on conflict between manifest-declared env and operator
        // env, operator wins. The manifest below declares OPERATOR_SMOKE
        // as a literal "manifest-default"; the global operator env
        // declares the same key as "operator-wins". The docker run
        // command must inject the operator value.
        //
        // The `[env.OPERATOR_SMOKE]` manifest shape below matches the
        // existing EnvEntry schema in `src/env_model.rs` — if that
        // schema has diverged (e.g. `kind`/`default` field names), the
        // implementer should update the TOML fixture to match the
        // current schema; the test's *assertions* (operator-wins /
        // manifest-default not present) are unchanged.
        let temp = tempdir().unwrap();
        let paths = JackinPaths::for_tests(temp.path());
        paths.ensure_base_dirs().unwrap();

        std::fs::write(
            &paths.config_file,
            r#"[env]
OPERATOR_SMOKE = "operator-wins"

[roles.agent-smith]
git = "https://github.com/jackin-project/jackin-agent-smith.git"
trusted = true
"#,
        )
        .unwrap();

        let mut config = AppConfig::load_or_init(&paths).unwrap();
        let selector = RoleSelector::new(None, "agent-smith");
        let mut runner = FakeRunner::for_load_agent([
            String::new(),
            String::new(),
            String::new(),
            String::new(),
            "jackin-agent-smith".to_string(),
        ]);

        let repo_dir = paths.roles_dir.join("agent-smith");
        std::fs::create_dir_all(&repo_dir).unwrap();
        std::fs::write(
            repo_dir.join("Dockerfile"),
            "FROM projectjackin/construct:trixie\n",
        )
        .unwrap();
        std::fs::write(
            repo_dir.join("jackin.role.toml"),
            r#"dockerfile = "Dockerfile"

[env.OPERATOR_SMOKE]
default = "manifest-default"

[claude]
plugins = []
"#,
        )
        .unwrap();

        let workspace = repo_workspace(&repo_dir);
        load_role(
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
            .find(|call| call.contains("docker run -d -it"))
            .unwrap();
        assert!(
            run_cmd.contains("-e OPERATOR_SMOKE=operator-wins"),
            "operator env must win over manifest env on conflict; got: {run_cmd}"
        );
        assert!(
            !run_cmd.contains("-e OPERATOR_SMOKE=manifest-default"),
            "manifest value must NOT leak when operator overrides it; got: {run_cmd}"
        );
    }

    #[test]
    fn load_agent_injects_host_ref_operator_env() {
        let temp = tempdir().unwrap();
        let paths = JackinPaths::for_tests(temp.path());
        paths.ensure_base_dirs().unwrap();

        // No process-env mutation anywhere — the host env for the
        // resolver is supplied via `LoadOptions::host_env`, a plain
        // `BTreeMap<String, String>`. This keeps the test free of
        // any `std::env` write, which the crate-level
        // `unsafe_code = "forbid"` lint forbids.
        std::fs::write(
            &paths.config_file,
            r#"[env]
FROM_HOST = "$JACKIN_PR2_SMOKE_HOST_VAR"

[roles.agent-smith]
git = "https://github.com/jackin-project/jackin-agent-smith.git"
trusted = true
"#,
        )
        .unwrap();

        let mut config = AppConfig::load_or_init(&paths).unwrap();
        let selector = RoleSelector::new(None, "agent-smith");
        let mut runner = FakeRunner::for_load_agent([
            String::new(),
            String::new(),
            String::new(),
            String::new(),
            "jackin-agent-smith".to_string(),
        ]);

        let repo_dir = paths.roles_dir.join("agent-smith");
        std::fs::create_dir_all(&repo_dir).unwrap();
        std::fs::write(
            repo_dir.join("Dockerfile"),
            "FROM projectjackin/construct:trixie\n",
        )
        .unwrap();
        std::fs::write(
            repo_dir.join("jackin.role.toml"),
            r#"dockerfile = "Dockerfile"

[claude]
plugins = []
"#,
        )
        .unwrap();

        let mut host_env = std::collections::BTreeMap::new();
        host_env.insert(
            "JACKIN_PR2_SMOKE_HOST_VAR".to_string(),
            "from-host-env".to_string(),
        );

        let opts = LoadOptions {
            host_env: Some(host_env),
            ..LoadOptions::default()
        };

        let workspace = repo_workspace(&repo_dir);
        load_role(
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
            .find(|call| call.contains("docker run -d -it"))
            .unwrap();
        assert!(
            run_cmd.contains("-e FROM_HOST=from-host-env"),
            "host-ref operator env must resolve and inject; got: {run_cmd}"
        );
    }

    #[cfg(unix)]
    #[test]
    fn load_agent_injects_op_cli_resolved_value() {
        use std::os::unix::fs::PermissionsExt;

        let temp = tempdir().unwrap();
        let paths = JackinPaths::for_tests(temp.path());
        paths.ensure_base_dirs().unwrap();

        let bin_dir = temp.path().join("fake-bin");
        std::fs::create_dir_all(&bin_dir).unwrap();
        let bin_path = bin_dir.join("op");
        // The resolver first runs `op --version` as a reachability probe
        // when any value carries an OpRef, then calls `op read op://...`
        // with the canonical UUID URI. The fake must handle both.
        std::fs::write(
            &bin_path,
            "#!/bin/sh\nif [ \"$1\" = \"--version\" ]; then echo '2.30.0'; exit 0; fi\nif [ \"$1\" = \"read\" ] && [ \"$2\" = \"op://abc-vault/abc-item/api-token\" ]; then printf '%s' 'resolved-op-token'; exit 0; fi\nexit 99\n",
        )
        .unwrap();
        let mut perms = std::fs::metadata(&bin_path).unwrap().permissions();
        perms.set_mode(0o755);
        std::fs::set_permissions(&bin_path, perms).unwrap();

        std::fs::write(
            &paths.config_file,
            r#"[env]
OPERATOR_TOKEN = {op = "op://abc-vault/abc-item/api-token", path = "Personal/api/token"}

[roles.agent-smith]
git = "https://github.com/jackin-project/jackin-agent-smith.git"
trusted = true
"#,
        )
        .unwrap();

        let mut config = AppConfig::load_or_init(&paths).unwrap();
        let selector = RoleSelector::new(None, "agent-smith");
        let mut runner = FakeRunner::for_load_agent([
            String::new(),
            String::new(),
            String::new(),
            String::new(),
            "jackin-agent-smith".to_string(),
        ]);

        let repo_dir = paths.roles_dir.join("agent-smith");
        std::fs::create_dir_all(&repo_dir).unwrap();
        std::fs::write(
            repo_dir.join("Dockerfile"),
            "FROM projectjackin/construct:trixie\n",
        )
        .unwrap();
        std::fs::write(
            repo_dir.join("jackin.role.toml"),
            r#"dockerfile = "Dockerfile"

[claude]
plugins = []
"#,
        )
        .unwrap();

        // Inject the fake `op` binary path via `LoadOptions::op_runner`.
        // No process env mutation — `OpCli::with_binary` takes the path
        // as a direct argument, so the `unsafe_code = "forbid"`
        // crate-level lint stays intact and sibling tests running in
        // parallel via cargo-nextest cannot race on any shared env var.
        let op_runner: Box<dyn crate::operator_env::OpRunner> = Box::new(
            crate::operator_env::OpCli::with_binary(bin_path.to_string_lossy().to_string()),
        );
        let opts = LoadOptions {
            op_runner: Some(op_runner),
            ..LoadOptions::default()
        };

        let workspace = repo_workspace(&repo_dir);
        load_role(
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
            .find(|call| call.contains("docker run -d -it"))
            .unwrap();
        assert!(
            run_cmd.contains("-e OPERATOR_TOKEN=resolved-op-token"),
            "op:// ref must resolve via the injected OpCli and inject; got: {run_cmd}"
        );
    }

    // ── claim_container_name tests ────────────────────────────────────────────

    /// `NotFound` → claim the primary slot directly (no docker rm issued).
    #[test]
    fn claim_container_name_not_found_claims_primary() {
        let temp = tempdir().unwrap();
        let paths = JackinPaths::for_tests(temp.path());
        let selector = RoleSelector::new(None, "agent-smith");
        // inspect returns "" → NotFound
        let mut runner = FakeRunner::with_capture_queue([String::new()]);

        let (name, _lock) = claim_container_name(&paths, &selector, &mut runner).unwrap();

        assert_eq!(name, "jackin-agent-smith");
        assert!(runner.recorded.iter().any(|call| {
            call.contains(
                "docker inspect --format {{.State.Running}} {{.State.ExitCode}} {{.State.OOMKilled}} jackin-agent-smith",
            )
        }));
        assert!(
            !runner
                .recorded
                .iter()
                .any(|call| call.contains("docker rm"))
        );
    }

    /// Running → skip primary, claim clone-1.
    #[test]
    fn claim_container_name_running_skips_to_clone() {
        let temp = tempdir().unwrap();
        let paths = JackinPaths::for_tests(temp.path());
        let selector = RoleSelector::new(None, "agent-smith");
        // primary inspect → Running; clone-1 inspect → NotFound
        let mut runner =
            FakeRunner::with_capture_queue(["true 0 false".to_string(), String::new()]);

        let (name, _lock) = claim_container_name(&paths, &selector, &mut runner).unwrap();

        assert_eq!(name, "jackin-agent-smith-clone-1");
        assert!(
            !runner
                .recorded
                .iter()
                .any(|call| call.contains("docker rm"))
        );
    }

    /// Stopped / exit 0 → docker rm issued, same slot reclaimed.
    #[test]
    fn claim_container_name_clean_exit_removes_and_reclaims() {
        let temp = tempdir().unwrap();
        let paths = JackinPaths::for_tests(temp.path());
        let selector = RoleSelector::new(None, "agent-smith");
        // primary inspect → Stopped / exit 0 / no OOM
        let mut runner = FakeRunner::with_capture_queue(["false 0 false".to_string()]);

        let (name, _lock) = claim_container_name(&paths, &selector, &mut runner).unwrap();

        assert_eq!(name, "jackin-agent-smith");
        assert!(
            runner
                .recorded
                .iter()
                .any(|call| call == "docker rm jackin-agent-smith")
        );
    }

    /// Stopped / non-zero exit → skip primary, claim clone-1 (hardline territory).
    #[test]
    fn claim_container_name_crashed_skips_to_clone() {
        let temp = tempdir().unwrap();
        let paths = JackinPaths::for_tests(temp.path());
        let selector = RoleSelector::new(None, "agent-smith");
        // primary inspect → Stopped / exit 1; clone-1 → NotFound
        let mut runner =
            FakeRunner::with_capture_queue(["false 1 false".to_string(), String::new()]);

        let (name, _lock) = claim_container_name(&paths, &selector, &mut runner).unwrap();

        assert_eq!(name, "jackin-agent-smith-clone-1");
        assert!(
            !runner
                .recorded
                .iter()
                .any(|call| call.contains("docker rm"))
        );
    }

    /// slot 0 crashed, slot 1 clean-exit → slot 1 reclaimed after rm, not slot 2.
    #[test]
    fn claim_container_name_crashed_then_clean_exit_reclaims_slot_1() {
        let temp = tempdir().unwrap();
        let paths = JackinPaths::for_tests(temp.path());
        let selector = RoleSelector::new(None, "agent-smith");
        // primary → crashed; clone-1 → clean exit
        let mut runner = FakeRunner::with_capture_queue([
            "false 1 false".to_string(),
            "false 0 false".to_string(),
        ]);

        let (name, _lock) = claim_container_name(&paths, &selector, &mut runner).unwrap();

        assert_eq!(name, "jackin-agent-smith-clone-1");
        assert!(
            runner
                .recorded
                .iter()
                .any(|call| call == "docker rm jackin-agent-smith-clone-1")
        );
        assert!(!runner.recorded.iter().any(|call| call.contains("clone-2")));
    }

    #[test]
    fn verify_token_env_present_accepts_resolved_token() {
        let mut vars = std::collections::BTreeMap::new();
        vars.insert(
            "CLAUDE_CODE_OAUTH_TOKEN".to_string(),
            "sk-ant-oat01-redacted".to_string(),
        );
        assert!(verify_token_env_present(&vars).is_ok());
    }

    #[test]
    fn verify_token_env_missing_returns_actionable_error() {
        let vars = std::collections::BTreeMap::<String, String>::new();
        let err = verify_token_env_present(&vars).unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("CLAUDE_CODE_OAUTH_TOKEN"), "got: {msg}");
        // Both remediation paths must be surfaced.
        assert!(
            msg.contains("op://"),
            "error should mention the 1Password remediation path; got: {msg}"
        );
        assert!(
            msg.contains("$CLAUDE_CODE_OAUTH_TOKEN"),
            "error should mention the host env remediation path; got: {msg}"
        );
        assert!(
            msg.contains("[env]"),
            "error should point the operator at the [env] manifest table; got: {msg}"
        );
    }

    #[test]
    fn inspect_attach_outcome_capture_failure_returns_still_running() {
        // A docker daemon hiccup or a container removed mid-inspect must
        // NOT route through finalize_clean_exit's auto-cleanup path —
        // returning still_running keeps the records preserved for
        // `jackin hardline` to recover.
        use crate::isolation::finalize::AttachOutcome;
        let mut runner = crate::runtime::test_support::FakeRunner {
            fail_on: vec!["docker inspect".into()],
            ..Default::default()
        };
        let outcome = inspect_attach_outcome(&mut runner, "jackin-x").unwrap();
        assert_eq!(outcome, AttachOutcome::still_running());
    }

    /// Helper for `inspect_attach_outcome` status tests — returns a
    /// `FakeRunner` whose `docker inspect` capture returns the given
    /// `status|exit_code|oom` line. Other docker calls also queue the
    /// same response (we make only one inspect call per test).
    fn inspect_runner(
        status: &str,
        exit_code: i32,
        oom: bool,
    ) -> crate::runtime::test_support::FakeRunner {
        crate::runtime::test_support::FakeRunner {
            capture_queue: std::collections::VecDeque::from(vec![format!(
                "{status}|{exit_code}|{oom}\n",
            )]),
            ..Default::default()
        }
    }

    /// `exited` with `exit_code=0` → stopped(0) → enters `finalize_clean_exit`
    /// which is the documented happy path for clean container exits.
    #[test]
    fn inspect_attach_outcome_exited_zero_returns_stopped() {
        use crate::isolation::finalize::AttachOutcome;
        let mut runner = inspect_runner("exited", 0, false);
        let outcome = inspect_attach_outcome(&mut runner, "jackin-x").unwrap();
        assert_eq!(outcome, AttachOutcome::stopped(0));
    }

    /// `exited` with non-zero `exit_code` → preserved by finalize.
    #[test]
    fn inspect_attach_outcome_exited_nonzero_returns_stopped_with_code() {
        use crate::isolation::finalize::AttachOutcome;
        let mut runner = inspect_runner("exited", 137, false);
        let outcome = inspect_attach_outcome(&mut runner, "jackin-x").unwrap();
        assert_eq!(outcome, AttachOutcome::stopped(137));
    }

    /// `exited` with OOMKilled=true → `oom_killed`.
    #[test]
    fn inspect_attach_outcome_exited_oom_returns_oom_killed() {
        use crate::isolation::finalize::AttachOutcome;
        let mut runner = inspect_runner("exited", 137, true);
        let outcome = inspect_attach_outcome(&mut runner, "jackin-x").unwrap();
        assert_eq!(outcome, AttachOutcome::oom_killed());
    }

    /// `running` → `still_running`. The basic happy detach case.
    #[test]
    fn inspect_attach_outcome_running_returns_still_running() {
        use crate::isolation::finalize::AttachOutcome;
        let mut runner = inspect_runner("running", 0, false);
        let outcome = inspect_attach_outcome(&mut runner, "jackin-x").unwrap();
        assert_eq!(outcome, AttachOutcome::still_running());
    }

    /// `paused` → `still_running`. The container hasn't exited; treating
    /// it as stopped(0) would let `finalize_clean_exit` auto-delete its
    /// worktrees while the container is paused but recoverable.
    #[test]
    fn inspect_attach_outcome_paused_returns_still_running() {
        use crate::isolation::finalize::AttachOutcome;
        let mut runner = inspect_runner("paused", 0, false);
        let outcome = inspect_attach_outcome(&mut runner, "jackin-x").unwrap();
        assert_eq!(
            outcome,
            AttachOutcome::still_running(),
            "paused containers must NOT route through finalize_clean_exit's auto-cleanup path"
        );
    }

    /// `restarting`, `removing`, `created` → `still_running` for the same
    /// reason as `paused`: not exited, no real exit code to act on.
    #[test]
    fn inspect_attach_outcome_transient_states_return_still_running() {
        use crate::isolation::finalize::AttachOutcome;
        for status in ["restarting", "removing", "created"] {
            let mut runner = inspect_runner(status, 0, false);
            let outcome = inspect_attach_outcome(&mut runner, "jackin-x").unwrap();
            assert_eq!(
                outcome,
                AttachOutcome::still_running(),
                "status `{status}` must map to still_running",
            );
        }
    }

    /// `dead` → `still_running` (conservative: daemon failed to
    /// deinitialize; records preserved for inspection).
    #[test]
    fn inspect_attach_outcome_dead_returns_still_running() {
        use crate::isolation::finalize::AttachOutcome;
        let mut runner = inspect_runner("dead", 0, false);
        let outcome = inspect_attach_outcome(&mut runner, "jackin-x").unwrap();
        assert_eq!(outcome, AttachOutcome::still_running());
    }

    /// Unknown status (future Docker versions, exotic runtimes) →
    /// `still_running` with `debug_log`. Conservative direction so a new
    /// status string never accidentally triggers data deletion.
    #[test]
    fn inspect_attach_outcome_unknown_status_returns_still_running() {
        use crate::isolation::finalize::AttachOutcome;
        let mut runner = inspect_runner("hibernated", 0, false);
        let outcome = inspect_attach_outcome(&mut runner, "jackin-x").unwrap();
        assert_eq!(outcome, AttachOutcome::still_running());
    }
}
