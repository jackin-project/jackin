use crate::config::AppConfig;
use crate::docker::{CommandRunner, RunOptions};
use crate::instance::{
    DockerResources, InstanceIndex, InstanceManifest, InstanceQuery, InstanceStatus,
    NewInstanceManifest, RoleState,
};
use crate::paths::JackinPaths;
use crate::selector::RoleSelector;
use crate::tui;
use crate::version_check;
use anyhow::Context;
use fs2::FileExt;
use owo_colors::OwoColorize;
use std::io::IsTerminal;

use super::attach::{ContainerState, hardline_agent, inspect_container_state, wait_for_dind};
use super::cleanup::{gc_orphaned_resources, run_cleanup_command};
use super::discovery::list_running_agent_display_names;
use super::identity::{GitIdentity, build_config_rows, load_git_identity, load_host_identity};
use super::image::build_agent_image;
use super::naming::{
    LABEL_KEEP_AWAKE, LABEL_KIND_DIND, LABEL_KIND_ROLE, LABEL_MANAGED, dind_certs_volume,
    format_role_display, image_name, image_name_for_branch,
};
use super::repo_cache::resolve_agent_repo;

const MISE_TRUSTED_CONFIG_PATHS_ENV: &str = "MISE_TRUSTED_CONFIG_PATHS";

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

    /// When set, resolve this branch of the role repo instead of the default
    /// branch, build the image locally from the branch's Dockerfile (ignoring
    /// any `published_image`), and tag it with a branch-specific name so the
    /// stable image is not overwritten.
    pub role_branch: Option<String>,

    /// Exact missing instance to restore instead of scanning for candidates.
    pub restore_container_base: Option<String>,

    /// Role source URL captured in the instance manifest for restore paths.
    pub restore_role_source_git: Option<String>,
}

impl LoadOptions {
    /// Build options for `jackin load`. Debug mode implies `no_intro`.
    pub fn for_load(no_intro: bool, debug: bool, rebuild: bool) -> Self {
        Self {
            no_intro: no_intro || debug,
            debug,
            rebuild,
            ..Self::default()
        }
    }

    /// Build options for the operator console (`jackin console`).
    /// Debug mode implies `no_intro`.
    pub fn for_launch(debug: bool) -> Self {
        Self {
            no_intro: debug,
            debug,
            ..Self::default()
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
            role_branch: None,
            restore_container_base: None,
            restore_role_source_git: None,
        }
    }
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

/// Returns the per-agent mount strings in jackin's `src:dst[:ro]`
/// idiom for `docker run -v`.
///
/// Every agent in `manifest.supported_agents()` is represented on
/// `state.auth`, so the mount block checks `auth.*` flags rather than
/// matching the selected-agent variant — every provisioned agent's home
/// state reaches the container regardless of which agent started the
/// session, which is what lets `hardline --new` switch agents without
/// re-authentication.
fn agent_mounts(state: &crate::instance::RoleState) -> Vec<String> {
    let mut mounts = vec![format!(
        "{}:/jackin/state",
        state.root.join("state").display()
    )];

    if let Some(claude) = &state.auth.claude {
        mounts.push(format!(
            "{}:/home/agent/.claude",
            state.root.join("home/.claude").display()
        ));
        mounts.push(format!(
            "{}:/home/agent/.claude.json",
            state.root.join("home/.claude.json").display()
        ));
        // `forward_auth = true` for Sync (host-derived credentials) and
        // OAuthToken (the onboarding skeleton). ApiKey and Ignore set it
        // to false so a `{}` placeholder left behind by `wipe_claude_state`
        // never reaches the container. The per-file `exists()` guard keeps
        // the OAuthToken arm from mounting a stale `credentials.json` if
        // the provision-step removal failed silently.
        if claude.forward_auth {
            if claude.account_json.exists() {
                mounts.push(format!(
                    "{}:/jackin/claude/account.json",
                    claude.account_json.display()
                ));
            }
            if claude.credentials_json.exists() {
                mounts.push(format!(
                    "{}:/jackin/claude/credentials.json",
                    claude.credentials_json.display()
                ));
            }
        }
    }

    if let Some(codex) = &state.auth.codex {
        mounts.push(format!(
            "{}:/home/agent/.codex",
            state.root.join("home/.codex").display()
        ));
        if let Some(auth_json) = &codex.auth_json {
            mounts.push(format!("{}:/jackin/codex/auth.json", auth_json.display()));
        }
    }

    if let Some(amp) = &state.auth.amp {
        mounts.push(format!(
            "{}:/home/agent/.local/share/amp",
            state.root.join("home/.local/share/amp").display()
        ));
        // Bound RW at the docker level so future plumbing (symlink / bind
        // re-mount) for live bidirectional sync — see
        // `roadmap/live-auth-sync.mdx` — can rely on a writable target.
        // The entrypoint currently `cp`s the file, so in-container rotation
        // does not flow back today.
        if let Some(secrets_json) = &amp.secrets_json {
            mounts.push(format!(
                "{}:/jackin/amp/secrets.json",
                secrets_json.display()
            ));
        }
    }

    if let Some(kimi) = &state.auth.kimi {
        mounts.push(format!(
            "{}:/home/agent/.kimi",
            state.root.join("home/.kimi").display()
        ));
        if kimi.forward_auth {
            mounts.push(format!(
                "{}:/jackin/kimi",
                state.root.join("kimi").display()
            ));
        }
    }

    if let Some(opencode) = &state.auth.opencode {
        mounts.push(format!(
            "{}:/home/agent/.local/share/opencode",
            state.root.join("home/.local/share/opencode").display()
        ));
        if let Some(auth_json) = &opencode.auth_json {
            mounts.push(format!(
                "{}:/jackin/opencode/auth.json",
                auth_json.display()
            ));
        }
    }

    mounts
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

fn workspace_mise_trusted_config_paths(
    workspace: &crate::workspace::ResolvedWorkspace,
) -> Option<String> {
    let mut paths = std::collections::BTreeSet::new();
    if !workspace.workdir.trim().is_empty() {
        paths.insert(workspace.workdir.clone());
    }
    for mount in &workspace.mounts {
        if !mount.dst.trim().is_empty() {
            paths.insert(mount.dst.clone());
        }
    }

    (!paths.is_empty()).then(|| paths.into_iter().collect::<Vec<_>>().join(":"))
}

fn inject_workspace_mise_env(
    vars: &mut Vec<(String, String)>,
    workspace: &crate::workspace::ResolvedWorkspace,
) {
    if vars
        .iter()
        .any(|(key, _)| key == MISE_TRUSTED_CONFIG_PATHS_ENV)
    {
        return;
    }

    if let Some(value) = workspace_mise_trusted_config_paths(workspace) {
        vars.push((MISE_TRUSTED_CONFIG_PATHS_ENV.to_string(), value));
    }
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
    // Errors here are recoverable: fall back to xterm-256color so the
    // session still launches, but log the cause so an operator running
    // with `--debug` can see why their host's TERM didn't make it in.
    match export_host_terminfo(&host_term, cache_dir) {
        Ok(terminfo_dir) => {
            let mount = format!("{}:/home/agent/.terminfo:ro", terminfo_dir.display());
            (host_term, Some(mount))
        }
        Err(e) => {
            crate::debug_log!(
                "terminfo",
                "export failed for TERM={host_term}: {e:#}; falling back to xterm-256color (container loses {host_term}-specific capabilities)",
            );
            ("xterm-256color".to_string(), None)
        }
    }
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
    anyhow::ensure!(!term.is_empty(), "terminal name is empty");
    let terminfo_dir = cache_dir.join("terminfo");

    let linux_entry_path = linux_terminfo_entry_path(&terminfo_dir, term);
    if linux_entry_path.exists() {
        return Ok(terminfo_dir);
    }

    // A cache built by an earlier jackin on macOS lives only under the
    // hex-byte dir; relocate it instead of re-running infocmp+tic.
    let hex_entry_path = macos_terminfo_entry_path(&terminfo_dir, term);
    if hex_entry_path.exists() {
        copy_to_linux_layout(&hex_entry_path, &linux_entry_path)?;
        return Ok(terminfo_dir);
    }

    // Export the source from the host.
    crate::debug_log!("terminfo", "infocmp -x {term}");
    let infocmp = std::process::Command::new("infocmp")
        .args(["-x", term])
        .output()?;
    anyhow::ensure!(
        infocmp.status.success(),
        "infocmp failed for {term}: {}",
        String::from_utf8_lossy(&infocmp.stderr).trim()
    );

    std::fs::create_dir_all(&terminfo_dir)?;

    // Compile into the cache directory. Capture (don't suppress) stderr
    // so a non-zero `tic` exit surfaces the real cause instead of the
    // generic "tic failed" message; `tic`'s harmless success-time
    // warnings (e.g. Ghostty's "alias multiply defined") are dropped on
    // the success branch below.
    crate::debug_log!("terminfo", "tic -x -o {} -", terminfo_dir.display());
    let mut tic = std::process::Command::new("tic")
        .args(["-x", "-o"])
        .arg(&terminfo_dir)
        .arg("-")
        .stdin(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()?;
    {
        use std::io::Write;
        let mut stdin = tic
            .stdin
            .take()
            .expect("tic stdin was configured as Stdio::piped");
        stdin.write_all(&infocmp.stdout)?;
    }
    let output = tic.wait_with_output()?;
    if !output.status.success() {
        anyhow::bail!(
            "tic failed to compile terminfo for {term}: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }
    normalize_terminfo_entry_path(&terminfo_dir, term)?;

    Ok(terminfo_dir)
}

/// Path Linux ncurses (inside the role container) reads to resolve
/// `term`: `<first-char>/<term>`, e.g. `x/xterm-ghostty`. Caller
/// guarantees `term` is non-empty; ASCII first char is assumed (every
/// real terminfo name follows that — `xterm-*`, `ghostty`, `screen-*`,
/// `kitty`, ...).
fn linux_terminfo_entry_path(terminfo_dir: &std::path::Path, term: &str) -> std::path::PathBuf {
    let first = term
        .chars()
        .next()
        .expect("non-empty term checked by caller");
    terminfo_dir.join(first.to_string()).join(term)
}

/// Path macOS BSD `tic` writes to: `<hex-byte>/<term>`, e.g.
/// `78/xterm-ghostty` (since `'x' == 0x78`). Hex is lowercase to match
/// what BSD `tic` actually emits. Caller guarantees `term` is
/// non-empty.
fn macos_terminfo_entry_path(terminfo_dir: &std::path::Path, term: &str) -> std::path::PathBuf {
    let first_byte = term
        .bytes()
        .next()
        .expect("non-empty term checked by caller");
    terminfo_dir.join(format!("{first_byte:x}")).join(term)
}

/// Reconcile macOS-`tic`'s hex-byte directory layout with the
/// first-char layout Linux ncurses expects so the mounted cache
/// resolves inside containers. No-op on Linux hosts (where `tic`
/// already wrote the Linux layout) and on caches normalized by a
/// previous run.
fn normalize_terminfo_entry_path(terminfo_dir: &std::path::Path, term: &str) -> anyhow::Result<()> {
    anyhow::ensure!(!term.is_empty(), "terminal name is empty");

    let linux_entry_path = linux_terminfo_entry_path(terminfo_dir, term);
    if linux_entry_path.exists() {
        return Ok(());
    }

    let hex_entry_path = macos_terminfo_entry_path(terminfo_dir, term);
    anyhow::ensure!(
        hex_entry_path.exists(),
        "compiled terminfo entry for {term} not found at {} or {}",
        linux_entry_path.display(),
        hex_entry_path.display()
    );

    copy_to_linux_layout(&hex_entry_path, &linux_entry_path)
}

/// Copy a terminfo entry from `hex_entry_path` into `linux_entry_path`
/// atomically: write to a sibling temp file then `rename` so a
/// concurrent jackin reader on the same cache never observes a partial
/// or truncated entry. Both paths must live on the same filesystem
/// (caller already routes them under a single `terminfo_dir`, so
/// `rename` stays cross-device-safe).
fn copy_to_linux_layout(
    hex_entry_path: &std::path::Path,
    linux_entry_path: &std::path::Path,
) -> anyhow::Result<()> {
    let parent = linux_entry_path.parent().ok_or_else(|| {
        anyhow::anyhow!(
            "linux terminfo entry path {} has no parent directory",
            linux_entry_path.display()
        )
    })?;
    std::fs::create_dir_all(parent)?;

    let file_name = linux_entry_path
        .file_name()
        .and_then(|s| s.to_str())
        .ok_or_else(|| {
            anyhow::anyhow!(
                "linux terminfo entry path {} has no file name",
                linux_entry_path.display()
            )
        })?;
    let tmp_path = parent.join(format!(".{file_name}.tmp.{}", std::process::id()));
    std::fs::copy(hex_entry_path, &tmp_path)?;
    std::fs::rename(&tmp_path, linux_entry_path)?;
    Ok(())
}

// ── Role source trust ───────────────────────────────────────────────────

/// Display an untrusted-role warning and ask the operator to confirm.
/// Aborts when stdin is not a terminal or the operator declines.
/// Branch-specific trust confirmation.
///
/// Even when a role is already trusted, an unmerged branch contains unreviewed
/// code. The operator trusted the *default* branch, not this PR. A malicious
/// contributor could craft a branch whose Dockerfile runs arbitrary commands
/// during the image build on the operator's machine.
///
/// This gate always fires when `--role-branch` is set, regardless of the
/// role's `trusted` state in config. It is intentionally separate from
/// `confirm_agent_trust` so the two gates compose: loading an *untrusted*
/// role on a branch requires confirming both.
fn confirm_branch_trust(
    selector: &RoleSelector,
    source: &crate::config::RoleSource,
    branch: &str,
) -> anyhow::Result<()> {
    if !std::io::stdin().is_terminal() {
        anyhow::bail!(
            "role \"{selector}\" is being loaded from unmerged branch \"{branch}\".\n\
             Branch builds require interactive confirmation — run the command in a terminal."
        );
    }

    eprintln!();
    eprintln!(
        "{}",
        "!! Unreviewed branch — verify before proceeding !!"
            .red()
            .bold()
    );
    eprintln!();
    eprintln!("  role:   {}", selector.to_string().bold());
    eprintln!("  source: {}", source.git.yellow());
    eprintln!("  branch: {}", branch.yellow().bold());
    eprintln!();
    eprintln!(
        "  {}",
        "This branch has not been merged to the default branch.".bold()
    );
    eprintln!("  Its Dockerfile and scripts may differ from the trusted main branch.");
    eprintln!("  A malicious contributor could introduce harmful code that runs");
    eprintln!("  on your machine during the image build.");
    eprintln!();
    eprintln!(
        "  {}",
        "Review the branch diff in the role repository before continuing.".dimmed()
    );
    eprintln!();

    let confirmed = dialoguer::Confirm::new()
        .with_prompt(format!(
            "Have you reviewed branch \"{branch}\" and verified it is safe to build?"
        ))
        .default(false)
        .interact()?;

    if !confirmed {
        anyhow::bail!(
            "branch \"{branch}\" not confirmed — aborting.\n\
             Review the Dockerfile and scripts on that branch before loading it."
        );
    }

    Ok(())
}

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
    git_coauthor_trailer: bool,
    agent: crate::agent::Agent,
    resolved_env: &'a crate::env_resolver::ResolvedEnv,
    /// Resolved `[…github.env]` map (post `op://` + `$NAME`
    /// resolution). `GH_TOKEN` carries the token in the launcher's
    /// preferred env-injection path; `GH_HOST` and
    /// `GH_ENTERPRISE_TOKEN` are forwarded as-is when set so GHE
    /// targets work end to end.
    github_env: &'a std::collections::BTreeMap<String, String>,
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
        git_coauthor_trailer,
        agent,
        resolved_env,
        github_env,
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
    // hostname-mismatch error.
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
        LABEL_KIND_DIND,
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
    let testcontainers_host_override = format!(
        "{}={dind}",
        crate::env_model::TESTCONTAINERS_HOST_OVERRIDE_ENV_NAME
    );
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
        "--name",
        container_name,
        "--hostname",
        container_name,
        "--network",
        network,
        "--label",
        LABEL_MANAGED,
        "--label",
        LABEL_KIND_ROLE,
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
        &testcontainers_host_override,
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
    let git_coauthor_trailer_env = (*git_coauthor_trailer).then(|| {
        format!(
            "{}=1",
            crate::env_model::JACKIN_GIT_COAUTHOR_TRAILER_ENV_NAME
        )
    });
    if let Some(ref env) = git_coauthor_trailer_env {
        run_args.extend_from_slice(&["-e", env.as_str()]);
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
        crate::env_model::JACKIN_ENV_NAME,
        crate::env_model::JACKIN_ENV_VALUE
    ));
    // DinD reachable only via Docker network; route past HTTP_PROXY by adding
    // hostname to NO_PROXY in both casings — Go reads upper, curl/Python
    // requests/wget read lower. Mirror the merged value across both casings
    // so an operator who declared only one variant still gets full bypass
    // coverage for tools that read the other.
    let proxy_seen = resolved_env.vars.iter().any(|(k, _)| is_proxy_env_name(k));
    let upper_existing = resolved_env
        .vars
        .iter()
        .find_map(|(k, v)| (k == NO_PROXY_UPPER).then_some(v.as_str()));
    let lower_existing = resolved_env
        .vars
        .iter()
        .find_map(|(k, v)| (k == NO_PROXY_LOWER).then_some(v.as_str()));
    for (key, value) in &resolved_env.vars {
        if crate::env_model::is_reserved(key) {
            continue;
        }
        if key == NO_PROXY_UPPER || key == NO_PROXY_LOWER {
            // Synthesized below from merged casing — skip the inline emit.
            continue;
        }
        env_strings.push(format!("{key}={value}"));
    }
    // Trigger synth when any proxy class OR any NO_PROXY casing is declared.
    // The latter covers operators who set NO_PROXY without an HTTP_PROXY
    // (transparent proxy, /etc/environment, container-injected proxy vars).
    if proxy_seen || upper_existing.is_some() || lower_existing.is_some() {
        let upper_value = upper_existing
            .or(lower_existing)
            .map_or_else(|| dind.to_string(), |v| append_no_proxy_host(v, dind));
        let lower_value = lower_existing
            .or(upper_existing)
            .map_or_else(|| dind.to_string(), |v| append_no_proxy_host(v, dind));
        env_strings.push(format!("{NO_PROXY_UPPER}={upper_value}"));
        env_strings.push(format!("{NO_PROXY_LOWER}={lower_value}"));
    }

    // GitHub auth env wiring. Token mode and Sync-with-host-token both
    // export GH_TOKEN AND GITHUB_TOKEN from the same source — `gh`
    // prefers GH_TOKEN, but the official github-mcp-server and most
    // GitHub-Actions-style scripts read GITHUB_TOKEN. Exporting both
    // closes Known Gap 3 in the roadmap doc. GH_HOST and
    // GH_ENTERPRISE_TOKEN are passed through as-is when declared by
    // the operator so GHE workspaces work end to end.
    let gh_token = state.gh_provision_outcome.token();
    push_env_if_present(
        &mut env_strings,
        crate::env_model::GH_TOKEN_ENV_NAME,
        gh_token,
    );
    push_env_if_present(
        &mut env_strings,
        crate::env_model::GITHUB_TOKEN_ENV_NAME,
        gh_token,
    );
    push_env_if_present(
        &mut env_strings,
        crate::env_model::GH_HOST_ENV_NAME,
        github_env
            .get(crate::env_model::GH_HOST_ENV_NAME)
            .map(String::as_str),
    );
    push_env_if_present(
        &mut env_strings,
        crate::env_model::GH_ENTERPRISE_TOKEN_ENV_NAME,
        github_env
            .get(crate::env_model::GH_ENTERPRISE_TOKEN_ENV_NAME)
            .map(String::as_str),
    );

    for env_str in &env_strings {
        run_args.push("-e");
        run_args.push(env_str);
    }
    run_args.extend_from_slice(&[
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
    // Use the supervisor as PID 1 so the container outlives individual agent
    // sessions. The primary agent session starts immediately below via
    // `docker exec tmux new-session`, and model/CLI flags are passed there
    // rather than as CMD args to the image.
    run_args.extend_from_slice(&["--entrypoint", "/jackin/runtime/supervisor.sh"]);
    run_args.push(image);
    runner.run("docker", &run_args, None, &docker_run_opts)?;

    // Collect entrypoint args to forward model overrides into the tmux session.
    let mut session_arg_strings: Vec<String> = Vec::new();
    if let Some(model) = state.claude_model() {
        session_arg_strings.push("--model".to_string());
        session_arg_strings.push(model.to_string());
    }
    if let Some(model) = state.codex_model() {
        session_arg_strings.push("-m".to_string());
        session_arg_strings.push(model.to_string());
    }
    if let Some(model) = state.kimi_model() {
        session_arg_strings.push("--model".to_string());
        session_arg_strings.push(model.to_string());
    }
    if let Some(model) = state.opencode_model() {
        session_arg_strings.push("-m".to_string());
        session_arg_strings.push(model.to_string());
    }

    // Reconcile keep_awake AFTER the role container is running but
    // BEFORE the foreground session blocks. This is the only window in
    // which an interactive `jackin load` can spawn caffeinate.
    super::caffeinate::reconcile(paths, runner);

    // Pre-session safety check: if the supervisor exited immediately (missing
    // or broken supervisor script), surface the container logs rather than
    // failing with a cryptic docker exec error.
    if let Some(err) = diagnose_premature_exit(runner, container_name) {
        return Err(err);
    }

    // Start the first agent session inside the running container. Named
    // jackin-<agent>-<id> using the same convention as secondary sessions —
    // there is no primary/secondary distinction; all sessions are equal.
    // TMUX= prevents nested-session warnings when the operator's host
    // terminal is itself inside a tmux session.
    let first_session_name = format!(
        "jackin-{}-{}",
        agent.slug(),
        super::attach::short_session_id()
    );
    let mut exec_args: Vec<&str> = vec![
        "exec",
        "-e",
        "TMUX=",
        "-it",
        container_name,
        "tmux",
        "new-session",
        "-e",
        &jackin_agent_env,
        "-s",
        &first_session_name,
        "--",
        "/jackin/runtime/entrypoint.sh",
    ];
    for s in &session_arg_strings {
        exec_args.push(s.as_str());
    }
    let session_result = runner.run("docker", &exec_args, None, &RunOptions::default());
    // Ensure cleanup debug logs start on a fresh line after the interactive session
    eprintln!();
    if session_result.is_err()
        && let Some(err) = diagnose_premature_exit(runner, container_name)
    {
        return Err(err);
    }
    session_result?;

    Ok(())
}

/// Detect a container that exited before (or during) the foreground session
/// and return an actionable error including the captured `docker logs`.
///
/// `docker exec` against a stopped container returns "container is not
/// running" with no hint at the underlying supervisor failure (bad
/// entrypoint script, auth crash, missing mount, …). This wraps the
/// inspect + log fetch so the surfaced error names the exit code, OOM
/// flag, and the last lines of the container's combined stdout/stderr.
///
/// Returns `None` when the container is still running (the normal
/// happy path) so the caller can proceed to the session exec.
fn diagnose_premature_exit(
    runner: &mut impl crate::docker::CommandRunner,
    container_name: &str,
) -> Option<anyhow::Error> {
    use super::attach::{ContainerState, inspect_container_state};

    match inspect_container_state(runner, container_name) {
        // Default to letting the `docker exec` attempt proceed when state is
        // ambiguous: the daemon's own error from a true `NotFound`
        // (`No such container`) is just as actionable as anything we
        // could synthesize, and a transient inspect hiccup must not
        // hijack an otherwise-healthy launch.
        ContainerState::Running
        | ContainerState::NotFound
        | ContainerState::InspectUnavailable(_) => None,
        ContainerState::Stopped {
            exit_code,
            oom_killed,
        } => {
            let logs = runner
                .capture("docker", &["logs", "--tail", "40", container_name], None)
                .ok()
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty());
            let reason = if oom_killed {
                "OOM killed".to_string()
            } else {
                format!("exit {exit_code}")
            };
            let body = logs.map_or_else(
                || {
                    format!(
                        "container {container_name} exited before attach ({reason}) and produced no log output"
                    )
                },
                |text| {
                    format!(
                        "container {container_name} exited before attach ({reason}); last 40 log lines:\n{text}"
                    )
                },
            );
            Some(anyhow::anyhow!(body))
        }
    }
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

enum GitPullResult {
    Success { stdout: String },
    Failure { src: String, stderr: String },
    SpawnError { src: String, error: std::io::Error },
    JoinError { src: String },
}

fn pull_workspace_repos(workspace: &crate::workspace::ResolvedWorkspace, debug: bool) {
    pull_workspace_repos_with_git(workspace, debug, std::path::Path::new("git"));
}

fn pull_workspace_repos_with_git(
    workspace: &crate::workspace::ResolvedWorkspace,
    debug: bool,
    git_program: &std::path::Path,
) {
    let mut pulls = Vec::new();
    let mut seen_srcs = std::collections::HashSet::new();

    for mount in &workspace.mounts {
        let src = std::path::Path::new(&mount.src);
        if !src.join(".git").exists() {
            continue;
        }
        let src = mount.src.clone();
        if !seen_srcs.insert(src.clone()) {
            continue;
        }
        if debug {
            eprintln!("[jackin debug] git pull in {}", mount.src);
        }
        eprintln!("  Pulling {} …", crate::tui::shorten_home(&mount.src));
        let git_program = git_program.to_path_buf();
        pulls.push((
            src.clone(),
            std::thread::spawn(move || {
                match std::process::Command::new(git_program)
                    .args(["-C", &src, "pull"])
                    .output()
                {
                    Ok(out) if out.status.success() => GitPullResult::Success {
                        stdout: String::from_utf8_lossy(&out.stdout).into_owned(),
                    },
                    Ok(out) => GitPullResult::Failure {
                        src,
                        stderr: String::from_utf8_lossy(&out.stderr).into_owned(),
                    },
                    Err(error) => GitPullResult::SpawnError { src, error },
                }
            }),
        ));
    }

    for (src, handle) in pulls {
        match handle.join().unwrap_or(GitPullResult::JoinError { src }) {
            GitPullResult::Success { stdout } => {
                let trimmed = stdout.trim();
                if !trimmed.is_empty() {
                    eprintln!("    {trimmed}");
                }
            }
            GitPullResult::Failure { src, stderr } => {
                eprintln!("  Warning: git pull failed in {}: {}", src, stderr.trim());
            }
            GitPullResult::SpawnError { src, error } => {
                eprintln!("  Warning: could not run git pull in {src}: {error}");
            }
            GitPullResult::JoinError { src } => {
                eprintln!("  Warning: git pull thread panicked in {src}");
            }
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
        confirm_branch_trust,
    )
}

#[allow(clippy::too_many_lines, clippy::too_many_arguments)]
fn load_role_with(
    paths: &JackinPaths,
    config: &mut AppConfig,
    selector: &RoleSelector,
    workspace: &crate::workspace::ResolvedWorkspace,
    runner: &mut impl CommandRunner,
    opts: &LoadOptions,
    confirm_trust: impl FnOnce(&RoleSelector, &crate::config::RoleSource) -> anyhow::Result<()>,
    confirm_branch: impl FnOnce(&RoleSelector, &crate::config::RoleSource, &str) -> anyhow::Result<()>,
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

    if workspace.git_pull_on_entry {
        pull_workspace_repos(workspace, opts.debug);
    }

    let (source, is_new, restore_source_override) =
        resolve_launch_role_source(config, selector, opts.restore_role_source_git.as_deref())?;

    let mut steps = StepCounter::new(opts.no_intro, &selector.name);

    // Step 1: Resolve role identity (clone or update repo)
    steps.next("Resolving role identity");

    let (cached_repo, validated_repo, repo_lock) = resolve_agent_repo(
        paths,
        selector,
        &source.git,
        runner,
        opts.debug,
        opts.role_branch.as_deref(),
    )?;

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

    if !restore_source_override && (is_new || newly_trusted) {
        let mut editor = crate::config::ConfigEditor::open(paths)?;
        if let Some(role_source) = config.roles.get(&selector.key()) {
            editor.upsert_agent_source(&selector.key(), role_source);
        }
        editor.set_agent_trust(&selector.key(), true);
        *config = editor.save()?;
    }

    let agent_display_name = validated_repo.manifest.display_name(&selector.name);
    steps.role_name.clone_from(&agent_display_name);

    let agent = match opts.agent.or(workspace.default_agent) {
        Some(a) => a,
        None if std::io::stdin().is_terminal()
            && validated_repo.manifest.supported_agents().len() >= 2 =>
        {
            let supported = validated_repo.manifest.supported_agents();
            let labels: Vec<String> = supported.iter().map(|a| a.slug().to_string()).collect();
            let selection = dialoguer::Select::new()
                .with_prompt(format!(
                    "Role \"{}\" supports multiple agents. Choose one",
                    selector.key()
                ))
                .items(&labels)
                .default(0)
                .interact()?;
            supported[selection]
        }
        None => crate::agent::Agent::Claude,
    };
    validate_agent_supported(selector, &validated_repo.manifest, agent)?;

    // Branch trust gate: fires even for already-trusted roles because the
    // operator trusted the default branch, not this unreviewed PR branch.
    if let Some(branch) = opts.role_branch.as_deref() {
        confirm_branch(selector, &source, branch)?;
    }

    // Logo (if present in role repo)
    tui::print_logo(&cached_repo.repo_dir.join("logo.txt"));

    // `load_role` receives a `ResolvedWorkspace` (mounts + workdir),
    // not a name. Recover the name by matching workdir, mirroring the
    // identification rule used by `jackin workspace show`.
    let workspace_name = config
        .workspaces
        .iter()
        .find(|(_, w)| w.workdir == workspace.workdir)
        .map(|(name, _)| name.clone());

    let role_key = selector.key();
    let restore_container = if let Some(container) = opts.restore_container_base.as_ref() {
        Some(container.clone())
    } else {
        match resolve_restore_candidate(
            paths,
            workspace_name.as_deref(),
            workspace.label.as_str(),
            &workspace.workdir,
            &role_key,
            agent,
            runner,
        )? {
            RestoreResolution::StartFresh => None,
            RestoreResolution::RestoreCurrentRole(container) => Some(container),
            RestoreResolution::RecoverRelatedRole(container) => {
                let load_result = hardline_agent(paths, &container, runner).map(|()| container);
                let agent_display_name = match &load_result {
                    Ok(container_name) => format_role_display(container_name, &agent_display_name),
                    Err(_) => agent_display_name,
                };
                match load_result {
                    Ok(_) => {
                        render_exit(&agent_display_name, runner, opts);
                        return Ok(());
                    }
                    Err(error) => {
                        render_exit(&agent_display_name, runner, opts);
                        return Err(error);
                    }
                }
            }
            RestoreResolution::RebuildRelatedRole(manifest) => {
                let selector = RoleSelector::parse(&manifest.role_key)?;
                let related_opts = related_restore_load_options(opts, &manifest)?;
                let load_result =
                    load_role(paths, config, &selector, workspace, runner, &related_opts)
                        .map(|()| manifest.container_base);
                let agent_display_name = match &load_result {
                    Ok(container_name) => format_role_display(container_name, &agent_display_name),
                    Err(_) => agent_display_name,
                };
                match load_result {
                    Ok(_) => {
                        render_exit(&agent_display_name, runner, opts);
                        return Ok(());
                    }
                    Err(error) => {
                        render_exit(&agent_display_name, runner, opts);
                        return Err(error);
                    }
                }
            }
        }
    };
    let restoring = restore_container.is_some();
    let (container_name, _name_lock) = if let Some(container_name) = restore_container {
        claim_known_container_name(paths, &container_name, runner)?
    } else {
        claim_container_name(paths, workspace_name.as_deref(), selector, runner)?
    };

    let image_tag = opts.role_branch.as_deref().map_or_else(
        || image_name(selector),
        |b| image_name_for_branch(selector, b),
    );
    let config_rows = build_config_rows(
        &agent_display_name,
        &container_name,
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
    inject_workspace_mise_env(&mut merged_vars, workspace);
    let resolved_env = crate::env_resolver::ResolvedEnv { vars: merged_vars };

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
        let rebuild = opts.rebuild;
        let agent_update = !rebuild && {
            let img = image_name(selector);
            let needs_update = match agent {
                crate::agent::Agent::Claude => {
                    version_check::needs_claude_update(paths, &img, runner)
                }
                crate::agent::Agent::Opencode => {
                    version_check::needs_opencode_update(paths, &img, runner)
                }
                _ => false,
            };
            if needs_update {
                let name = match agent {
                    crate::agent::Agent::Claude => "Claude",
                    crate::agent::Agent::Opencode => "OpenCode",
                    _ => unreachable!(),
                };
                eprintln!("        {name} update available — refreshing agent layer");
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
            agent_update,
            opts.debug,
            opts.role_branch.as_deref(),
            runner,
            repo_lock,
        )?;

        let container_state = paths.data_dir.join(&container_name);
        let network = format!("{container_name}-net");
        let dind = format!("{container_name}-dind");
        let certs_volume = dind_certs_volume(&container_name);
        let host_workdir_fingerprint = manifest_host_workdir_fingerprint(workspace);
        let new_manifest = InstanceManifest::new(NewInstanceManifest {
            container_base: &container_name,
            workspace_name: workspace_name.as_deref(),
            workspace_label: workspace.label.as_str(),
            workdir: &workspace.workdir,
            host_workdir_fingerprint: &host_workdir_fingerprint,
            role_key: &role_key,
            role_display_name: &agent_display_name,
            agent_runtime: agent,
            role_source_git: &source.git,
            role_source_ref: opts.role_branch.as_deref(),
            image_tag: &image,
            docker: DockerResources {
                role_container: container_name.clone(),
                dind_container: dind.clone(),
                network: network.clone(),
                certs_volume: certs_volume.clone(),
            },
        });
        // `read_optional` already separates "manifest absent" (fall back
        // to `new_manifest` and re-record the recovered identity) from
        // "manifest unreadable" (must surface — the operator either
        // repairs the file or purges the recorded state).
        let mut instance_manifest = if restoring {
            InstanceManifest::read_optional(&container_state)
                .with_context(|| {
                    format!(
                        "restoring container `{container_name}`: existing manifest is unreadable; \
                         repair or remove the file, or run `jackin eject {container_name} --purge` to discard the recorded identity"
                    )
                })?
                .unwrap_or(new_manifest)
        } else {
            new_manifest
        };
        write_instance_status(
            paths,
            &container_state,
            &mut instance_manifest,
            InstanceStatus::Active,
        )?;

        let auth_mode = crate::config::resolve_mode(
            config,
            agent,
            workspace_name.as_deref().unwrap_or(""),
            &role_key,
        );

        // Modes that inject a credential require the well-known env
        // var to resolve to a non-empty value; fail fast with an
        // actionable structured error so the operator sees the
        // problem before we spend time starting the network and DinD
        // sidecar. Sync / Ignore short-circuit inside the helper.
        //
        // Build the per-layer mode-resolution and env-layer traces
        // here (in the caller) so the structured error carries the
        // full picture. The helpers mirror the layers walked by
        // `crate::config::resolve_mode` and
        // `operator_env::build_attributed_layers` respectively.
        let workspace_name_str = workspace_name.as_deref().unwrap_or("");
        let mode_resolution = build_mode_resolution(config, agent, workspace_name_str, &role_key);
        let env_layers = agent
            .required_env_var(auth_mode)
            .map_or_else(Vec::new, |env_var| {
                build_env_layer_states(config, workspace_name_str, &role_key, env_var)
            });
        verify_credential_env_present(
            agent,
            auth_mode,
            &operator_env,
            &mode_resolution,
            &env_layers,
            workspace_name_str,
            &role_key,
        )?;

        // Resolve the GitHub-auth axis. Layered like the per-agent
        // resolver but with no agent dimension — `.config/gh/` is
        // shared by every agent in the container.
        let github_mode = crate::config::resolve_github_mode(config, workspace_name_str, &role_key);
        let github_env_decls =
            crate::config::build_github_env_layers(config, workspace_name_str, &role_key);
        // Resolve `[…github.env]` only under modes that consume it.
        // `Sync` and `Token` both seed `GH_TOKEN` / `GH_HOST` /
        // `GH_ENTERPRISE_TOKEN` from the resolved map (Token also
        // pre-flight-checks `GH_TOKEN`). `Ignore` exports nothing, so
        // we skip the resolve to avoid unnecessary `op://` shellouts
        // — note this also defers `op://` validation errors under
        // Ignore until the operator flips back to a non-Ignore mode.
        //
        // Failures are aggregated and surfaced as a structured error
        // so a missing op-CLI doesn't produce N parallel anyhows.
        let github_resolved_env = if matches!(github_mode, crate::config::GithubAuthMode::Ignore) {
            std::collections::BTreeMap::new()
        } else {
            resolve_github_env_map(&github_env_decls, opts)?
        };
        let github_ctx = crate::instance::GithubAuthContext {
            mode: github_mode,
            token: github_resolved_env
                .get(crate::env_model::GH_TOKEN_ENV_NAME)
                .cloned(),
        };

        // Token-mode pre-flight: GH_TOKEN must resolve to a non-empty
        // value before we spend time starting DinD.
        verify_github_token_present(
            github_mode,
            github_ctx.token.as_deref(),
            workspace_name_str,
            role_key.as_str(),
        )?;

        // Per-supported-agent mode resolution — each agent in
        // `manifest.supported_agents()` honors its own configured
        // `auth_forward`. Passing the selected agent's mode would wipe
        // sibling agents' durable state when modes diverge.
        let resolve_supported_mode = |a: crate::agent::Agent| {
            crate::config::resolve_mode(config, a, workspace_name_str, &role_key)
        };
        let (state, auth_outcome) = RoleState::prepare(
            paths,
            &container_name,
            &validated_repo.manifest,
            &resolve_supported_mode,
            &github_ctx,
            &paths.home_dir,
            agent,
        )?;

        // Diagnostic line: surface the active auth mode and, for token
        // mode, the source reference of CLAUDE_CODE_OAUTH_TOKEN drawn
        // from the operator env config's raw declaration (the op://
        // reference or $NAME ref as written). Resolved values are never
        // printed.
        // Resolve the credential source-reference once per launch and
        // gate it on a non-empty resolved value, so a layer that
        // contributed an empty/whitespace string is not advertised as
        // the source. The raw lookup is the operator-typed declaration
        // (`op://...`, `$VAR`, literal); the env-var name is the
        // fallback when the resolver tracked the value but no raw
        // declaration string is recorded.
        let resolved_source: Option<String> =
            agent.required_env_var(auth_mode).and_then(|env_var| {
                let raw = lookup_operator_env_raw(
                    config,
                    Some(&role_key),
                    workspace_name.as_deref(),
                    env_var,
                );
                let has_value = resolved_env
                    .vars
                    .iter()
                    .any(|(k, v)| k == env_var && !v.trim().is_empty());
                has_value.then(|| raw.unwrap_or_else(|| env_var.to_string()))
            });

        if agent == crate::agent::Agent::Codex {
            tui::codex_auth_notice(resolved_source.as_deref(), (auth_mode, auth_outcome).into());
        } else {
            let expiry_days = workspace_name
                .as_deref()
                .filter(|_| auth_mode == crate::config::AuthForwardMode::OAuthToken)
                .and_then(|ws| {
                    match crate::workspace::token_setup::expiry_days_for_launch(paths, ws) {
                        Ok(days) => days,
                        Err(e) => {
                            // Malformed cache stamp: warn so the operator sees
                            // it once on the next launch instead of having the
                            // banner silently degrade to "no expiry known".
                            eprintln!(
                                "[jackin] note: token expiry cache for workspace {ws:?} \
                                 is unreadable ({e}); re-run \
                                 `jackin workspace claude-token setup {ws}` to refresh."
                            );
                            None
                        }
                    }
                });
            tui::auth_mode_notice(
                agent,
                &auth_mode.to_string(),
                resolved_source.as_deref(),
                expiry_days,
            );
            tui::agent_outcome_notice(agent, auth_mode, auth_outcome);
        }

        // GitHub auth summary line — agent-neutral. The breadcrumb walks
        // the [github.env] layers (NOT the regular operator-env tree)
        // because the proposal documents [github.env] as the canonical
        // place for GH_TOKEN. Falling back to lookup_operator_env_raw
        // would render bare "GH_TOKEN" when the operator follows the
        // docs.
        {
            let gh_token_key = crate::env_model::GH_TOKEN_ENV_NAME;
            let token_breadcrumb = github_env_decls.get(gh_token_key).map_or_else(
                || gh_token_key.to_string(),
                |value| auth_token_source_reference(gh_token_key, Some(value.as_display_str())),
            );
            tui::github_auth_notice(&state.gh_provision_outcome, Some(&token_breadcrumb));
        }

        // Materialize workspace mounts: shared mounts pass through;
        // worktree-isolated mounts get a per-container `git worktree`
        // staged on the host. Must run AFTER `RoleState::prepare` (so the
        // per-container state directory exists) and BEFORE the docker run
        // command is assembled (so the docker `-v` flags reflect the
        // per-mount bind sources).
        let interactive = std::io::stdin().is_terminal();
        let workspace_label = workspace.label.as_str();
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
            git_coauthor_trailer: config.git.auto_coauthor_trailer,
            agent,
            resolved_env: &resolved_env,
            github_env: &github_resolved_env,
            cache_dir: &paths.cache_dir,
            paths,
        };
        let mut cleanup = LoadCleanup::new(
            container_name.clone(),
            dind.clone(),
            certs_volume,
            network.clone(),
        );
        let launch_result = launch_role_runtime(&ctx, &mut steps, runner);
        if launch_result.is_err() {
            // FailedSetup write error must not abort cleanup; surface via debug.
            if let Err(status_err) = write_instance_status(
                paths,
                &container_state,
                &mut instance_manifest,
                InstanceStatus::FailedSetup,
            ) {
                crate::debug_log!(
                    "instance",
                    "failed to mark FailedSetup for {} after launch error: {status_err}",
                    container_name,
                );
            }
            cleanup.run(runner);
        }
        launch_result?;
        write_instance_status(
            paths,
            &container_state,
            &mut instance_manifest,
            InstanceStatus::Running,
        )?;

        // Finalize per-mount isolation worktrees BEFORE the container teardown
        // decision below: clean exits without dirty/unpushed state get their
        // worktrees swept; dirty state is preserved (with an interactive prompt
        // when stdin is a TTY). A `ReturnToAgent` choice restarts + re-attaches
        // the container exactly once so the operator can address the dirty
        // state inside the role, then the safe cleanup is retried.
        let interactive_finalize = std::io::stdin().is_terminal();
        let mut prompt = crate::isolation::finalize::StdinPrompt;
        let outcome = inspect_attach_outcome(runner, &container_name)?;
        write_instance_attach_outcome(paths, &container_state, &mut instance_manifest, outcome)?;
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
            crate::isolation::finalize::FinalizeDecision::Preserved
        ) {
            let status = preserved_instance_status(&container_state)?;
            write_instance_status(paths, &container_state, &mut instance_manifest, status)?;
        }
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
            write_instance_attach_outcome(
                paths,
                &container_state,
                &mut instance_manifest,
                outcome2,
            )?;
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
            } if matches!(
                decision,
                crate::isolation::finalize::FinalizeDecision::Preserved
            ) =>
            {
                cleanup.run(runner);
            }
            ContainerState::Stopped {
                exit_code: 0,
                oom_killed: false,
            } => {
                write_instance_status(
                    paths,
                    &container_state,
                    &mut instance_manifest,
                    InstanceStatus::CleanExited,
                )?;
                cleanup.run(runner);
            }
            ContainerState::Stopped { .. } => {
                write_instance_status(
                    paths,
                    &container_state,
                    &mut instance_manifest,
                    InstanceStatus::Crashed,
                )?;
                cleanup.disarm();
            }
            ContainerState::InspectUnavailable(reason) => {
                cleanup.disarm();
                anyhow::bail!(
                    "{}",
                    super::attach::docker_unavailable_msg(
                        &format!("inspect container `{container_name}` after the session"),
                        &reason,
                    )
                );
            }
            ContainerState::NotFound
                if matches!(
                    decision,
                    crate::isolation::finalize::FinalizeDecision::Preserved
                ) => {}
            ContainerState::NotFound => {
                write_instance_status(
                    paths,
                    &container_state,
                    &mut instance_manifest,
                    InstanceStatus::CleanExited,
                )?;
                cleanup.run(runner);
            }
        }

        Ok(container_name)
    })();

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

fn resolve_launch_role_source(
    config: &mut AppConfig,
    selector: &RoleSelector,
    restore_role_source_git: Option<&str>,
) -> anyhow::Result<(crate::config::RoleSource, bool, bool)> {
    if let Some(git) = restore_role_source_git {
        let mut source = config
            .roles
            .get(&selector.key())
            .cloned()
            .unwrap_or_default();
        source.git = git.to_string();
        source.trusted = true;
        return Ok((source, false, true));
    }
    let (source, is_new) = config.resolve_role_source(selector)?;
    Ok((source, is_new, false))
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

#[derive(Debug, Clone, PartialEq, Eq)]
enum RestoreResolution {
    StartFresh,
    RestoreCurrentRole(String),
    RecoverRelatedRole(String),
    RebuildRelatedRole(Box<InstanceManifest>),
}

#[allow(clippy::too_many_lines)]
fn resolve_restore_candidate(
    paths: &JackinPaths,
    workspace_name: Option<&str>,
    workspace_label: &str,
    workdir: &str,
    role_key: &str,
    agent: crate::agent::Agent,
    runner: &mut impl CommandRunner,
) -> anyhow::Result<RestoreResolution> {
    let mut candidates = Vec::new();
    for manifest in matching_instance_manifests(
        paths,
        workspace_name,
        workspace_label,
        workdir,
        role_key,
        agent,
    )? {
        if !manifest.is_restore_candidate() {
            continue;
        }
        let docker_state = inspect_container_state(runner, &manifest.container_base);
        if let ContainerState::InspectUnavailable(reason) = docker_state {
            anyhow::bail!(
                "{}",
                super::attach::docker_unavailable_msg(
                    &format!(
                        "inspect matching jackin instance `{}`",
                        manifest.container_base
                    ),
                    &reason,
                )
            );
        }
        if matches!(docker_state, ContainerState::NotFound) {
            candidates.push(manifest);
        }
    }

    let related = related_restore_candidates(
        paths,
        workspace_name,
        workspace_label,
        workdir,
        role_key,
        agent,
        runner,
    )?;

    match candidates.as_slice() {
        [] if related.is_empty() => Ok(RestoreResolution::StartFresh),
        [] => prompt_related_restore_candidate(workspace_label, &related),
        [only] if !std::io::stdin().is_terminal() => anyhow::bail!(
            "restore is available for `{}` but stdin is not interactive; run `jackin hardline {}` to inspect it or `jackin load` interactively from the matching workspace to rebuild jackin-managed local state. Run `jackin eject {} --purge` to discard it before starting a fresh load. Anything written only to the deleted container's writable layer is gone and will not be restored, including ad-hoc package installs, global files outside mounted paths, and DinD images.",
            only.container_base,
            only.container_base,
            only.container_base
        ),
        [only] => {
            let mut options = vec![format!("Restore {}", restore_candidate_label(paths, only))];
            options.extend(related.iter().map(|candidate| {
                format!(
                    "Recover other role with hardline {}",
                    related_restore_candidate_label(paths, candidate)
                )
            }));
            options.push("Start fresh instead".to_string());
            let option_refs: Vec<&str> = options.iter().map(String::as_str).collect();
            let choice = tui::prompt_choice(
                &format!("Unfinished jackin state exists for workspace `{workspace_label}`."),
                &option_refs,
            )?;
            if choice == 0 {
                Ok(RestoreResolution::RestoreCurrentRole(
                    only.container_base.clone(),
                ))
            } else if let Some(candidate) = related.get(choice.saturating_sub(1)) {
                recover_related_restore_candidate(candidate)
            } else {
                supersede_restore_candidates(paths, candidates)?;
                Ok(RestoreResolution::StartFresh)
            }
        }
        _ if !std::io::stdin().is_terminal() => anyhow::bail!(
            "multiple restore candidates exist for role `{role_key}` in workspace `{workspace_label}`; run `jackin hardline <container>` for the instance to recover or purge stale instances before starting a fresh load"
        ),
        _ => {
            let mut options: Vec<String> = candidates
                .iter()
                .map(|manifest| format!("Restore {}", restore_candidate_label(paths, manifest)))
                .collect();
            options.extend(related.iter().map(|candidate| {
                format!(
                    "Recover other role with hardline {}",
                    related_restore_candidate_label(paths, candidate)
                )
            }));
            options.push("Start fresh instead".to_string());
            let option_refs: Vec<&str> = options.iter().map(String::as_str).collect();
            let choice = tui::prompt_choice(
                &format!(
                    "Multiple unfinished jackin instances exist for workspace `{workspace_label}`."
                ),
                &option_refs,
            )?;
            if choice < candidates.len() {
                Ok(RestoreResolution::RestoreCurrentRole(
                    candidates[choice].container_base.clone(),
                ))
            } else if let Some(candidate) = related.get(choice - candidates.len()) {
                recover_related_restore_candidate(candidate)
            } else {
                supersede_restore_candidates(paths, candidates)?;
                Ok(RestoreResolution::StartFresh)
            }
        }
    }
}

#[derive(Debug)]
struct RelatedRestoreCandidate {
    manifest: InstanceManifest,
    docker_state: ContainerState,
}

fn related_restore_candidates(
    paths: &JackinPaths,
    workspace_name: Option<&str>,
    workspace_label: &str,
    workdir: &str,
    role_key: &str,
    agent: crate::agent::Agent,
    runner: &mut impl CommandRunner,
) -> anyhow::Result<Vec<RelatedRestoreCandidate>> {
    let mut candidates = Vec::new();
    for manifest in InstanceIndex::matching_manifests(
        &paths.data_dir,
        InstanceQuery {
            workspace_name,
            workspace_label,
            workdir,
            role_key: None,
            agent_runtime: None,
        },
    )? {
        if manifest.role_key == role_key && manifest.agent_runtime == agent.slug() {
            continue;
        }
        if !manifest.is_restore_candidate() {
            continue;
        }
        let docker_state = inspect_container_state(runner, &manifest.container_base);
        let should_prompt = match docker_state {
            ContainerState::InspectUnavailable(_) | ContainerState::NotFound => true,
            ContainerState::Running | ContainerState::Stopped { .. } => false,
        };
        if should_prompt {
            candidates.push(RelatedRestoreCandidate {
                manifest,
                docker_state,
            });
        }
    }
    Ok(candidates)
}

fn prompt_related_restore_candidate(
    workspace_label: &str,
    candidates: &[RelatedRestoreCandidate],
) -> anyhow::Result<RestoreResolution> {
    if !std::io::stdin().is_terminal() {
        anyhow::bail!(
            "unfinished jackin instances exist for workspace `{workspace_label}` under a different role or agent; run `jackin hardline <instance>` to inspect or recover them before starting a fresh load"
        );
    }

    let mut options: Vec<String> = candidates
        .iter()
        .map(related_restore_candidate_action_label)
        .collect();
    options.push("Start fresh instead".to_string());
    let option_refs: Vec<&str> = options.iter().map(String::as_str).collect();
    let choice = tui::prompt_choice(
        &format!("Unfinished jackin instances exist for workspace `{workspace_label}`."),
        &option_refs,
    )?;
    if let Some(candidate) = candidates.get(choice) {
        return recover_related_restore_candidate(candidate);
    }
    Ok(RestoreResolution::StartFresh)
}

fn recover_related_restore_candidate(
    candidate: &RelatedRestoreCandidate,
) -> anyhow::Result<RestoreResolution> {
    match candidate.docker_state {
        ContainerState::Running | ContainerState::Stopped { .. } => Ok(
            RestoreResolution::RecoverRelatedRole(candidate.manifest.container_base.clone()),
        ),
        ContainerState::NotFound => Ok(RestoreResolution::RebuildRelatedRole(Box::new(
            candidate.manifest.clone(),
        ))),
        ContainerState::InspectUnavailable(ref reason) => {
            anyhow::bail!(
                "{}",
                super::attach::docker_unavailable_msg(
                    &format!(
                        "inspect related jackin instance `{}`",
                        candidate.manifest.container_base
                    ),
                    reason,
                )
            );
        }
    }
}

fn related_restore_load_options(
    current: &LoadOptions,
    manifest: &InstanceManifest,
) -> anyhow::Result<LoadOptions> {
    Ok(LoadOptions {
        no_intro: current.no_intro,
        debug: current.debug,
        rebuild: current.rebuild,
        force: current.force,
        host_env: current.host_env.clone(),
        agent: Some(manifest.agent()?),
        role_branch: manifest.role_source_ref.clone(),
        restore_container_base: Some(manifest.container_base.clone()),
        restore_role_source_git: Some(manifest.role_source_git.clone()),
        ..LoadOptions::default()
    })
}

fn related_restore_candidate_action_label(candidate: &RelatedRestoreCandidate) -> String {
    match candidate.docker_state {
        ContainerState::Running | ContainerState::Stopped { .. } => {
            format!(
                "Recover now {}",
                related_restore_candidate_label_for_prompt(candidate)
            )
        }
        ContainerState::NotFound => {
            format!(
                "Rebuild now {}",
                related_restore_candidate_label_for_prompt(candidate)
            )
        }
        ContainerState::InspectUnavailable(_) => {
            format!(
                "Recover with hardline {}",
                related_restore_candidate_label_for_prompt(candidate)
            )
        }
    }
}

fn related_restore_candidate_label(
    paths: &JackinPaths,
    candidate: &RelatedRestoreCandidate,
) -> String {
    format!(
        "{} docker:{}",
        restore_candidate_label(paths, &candidate.manifest),
        candidate.docker_state.short_label()
    )
}

fn related_restore_candidate_label_for_prompt(candidate: &RelatedRestoreCandidate) -> String {
    format!(
        "{} role:{} agent:{} status:{} docker:{} updated:{}",
        candidate.manifest.instance_id,
        candidate.manifest.role_key,
        candidate.manifest.agent_runtime,
        candidate.manifest.status.label(),
        candidate.docker_state.short_label(),
        candidate.manifest.updated_at
    )
}

fn restore_candidate_label(paths: &JackinPaths, manifest: &InstanceManifest) -> String {
    let state_dir = paths.data_dir.join(&manifest.container_base);
    let isolation = crate::isolation::state::MountSummary::prompt_label_for_state_dir(&state_dir);
    let attach = manifest
        .last_attach_outcome
        .as_deref()
        .map_or_else(String::new, |outcome| format!(" attach:{outcome}"));
    format!(
        "{} status:{} agent:{} role:{} updated:{} {}{}",
        manifest.instance_id,
        manifest.status.label(),
        manifest.agent_runtime,
        manifest.role_key,
        manifest.updated_at,
        isolation,
        attach
    )
}

fn supersede_restore_candidates(
    paths: &JackinPaths,
    candidates: Vec<InstanceManifest>,
) -> anyhow::Result<()> {
    for mut manifest in candidates {
        let state_dir = paths.data_dir.join(&manifest.container_base);
        write_instance_status(paths, &state_dir, &mut manifest, InstanceStatus::Superseded)?;
    }
    Ok(())
}

fn matching_instance_manifests(
    paths: &JackinPaths,
    workspace_name: Option<&str>,
    workspace_label: &str,
    workdir: &str,
    role_key: &str,
    agent: crate::agent::Agent,
) -> anyhow::Result<Vec<InstanceManifest>> {
    InstanceIndex::matching_manifests(
        &paths.data_dir,
        InstanceQuery {
            workspace_name,
            workspace_label,
            workdir,
            role_key: Some(role_key),
            agent_runtime: Some(agent),
        },
    )
}

fn write_instance_status(
    paths: &JackinPaths,
    state_dir: &std::path::Path,
    manifest: &mut InstanceManifest,
    status: InstanceStatus,
) -> anyhow::Result<()> {
    manifest.mark_status(status);
    manifest.write(state_dir)?;
    InstanceIndex::update_manifest(&paths.data_dir, manifest)?;
    Ok(())
}

fn write_instance_attach_outcome(
    paths: &JackinPaths,
    state_dir: &std::path::Path,
    manifest: &mut InstanceManifest,
    outcome: crate::isolation::finalize::AttachOutcome,
) -> anyhow::Result<()> {
    manifest.touch();
    manifest.last_attach_outcome = Some(format_attach_outcome(outcome));
    manifest.write(state_dir)?;
    InstanceIndex::update_manifest(&paths.data_dir, manifest)?;
    Ok(())
}

pub(super) fn record_instance_attach_outcome(
    paths: &JackinPaths,
    container_name: &str,
    outcome: crate::isolation::finalize::AttachOutcome,
) -> anyhow::Result<()> {
    let state_dir = paths.data_dir.join(container_name);
    // Missing manifest is a legitimate no-op; corrupt manifest is
    // logged so the attach-outcome record is not silently dropped.
    let Some(mut manifest) =
        InstanceManifest::read_or_log(&state_dir, "record_instance_attach_outcome")
    else {
        return Ok(());
    };
    write_instance_attach_outcome(paths, &state_dir, &mut manifest, outcome)
}

fn format_attach_outcome(outcome: crate::isolation::finalize::AttachOutcome) -> String {
    if outcome.oom_killed {
        return "oom_killed".to_string();
    }
    outcome
        .exit_code
        .map_or_else(|| "running".to_string(), |code| format!("exit:{code}"))
}

fn preserved_instance_status(state_dir: &std::path::Path) -> anyhow::Result<InstanceStatus> {
    use crate::isolation::state::CleanupStatus;

    let records = crate::isolation::state::read_records(state_dir)?;
    if records
        .iter()
        .any(|record| record.cleanup_status == CleanupStatus::PreservedDirty)
    {
        return Ok(InstanceStatus::PreservedDirty);
    }
    if records
        .iter()
        .any(|record| record.cleanup_status == CleanupStatus::PreservedUnpushed)
    {
        return Ok(InstanceStatus::PreservedUnpushed);
    }
    Ok(InstanceStatus::RestoreAvailable)
}

fn manifest_host_workdir_fingerprint(workspace: &crate::workspace::ResolvedWorkspace) -> String {
    workspace
        .mounts
        .iter()
        .filter(|mount| path_covers_workdir(&mount.dst, &workspace.workdir))
        .max_by_key(|mount| mount.dst.len())
        .map_or_else(
            || crate::instance::manifest::host_path_fingerprint(&workspace.workdir),
            |mount| crate::instance::manifest::host_path_fingerprint(&mount.src),
        )
}

fn path_covers_workdir(mount_dst: &str, workdir: &str) -> bool {
    let mount_dst = mount_dst.trim_end_matches('/');
    workdir == mount_dst
        || workdir
            .strip_prefix(mount_dst)
            .is_some_and(|suffix| suffix.starts_with('/'))
}

/// Cap retries so a filesystem without working flock (NFS without
/// lockd, exotic mount) surfaces as an actionable error instead of an
/// unbounded spin. 64 attempts at 40 bits of ID entropy is enough that
/// a genuine collision-space exhaustion is astronomically unlikely;
/// hitting the cap signals an environmental fault, not bad luck.
const CLAIM_MAX_ATTEMPTS: u32 = 64;

/// Claim a unique DNS-safe container name by acquiring an exclusive lock file.
/// Random IDs avoid deterministic role slots; the lock still protects the
/// vanishingly small random-collision window and concurrent launch races.
fn claim_container_name(
    paths: &JackinPaths,
    workspace_name: Option<&str>,
    selector: &RoleSelector,
    runner: &mut impl CommandRunner,
) -> anyhow::Result<(String, std::fs::File)> {
    std::fs::create_dir_all(&paths.data_dir)?;

    let mut last_lock_err: Option<std::io::Error> = None;
    let mut last_unlink_err: Option<std::io::Error> = None;
    let mut occupied_attempts = 0u32;

    for attempt in 0..CLAIM_MAX_ATTEMPTS {
        let name = crate::instance::new_container_name(workspace_name, selector);

        let slot_free = match inspect_container_state(runner, &name) {
            ContainerState::Stopped {
                exit_code: 0,
                oom_killed: false,
            } => match runner.capture("docker", &["rm", &name], None) {
                Ok(_) => true,
                Err(error) if super::cleanup::is_missing_cleanup_error(&error) => true,
                Err(error) => {
                    return Err(error.context(format!(
                        "removing stale container `{name}` before reclaiming its name"
                    )));
                }
            },
            ContainerState::Running | ContainerState::Stopped { .. } => false,
            ContainerState::NotFound => true,
            ContainerState::InspectUnavailable(reason) => {
                anyhow::bail!(
                    "{}",
                    super::attach::docker_unavailable_msg(
                        &format!("claim container name `{name}`"),
                        &reason,
                    )
                );
            }
        };

        if slot_free {
            match try_acquire_name_lock(&paths.data_dir, &name) {
                Ok(lock_file) => return Ok((name, lock_file)),
                Err(NameLockError { lock, unlink }) => {
                    crate::debug_log!(
                        "runtime",
                        "claim_container_name: lock contention on {name} (attempt {attempt}): {lock}",
                    );
                    if let Some(unlink_err) = unlink {
                        last_unlink_err = Some(unlink_err);
                    }
                    last_lock_err = Some(lock);
                }
            }
        } else {
            occupied_attempts += 1;
        }
    }

    // Pick the failure mode the operator should investigate first.
    // An unlink error means broken-flock; a lock error means
    // contention; "every candidate occupied" means Docker's namespace
    // is full for this slug.
    let lock_summary = match (last_lock_err, last_unlink_err) {
        (Some(lock), Some(unlink)) => {
            format!("lock contention ({lock}); lock unlink also failed ({unlink})")
        }
        (Some(lock), None) => format!("lock contention ({lock})"),
        (None, _) if occupied_attempts == CLAIM_MAX_ATTEMPTS => {
            "all candidates already exist in Docker".to_string()
        }
        (None, _) => "no lock attempted".to_string(),
    };
    anyhow::bail!(
        "exhausted {CLAIM_MAX_ATTEMPTS} attempts to claim a unique container name ({lock_summary})"
    );
}

fn claim_known_container_name(
    paths: &JackinPaths,
    container_name: &str,
    runner: &mut impl CommandRunner,
) -> anyhow::Result<(String, std::fs::File)> {
    match inspect_container_state(runner, container_name) {
        ContainerState::NotFound => {}
        ContainerState::Running | ContainerState::Stopped { .. } => {
            anyhow::bail!(
                "cannot restore `{container_name}` because its Docker container already exists; use `jackin hardline {container_name}`"
            );
        }
        ContainerState::InspectUnavailable(reason) => {
            anyhow::bail!(
                "{}",
                super::attach::docker_unavailable_msg(
                    &format!("restore `{container_name}`"),
                    &reason,
                )
            );
        }
    }

    std::fs::create_dir_all(&paths.data_dir)?;
    match try_acquire_name_lock(&paths.data_dir, container_name) {
        Ok(lock_file) => Ok((container_name.to_string(), lock_file)),
        Err(NameLockError { lock, .. }) => anyhow::bail!(
            "cannot restore `{container_name}` because another jackin process holds its lock ({lock})"
        ),
    }
}

/// Try to acquire an exclusive flock on `<data_dir>/<name>.lock`.
/// On contention drops the handle before unlinking — broken-flock
/// filesystems (NFS without lockd) leak the artefact otherwise.
struct NameLockError {
    lock: std::io::Error,
    unlink: Option<std::io::Error>,
}

fn try_acquire_name_lock(
    data_dir: &std::path::Path,
    name: &str,
) -> Result<std::fs::File, NameLockError> {
    let lock_path = data_dir.join(format!("{name}.lock"));
    let lock_file = match std::fs::File::create(&lock_path) {
        Ok(f) => f,
        Err(lock) => return Err(NameLockError { lock, unlink: None }),
    };
    if let Err(lock) = lock_file.try_lock_exclusive() {
        drop(lock_file);
        let unlink = std::fs::remove_file(&lock_path).err().inspect(|err| {
            crate::debug_log!(
                "runtime",
                "try_acquire_name_lock: failed to unlink {} after lock contention: {err}",
                lock_path.display(),
            );
        });
        return Err(NameLockError { lock, unlink });
    }
    Ok(lock_file)
}

/// What we found in a single env layer when looking up the credential
/// var required by an `auth_forward` mode.
///
/// Carried inside `LaunchError::AuthCredentialMissing` so both CLI text
/// rendering and TUI structured rendering can reuse the same trace
/// without re-deriving it from the resolved env map.
///
/// All three variants are constructed today: `Unset` by both
/// `verify_credential_env_present`'s tests and `build_env_layer_states`
/// when a layer is silent; `ResolvedLiteral` / `ResolvedOpRef` by
/// `build_env_layer_states` when a layer declares the var.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EnvLayerState {
    /// Layer does not declare the var at all.
    Unset,
    /// Layer declares the var with a literal (or `$VAR`) value that
    /// resolved to a non-empty string.
    ResolvedLiteral,
    /// Layer declares the var with an `op://...` reference that
    /// resolved to a non-empty string.
    ResolvedOpRef,
}

impl std::fmt::Display for EnvLayerState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Unset => write!(f, "unset"),
            Self::ResolvedLiteral => write!(f, "resolved (literal)"),
            Self::ResolvedOpRef => write!(f, "resolved (op://...)"),
        }
    }
}

/// Errors produced by launch-time validation that benefit from
/// structured fields (e.g. TUI rendering, multi-line CLI output) rather
/// than the stringy `anyhow::bail!` shape used elsewhere in this file.
///
/// Today this enum carries a single variant — the auth-credential
/// pre-flight failure — but it's defined as an enum so that future
/// launch-time validators (`DinD` readiness, image build preconditions,
/// etc.) can grow structured variants alongside it without churning the
/// type at every call site.
//
// Constructed by Task 13's `verify_credential_env_present` and bubbled
// through Task 14's `load_role_with` integration.
#[derive(Debug, thiserror::Error)]
pub enum LaunchError {
    /// `auth_forward` mode requires a credential env var to resolve to
    /// a non-empty value, but the resolved operator env doesn't carry
    /// it. Carries enough structure for both CLI rendering (multi-line
    /// text via the `Display` impl) and TUI rendering (structured
    /// panel) to reuse the same data without re-deriving it.
    #[error("{}", render_auth_credential_missing(
        *.agent,
        *.mode,
        .env_var,
        .workspace,
        .role,
        .mode_resolution,
        .env_layers,
    ))]
    AuthCredentialMissing {
        /// Agent the launch was for (drives the var name and remediation copy).
        agent: crate::agent::Agent,
        /// Resolved `auth_forward` mode that requires the credential.
        mode: crate::config::AuthForwardMode,
        /// Well-known credential env var (e.g. `ANTHROPIC_API_KEY`,
        /// `CLAUDE_CODE_OAUTH_TOKEN`, `OPENAI_API_KEY`, `AMP_API_KEY`) that must
        /// resolve to a non-empty value for `mode`.
        env_var: &'static str,
        /// Workspace name the launch targets (for messaging).
        workspace: String,
        /// Role selector key the launch targets (for messaging).
        role: String,
        /// Trace of the 3-layer mode resolution: each entry pairs a
        /// human-readable layer label (e.g. `"workspace × role × claude"`)
        /// with the mode value declared at that layer (`None` = layer
        /// is silent). Layers are ordered most-specific first.
        mode_resolution: Vec<(String, Option<crate::config::AuthForwardMode>)>,
        /// Trace of the env-layer resolution for `env_var`: each entry
        /// pairs a TOML-table label (e.g. `"[workspaces.proj.env]"`)
        /// with what we found in that layer. Layers are ordered
        /// lowest-to-highest priority so the rendered output reads
        /// chronologically the same way operators read TOML.
        env_layers: Vec<(String, EnvLayerState)>,
    },
}

/// Constant gutter between the layer-label column and the `->` arrow
/// in `render_auth_credential_missing` output. Sized so even the longest
/// label has visible whitespace before the arrow (matches the spec test
/// fixture `workspace × role × claude    -> api_key`).
const RENDER_LABEL_GUTTER: usize = 4;

/// Cap on the layer-label column width. Keeps a pathologically-long
/// label (60+ chars) from blowing up line width while still
/// comfortably fitting any realistic env-table path.
const RENDER_LABEL_WIDTH_CAP: usize = 60;

/// Compute the padded column width used for the layer-label column in
/// `render_auth_credential_missing`. Pulled out so both the
/// mode-resolution and env-layer sections share the same arithmetic
/// without repeating the gutter / cap constants inline.
fn render_label_width<T>(rows: &[(String, T)]) -> usize {
    rows.iter()
        .map(|(l, _)| l.chars().count())
        .max()
        .unwrap_or(0)
        .saturating_add(RENDER_LABEL_GUTTER)
        .min(RENDER_LABEL_WIDTH_CAP)
}

/// Render the structured multi-line `AuthCredentialMissing` message
/// for CLI display. The TUI panel consumes the structured fields
/// directly and ignores this rendering — they intentionally share the
/// data, not the formatting.
fn render_auth_credential_missing(
    agent: crate::agent::Agent,
    mode: crate::config::AuthForwardMode,
    env_var: &str,
    workspace: &str,
    role: &str,
    mode_resolution: &[(String, Option<crate::config::AuthForwardMode>)],
    env_layers: &[(String, EnvLayerState)],
) -> String {
    use std::fmt::Write as _;
    let mut out = String::new();

    let _ = writeln!(
        out,
        "cannot launch {agent} in workspace '{workspace}' role '{role}'"
    );
    let _ = writeln!(
        out,
        "       \u{2014} auth_forward is '{mode}', which requires {env_var}"
    );
    let _ = writeln!(
        out,
        "         to resolve to a non-empty value, but it is unset."
    );

    if !mode_resolution.is_empty() {
        let _ = writeln!(out);
        let _ = writeln!(out, "  Effective auth resolution:");
        let label_width = render_label_width(mode_resolution);
        for (idx, (label, value)) in mode_resolution.iter().enumerate() {
            let value_str = value
                .as_ref()
                .map_or_else(|| "(none)".to_string(), ToString::to_string);
            let suffix = if idx == 0 { "  (most-specific)" } else { "" };
            let _ = writeln!(out, "    {label:<label_width$}-> {value_str}{suffix}");
        }
    }

    if !env_layers.is_empty() {
        let _ = writeln!(out);
        let _ = writeln!(
            out,
            "  Env layer resolution for {env_var} (lowest -> highest):"
        );
        let label_width = render_label_width(env_layers);
        for (label, state) in env_layers {
            let _ = writeln!(out, "    {label:<label_width$}-> {state}");
        }
    }

    let agent_title = match agent {
        crate::agent::Agent::Claude => "Claude",
        crate::agent::Agent::Codex => "Codex",
        crate::agent::Agent::Amp => "Amp",
        crate::agent::Agent::Kimi => "Kimi",
        crate::agent::Agent::Opencode => "OpenCode",
    };

    let _ = writeln!(out);
    let _ = writeln!(out, "  Fix one of:");
    let _ = writeln!(
        out,
        "    - Open the Auth panel:  jackin tui workspaces  \u{2192} '{workspace}' \u{2192} Auth \u{2192} {role} / {agent_title}"
    );
    // `jackin config env set` does not yet support `--workspace`; show
    // the role-scoped form (the closest existing remediation) so we
    // don't print a flag the operator can't actually use today.
    let _ = writeln!(
        out,
        "    - Or by hand:           jackin config env set {env_var} <value> --role {role}"
    );
    let _ = writeln!(
        out,
        "    - Or change the mode:   set auth_forward = 'sync' at one of the layers above"
    );

    // Trim the trailing newline left by the final `writeln!` so callers
    // composing this into larger errors don't get an awkward extra blank
    // line.
    if out.ends_with('\n') {
        out.pop();
    }
    out
}

/// Token-mode pre-flight for the `[github]` axis: `GH_TOKEN` must
/// resolve to a non-empty value before launch proceeds. The other
/// modes (`Sync` / `Ignore`) have nothing to verify here.
///
/// Extracted from `load_role_with` so the bail-message shape and
/// trigger condition can be unit-pinned without orchestrating the
/// full launch flow.
fn verify_github_token_present(
    github_mode: crate::config::GithubAuthMode,
    resolved_token: Option<&str>,
    workspace: &str,
    role: &str,
) -> anyhow::Result<()> {
    if !matches!(github_mode, crate::config::GithubAuthMode::Token) {
        return Ok(());
    }
    if resolved_token.is_some_and(|s| !s.is_empty()) {
        return Ok(());
    }
    anyhow::bail!(
        "auth_forward = \"token\" for [github] in workspace '{workspace}' role '{role}' \
         requires GH_TOKEN to resolve to a non-empty value, but it is unset.\n\n\
         Fix one of:\n  \
         - Add GH_TOKEN under [github.env] (or [workspaces.{workspace}.github.env], or \
         [workspaces.{workspace}.roles.{role}.github.env]).\n  \
         - Or change the mode: set auth_forward = \"sync\" or \"ignore\"."
    );
}

/// Resolve the `[…github.env]` declarations through the same
/// `op://` + host-env dispatch as regular operator env. Honors the
/// `op_runner` / `host_env` test seams on `LoadOptions` so tests stay
/// hermetic.
fn resolve_github_env_map(
    declarations: &std::collections::BTreeMap<String, crate::operator_env::EnvValue>,
    opts: &LoadOptions,
) -> anyhow::Result<std::collections::BTreeMap<String, String>> {
    let mut resolved: std::collections::BTreeMap<String, String> =
        std::collections::BTreeMap::new();
    if declarations.is_empty() {
        return Ok(resolved);
    }
    let default_runner = crate::operator_env::OpCli::new();
    let runner: &dyn crate::operator_env::OpRunner =
        opts.op_runner.as_deref().unwrap_or(&default_runner);
    let mut host_env_fn = |name: &str| -> Result<String, std::env::VarError> {
        opts.host_env.as_ref().map_or_else(
            || std::env::var(name),
            |map| map.get(name).cloned().ok_or(std::env::VarError::NotPresent),
        )
    };
    let mut errors: Vec<String> = Vec::new();
    for (key, value) in declarations {
        match crate::operator_env::resolve_env_value(
            "[github.env]",
            key,
            value,
            runner,
            &mut host_env_fn,
        ) {
            Ok(v) => {
                resolved.insert(key.clone(), v);
            }
            Err(e) => errors.push(format!("  - {e}")),
        }
    }
    if !errors.is_empty() {
        anyhow::bail!(
            "github env resolution failed for {} var(s):\n{}",
            errors.len(),
            errors.join("\n")
        );
    }
    Ok(resolved)
}

/// Verify that the credential env var required by the resolved
/// `auth_forward` mode is present (and non-empty) in the merged
/// operator-env map. Drives the per-(agent, mode) lookup through
/// `Agent::required_env_var`, which is the single source of truth
/// for which env var carries which credential.
///
/// Returns `Ok(())` for modes that don't inject a credential
/// (`Sync`, `Ignore`) — the operator may still need a host-side
/// credential in those modes, but the launch-time pre-flight has
/// nothing to verify in the merged env.
///
/// Otherwise looks up the well-known env var in `merged_env`. If the
/// value is non-empty, returns `Ok(())`. If it is missing or empty,
/// returns `LaunchError::AuthCredentialMissing` carrying the
/// `mode_resolution` and `env_layers` traces passed in by the caller,
/// so both CLI and TUI rendering can surface a structured remediation
/// panel. The caller (`load_role_with`, see `build_mode_resolution` /
/// `build_env_layer_states`) owns trace derivation; this helper only
/// looks up the env var and constructs the error.
pub fn verify_credential_env_present(
    agent: crate::agent::Agent,
    mode: crate::config::AuthForwardMode,
    merged_env: &std::collections::BTreeMap<String, String>,
    mode_resolution: &[(String, Option<crate::config::AuthForwardMode>)],
    env_layers: &[(String, EnvLayerState)],
    workspace: &str,
    role: &str,
) -> Result<(), LaunchError> {
    let Some(env_var) = agent.required_env_var(mode) else {
        return Ok(());
    };
    let value = merged_env.get(env_var).map_or("", String::as_str);
    if !value.is_empty() {
        return Ok(());
    }

    Err(LaunchError::AuthCredentialMissing {
        agent,
        mode,
        env_var,
        workspace: workspace.to_string(),
        role: role.to_string(),
        mode_resolution: mode_resolution.to_vec(),
        env_layers: env_layers.to_vec(),
    })
}

/// Build the 3-layer mode-resolution trace (most-specific first) that
/// `LaunchError::AuthCredentialMissing` carries for rendering. Walks
/// the same layers as [`crate::config::resolve_mode`] but records each
/// layer's value (or `None` when silent) so the operator can see at a
/// glance which TOML layer wins.
fn build_mode_resolution(
    cfg: &AppConfig,
    agent: crate::agent::Agent,
    workspace: &str,
    role: &str,
) -> Vec<(String, Option<crate::config::AuthForwardMode>)> {
    use crate::agent::Agent;
    let agent_at_global = match agent {
        Agent::Claude => cfg.claude.as_ref().map(|c| c.auth_forward),
        Agent::Codex => cfg.codex.as_ref().map(|c| c.auth_forward),
        Agent::Amp => cfg.amp.as_ref().map(|c| c.auth_forward),
        Agent::Kimi => cfg.kimi.as_ref().map(|c| c.auth_forward),
        Agent::Opencode => cfg.opencode.as_ref().map(|c| c.auth_forward),
    };
    let agent_at_workspace = cfg.workspaces.get(workspace).and_then(|ws| match agent {
        Agent::Claude => ws.claude.as_ref().map(|c| c.auth_forward),
        Agent::Codex => ws.codex.as_ref().map(|c| c.auth_forward),
        Agent::Amp => ws.amp.as_ref().map(|c| c.auth_forward),
        Agent::Kimi => ws.kimi.as_ref().map(|c| c.auth_forward),
        Agent::Opencode => ws.opencode.as_ref().map(|c| c.auth_forward),
    });
    let agent_at_ws_role = cfg
        .workspaces
        .get(workspace)
        .and_then(|ws| ws.roles.get(role))
        .and_then(|ro| match agent {
            Agent::Claude => ro.claude.as_ref().map(|c| c.auth_forward),
            Agent::Codex => ro.codex.as_ref().map(|c| c.auth_forward),
            Agent::Amp => ro.amp.as_ref().map(|c| c.auth_forward),
            Agent::Kimi => ro.kimi.as_ref().map(|c| c.auth_forward),
            Agent::Opencode => ro.opencode.as_ref().map(|c| c.auth_forward),
        });
    vec![
        (format!("workspace × role × {agent}"), agent_at_ws_role),
        (format!("workspace × {agent}"), agent_at_workspace),
        (format!("global × {agent}"), agent_at_global),
    ]
}

/// Build the 4-layer env-layer trace (lowest precedence first) for the
/// credential var. Layers mirror `operator_env::build_attributed_layers`:
/// `[env]` < `[roles.<role>.env]` < `[workspaces.<ws>.env]` <
/// `[workspaces.<ws>.roles.<role>.env]`. Each entry records whether the
/// layer declared the var as a literal, an `op://...` reference, or
/// not at all.
fn build_env_layer_states(
    cfg: &AppConfig,
    workspace: &str,
    role: &str,
    env_var: &str,
) -> Vec<(String, EnvLayerState)> {
    const fn classify(value: &crate::operator_env::EnvValue) -> EnvLayerState {
        match value {
            crate::operator_env::EnvValue::Plain(_) => EnvLayerState::ResolvedLiteral,
            crate::operator_env::EnvValue::OpRef(_) => EnvLayerState::ResolvedOpRef,
        }
    }
    let global = cfg.env.get(env_var).map_or(EnvLayerState::Unset, classify);
    let role_global = cfg
        .roles
        .get(role)
        .and_then(|r| r.env.get(env_var))
        .map_or(EnvLayerState::Unset, classify);
    let workspace_global = cfg
        .workspaces
        .get(workspace)
        .and_then(|ws| ws.env.get(env_var))
        .map_or(EnvLayerState::Unset, classify);
    let workspace_role = cfg
        .workspaces
        .get(workspace)
        .and_then(|ws| ws.roles.get(role))
        .and_then(|ro| ro.env.get(env_var))
        .map_or(EnvLayerState::Unset, classify);
    vec![
        ("[env]".to_string(), global),
        (format!("[roles.{role}.env]"), role_global),
        (format!("[workspaces.{workspace}.env]"), workspace_global),
        (
            format!("[workspaces.{workspace}.roles.{role}.env]"),
            workspace_role,
        ),
    ]
}

/// Return a printable source reference for the credential env var
/// `env_var` (e.g. `"CLAUDE_CODE_OAUTH_TOKEN"`, `"ANTHROPIC_API_KEY"`)
/// given the raw (unresolved) declaration value from the operator env
/// config (e.g. `"Private/Claude/security/auth token"` or
/// `"$CLAUDE_CODE_OAUTH_TOKEN"`). Produces the `"KEY ← value"` form
/// consumed by `tui::auth_mode_notice`. When `raw` is `None` or the
/// display string is empty, falls back to the bare env-var name.
/// Append `KEY=value` to `env_strings` when `value` is `Some` and
/// non-empty. Centralizes the "skip the env push when the value is
/// missing or blank" check used by every optional env injection.
fn push_env_if_present(env_strings: &mut Vec<String>, key: &str, value: Option<&str>) {
    if let Some(v) = value
        && !v.is_empty()
    {
        env_strings.push(format!("{key}={v}"));
    }
}

/// Canonical CLI proxy env vars `curl`, `wget`, and Go's HTTP client read.
/// `FTP_PROXY` / `RSYNC_PROXY` are intentionally out of scope: they don't
/// reach `DinD`'s daemon socket, so adding them here would only widen the
/// detection surface without changing bypass behavior.
const PROXY_VAR_NAMES: &[&str] = &[
    "HTTP_PROXY",
    "HTTPS_PROXY",
    "ALL_PROXY",
    "http_proxy",
    "https_proxy",
    "all_proxy",
];
const NO_PROXY_UPPER: &str = "NO_PROXY";
const NO_PROXY_LOWER: &str = "no_proxy";

fn is_proxy_env_name(key: &str) -> bool {
    PROXY_VAR_NAMES.contains(&key)
}

fn append_no_proxy_host(value: &str, host: &str) -> String {
    if value
        .split(',')
        .map(str::trim)
        .any(|entry| entry.eq_ignore_ascii_case(host))
    {
        return value.to_string();
    }

    if value.trim().is_empty() {
        host.to_string()
    } else {
        format!("{value},{host}")
    }
}

fn auth_token_source_reference(env_var: &str, raw: Option<&str>) -> String {
    match raw {
        None | Some("") => env_var.to_string(),
        Some(value) => format!("{env_var} \u{2190} {value}"),
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

    fn workspace_manifest(
        container_name: &str,
        role_key: &str,
        role_display_name: &str,
        agent: crate::agent::Agent,
    ) -> InstanceManifest {
        let role_source_git = format!("https://example.invalid/{role_key}.git");
        let image_tag = format!("{}{role_key}", crate::runtime::naming::IMAGE_PREFIX);
        InstanceManifest::new(NewInstanceManifest {
            container_base: container_name,
            workspace_name: Some("workspace"),
            workspace_label: "workspace",
            workdir: "/workspace",
            host_workdir_fingerprint: "sha256:test",
            role_key,
            role_display_name,
            agent_runtime: agent,
            role_source_git: &role_source_git,
            role_source_ref: None,
            image_tag: &image_tag,
            docker: DockerResources {
                role_container: container_name.to_string(),
                dind_container: format!("{container_name}-dind"),
                network: format!("{container_name}-net"),
                certs_volume: format!("{container_name}-dind-certs"),
            },
        })
    }

    fn write_indexed_manifest(paths: &JackinPaths, manifest: &InstanceManifest) {
        manifest
            .write(&paths.data_dir.join(&manifest.container_base))
            .unwrap();
        InstanceIndex::update_manifest(&paths.data_dir, manifest).unwrap();
    }

    fn resolve_workspace_restore(
        paths: &JackinPaths,
        role_key: &str,
        runner: &mut impl CommandRunner,
    ) -> anyhow::Result<RestoreResolution> {
        resolve_restore_candidate(
            paths,
            Some("workspace"),
            "workspace",
            "/workspace",
            role_key,
            crate::agent::Agent::Claude,
            runner,
        )
    }

    #[test]
    fn normalize_terminfo_entry_path_copies_macos_hex_dir_to_linux_char_dir() {
        let tmp = tempdir().unwrap();
        let terminfo_dir = tmp.path().join("terminfo");
        let macos_dir = terminfo_dir.join("78");
        std::fs::create_dir_all(&macos_dir).unwrap();
        std::fs::write(macos_dir.join("xterm-ghostty"), b"compiled-entry").unwrap();

        normalize_terminfo_entry_path(&terminfo_dir, "xterm-ghostty").unwrap();

        let linux_entry = terminfo_dir.join("x").join("xterm-ghostty");
        assert_eq!(
            std::fs::read(linux_entry).unwrap(),
            b"compiled-entry",
            "Linux ncurses must be able to find macOS-compiled Ghostty terminfo"
        );
    }

    #[test]
    fn normalize_terminfo_entry_path_accepts_existing_linux_char_dir() {
        let tmp = tempdir().unwrap();
        let terminfo_dir = tmp.path().join("terminfo");
        let linux_dir = terminfo_dir.join("g");
        std::fs::create_dir_all(&linux_dir).unwrap();
        std::fs::write(linux_dir.join("ghostty"), b"compiled-entry").unwrap();

        normalize_terminfo_entry_path(&terminfo_dir, "ghostty").unwrap();

        assert_eq!(
            std::fs::read(linux_dir.join("ghostty")).unwrap(),
            b"compiled-entry"
        );
        assert!(
            !terminfo_dir.join("67").exists(),
            "no-op normalize must not create the macOS hex dir"
        );
    }

    #[test]
    fn normalize_terminfo_entry_path_errors_when_neither_layout_present() {
        let tmp = tempdir().unwrap();
        let terminfo_dir = tmp.path().join("terminfo");

        let err = normalize_terminfo_entry_path(&terminfo_dir, "xterm-ghostty").unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("not found"), "got: {msg}");
        assert!(msg.contains("x/xterm-ghostty"), "got: {msg}");
        assert!(msg.contains("78/xterm-ghostty"), "got: {msg}");
    }

    #[test]
    fn normalize_terminfo_entry_path_errors_on_empty_term() {
        let tmp = tempdir().unwrap();
        let err = normalize_terminfo_entry_path(&tmp.path().join("terminfo"), "").unwrap_err();
        assert!(
            err.to_string().contains("terminal name is empty"),
            "got: {err}"
        );
    }

    #[cfg(unix)]
    #[test]
    fn normalize_terminfo_entry_path_resolves_alias_symlink_in_hex_dir() {
        // Ghostty terminfo source has `xterm-ghostty|ghostty,...`; BSD
        // `tic` writes one file plus alias symlinks. `fs::copy` follows
        // symlinks, so the Linux destination ends up with the file
        // content rather than a dangling link.
        let tmp = tempdir().unwrap();
        let terminfo_dir = tmp.path().join("terminfo");
        let primary_dir = terminfo_dir.join("78");
        let alias_dir = terminfo_dir.join("67");
        std::fs::create_dir_all(&primary_dir).unwrap();
        std::fs::create_dir_all(&alias_dir).unwrap();
        let primary = primary_dir.join("xterm-ghostty");
        std::fs::write(&primary, b"compiled-entry").unwrap();
        std::os::unix::fs::symlink(&primary, alias_dir.join("ghostty")).unwrap();

        normalize_terminfo_entry_path(&terminfo_dir, "ghostty").unwrap();

        assert_eq!(
            std::fs::read(terminfo_dir.join("g").join("ghostty")).unwrap(),
            b"compiled-entry",
            "alias symlink in hex dir must resolve to the primary entry's content"
        );
    }

    #[test]
    fn macos_terminfo_entry_path_lowercase_hex_letter_for_kitty() {
        // 'k' = 0x6b. BSD `tic` formats the byte as lowercase hex; an
        // accidental {:X} (uppercase) would silently break lookups.
        let dir = std::path::Path::new("/tmp/test-cache");
        assert_eq!(
            macos_terminfo_entry_path(dir, "kitty"),
            dir.join("6b").join("kitty"),
        );
        assert_eq!(
            linux_terminfo_entry_path(dir, "kitty"),
            dir.join("k").join("kitty"),
        );
    }

    #[test]
    fn export_host_terminfo_returns_cached_linux_entry_without_invoking_subprocess() {
        // Pre-populated linux entry → early return before infocmp/tic.
        // Test passes on hosts without infocmp/tic installed because the
        // happy path never reaches the subprocess fork.
        let tmp = tempdir().unwrap();
        let cache_dir = tmp.path();
        let terminfo_dir = cache_dir.join("terminfo");
        let linux_dir = terminfo_dir.join("x");
        std::fs::create_dir_all(&linux_dir).unwrap();
        std::fs::write(linux_dir.join("xterm-ghostty"), b"pre-existing").unwrap();

        let result = export_host_terminfo("xterm-ghostty", cache_dir).unwrap();
        assert_eq!(result, terminfo_dir);
        assert_eq!(
            std::fs::read(linux_dir.join("xterm-ghostty")).unwrap(),
            b"pre-existing",
            "cache-hit must not rewrite the existing entry"
        );
    }

    #[test]
    fn export_host_terminfo_relocates_macos_hex_layout_without_invoking_subprocess() {
        // Pre-populated hex entry → upgrade-path branch fires before
        // infocmp/tic. Operators who built the cache before this PR
        // should not pay the subprocess cost on every launch.
        let tmp = tempdir().unwrap();
        let cache_dir = tmp.path();
        let terminfo_dir = cache_dir.join("terminfo");
        let hex_dir = terminfo_dir.join("78");
        std::fs::create_dir_all(&hex_dir).unwrap();
        std::fs::write(hex_dir.join("xterm-ghostty"), b"stale-cache").unwrap();

        let result = export_host_terminfo("xterm-ghostty", cache_dir).unwrap();
        assert_eq!(result, terminfo_dir);
        assert_eq!(
            std::fs::read(terminfo_dir.join("x").join("xterm-ghostty")).unwrap(),
            b"stale-cache",
            "upgrade-path must relocate the hex entry into the linux layout"
        );
    }

    #[test]
    fn export_host_terminfo_errors_on_empty_term() {
        let tmp = tempdir().unwrap();
        let err = export_host_terminfo("", tmp.path()).unwrap_err();
        assert!(
            err.to_string().contains("terminal name is empty"),
            "got: {err}"
        );
    }

    #[test]
    fn diagnose_premature_exit_returns_none_when_container_running() {
        // Single inspect = "running" → fast path returns Ok(()) so attach
        // proceeds. The function must NOT consume the logs queue entry in
        // this case.
        let mut runner = FakeRunner::with_capture_queue(["true 0 false".to_string()]);
        let result = super::diagnose_premature_exit(&mut runner, "jk-the-architect");
        assert!(
            result.is_none(),
            "running container must not be diagnosed as a failure"
        );
    }

    #[test]
    fn diagnose_premature_exit_includes_logs_when_container_already_stopped() {
        // First capture: inspect → exited 127 (entrypoint command not found).
        // Second capture: docker logs → entrypoint stderr.
        let mut runner = FakeRunner::with_capture_queue([
            "false 127 false".to_string(),
            "/jackin/runtime/entrypoint.sh: line 85: exec: codex: not found".to_string(),
        ]);
        let err = super::diagnose_premature_exit(&mut runner, "jk-the-architect")
            .expect("stopped container must produce a diagnostic error");
        let msg = err.to_string();
        assert!(
            msg.contains("exit 127"),
            "exit code missing from msg: {msg}"
        );
        assert!(
            msg.contains("codex: not found"),
            "logs missing from msg: {msg}"
        );
        assert!(
            runner
                .recorded
                .iter()
                .any(|c| c.contains("docker logs --tail 40 jk-the-architect")),
            "must shell out to `docker logs` to capture the entrypoint output"
        );
    }

    #[test]
    fn diagnose_premature_exit_flags_oom_kill_distinct_from_normal_exit() {
        let mut runner =
            FakeRunner::with_capture_queue(["false 137 true".to_string(), String::new()]);
        let err = super::diagnose_premature_exit(&mut runner, "jackin-x")
            .expect("OOM-killed container is a premature exit");
        let msg = err.to_string();
        assert!(msg.contains("OOM killed"), "expected OOM marker in: {msg}");
        assert!(
            msg.contains("no log output"),
            "empty logs branch missing: {msg}"
        );
    }

    #[test]
    fn diagnose_premature_exit_passes_through_when_inspect_returns_notfound() {
        // Empty inspect output maps to `ContainerState::NotFound`. We
        // intentionally defer to the exec error rather than synthesize a
        // less-helpful diagnostic — and a transient inspect hiccup must not
        // abort an otherwise-healthy launch.
        let mut runner = FakeRunner::with_capture_queue([String::new()]);
        assert!(
            super::diagnose_premature_exit(&mut runner, "jackin-x").is_none(),
            "NotFound must not abort launch before exec attempt"
        );
    }

    #[test]
    fn agent_mounts_for_claude_ignore_mode_mounts_state_but_no_auth_handoff() {
        // Ignore mode must still mount durable Claude home state so
        // conversations/plugins survive a Docker delete, but auth handoff
        // files under /jackin/claude/ must not flow into the container.
        use crate::agent::Agent;
        use crate::instance::RoleState;

        let temp = tempdir().unwrap();
        let paths = JackinPaths::for_tests(temp.path());
        let manifest_temp = tempdir().unwrap();
        std::fs::write(
            manifest_temp.path().join("jackin.role.toml"),
            r#"version = "v1alpha3"
dockerfile = "Dockerfile"

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
            "jk-agent-smith",
            &manifest,
            &|_| crate::config::AuthForwardMode::Ignore,
            &crate::instance::GithubAuthContext::default(),
            temp.path(),
            Agent::Claude,
        )
        .unwrap();

        let mounts = agent_mounts(&state);
        assert!(
            mounts.iter().any(|m| m.contains(":/jackin/state")),
            "jackin state mount missing: {mounts:?}"
        );
        assert!(
            mounts.iter().any(|m| m.contains(":/home/agent/.claude")),
            "durable Claude home mount missing: {mounts:?}"
        );
        assert!(
            mounts
                .iter()
                .any(|m| m.contains(":/home/agent/.claude.json")),
            "durable Claude account file mount missing: {mounts:?}"
        );
        assert!(
            !mounts.iter().any(|m| m.contains("/jackin/claude/")),
            "ignore mode must not mount Claude auth handoff files: {mounts:?}"
        );
    }

    #[test]
    fn agent_mounts_for_claude_sync_mode_forwards_auth_files() {
        // Sync mode + host auth present → both account.json and
        // credentials.json flow under /jackin/claude/. Plugins are baked
        // into the image and do not need a runtime mount.
        use crate::agent::Agent;
        use crate::instance::RoleState;

        let temp = tempdir().unwrap();
        let paths = JackinPaths::for_tests(temp.path());
        let manifest_temp = tempdir().unwrap();
        std::fs::write(
            manifest_temp.path().join("jackin.role.toml"),
            r#"version = "v1alpha3"
dockerfile = "Dockerfile"

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

        // Seed a fake host home with both Claude files so sync resolves.
        let host_home = temp.path().join("host_home");
        std::fs::create_dir_all(host_home.join(".claude")).unwrap();
        std::fs::write(
            host_home.join(".claude.json"),
            r#"{"oauthAccount":{"emailAddress":"test@example.com"}}"#,
        )
        .unwrap();
        std::fs::write(
            host_home.join(".claude/.credentials.json"),
            r#"{"claudeAiOauth":{"accessToken":"t","refreshToken":"r"}}"#,
        )
        .unwrap();

        let (state, _) = RoleState::prepare(
            &paths,
            "jk-agent-smith",
            &manifest,
            &|_| crate::config::AuthForwardMode::Sync,
            &crate::instance::GithubAuthContext::default(),
            &host_home,
            Agent::Claude,
        )
        .unwrap();

        let mounts = agent_mounts(&state);
        assert!(
            mounts
                .iter()
                .any(|m| m.contains("/jackin/claude/account.json") && !m.ends_with(":ro")),
            "account.json mount missing under /jackin/claude/: {mounts:?}",
        );
        assert!(
            mounts
                .iter()
                .any(|m| m.contains("/jackin/claude/credentials.json") && !m.ends_with(":ro")),
            "credentials.json mount missing under /jackin/claude/: {mounts:?}",
        );
    }

    #[test]
    fn agent_mounts_for_claude_oauth_token_mode_mounts_skeleton_only() {
        // OAuthToken mode writes a `{"hasCompletedOnboarding":true}`
        // skeleton at account.json (so the in-container CLI does not
        // run its login wizard) and removes credentials.json. The
        // launcher must mount the skeleton AND must not mount any
        // stale credentials.json that survived the provision step.
        use crate::agent::Agent;
        use crate::instance::RoleState;

        let temp = tempdir().unwrap();
        let paths = JackinPaths::for_tests(temp.path());
        let manifest_temp = tempdir().unwrap();
        std::fs::write(
            manifest_temp.path().join("jackin.role.toml"),
            r#"version = "v1alpha3"
dockerfile = "Dockerfile"

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
            "jk-agent-smith",
            &manifest,
            &|_| crate::config::AuthForwardMode::OAuthToken,
            &crate::instance::GithubAuthContext::default(),
            temp.path(),
            Agent::Claude,
        )
        .unwrap();

        let mounts = agent_mounts(&state);
        assert!(
            mounts
                .iter()
                .any(|m| m.contains("/jackin/claude/account.json")),
            "account.json skeleton must be mounted under oauth_token mode: {mounts:?}",
        );
        assert!(
            !mounts
                .iter()
                .any(|m| m.contains("/jackin/claude/credentials.json")),
            "credentials.json must NOT be mounted under oauth_token mode \
             (the env var is the credential): {mounts:?}",
        );
    }

    #[test]
    fn agent_mounts_for_codex_without_auth_mounts_state_but_no_auth_handoff() {
        use crate::agent::Agent;
        use crate::instance::RoleState;

        let temp = tempdir().unwrap();
        let paths = JackinPaths::for_tests(temp.path());
        let manifest_temp = tempdir().unwrap();
        std::fs::write(
            manifest_temp.path().join("jackin.role.toml"),
            r#"version = "v1alpha3"
dockerfile = "Dockerfile"
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
            "jk-agent-smith",
            &manifest,
            &|_| crate::config::AuthForwardMode::Ignore,
            &crate::instance::GithubAuthContext::default(),
            temp.path(),
            Agent::Codex,
        )
        .unwrap();

        let mounts = agent_mounts(&state);
        assert!(
            mounts.iter().any(|m| m.contains(":/jackin/state")),
            "jackin state mount missing: {mounts:?}"
        );
        assert!(
            mounts.iter().any(|m| m.contains(":/home/agent/.codex")),
            "durable Codex home mount missing: {mounts:?}"
        );
        assert!(
            !mounts.iter().any(|m| m.contains("/jackin/codex/auth.json")),
            "no auth.json handoff when auth is ignored: {mounts:?}"
        );
    }

    #[test]
    fn agent_mounts_for_codex_synced_includes_auth_json() {
        use crate::agent::Agent;
        use crate::instance::RoleState;

        let temp = tempdir().unwrap();
        let paths = JackinPaths::for_tests(temp.path());
        let manifest_temp = tempdir().unwrap();
        std::fs::write(
            manifest_temp.path().join("jackin.role.toml"),
            r#"version = "v1alpha3"
dockerfile = "Dockerfile"
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

        // Stage a host ~/.codex/auth.json so Sync mode succeeds.
        let host_home = temp.path().join("host_home");
        std::fs::create_dir_all(host_home.join(".codex")).unwrap();
        std::fs::write(
            host_home.join(".codex/auth.json"),
            "{\"auth_mode\":\"chatgpt\"}",
        )
        .unwrap();

        let (state, _) = RoleState::prepare(
            &paths,
            "jk-agent-smith",
            &manifest,
            &|_| crate::config::AuthForwardMode::Sync,
            &crate::instance::GithubAuthContext::default(),
            &host_home,
            Agent::Codex,
        )
        .unwrap();

        let mounts = agent_mounts(&state);
        assert!(
            mounts.iter().any(|m| m.contains(":/home/agent/.codex")),
            "durable Codex home mount missing: {mounts:?}"
        );
        assert!(
            mounts
                .iter()
                .any(|m| m.contains("/jackin/codex/auth.json") && !m.ends_with(":ro")),
            "auth.json handoff missing: {mounts:?}"
        );
    }

    #[test]
    fn agent_mounts_for_codex_host_missing_omits_auth_json() {
        use crate::agent::Agent;
        use crate::instance::RoleState;

        let temp = tempdir().unwrap();
        let paths = JackinPaths::for_tests(temp.path());
        let manifest_temp = tempdir().unwrap();
        std::fs::write(
            manifest_temp.path().join("jackin.role.toml"),
            r#"version = "v1alpha3"
dockerfile = "Dockerfile"
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
            "jk-agent-smith",
            &manifest,
            &|_| crate::config::AuthForwardMode::Sync,
            &crate::instance::GithubAuthContext::default(),
            temp.path().join("empty_host_home").as_path(),
            Agent::Codex,
        )
        .unwrap();

        let mounts = agent_mounts(&state);
        assert!(
            mounts.iter().any(|m| m.contains(":/home/agent/.codex")),
            "durable Codex home mount missing: {mounts:?}"
        );
        assert!(
            !mounts.iter().any(|m| m.contains("/jackin/codex/auth.json")),
            "no auth.json handoff when host has no ~/.codex/auth.json: {mounts:?}"
        );
    }

    #[test]
    fn agent_mounts_for_amp_synced_includes_secrets_json() {
        use crate::agent::Agent;
        use crate::instance::RoleState;

        let temp = tempdir().unwrap();
        let paths = JackinPaths::for_tests(temp.path());
        let manifest_temp = tempdir().unwrap();
        std::fs::write(
            manifest_temp.path().join("jackin.role.toml"),
            r#"version = "v1alpha3"
dockerfile = "Dockerfile"
agents = ["amp"]

[amp]
"#,
        )
        .unwrap();
        std::fs::write(
            manifest_temp.path().join("Dockerfile"),
            "FROM projectjackin/construct:trixie\n",
        )
        .unwrap();
        let manifest = crate::manifest::RoleManifest::load(manifest_temp.path()).unwrap();

        let host_home = temp.path().join("host_home");
        std::fs::create_dir_all(host_home.join(".local/share/amp")).unwrap();
        std::fs::write(
            host_home.join(".local/share/amp/secrets.json"),
            "{\"apiKey@https://ampcode.com/\":\"sgamp_user_test\"}",
        )
        .unwrap();

        let (state, _) = RoleState::prepare(
            &paths,
            "jk-the-architect",
            &manifest,
            &|_| crate::config::AuthForwardMode::Sync,
            &crate::instance::GithubAuthContext::default(),
            &host_home,
            Agent::Amp,
        )
        .unwrap();

        let mounts = agent_mounts(&state);
        assert!(
            mounts
                .iter()
                .any(|m| m.contains(":/home/agent/.local/share/amp")),
            "durable Amp data mount missing: {mounts:?}"
        );
        assert!(
            mounts
                .iter()
                .any(|m| m.contains("/jackin/amp/secrets.json") && !m.ends_with(":ro")),
            "secrets.json handoff missing: {mounts:?}"
        );
    }

    #[test]
    fn agent_mounts_for_amp_ignore_mounts_state_but_no_auth_handoff() {
        use crate::agent::Agent;
        use crate::instance::RoleState;

        let temp = tempdir().unwrap();
        let paths = JackinPaths::for_tests(temp.path());
        let manifest_temp = tempdir().unwrap();
        std::fs::write(
            manifest_temp.path().join("jackin.role.toml"),
            r#"version = "v1alpha3"
dockerfile = "Dockerfile"
agents = ["amp"]

[amp]
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
            "jk-the-architect",
            &manifest,
            &|_| crate::config::AuthForwardMode::Ignore,
            &crate::instance::GithubAuthContext::default(),
            temp.path(),
            Agent::Amp,
        )
        .unwrap();

        let mounts = agent_mounts(&state);
        assert!(
            mounts.iter().any(|m| m.contains(":/jackin/state")),
            "jackin state mount missing: {mounts:?}"
        );
        assert!(
            mounts
                .iter()
                .any(|m| m.contains(":/home/agent/.local/share/amp")),
            "durable Amp data mount missing: {mounts:?}"
        );
        assert!(
            !mounts
                .iter()
                .any(|m| m.contains("/jackin/amp/secrets.json")),
            "ignore mode must not mount Amp auth handoff files: {mounts:?}"
        );
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
                    "/data/jk-the-architect/git/worktree/repo/Users/donbeave/Projects/jackin-project/jackin/jk-the-architect"
                        .into(),
                dst: "/Users/donbeave/Projects/jackin-project/jackin".into(),
                readonly: false,
                isolation: MountIsolation::Worktree,
                worktree_aux: Some(WorktreeAuxMounts {
                    host_git_dir: "/Users/donbeave/Projects/jackin-project/jackin/.git".into(),
                    host_git_target:
                        "/jackin/host/Users/donbeave/Projects/jackin-project/jackin/.git".into(),
                    git_file_override:
                        "/data/jk-the-architect/git/overrides/Users/donbeave/Projects/jackin-project/jackin/.git"
                            .into(),
                    git_file_target: "/Users/donbeave/Projects/jackin-project/jackin/.git".into(),
                    gitdir_back_override:
                        "/data/jk-the-architect/git/overrides/Users/donbeave/Projects/jackin-project/jackin/gitdir"
                            .into(),
                    gitdir_back_target:
                        "/jackin/host/Users/donbeave/Projects/jackin-project/jackin/.git/worktrees/jk-the-architect/gitdir"
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
            "/data/jk-the-architect/git/worktree/repo/Users/donbeave/Projects/jackin-project/jackin/jk-the-architect:/Users/donbeave/Projects/jackin-project/jackin"
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
                ":/jackin/host/Users/donbeave/Projects/jackin-project/jackin/.git/worktrees/jk-the-architect/gitdir:ro"
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

    #[test]
    fn workspace_mise_paths_cover_workdir_and_mount_destinations() {
        let workspace = crate::workspace::ResolvedWorkspace {
            label: "sample-workspace".to_string(),
            workdir: "/workspace".to_string(),
            mounts: vec![
                crate::workspace::MountConfig {
                    src: "/host/jackin".to_string(),
                    dst: "/workspace/jackin".to_string(),
                    readonly: false,
                    isolation: crate::isolation::MountIsolation::Shared,
                },
                crate::workspace::MountConfig {
                    src: "/host/homebrew-tap".to_string(),
                    dst: "/workspace/homebrew-tap".to_string(),
                    readonly: false,
                    isolation: crate::isolation::MountIsolation::Shared,
                },
            ],
            default_agent: None,
            keep_awake_enabled: false,
            git_pull_on_entry: false,
        };

        let value = workspace_mise_trusted_config_paths(&workspace).unwrap();

        assert_eq!(
            value,
            "/workspace:/workspace/homebrew-tap:/workspace/jackin"
        );
    }

    #[test]
    fn workspace_mise_env_does_not_override_operator_value() {
        let workspace = repo_workspace(std::path::Path::new("/host/repo"));
        let mut vars = vec![(
            MISE_TRUSTED_CONFIG_PATHS_ENV.to_string(),
            "/operator/trusted".to_string(),
        )];

        inject_workspace_mise_env(&mut vars, &workspace);

        assert_eq!(
            vars,
            vec![(
                MISE_TRUSTED_CONFIG_PATHS_ENV.to_string(),
                "/operator/trusted".to_string()
            )]
        );
    }

    #[test]
    fn git_pull_on_entry_starts_all_repo_pulls_before_waiting() {
        let temp = tempdir().unwrap();
        let bin_dir = temp.path().join("bin");
        let marker_dir = temp.path().join("markers");
        std::fs::create_dir_all(&bin_dir).unwrap();
        std::fs::create_dir_all(&marker_dir).unwrap();

        let git_script = bin_dir.join("git");
        std::fs::write(
            &git_script,
            r#"#!/bin/sh
set -eu
marker_dir="$(dirname "$0")/../markers"
touch "$marker_dir/$(basename "$2").started"
i=0
while [ "$(find "$marker_dir" -name '*.started' | wc -l | tr -d ' ')" -lt 2 ]; do
  i=$((i + 1))
  if [ "$i" -gt 80 ]; then
    echo "timed out waiting for peer pull" >&2
    exit 42
  fi
  sleep 0.025
done
echo "pulled $2"
"#,
        )
        .unwrap();
        let mut perms = std::fs::metadata(&git_script).unwrap().permissions();
        std::os::unix::fs::PermissionsExt::set_mode(&mut perms, 0o755);
        std::fs::set_permissions(&git_script, perms).unwrap();

        let repo_a = temp.path().join("repo-a");
        let repo_b = temp.path().join("repo-b");
        std::fs::create_dir_all(repo_a.join(".git")).unwrap();
        std::fs::create_dir_all(repo_b.join(".git")).unwrap();

        let workspace = crate::workspace::ResolvedWorkspace {
            label: "parallel".to_string(),
            workdir: "/workspace".to_string(),
            mounts: vec![
                crate::workspace::MountConfig {
                    src: repo_a.display().to_string(),
                    dst: "/workspace/a".to_string(),
                    readonly: false,
                    isolation: crate::isolation::MountIsolation::Shared,
                },
                crate::workspace::MountConfig {
                    src: repo_b.display().to_string(),
                    dst: "/workspace/b".to_string(),
                    readonly: false,
                    isolation: crate::isolation::MountIsolation::Shared,
                },
            ],
            default_agent: None,
            keep_awake_enabled: false,
            git_pull_on_entry: true,
        };

        pull_workspace_repos_with_git(&workspace, false, &git_script);

        assert!(marker_dir.join("repo-a.started").is_file());
        assert!(marker_dir.join("repo-b.started").is_file());
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
            git_pull_on_entry: false,
        }
    }

    fn arg_after(command: &str, flag: &str) -> String {
        let mut args = command.split_whitespace();
        while let Some(arg) = args.next() {
            if arg == flag {
                return args.next().unwrap_or_default().to_string();
            }
        }
        String::new()
    }

    fn launched_role_container_name(runner: &FakeRunner) -> String {
        let command = runner
            .recorded
            .iter()
            .find(|call| call.contains("docker run -d --name ") && call.contains("supervisor.sh"))
            .expect("expected role docker run command");
        arg_after(command, "--name")
    }

    fn launched_dind_container_name(runner: &FakeRunner) -> String {
        let command = runner
            .recorded
            .iter()
            .find(|call| call.contains("docker run -d --name ") && !call.contains("supervisor.sh"))
            .expect("expected DinD docker run command");
        arg_after(command, "--name")
    }

    fn dind_env_from_run_cmd(run_cmd: &str) -> String {
        run_cmd
            .split_whitespace()
            .find_map(|arg| arg.strip_prefix("JACKIN_DIND_HOSTNAME="))
            .expect("expected JACKIN_DIND_HOSTNAME env")
            .to_string()
    }

    #[test]
    fn validate_agent_supported_rejects_unsupported_choice() {
        let temp = tempdir().unwrap();
        std::fs::write(
            temp.path().join("jackin.role.toml"),
            r#"version = "v1alpha3"
dockerfile = "Dockerfile"

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
    fn trust_gate_rejects_untrusted_agent_in_non_interactive_context() {
        let selector = RoleSelector::new(Some("evil-org"), "backdoor");
        let source = crate::config::RoleSource {
            git: "https://github.com/evil-org/jackin-backdoor.git".to_string(),
            trusted: false,
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

    #[test]
    fn restore_role_source_override_uses_manifest_source_without_mutating_config() {
        let selector = RoleSelector::new(None, "agent-smith");
        let mut config = AppConfig::default();
        config.roles.insert(
            "agent-smith".to_string(),
            crate::config::RoleSource {
                git: "https://example.invalid/current.git".to_string(),
                trusted: true,
                env: std::collections::BTreeMap::new(),
            },
        );

        let (source, is_new, restore_override) = resolve_launch_role_source(
            &mut config,
            &selector,
            Some("https://example.invalid/recorded.git"),
        )
        .unwrap();

        assert_eq!(source.git, "https://example.invalid/recorded.git");
        assert!(source.trusted);
        assert!(!is_new);
        assert!(restore_override);
        assert_eq!(
            config.roles.get("agent-smith").unwrap().git,
            "https://example.invalid/current.git"
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
        let mut runner =
            FakeRunner::for_load_agent(["false 0 false".to_string(), "false 0 false".to_string()]);

        let repo_dir = crate::repo::CachedRepo::new(&paths, &selector).repo_dir;
        std::fs::create_dir_all(&repo_dir).unwrap();
        std::fs::write(
            repo_dir.join("Dockerfile"),
            "FROM projectjackin/construct:trixie\n",
        )
        .unwrap();
        std::fs::write(
            repo_dir.join("jackin.role.toml"),
            r#"version = "v1alpha3"
dockerfile = "Dockerfile"

[claude]
model = "sonnet"
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
            |_, _, _| Ok(()),
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
            call.contains("docker build ") && call.contains("-t jk_chainargos_the-architect")
        }));
        assert!(runner.recorded.iter().any(|call| {
            call.contains("docker inspect --format {{.State.Running}} {{.State.ExitCode}} {{.State.OOMKilled}} jk-")
                && call.contains("thearchitect")
        }));
        let run_cmd = runner
            .recorded
            .iter()
            .find(|call| {
                call.contains("docker run -d --name jk-")
                    && call.contains("thearchitect")
                    && call.contains("supervisor.sh")
            })
            .unwrap();
        // Model flag is forwarded to the tmux session, not the docker run CMD.
        let session_cmd = runner
            .recorded
            .iter()
            .find(|call| call.contains("tmux new-session") && call.contains("entrypoint.sh"))
            .unwrap();
        assert!(session_cmd.contains(" --model sonnet"));
        let container_name = launched_role_container_name(&runner);
        assert!(crate::instance::naming::is_dns_label(&container_name));
        assert!(!container_name.contains("__"));
        assert!(!container_name.contains("clone"));
        assert!(!run_cmd.contains("JACKIN_CODEX_MODEL"));
        assert!(
            !runner
                .recorded
                .iter()
                .any(|call| call.contains("claude plugin install"))
        );

        let dind = launched_dind_container_name(&runner);
        assert!(crate::instance::naming::is_dns_label(&dind));
        assert!(!dind.contains("__"));
        let dind_cmd = runner
            .recorded
            .iter()
            .find(|call| call.contains(&format!("docker run -d --name {dind}")))
            .expect("expected DinD startup command");
        assert!(
            dind_cmd.contains(&format!("DOCKER_TLS_SAN=DNS:{dind}")),
            "DinD SAN must include the DNS-safe DinD name with a DNS: prefix"
        );
    }

    #[test]
    fn load_namespaced_agent_aborts_when_trust_declined() {
        let temp = tempdir().unwrap();
        let paths = JackinPaths::for_tests(temp.path());
        let mut config = AppConfig::load_or_init(&paths).unwrap();
        let selector = RoleSelector::new(Some("evil-org"), "backdoor");
        let mut runner = FakeRunner::for_load_agent([String::new(), String::new()]);

        let repo_dir = crate::repo::CachedRepo::new(&paths, &selector).repo_dir;
        std::fs::create_dir_all(&repo_dir).unwrap();
        std::fs::write(
            repo_dir.join("Dockerfile"),
            "FROM projectjackin/construct:trixie\n",
        )
        .unwrap();
        std::fs::write(
            repo_dir.join("jackin.role.toml"),
            r#"version = "v1alpha3"
dockerfile = "Dockerfile"

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
            |_, _, _| Ok(()),
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
        let mut runner =
            FakeRunner::for_load_agent(["false 0 false".to_string(), "false 0 false".to_string()]);

        let repo_dir = crate::repo::CachedRepo::new(&paths, &selector).repo_dir;
        std::fs::create_dir_all(&repo_dir).unwrap();
        std::fs::write(
            repo_dir.join("Dockerfile"),
            "FROM projectjackin/construct:trixie\n",
        )
        .unwrap();
        std::fs::write(
            repo_dir.join("jackin.role.toml"),
            r#"version = "v1alpha3"
dockerfile = "Dockerfile"

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
            git_pull_on_entry: false,
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
            .find(|call| call.contains("docker run -d") && call.contains("supervisor.sh"))
            .unwrap();
        assert!(run_cmd.contains(&format!("{}:/test-data:ro", mount_src.display())));
    }

    #[test]
    fn load_agent_runs_attached_without_runtime_plugins_mount() {
        let temp = tempdir().unwrap();
        let paths = JackinPaths::for_tests(temp.path());
        let mut config = AppConfig::load_or_init(&paths).unwrap();
        let selector = RoleSelector::new(None, "agent-smith");
        let mut runner = FakeRunner::for_load_agent([
            String::new(),
            String::new(),
            "false 0 false".to_string(),
            "false 0 false".to_string(),
        ]);

        let repo_dir = crate::repo::CachedRepo::new(&paths, &selector).repo_dir;
        std::fs::create_dir_all(&repo_dir).unwrap();
        std::fs::write(
            repo_dir.join("Dockerfile"),
            "FROM projectjackin/construct:trixie\n",
        )
        .unwrap();
        std::fs::write(
            repo_dir.join("jackin.role.toml"),
            r#"version = "v1alpha3"
dockerfile = "Dockerfile"

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
            runner
                .recorded
                .iter()
                .any(|call| call.contains("docker build ") && call.contains("-t jk_agent-smith"))
        );
        assert!(
            runner
                .run_recorded
                .iter()
                .any(|call| call.contains("docker build "))
        );
        assert!(runner.recorded.iter().any(|call| {
            call.contains("docker inspect --format {{.State.Running}} {{.State.ExitCode}} {{.State.OOMKilled}} jk-")
                && call.contains("agentsmith")
        }));
        assert!(
            runner.recorded.iter().any(
                |call| call.contains("docker run -d --name jk-") && call.contains("agentsmith")
            )
        );
        assert!(
            !runner
                .recorded
                .iter()
                .any(|call| call.contains("/jackin/claude/plugins.json:ro"))
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

        let repo_dir = crate::repo::CachedRepo::new(&paths, &selector).repo_dir;
        std::fs::create_dir_all(&repo_dir).unwrap();
        std::fs::write(
            repo_dir.join("Dockerfile"),
            "FROM projectjackin/construct:trixie\n",
        )
        .unwrap();
        std::fs::write(
            repo_dir.join("jackin.role.toml"),
            r#"version = "v1alpha3"
dockerfile = "Dockerfile"
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
        // No published_image and no --rebuild → workspace mode without --pull
        assert!(!build_cmd.contains("--pull"));

        let run_cmd = runner
            .recorded
            .iter()
            .find(|call| call.contains("docker run -d") && call.contains("supervisor.sh"))
            .unwrap();
        assert!(
            !run_cmd.contains("JACKIN_AGENT"),
            "JACKIN_AGENT must not be in docker run"
        );
        assert!(!run_cmd.contains("JACKIN_CODEX_MODEL"));
        // Model flag and JACKIN_AGENT are forwarded to the tmux session, not the docker run CMD.
        let session_cmd = runner
            .recorded
            .iter()
            .find(|call| call.contains("tmux new-session") && call.contains("entrypoint.sh"))
            .unwrap();
        assert!(session_cmd.contains("JACKIN_AGENT=codex"));
        assert!(session_cmd.contains(" -m gpt-5"));
        assert!(run_cmd.contains("-e OPENAI_API_KEY=test-openai-key"));
        assert!(!run_cmd.contains("/jackin/codex/config.toml"));
        // Multi-agent role `agents = ["claude", "codex"]` provisions
        // every supported agent's home state so `hardline --new --agent
        // claude` can switch agents without re-authentication. The
        // selected-agent runtime is still Codex (`JACKIN_AGENT=codex` /
        // `-m gpt-5`), but Claude's mounts must be present.
        assert!(run_cmd.contains("/home/agent/.claude"));
        assert!(run_cmd.contains("/home/agent/.codex"));
        assert!(
            !paths
                .data_dir
                .join("jk-agent-smith")
                .join("codex")
                .join("config.toml")
                .exists()
        );
    }

    /// Codex CLI drives interactive `ChatGPT` login when no API key is
    /// present, so jackin must not gate launch on `OPENAI_API_KEY`.
    #[test]
    fn load_agent_launches_codex_without_openai_key() {
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

        let repo_dir = crate::repo::CachedRepo::new(&paths, &selector).repo_dir;
        std::fs::create_dir_all(&repo_dir).unwrap();
        std::fs::write(
            repo_dir.join("Dockerfile"),
            "FROM projectjackin/construct:trixie\n",
        )
        .unwrap();
        std::fs::write(
            repo_dir.join("jackin.role.toml"),
            r#"version = "v1alpha3"
dockerfile = "Dockerfile"
agents = ["codex"]

[codex]
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

        let run_cmd = runner
            .recorded
            .iter()
            .find(|call| call.contains("docker run -d") && call.contains("supervisor.sh"))
            .expect("role docker run should fire even without OPENAI_API_KEY");
        assert!(
            !run_cmd.contains("JACKIN_AGENT"),
            "JACKIN_AGENT must not be in docker run"
        );
        assert!(!run_cmd.contains("-e OPENAI_API_KEY="));
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
            "jk-agent-smith".to_string(),
        ]);

        let repo_dir = crate::repo::CachedRepo::new(&paths, &selector).repo_dir;
        std::fs::create_dir_all(&repo_dir).unwrap();
        std::fs::write(
            repo_dir.join("Dockerfile"),
            "FROM projectjackin/construct:trixie\n",
        )
        .unwrap();
        std::fs::write(
            repo_dir.join("jackin.role.toml"),
            r#"version = "v1alpha3"
dockerfile = "Dockerfile"

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
            git_pull_on_entry: false,
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
            .find(|call| call.contains("docker run -d") && call.contains("supervisor.sh"))
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
            "jk-agent-smith".to_string(),
        ]);

        let repo_dir = crate::repo::CachedRepo::new(&paths, &selector).repo_dir;
        std::fs::create_dir_all(&repo_dir).unwrap();
        std::fs::write(
            repo_dir.join("Dockerfile"),
            "FROM projectjackin/construct:trixie\n",
        )
        .unwrap();
        std::fs::write(
            repo_dir.join("jackin.role.toml"),
            r#"version = "v1alpha3"
dockerfile = "Dockerfile"

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
            git_pull_on_entry: false,
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
            .find(|call| call.contains("docker build ") && call.contains("-t jk_agent-smith"))
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
    fn load_agent_omits_pull_flag_in_normal_workspace_build() {
        let temp = tempfile::tempdir().unwrap();
        let paths = JackinPaths::for_tests(temp.path());
        let mut config = AppConfig::load_or_init(&paths).unwrap();
        let selector = RoleSelector::new(None, "agent-smith");
        let mut runner = FakeRunner::for_load_agent([String::new()]);

        let repo_dir = crate::repo::CachedRepo::new(&paths, &selector).repo_dir;
        std::fs::create_dir_all(&repo_dir).unwrap();
        std::fs::write(
            repo_dir.join("Dockerfile"),
            "FROM projectjackin/construct:trixie\n",
        )
        .unwrap();
        std::fs::write(
            repo_dir.join("jackin.role.toml"),
            r#"version = "v1alpha3"
dockerfile = "Dockerfile"

[claude]
plugins = []
"#,
        )
        .unwrap();

        load_role(
            &paths,
            &mut config,
            &selector,
            &repo_workspace(&repo_dir),
            &mut runner,
            &LoadOptions::default(),
        )
        .unwrap();

        let build_cmd = runner
            .recorded
            .iter()
            .find(|call| call.contains("docker build "))
            .unwrap();
        assert!(
            !build_cmd.contains("--pull"),
            "workspace mode without --rebuild must not pass --pull"
        );
    }

    #[test]
    fn load_agent_passes_pull_flag_when_rebuild() {
        let temp = tempfile::tempdir().unwrap();
        let paths = JackinPaths::for_tests(temp.path());
        let mut config = AppConfig::load_or_init(&paths).unwrap();
        let selector = RoleSelector::new(None, "agent-smith");
        let mut runner = FakeRunner::for_load_agent([String::new()]);

        let repo_dir = crate::repo::CachedRepo::new(&paths, &selector).repo_dir;
        std::fs::create_dir_all(&repo_dir).unwrap();
        std::fs::write(
            repo_dir.join("Dockerfile"),
            "FROM projectjackin/construct:trixie\n",
        )
        .unwrap();
        std::fs::write(
            repo_dir.join("jackin.role.toml"),
            r#"version = "v1alpha3"
dockerfile = "Dockerfile"

[claude]
plugins = []
"#,
        )
        .unwrap();

        load_role(
            &paths,
            &mut config,
            &selector,
            &repo_workspace(&repo_dir),
            &mut runner,
            &LoadOptions {
                rebuild: true,
                ..LoadOptions::default()
            },
        )
        .unwrap();

        let build_cmd = runner
            .recorded
            .iter()
            .find(|call| call.contains("docker build "))
            .unwrap();
        assert!(
            build_cmd.contains("--pull"),
            "--rebuild must pass --pull to refresh the base image"
        );
    }

    #[test]
    fn load_agent_passes_pull_flag_with_published_image() {
        let temp = tempfile::tempdir().unwrap();
        let paths = JackinPaths::for_tests(temp.path());
        let mut config = AppConfig::load_or_init(&paths).unwrap();
        let selector = RoleSelector::new(None, "agent-smith");
        let mut runner = FakeRunner::for_load_agent([String::new()]);

        let repo_dir = crate::repo::CachedRepo::new(&paths, &selector).repo_dir;
        std::fs::create_dir_all(&repo_dir).unwrap();
        std::fs::write(
            repo_dir.join("Dockerfile"),
            "FROM projectjackin/construct:trixie\n",
        )
        .unwrap();
        std::fs::write(
            repo_dir.join("jackin.role.toml"),
            r#"version = "v1alpha3"
dockerfile = "Dockerfile"
published_image = "docker.io/myorg/my-role:latest"

[claude]
plugins = []
"#,
        )
        .unwrap();

        load_role(
            &paths,
            &mut config,
            &selector,
            &repo_workspace(&repo_dir),
            &mut runner,
            &LoadOptions::default(),
        )
        .unwrap();

        let build_cmd = runner
            .recorded
            .iter()
            .find(|call| call.contains("docker build "))
            .unwrap();
        assert!(
            build_cmd.contains("--pull"),
            "pre-built image mode must pass --pull to check for registry updates"
        );
        // Derived image must carry the construct image label.
        assert!(
            build_cmd.contains("jackin.construct_image=projectjackin/construct:trixie"),
            "build must label the construct image used; got: {build_cmd}"
        );
    }

    #[test]
    fn load_agent_ignores_published_image_when_rebuild() {
        let temp = tempfile::tempdir().unwrap();
        let paths = JackinPaths::for_tests(temp.path());
        let mut config = AppConfig::load_or_init(&paths).unwrap();
        let selector = RoleSelector::new(None, "agent-smith");
        let mut runner = FakeRunner::for_load_agent([String::new()]);

        let repo_dir = crate::repo::CachedRepo::new(&paths, &selector).repo_dir;
        std::fs::create_dir_all(&repo_dir).unwrap();
        std::fs::write(
            repo_dir.join("Dockerfile"),
            "FROM projectjackin/construct:trixie\n",
        )
        .unwrap();
        std::fs::write(
            repo_dir.join("jackin.role.toml"),
            r#"version = "v1alpha3"
dockerfile = "Dockerfile"
published_image = "docker.io/myorg/my-role:latest"

[claude]
plugins = []
"#,
        )
        .unwrap();

        load_role(
            &paths,
            &mut config,
            &selector,
            &repo_workspace(&repo_dir),
            &mut runner,
            &LoadOptions {
                rebuild: true,
                ..LoadOptions::default()
            },
        )
        .unwrap();

        // With --rebuild the workspace Dockerfile is used even when published_image is set.
        // The DerivedDockerfile must contain the workspace FROM, not the published image.
        let recorded = runner.recorded.join("\n");
        assert!(
            !recorded.contains("docker.io/myorg/my-role:latest"),
            "--rebuild must bypass published_image and build from the workspace Dockerfile"
        );
    }

    #[test]
    fn load_agent_rolls_back_runtime_on_attached_run_failure() {
        let temp = tempdir().unwrap();
        let paths = JackinPaths::for_tests(temp.path());
        let mut config = AppConfig::load_or_init(&paths).unwrap();
        let selector = RoleSelector::new(None, "agent-smith");
        let mut runner = FakeRunner {
            fail_on: vec!["supervisor.sh".to_string()],
            capture_queue: VecDeque::from(vec![
                String::new(),
                String::new(),
                String::new(),
                String::new(), // identity
                String::new(), // git pull
            ]),
            ..Default::default()
        };

        let repo_dir = crate::repo::CachedRepo::new(&paths, &selector).repo_dir;
        std::fs::create_dir_all(&repo_dir).unwrap();
        std::fs::write(
            repo_dir.join("Dockerfile"),
            "FROM projectjackin/construct:trixie\n",
        )
        .unwrap();
        std::fs::write(
            repo_dir.join("jackin.role.toml"),
            r#"version = "v1alpha3"
dockerfile = "Dockerfile"

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

        assert!(error.to_string().contains("docker run -d --name jk-"));
        let container_name = launched_role_container_name(&runner);
        let dind = format!("{container_name}-dind");
        let certs_volume = format!("{container_name}-dind-certs");
        let network = format!("{container_name}-net");
        assert!(
            runner
                .recorded
                .iter()
                .any(|call| call == &format!("docker rm -f {container_name}"))
        );
        assert!(
            runner
                .recorded
                .iter()
                .any(|call| call == &format!("docker rm -f {dind}"))
        );
        assert!(
            runner
                .recorded
                .iter()
                .any(|call| call == &format!("docker volume rm {certs_volume}"))
        );
        assert!(
            runner
                .recorded
                .iter()
                .any(|call| call == &format!("docker network rm {network}"))
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
            "jk-agent-smith".to_string(),
        ]);

        let repo_dir = crate::repo::CachedRepo::new(&paths, &selector).repo_dir;
        std::fs::create_dir_all(&repo_dir).unwrap();
        std::fs::write(
            repo_dir.join("Dockerfile"),
            "FROM projectjackin/construct:trixie\n",
        )
        .unwrap();
        std::fs::write(
            repo_dir.join("jackin.role.toml"),
            r#"version = "v1alpha3"
dockerfile = "Dockerfile"

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

        let dind = launched_dind_container_name(&runner);
        // DinD readiness check polls via docker exec
        assert!(
            runner
                .recorded
                .iter()
                .any(|call| call.contains(&format!("docker exec {dind} docker info")))
        );

        // DinD container is started before the readiness check
        let dind_start = runner
            .recorded
            .iter()
            .position(|call| call.contains(&format!("docker run -d --name {dind}")))
            .unwrap();
        let dind_check = runner
            .recorded
            .iter()
            .position(|call| call.contains(&format!("docker exec {dind} docker info")))
            .unwrap();
        assert!(dind_start < dind_check);

        // TLS cert verification runs after docker info check
        assert!(runner.recorded.iter().any(|call| {
            call.contains(&format!("docker exec {dind} test -f /certs/client/ca.pem"))
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
            "jk-agent-smith".to_string(),
        ]);

        let repo_dir = crate::repo::CachedRepo::new(&paths, &selector).repo_dir;
        std::fs::create_dir_all(&repo_dir).unwrap();
        std::fs::write(
            repo_dir.join("Dockerfile"),
            "FROM projectjackin/construct:trixie\n",
        )
        .unwrap();
        std::fs::write(
            repo_dir.join("jackin.role.toml"),
            r#"version = "v1alpha3"
dockerfile = "Dockerfile"

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

        let dind = launched_dind_container_name(&runner);
        let certs_volume = dind.strip_suffix("-dind").unwrap().to_string() + "-dind-certs";
        assert!(crate::instance::naming::is_dns_label(&dind), "{dind}");

        // DinD sidecar: TLS enabled with cert volume
        let dind_cmd = runner
            .recorded
            .iter()
            .find(|call| call.contains(&format!("docker run -d --name {dind}")))
            .unwrap();
        assert!(
            dind_cmd.contains("DOCKER_TLS_CERTDIR=/certs"),
            "DinD must enable TLS cert generation"
        );
        assert!(
            dind_cmd.contains(&format!("{certs_volume}:/certs/client")),
            "DinD must mount cert volume"
        );
        // DinD's auto-generated server cert must include the container name as a
        // Subject Alternative Name, because the role connects via
        // DOCKER_HOST=tcp://{dind}:2376. Without this, the TLS
        // handshake fails because the default SANs only cover the short
        // container ID, `docker`, and `localhost`.
        //
        // The `DNS:` prefix is mandatory: `dockerd-entrypoint.sh` passes
        // `DOCKER_TLS_SAN` through to openssl verbatim (without adding a type
        // prefix), and openssl rejects SAN entries that lack a type tag with
        // `v2i_GENERAL_NAME_ex: missing value`.
        assert!(
            dind_cmd.contains(&format!("DOCKER_TLS_SAN=DNS:{dind}")),
            "DinD SAN value must be prefixed with `DNS:` so openssl accepts it"
        );

        // Role container: TLS client config
        let run_cmd = runner
            .recorded
            .iter()
            .find(|call| call.contains("docker run -d") && call.contains("supervisor.sh"))
            .unwrap();
        assert!(
            run_cmd.contains(&format!("DOCKER_HOST=tcp://{dind}:2376")),
            "role must use TLS port 2376"
        );
        assert!(
            run_cmd.contains(&format!("TESTCONTAINERS_HOST_OVERRIDE={dind}")),
            "Testcontainers must receive the same DNS-safe DinD hostname"
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
            run_cmd.contains(&format!("{certs_volume}:/certs/client:ro")),
            "role must mount cert volume read-only"
        );
    }

    #[test]
    fn load_agent_adds_dind_to_no_proxy_when_proxy_is_configured() {
        let temp = tempdir().unwrap();
        let paths = JackinPaths::for_tests(temp.path());
        let mut config = AppConfig::load_or_init(&paths).unwrap();
        config.env.insert(
            "HTTPS_PROXY".to_string(),
            crate::operator_env::EnvValue::Plain("http://proxy.internal:8305".to_string()),
        );
        config.env.insert(
            "NO_PROXY".to_string(),
            crate::operator_env::EnvValue::Plain("localhost,127.0.0.1".to_string()),
        );
        let selector = RoleSelector::new(None, "agent-smith");
        let mut runner = FakeRunner::for_load_agent([
            String::new(),
            String::new(),
            String::new(),
            String::new(),
            "jk-agent-smith".to_string(),
        ]);

        let repo_dir = crate::repo::CachedRepo::new(&paths, &selector).repo_dir;
        std::fs::create_dir_all(&repo_dir).unwrap();
        std::fs::write(
            repo_dir.join("Dockerfile"),
            "FROM projectjackin/construct:trixie\n",
        )
        .unwrap();
        std::fs::write(
            repo_dir.join("jackin.role.toml"),
            r#"version = "v1alpha3"
dockerfile = "Dockerfile"

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
            .find(|call| call.contains("docker run -d") && call.contains("supervisor.sh"))
            .unwrap();
        let dind = dind_env_from_run_cmd(run_cmd);
        assert!(run_cmd.contains("HTTPS_PROXY=http://proxy.internal:8305"));
        // Both casings carry the merged list — operator's localhost,127.0.0.1
        // must survive into the lowercase synthesized variant for tools that
        // only read `no_proxy`.
        assert!(run_cmd.contains(&format!("NO_PROXY=localhost,127.0.0.1,{dind}")));
        assert!(run_cmd.contains(&format!("no_proxy=localhost,127.0.0.1,{dind}")));
    }

    #[test]
    fn load_agent_synthesizes_both_no_proxy_casings_when_only_proxy_set() {
        let (run_cmd, _temp) = run_load_with_env(&[("HTTPS_PROXY", "http://proxy.internal:8305")]);
        let dind = dind_env_from_run_cmd(&run_cmd);
        assert!(run_cmd.contains(&format!("NO_PROXY={dind}")));
        assert!(run_cmd.contains(&format!("no_proxy={dind}")));
    }

    #[test]
    fn load_agent_mirrors_no_proxy_to_missing_lower_casing() {
        let (run_cmd, _temp) = run_load_with_env(&[
            ("HTTPS_PROXY", "http://proxy.internal:8305"),
            ("NO_PROXY", "internal.corp"),
        ]);
        let dind = dind_env_from_run_cmd(&run_cmd);
        assert!(run_cmd.contains(&format!("NO_PROXY=internal.corp,{dind}")));
        assert!(run_cmd.contains(&format!("no_proxy=internal.corp,{dind}")));
    }

    #[test]
    fn load_agent_mirrors_lower_no_proxy_to_missing_upper_casing() {
        let (run_cmd, _temp) = run_load_with_env(&[
            ("https_proxy", "http://proxy.internal:8305"),
            ("no_proxy", "internal.corp"),
        ]);
        let dind = dind_env_from_run_cmd(&run_cmd);
        assert!(run_cmd.contains(&format!("NO_PROXY=internal.corp,{dind}")));
        assert!(run_cmd.contains(&format!("no_proxy=internal.corp,{dind}")));
    }

    #[test]
    fn load_agent_synthesizes_both_casings_when_only_no_proxy_declared() {
        // Operator may have proxy injected by /etc/environment, transparent
        // proxy, or container-injected vars; jackin only sees NO_PROXY.
        // Both casings must still receive the DinD bypass.
        let (run_cmd, _temp) = run_load_with_env(&[("NO_PROXY", "internal.corp")]);
        let dind = dind_env_from_run_cmd(&run_cmd);
        assert!(run_cmd.contains(&format!("NO_PROXY=internal.corp,{dind}")));
        assert!(run_cmd.contains(&format!("no_proxy=internal.corp,{dind}")));
    }

    #[test]
    fn load_agent_omits_no_proxy_when_no_proxy_env_declared() {
        let (run_cmd, _temp) = run_load_with_env(&[]);
        assert!(!run_cmd.contains("NO_PROXY="));
        assert!(!run_cmd.contains("no_proxy="));
    }

    fn run_load_with_env(entries: &[(&str, &str)]) -> (String, tempfile::TempDir) {
        let temp = tempdir().unwrap();
        let paths = JackinPaths::for_tests(temp.path());
        let mut config = AppConfig::load_or_init(&paths).unwrap();
        for (k, v) in entries {
            config.env.insert(
                (*k).to_string(),
                crate::operator_env::EnvValue::Plain((*v).to_string()),
            );
        }
        let selector = RoleSelector::new(None, "agent-smith");
        let mut runner = FakeRunner::for_load_agent([
            String::new(),
            String::new(),
            String::new(),
            String::new(),
            "jk-agent-smith".to_string(),
        ]);

        let repo_dir = crate::repo::CachedRepo::new(&paths, &selector).repo_dir;
        std::fs::create_dir_all(&repo_dir).unwrap();
        std::fs::write(
            repo_dir.join("Dockerfile"),
            "FROM projectjackin/construct:trixie\n",
        )
        .unwrap();
        std::fs::write(
            repo_dir.join("jackin.role.toml"),
            r#"version = "v1alpha3"
dockerfile = "Dockerfile"

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
            .find(|call| call.contains("docker run -d") && call.contains("supervisor.sh"))
            .unwrap()
            .clone();
        (run_cmd, temp)
    }

    #[test]
    fn append_no_proxy_host_is_idempotent() {
        assert_eq!(
            append_no_proxy_host("localhost,jk-agent-smith-dind", "jk-agent-smith-dind"),
            "localhost,jk-agent-smith-dind"
        );
        assert_eq!(
            append_no_proxy_host("", "jk-agent-smith-dind"),
            "jk-agent-smith-dind"
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
            "jk-agent-smith".to_string(),
        ]);

        let repo_dir = crate::repo::CachedRepo::new(&paths, &selector).repo_dir;
        std::fs::create_dir_all(&repo_dir).unwrap();
        std::fs::write(
            repo_dir.join("Dockerfile"),
            "FROM projectjackin/construct:trixie\n",
        )
        .unwrap();
        std::fs::write(
            repo_dir.join("jackin.role.toml"),
            r#"version = "v1alpha3"
dockerfile = "Dockerfile"

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
            .find(|call| call.contains("docker run -d") && call.contains("supervisor.sh"))
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
            "jk-agent-smith".to_string(),
        ]);

        let repo_dir = crate::repo::CachedRepo::new(&paths, &selector).repo_dir;
        std::fs::create_dir_all(&repo_dir).unwrap();
        std::fs::write(
            repo_dir.join("Dockerfile"),
            "FROM projectjackin/construct:trixie\n",
        )
        .unwrap();
        std::fs::write(
            repo_dir.join("jackin.role.toml"),
            r#"version = "v1alpha3"
dockerfile = "Dockerfile"

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
            .find(|call| call.contains("docker run -d") && call.contains("supervisor.sh"))
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
            "jk-agent-smith".to_string(),
        ]);

        let repo_dir = crate::repo::CachedRepo::new(&paths, &selector).repo_dir;
        std::fs::create_dir_all(&repo_dir).unwrap();
        std::fs::write(
            repo_dir.join("Dockerfile"),
            "FROM projectjackin/construct:trixie\n",
        )
        .unwrap();
        std::fs::write(
            repo_dir.join("jackin.role.toml"),
            r#"version = "v1alpha3"
dockerfile = "Dockerfile"

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
            .find(|call| call.contains("docker run -d") && call.contains("supervisor.sh"))
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
            "jk-agent-smith".to_string(),
        ]);

        let repo_dir = crate::repo::CachedRepo::new(&paths, &selector).repo_dir;
        std::fs::create_dir_all(&repo_dir).unwrap();
        std::fs::write(
            repo_dir.join("Dockerfile"),
            "FROM projectjackin/construct:trixie\n",
        )
        .unwrap();
        std::fs::write(
            repo_dir.join("jackin.role.toml"),
            r#"version = "v1alpha3"
dockerfile = "Dockerfile"

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
            .find(|call| call.contains("docker run -d") && call.contains("supervisor.sh"))
            .unwrap();
        let dind = dind_env_from_run_cmd(run_cmd);
        assert!(run_cmd.contains("-e JACKIN=1"));
        assert!(run_cmd.contains(&format!("-e JACKIN_DIND_HOSTNAME={dind}")));
        assert!(run_cmd.contains(&format!("-e TESTCONTAINERS_HOST_OVERRIDE={dind}")));
        assert!(!run_cmd.contains("JACKIN_DEBUG"));
    }

    #[test]
    fn load_agent_writes_instance_manifest() {
        let temp = tempdir().unwrap();
        let paths = JackinPaths::for_tests(temp.path());
        let mut config = AppConfig::load_or_init(&paths).unwrap();
        let selector = RoleSelector::new(None, "agent-smith");
        let mut runner = FakeRunner::for_load_agent([
            String::new(),
            String::new(),
            String::new(),
            "true 0 false".to_string(),
            "false 0 false".to_string(),
            "false 0 false".to_string(),
        ]);

        let repo_dir = crate::repo::CachedRepo::new(&paths, &selector).repo_dir;
        std::fs::create_dir_all(&repo_dir).unwrap();
        std::fs::write(
            repo_dir.join("Dockerfile"),
            "FROM projectjackin/construct:trixie\n",
        )
        .unwrap();
        std::fs::write(
            repo_dir.join("jackin.role.toml"),
            r#"version = "v1alpha3"
dockerfile = "Dockerfile"

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

        let container_name = launched_role_container_name(&runner);
        let manifest_path = paths
            .data_dir
            .join(&container_name)
            .join(".jackin/instance.json");
        let body = std::fs::read_to_string(manifest_path).unwrap();
        assert!(body.contains(r#""version": 1"#));
        assert!(body.contains(&format!(r#""container_base": "{container_name}""#)));
        assert!(body.contains(r#""role_key": "agent-smith""#));
        assert!(body.contains(r#""agent_runtime": "claude""#));
        assert!(body.contains(r#""host_workdir_fingerprint": "sha256:"#));
        assert!(body.contains(r#""status": "restore_available""#));
        let index_body = std::fs::read_to_string(paths.data_dir.join("instances.json")).unwrap();
        assert!(index_body.contains(&format!(r#""container_base": "{container_name}""#)));
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
            "jk-agent-smith".to_string(),
        ]);

        let repo_dir = crate::repo::CachedRepo::new(&paths, &selector).repo_dir;
        std::fs::create_dir_all(&repo_dir).unwrap();
        std::fs::write(
            repo_dir.join("Dockerfile"),
            "FROM projectjackin/construct:trixie\n",
        )
        .unwrap();
        std::fs::write(
            repo_dir.join("jackin.role.toml"),
            r#"version = "v1alpha3"
dockerfile = "Dockerfile"

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
            .find(|call| call.contains("docker run -d") && call.contains("supervisor.sh"))
            .unwrap();
        assert!(run_cmd.contains("-e JACKIN_DEBUG=1"));
    }

    #[test]
    fn load_agent_injects_coauthor_trailer_env_when_enabled() {
        let temp = tempdir().unwrap();
        let paths = JackinPaths::for_tests(temp.path());
        let mut config = AppConfig::load_or_init(&paths).unwrap();
        config.git.auto_coauthor_trailer = true;
        let selector = RoleSelector::new(None, "agent-smith");
        let mut runner = FakeRunner::for_load_agent([
            String::new(),
            String::new(),
            String::new(),
            String::new(),
            "jk-agent-smith".to_string(),
        ]);

        let repo_dir = crate::repo::CachedRepo::new(&paths, &selector).repo_dir;
        std::fs::create_dir_all(&repo_dir).unwrap();
        std::fs::write(
            repo_dir.join("Dockerfile"),
            "FROM projectjackin/construct:trixie\n",
        )
        .unwrap();
        std::fs::write(
            repo_dir.join("jackin.role.toml"),
            r#"version = "v1alpha3"
dockerfile = "Dockerfile"

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
            run_cmd.contains("-e JACKIN_GIT_COAUTHOR_TRAILER=1"),
            "{run_cmd}"
        );
    }

    #[test]
    fn load_agent_omits_coauthor_trailer_env_when_disabled() {
        let temp = tempdir().unwrap();
        let paths = JackinPaths::for_tests(temp.path());
        let mut config = AppConfig::load_or_init(&paths).unwrap();
        let selector = RoleSelector::new(None, "agent-smith");
        let mut runner = FakeRunner::for_load_agent([
            String::new(),
            String::new(),
            String::new(),
            String::new(),
            "jk-agent-smith".to_string(),
        ]);

        let repo_dir = crate::repo::CachedRepo::new(&paths, &selector).repo_dir;
        std::fs::create_dir_all(&repo_dir).unwrap();
        std::fs::write(
            repo_dir.join("Dockerfile"),
            "FROM projectjackin/construct:trixie\n",
        )
        .unwrap();
        std::fs::write(
            repo_dir.join("jackin.role.toml"),
            r#"version = "v1alpha3"
dockerfile = "Dockerfile"

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
            !run_cmd.contains("JACKIN_GIT_COAUTHOR_TRAILER"),
            "{run_cmd}"
        );
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
            "jk-agent-smith".to_string(),
        ]);

        let repo_dir = crate::repo::CachedRepo::new(&paths, &selector).repo_dir;
        std::fs::create_dir_all(&repo_dir).unwrap();
        std::fs::write(
            repo_dir.join("Dockerfile"),
            "FROM projectjackin/construct:trixie\n",
        )
        .unwrap();
        std::fs::write(
            repo_dir.join("jackin.role.toml"),
            r#"version = "v1alpha3"
dockerfile = "Dockerfile"

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
            .find(|call| call.contains("docker run -d") && call.contains("supervisor.sh"))
            .unwrap();
        assert!(
            run_cmd.contains("-e OPERATOR_SMOKE=smoke-literal"),
            "docker run must inject operator env; got: {run_cmd}"
        );
    }

    #[test]
    fn load_agent_injects_mise_trusted_paths_for_any_workspace() {
        let temp = tempdir().unwrap();
        let paths = JackinPaths::for_tests(temp.path());
        paths.ensure_base_dirs().unwrap();

        std::fs::write(
            &paths.config_file,
            r#"[roles.agent-smith]
git = "https://github.com/jackin-project/jackin-agent-smith.git"
trusted = true

[workspaces.sample-workspace]
workdir = "/workspace"

[[workspaces.sample-workspace.mounts]]
src = "/tmp"
dst = "/workspace"
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
            "jk-agent-smith".to_string(),
        ]);

        let repo_dir = crate::repo::CachedRepo::new(&paths, &selector).repo_dir;
        std::fs::create_dir_all(&repo_dir).unwrap();
        std::fs::write(
            repo_dir.join("Dockerfile"),
            "FROM projectjackin/construct:trixie\n",
        )
        .unwrap();
        std::fs::write(
            repo_dir.join("jackin.role.toml"),
            r#"version = "v1alpha3"
dockerfile = "Dockerfile"

[claude]
plugins = []
"#,
        )
        .unwrap();

        let workspace = crate::workspace::ResolvedWorkspace {
            label: "sample-workspace".to_string(),
            workdir: "/workspace".to_string(),
            mounts: vec![
                crate::workspace::MountConfig {
                    src: repo_dir.display().to_string(),
                    dst: "/workspace/jackin".to_string(),
                    readonly: false,
                    isolation: crate::isolation::MountIsolation::Shared,
                },
                crate::workspace::MountConfig {
                    src: repo_dir.display().to_string(),
                    dst: "/workspace/homebrew-tap".to_string(),
                    readonly: false,
                    isolation: crate::isolation::MountIsolation::Shared,
                },
            ],
            default_agent: None,
            keep_awake_enabled: false,
            git_pull_on_entry: false,
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
            .find(|call| call.contains("docker run -d") && call.contains("supervisor.sh"))
            .unwrap();
        assert!(
            run_cmd.contains(
                "-e MISE_TRUSTED_CONFIG_PATHS=/workspace:/workspace/homebrew-tap:/workspace/jackin"
            ),
            "workspace must inject mise trusted paths; got: {run_cmd}"
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
            "jk-agent-smith".to_string(),
        ]);

        let repo_dir = crate::repo::CachedRepo::new(&paths, &selector).repo_dir;
        std::fs::create_dir_all(&repo_dir).unwrap();
        std::fs::write(
            repo_dir.join("Dockerfile"),
            "FROM projectjackin/construct:trixie\n",
        )
        .unwrap();
        std::fs::write(
            repo_dir.join("jackin.role.toml"),
            r#"version = "v1alpha3"
dockerfile = "Dockerfile"

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
            .find(|call| call.contains("docker run -d") && call.contains("supervisor.sh"))
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
            "jk-agent-smith".to_string(),
        ]);

        let repo_dir = crate::repo::CachedRepo::new(&paths, &selector).repo_dir;
        std::fs::create_dir_all(&repo_dir).unwrap();
        std::fs::write(
            repo_dir.join("Dockerfile"),
            "FROM projectjackin/construct:trixie\n",
        )
        .unwrap();
        std::fs::write(
            repo_dir.join("jackin.role.toml"),
            r#"version = "v1alpha3"
dockerfile = "Dockerfile"

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
            .find(|call| call.contains("docker run -d") && call.contains("supervisor.sh"))
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
            "jk-agent-smith".to_string(),
        ]);

        let repo_dir = crate::repo::CachedRepo::new(&paths, &selector).repo_dir;
        std::fs::create_dir_all(&repo_dir).unwrap();
        std::fs::write(
            repo_dir.join("Dockerfile"),
            "FROM projectjackin/construct:trixie\n",
        )
        .unwrap();
        std::fs::write(
            repo_dir.join("jackin.role.toml"),
            r#"version = "v1alpha3"
dockerfile = "Dockerfile"

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
            .find(|call| call.contains("docker run -d") && call.contains("supervisor.sh"))
            .unwrap();
        assert!(
            run_cmd.contains("-e OPERATOR_TOKEN=resolved-op-token"),
            "op:// ref must resolve via the injected OpCli and inject; got: {run_cmd}"
        );
    }

    // ── claim_container_name tests ────────────────────────────────────────────

    /// `NotFound` → claim a unique ad-hoc name directly (no docker rm issued).
    #[test]
    fn claim_container_name_not_found_claims_unique_ad_hoc_name() {
        let temp = tempdir().unwrap();
        let paths = JackinPaths::for_tests(temp.path());
        let selector = RoleSelector::new(None, "agent-smith");
        // inspect returns "" → NotFound
        let mut runner = FakeRunner::with_capture_queue([String::new()]);

        let (name, _lock) = claim_container_name(&paths, None, &selector, &mut runner).unwrap();

        assert!(name.starts_with("jk-"), "{name}");
        assert!(name.contains("agentsmith"), "{name}");
        assert!(!name.contains("clone"), "{name}");
        assert!(crate::instance::naming::is_dns_label(&name), "{name}");
        assert!(
            crate::instance::naming::is_dns_label(&format!("{name}-dind")),
            "{name}"
        );
        assert!(runner.recorded.iter().any(|call| {
            call.contains("docker inspect --format {{.State.Running}} {{.State.ExitCode}} {{.State.OOMKilled}} jk-")
                && call.contains("agentsmith")
        }));
        assert!(
            !runner
                .recorded
                .iter()
                .any(|call| call.contains("docker rm"))
        );
    }

    #[test]
    fn claim_container_name_docker_unavailable_errors() {
        let temp = tempdir().unwrap();
        let paths = JackinPaths::for_tests(temp.path());
        let selector = RoleSelector::new(None, "agent-smith");
        let mut runner = FakeRunner::default();
        runner.fail_with.push((
            "docker inspect".to_string(),
            "Cannot connect to the Docker daemon at unix:///var/run/docker.sock".to_string(),
        ));

        let err = claim_container_name(&paths, None, &selector, &mut runner).unwrap_err();

        assert!(err.to_string().contains("cannot claim container name"));
        assert!(err.to_string().contains("Docker is unavailable"));
    }

    /// Running collision → skip that random name and claim another one.
    #[test]
    fn claim_container_name_running_collision_tries_another_unique_name() {
        let temp = tempdir().unwrap();
        let paths = JackinPaths::for_tests(temp.path());
        let selector = RoleSelector::new(None, "agent-smith");
        let mut runner =
            FakeRunner::with_capture_queue(["true 0 false".to_string(), String::new()]);

        let (name, _lock) = claim_container_name(&paths, None, &selector, &mut runner).unwrap();

        assert!(name.starts_with("jk-"), "{name}");
        assert!(name.ends_with("-agentsmith"), "{name}");
        assert!(!name.contains("clone"), "{name}");
        assert_eq!(
            runner
                .recorded
                .iter()
                .filter(|call| call.contains("docker inspect --format"))
                .count(),
            2
        );
        assert!(
            !runner
                .recorded
                .iter()
                .any(|call| call.contains("docker rm"))
        );
    }

    /// Stopped / exit 0 collision → docker rm issued, same random slot reclaimed.
    #[test]
    fn claim_container_name_clean_exit_removes_and_reclaims() {
        let temp = tempdir().unwrap();
        let paths = JackinPaths::for_tests(temp.path());
        let selector = RoleSelector::new(None, "agent-smith");
        let mut runner = FakeRunner::with_capture_queue(["false 0 false".to_string()]);

        let (name, _lock) = claim_container_name(&paths, None, &selector, &mut runner).unwrap();

        assert!(name.starts_with("jk-"), "{name}");
        assert!(name.ends_with("-agentsmith"), "{name}");
        assert!(
            runner
                .recorded
                .iter()
                .any(|call| call.starts_with("docker rm jk-") && call.contains("agentsmith"))
        );
    }

    /// Stopped / non-zero collision → skip it and claim another random name.
    #[test]
    fn claim_container_name_crashed_collision_tries_another_unique_name() {
        let temp = tempdir().unwrap();
        let paths = JackinPaths::for_tests(temp.path());
        let selector = RoleSelector::new(None, "agent-smith");
        let mut runner =
            FakeRunner::with_capture_queue(["false 1 false".to_string(), String::new()]);

        let (name, _lock) = claim_container_name(&paths, None, &selector, &mut runner).unwrap();

        assert!(name.starts_with("jk-"), "{name}");
        assert!(name.ends_with("-agentsmith"), "{name}");
        assert!(!name.contains("clone"), "{name}");
        assert!(
            !runner
                .recorded
                .iter()
                .any(|call| call.contains("docker rm"))
        );
    }

    #[test]
    fn claim_container_name_saved_workspace_includes_workspace_component() {
        let temp = tempdir().unwrap();
        let paths = JackinPaths::for_tests(temp.path());
        let selector = RoleSelector::new(None, "agent-smith");
        let mut runner = FakeRunner::with_capture_queue([String::new()]);

        let (name, _lock) =
            claim_container_name(&paths, Some("my-workspace"), &selector, &mut runner).unwrap();

        assert!(name.starts_with("jk-"), "{name}");
        assert!(
            name.contains("myworkspace") && name.ends_with("-agentsmith"),
            "{name}"
        );
        assert!(name.len() <= 58, "{name}");
    }

    #[test]
    fn restore_candidate_blocks_noninteractive_fresh_load() {
        let temp = tempdir().unwrap();
        let paths = JackinPaths::for_tests(temp.path());
        let container_name = "jk-k7p9m2xq-workspace-agentsmith";
        let manifest = workspace_manifest(
            container_name,
            "agent-smith",
            "Agent Smith",
            crate::agent::Agent::Claude,
        );
        manifest
            .write(&paths.data_dir.join(container_name))
            .unwrap();
        let mut runner = FakeRunner::with_capture_queue([String::new()]);

        let error = resolve_workspace_restore(&paths, "agent-smith", &mut runner).unwrap_err();

        assert!(error.to_string().contains("restore is available"));
        assert!(error.to_string().contains(container_name));
    }

    #[test]
    fn running_matching_instance_does_not_block_fresh_load() {
        let temp = tempdir().unwrap();
        let paths = JackinPaths::for_tests(temp.path());
        let container_name = "jk-k7p9m2xq-workspace-agentsmith";
        let manifest = workspace_manifest(
            container_name,
            "agent-smith",
            "Agent Smith",
            crate::agent::Agent::Claude,
        );
        write_indexed_manifest(&paths, &manifest);
        let mut runner = FakeRunner::with_capture_queue(["true 0 false".to_string()]);

        let candidate = resolve_workspace_restore(&paths, "agent-smith", &mut runner).unwrap();

        assert_eq!(candidate, RestoreResolution::StartFresh);
    }

    #[test]
    fn stopped_matching_instance_does_not_block_fresh_load() {
        let temp = tempdir().unwrap();
        let paths = JackinPaths::for_tests(temp.path());
        let container_name = "jk-k7p9m2xq-workspace-agentsmith";
        let manifest = workspace_manifest(
            container_name,
            "agent-smith",
            "Agent Smith",
            crate::agent::Agent::Claude,
        );
        write_indexed_manifest(&paths, &manifest);
        let mut runner = FakeRunner::with_capture_queue(["false 137 false".to_string()]);

        let candidate = resolve_workspace_restore(&paths, "agent-smith", &mut runner).unwrap();

        assert_eq!(candidate, RestoreResolution::StartFresh);
    }

    #[test]
    fn related_restore_candidate_blocks_noninteractive_fresh_load() {
        let temp = tempdir().unwrap();
        let paths = JackinPaths::for_tests(temp.path());
        let container_name = "jk-k7p9m2xq-workspace-thearchitect";
        let manifest = workspace_manifest(
            container_name,
            "the-architect",
            "The Architect",
            crate::agent::Agent::Claude,
        );
        write_indexed_manifest(&paths, &manifest);
        let mut runner = FakeRunner::with_capture_queue([String::new()]);

        let error = resolve_workspace_restore(&paths, "agent-smith", &mut runner).unwrap_err();

        let message = error.to_string();
        assert!(
            message.contains("different role or agent"),
            "unexpected error: {message}"
        );
        assert!(message.contains("jackin hardline <instance>"), "{message}");
    }

    #[test]
    fn running_related_instance_does_not_block_fresh_load() {
        let temp = tempdir().unwrap();
        let paths = JackinPaths::for_tests(temp.path());
        let container_name = "jk-k7p9m2xq-workspace-thearchitect";
        let manifest = workspace_manifest(
            container_name,
            "the-architect",
            "The Architect",
            crate::agent::Agent::Claude,
        );
        write_indexed_manifest(&paths, &manifest);
        let mut runner = FakeRunner::with_capture_queue(["true 0 false".to_string()]);

        let candidate = resolve_workspace_restore(&paths, "agent-smith", &mut runner).unwrap();

        assert_eq!(candidate, RestoreResolution::StartFresh);
    }

    #[test]
    fn stopped_related_instance_does_not_block_fresh_load() {
        let temp = tempdir().unwrap();
        let paths = JackinPaths::for_tests(temp.path());
        let container_name = "jk-k7p9m2xq-workspace-thearchitect";
        let manifest = workspace_manifest(
            container_name,
            "the-architect",
            "The Architect",
            crate::agent::Agent::Claude,
        );
        write_indexed_manifest(&paths, &manifest);
        let mut runner = FakeRunner::with_capture_queue(["false 137 false".to_string()]);

        let candidate = resolve_workspace_restore(&paths, "agent-smith", &mut runner).unwrap();

        assert_eq!(candidate, RestoreResolution::StartFresh);
    }

    #[test]
    fn related_restore_candidates_ignore_finished_instances() {
        let temp = tempdir().unwrap();
        let paths = JackinPaths::for_tests(temp.path());
        let container_name = "jk-k7p9m2xq-workspace-thearchitect";
        let mut manifest = workspace_manifest(
            container_name,
            "the-architect",
            "The Architect",
            crate::agent::Agent::Claude,
        );
        manifest.mark_status(InstanceStatus::CleanExited);
        write_indexed_manifest(&paths, &manifest);
        let mut runner = FakeRunner::default();

        let candidate = resolve_workspace_restore(&paths, "agent-smith", &mut runner).unwrap();

        assert_eq!(candidate, RestoreResolution::StartFresh);
        assert!(runner.recorded.is_empty());
    }

    #[test]
    fn related_restore_candidate_with_container_recovers_in_place() {
        let container_name = "jk-k7p9m2xq-workspace-thearchitect";
        let candidate = RelatedRestoreCandidate {
            manifest: workspace_manifest(
                container_name,
                "the-architect",
                "The Architect",
                crate::agent::Agent::Claude,
            ),
            docker_state: ContainerState::Running,
        };

        let resolution = recover_related_restore_candidate(&candidate).unwrap();

        assert_eq!(
            resolution,
            RestoreResolution::RecoverRelatedRole(container_name.to_string())
        );
        assert!(related_restore_candidate_action_label(&candidate).starts_with("Recover now"));
    }

    #[test]
    fn missing_related_restore_candidate_rebuilds_in_place() {
        let container_name = "jk-k7p9m2xq-workspace-thearchitect";
        let candidate = RelatedRestoreCandidate {
            manifest: workspace_manifest(
                container_name,
                "the-architect",
                "The Architect",
                crate::agent::Agent::Claude,
            ),
            docker_state: ContainerState::NotFound,
        };

        let resolution = recover_related_restore_candidate(&candidate).unwrap();

        assert!(matches!(
            resolution,
            RestoreResolution::RebuildRelatedRole(ref manifest)
                if manifest.container_base == container_name
        ));
        assert!(related_restore_candidate_action_label(&candidate).starts_with("Rebuild now"));
    }

    #[test]
    fn related_restore_load_options_use_manifest_source_ref_and_agent() {
        let container_name = "jk-k7p9m2xq-workspace-thearchitect";
        let mut manifest = workspace_manifest(
            container_name,
            "the-architect",
            "The Architect",
            crate::agent::Agent::Codex,
        );
        manifest.agent_runtime = "codex".to_string();
        manifest.role_source_ref = Some("restore-ref".to_string());
        let current = LoadOptions::for_load(false, true, false);

        let opts = related_restore_load_options(&current, &manifest).unwrap();

        assert!(opts.no_intro);
        assert!(opts.debug);
        assert_eq!(opts.agent, Some(crate::agent::Agent::Codex));
        assert_eq!(opts.role_branch.as_deref(), Some("restore-ref"));
        assert_eq!(opts.restore_container_base.as_deref(), Some(container_name));
        assert_eq!(
            opts.restore_role_source_git.as_deref(),
            Some("https://example.invalid/the-architect.git")
        );
        assert!(
            related_restore_candidate_action_label(&RelatedRestoreCandidate {
                manifest,
                docker_state: ContainerState::NotFound,
            })
            .starts_with("Rebuild now")
        );
    }

    #[test]
    fn supersede_restore_candidates_updates_manifest_and_index() {
        let temp = tempdir().unwrap();
        let paths = JackinPaths::for_tests(temp.path());
        let container_name = "jk-k7p9m2xq-workspace-agentsmith";
        let manifest = workspace_manifest(
            container_name,
            "agent-smith",
            "Agent Smith",
            crate::agent::Agent::Claude,
        );
        write_indexed_manifest(&paths, &manifest);

        supersede_restore_candidates(&paths, vec![manifest]).unwrap();

        let manifest = InstanceManifest::read(&paths.data_dir.join(container_name)).unwrap();
        assert_eq!(manifest.status, InstanceStatus::Superseded);
        let index = InstanceIndex::read_or_rebuild(&paths.data_dir).unwrap();
        assert_eq!(index.instances[0].status, InstanceStatus::Superseded);
    }

    #[test]
    fn restore_candidate_label_includes_manifest_and_mount_state() {
        let temp = tempdir().unwrap();
        let paths = JackinPaths::for_tests(temp.path());
        let container_name = "jk-k7p9m2xq-workspace-agentsmith";
        let mut manifest = workspace_manifest(
            container_name,
            "agent-smith",
            "Agent Smith",
            crate::agent::Agent::Codex,
        );
        manifest.mark_status(InstanceStatus::PreservedDirty);
        manifest.last_attach_outcome = Some("exit:137".into());
        crate::isolation::state::write_records(
            &paths.data_dir.join(container_name),
            &[crate::isolation::state::IsolationRecord {
                workspace: "workspace".into(),
                mount_dst: "/workspace".into(),
                original_src: "/host/workspace".into(),
                isolation: crate::isolation::MountIsolation::Worktree,
                worktree_path: "/tmp/worktree".into(),
                scratch_branch: "jackin/test".into(),
                base_commit: "abc123".into(),
                selector_key: "agent-smith".into(),
                container_name: container_name.into(),
                cleanup_status: crate::isolation::state::CleanupStatus::PreservedDirty,
            }],
        )
        .unwrap();

        let label = restore_candidate_label(&paths, &manifest);

        assert!(label.contains("k7p9m2xq"), "{label}");
        assert!(label.contains("status:preserved_dirty"), "{label}");
        assert!(label.contains("agent:codex"), "{label}");
        assert!(label.contains("role:agent-smith"), "{label}");
        assert!(label.contains("mounts:1 dirty:1 unpushed:0"), "{label}");
        assert!(label.contains("attach:exit:137"), "{label}");
        assert!(!label.contains(container_name), "{label}");
    }

    #[test]
    fn record_instance_attach_outcome_updates_manifest() {
        let temp = tempdir().unwrap();
        let paths = JackinPaths::for_tests(temp.path());
        let container_name = "jk-k7p9m2xq-workspace-agentsmith";
        let manifest = workspace_manifest(
            container_name,
            "agent-smith",
            "Agent Smith",
            crate::agent::Agent::Claude,
        );
        manifest
            .write(&paths.data_dir.join(container_name))
            .unwrap();

        record_instance_attach_outcome(
            &paths,
            container_name,
            crate::isolation::finalize::AttachOutcome::stopped(137),
        )
        .unwrap();

        let manifest = InstanceManifest::read(&paths.data_dir.join(container_name)).unwrap();
        assert_eq!(manifest.last_attach_outcome.as_deref(), Some("exit:137"));
    }

    #[test]
    fn format_attach_outcome_names_running_exit_and_oom() {
        use crate::isolation::finalize::AttachOutcome;

        assert_eq!(
            format_attach_outcome(AttachOutcome::still_running()),
            "running"
        );
        assert_eq!(format_attach_outcome(AttachOutcome::stopped(0)), "exit:0");
        assert_eq!(
            format_attach_outcome(AttachOutcome::oom_killed()),
            "oom_killed"
        );
    }

    #[test]
    fn verify_credential_sync_returns_ok_regardless() {
        use crate::agent::Agent;
        use crate::config::AuthForwardMode;
        let merged: std::collections::BTreeMap<String, String> = std::collections::BTreeMap::new();
        let layers: Vec<(String, EnvLayerState)> = vec![];
        let r = verify_credential_env_present(
            Agent::Claude,
            AuthForwardMode::Sync,
            &merged,
            &[],
            &layers,
            "proj",
            "smith",
        );
        assert!(r.is_ok());
    }

    #[test]
    fn verify_credential_ignore_returns_ok_regardless() {
        use crate::agent::Agent;
        use crate::config::AuthForwardMode;
        let merged: std::collections::BTreeMap<String, String> = std::collections::BTreeMap::new();
        let layers: Vec<(String, EnvLayerState)> = vec![];
        let r = verify_credential_env_present(
            Agent::Claude,
            AuthForwardMode::Ignore,
            &merged,
            &[],
            &layers,
            "proj",
            "smith",
        );
        assert!(r.is_ok());
    }

    #[test]
    fn verify_credential_api_key_present_ok() {
        use crate::agent::Agent;
        use crate::config::AuthForwardMode;
        let mut merged = std::collections::BTreeMap::new();
        merged.insert("ANTHROPIC_API_KEY".into(), "sk-ant-xxx".into());
        let layers: Vec<(String, EnvLayerState)> = vec![];
        let r = verify_credential_env_present(
            Agent::Claude,
            AuthForwardMode::ApiKey,
            &merged,
            &[],
            &layers,
            "proj",
            "smith",
        );
        assert!(r.is_ok());
    }

    #[test]
    fn verify_credential_api_key_missing_returns_structured_error() {
        use crate::agent::Agent;
        use crate::config::AuthForwardMode;
        let mut merged = std::collections::BTreeMap::new();
        merged.insert("ANTHROPIC_API_KEY".into(), String::new());
        let layers = vec![
            ("[env]".into(), EnvLayerState::Unset),
            ("[roles.smith.env]".into(), EnvLayerState::Unset),
            ("[workspaces.proj.env]".into(), EnvLayerState::Unset),
            (
                "[workspaces.proj.roles.smith.env]".into(),
                EnvLayerState::Unset,
            ),
        ];
        let mode_resolution = vec![
            (
                "workspace × role × claude".into(),
                Some(AuthForwardMode::ApiKey),
            ),
            ("workspace × claude".into(), None),
            ("global × claude".into(), None),
        ];
        let r = verify_credential_env_present(
            Agent::Claude,
            AuthForwardMode::ApiKey,
            &merged,
            &mode_resolution,
            &layers,
            "proj",
            "smith",
        );
        let err = r.unwrap_err();
        match err {
            LaunchError::AuthCredentialMissing {
                env_var,
                agent,
                mode,
                workspace,
                role,
                env_layers,
                mode_resolution,
                ..
            } => {
                assert_eq!(env_var, "ANTHROPIC_API_KEY");
                assert_eq!(agent, Agent::Claude);
                assert_eq!(mode, AuthForwardMode::ApiKey);
                assert_eq!(workspace, "proj");
                assert_eq!(role, "smith");
                // Helper passes the caller's traces through verbatim.
                assert_eq!(env_layers.len(), 4);
                assert_eq!(mode_resolution.len(), 3);
                assert_eq!(mode_resolution[0].1, Some(AuthForwardMode::ApiKey));
            }
        }
    }

    #[test]
    fn verify_credential_api_key_unset_returns_structured_error() {
        use crate::agent::Agent;
        use crate::config::AuthForwardMode;
        // ANTHROPIC_API_KEY not in map at all.
        let merged: std::collections::BTreeMap<String, String> = std::collections::BTreeMap::new();
        let layers: Vec<(String, EnvLayerState)> = vec![];
        let r = verify_credential_env_present(
            Agent::Claude,
            AuthForwardMode::ApiKey,
            &merged,
            &[],
            &layers,
            "proj",
            "smith",
        );
        assert!(matches!(r, Err(LaunchError::AuthCredentialMissing { .. })));
    }

    #[test]
    fn verify_credential_oauth_token_missing_for_claude() {
        use crate::agent::Agent;
        use crate::config::AuthForwardMode;
        let merged: std::collections::BTreeMap<String, String> = std::collections::BTreeMap::new();
        let layers = vec![("[env]".into(), EnvLayerState::Unset)];
        let r = verify_credential_env_present(
            Agent::Claude,
            AuthForwardMode::OAuthToken,
            &merged,
            &[],
            &layers,
            "proj",
            "smith",
        );
        let err = r.unwrap_err();
        match err {
            LaunchError::AuthCredentialMissing { env_var, .. } => {
                assert_eq!(env_var, "CLAUDE_CODE_OAUTH_TOKEN");
            }
        }
    }

    #[test]
    fn verify_credential_codex_api_key_missing() {
        use crate::agent::Agent;
        use crate::config::AuthForwardMode;
        let merged: std::collections::BTreeMap<String, String> = std::collections::BTreeMap::new();
        let layers: Vec<(String, EnvLayerState)> = vec![];
        let r = verify_credential_env_present(
            Agent::Codex,
            AuthForwardMode::ApiKey,
            &merged,
            &[],
            &layers,
            "proj",
            "smith",
        );
        let err = r.unwrap_err();
        match err {
            LaunchError::AuthCredentialMissing { env_var, agent, .. } => {
                assert_eq!(env_var, "OPENAI_API_KEY");
                assert_eq!(agent, Agent::Codex);
            }
        }
    }

    #[test]
    fn verify_credential_amp_api_key_missing() {
        use crate::agent::Agent;
        use crate::config::AuthForwardMode;
        let merged: std::collections::BTreeMap<String, String> = std::collections::BTreeMap::new();
        let layers: Vec<(String, EnvLayerState)> = vec![];
        let r = verify_credential_env_present(
            Agent::Amp,
            AuthForwardMode::ApiKey,
            &merged,
            &[],
            &layers,
            "proj",
            "smith",
        );
        let err = r.unwrap_err();
        match err {
            LaunchError::AuthCredentialMissing { env_var, agent, .. } => {
                assert_eq!(env_var, "AMP_API_KEY");
                assert_eq!(agent, Agent::Amp);
            }
        }
    }

    #[test]
    fn build_mode_resolution_populates_all_3_layers() {
        use crate::agent::Agent;
        use crate::config::{AgentAuthConfig, AuthForwardMode};
        use crate::workspace::WorkspaceConfig;

        let ws = WorkspaceConfig {
            claude: Some(AgentAuthConfig {
                auth_forward: AuthForwardMode::ApiKey,
            }),
            ..WorkspaceConfig::default()
        };
        let mut cfg = AppConfig {
            claude: Some(AgentAuthConfig {
                auth_forward: AuthForwardMode::Sync,
            }),
            ..AppConfig::default()
        };
        cfg.workspaces.insert("proj".into(), ws);

        let trace = build_mode_resolution(&cfg, Agent::Claude, "proj", "smith");
        assert_eq!(trace.len(), 3);
        // Ordered most-specific first: ws × role × claude (no override),
        // then ws × claude (api_key), then global × claude (sync).
        assert_eq!(trace[0].0, "workspace × role × claude");
        assert_eq!(trace[0].1, None);
        assert_eq!(trace[1].0, "workspace × claude");
        assert_eq!(trace[1].1, Some(AuthForwardMode::ApiKey));
        assert_eq!(trace[2].0, "global × claude");
        assert_eq!(trace[2].1, Some(AuthForwardMode::Sync));
    }

    #[test]
    fn build_mode_resolution_role_override_wins() {
        use crate::agent::Agent;
        use crate::config::{AgentAuthConfig, AuthForwardMode};
        use crate::workspace::{WorkspaceConfig, WorkspaceRoleOverride};

        let ro = WorkspaceRoleOverride {
            claude: Some(AgentAuthConfig {
                auth_forward: AuthForwardMode::OAuthToken,
            }),
            ..Default::default()
        };
        let mut ws = WorkspaceConfig::default();
        ws.roles.insert("smith".into(), ro);
        let mut cfg = AppConfig::default();
        cfg.workspaces.insert("proj".into(), ws);

        let trace = build_mode_resolution(&cfg, Agent::Claude, "proj", "smith");
        assert_eq!(trace[0].1, Some(AuthForwardMode::OAuthToken));
        assert_eq!(trace[1].1, None);
        assert_eq!(trace[2].1, None);
    }

    #[test]
    fn build_env_layer_states_classifies_present_vs_absent() {
        use crate::operator_env::{EnvValue, OpRef};
        use crate::workspace::{WorkspaceConfig, WorkspaceRoleOverride};

        let mut ro = WorkspaceRoleOverride::default();
        ro.env.insert(
            "ANTHROPIC_API_KEY".into(),
            EnvValue::OpRef(OpRef {
                op: "op://uuid/test/field".into(),
                path: "Test/api/key".into(),
            }),
        );
        let mut ws = WorkspaceConfig::default();
        ws.roles.insert("smith".into(), ro);
        let mut cfg = AppConfig::default();
        cfg.workspaces.insert("proj".into(), ws);

        let layers = build_env_layer_states(&cfg, "proj", "smith", "ANTHROPIC_API_KEY");
        assert_eq!(layers.len(), 4);
        assert_eq!(layers[0].0, "[env]");
        assert_eq!(layers[0].1, EnvLayerState::Unset);
        assert_eq!(layers[1].0, "[roles.smith.env]");
        assert_eq!(layers[1].1, EnvLayerState::Unset);
        assert_eq!(layers[2].0, "[workspaces.proj.env]");
        assert_eq!(layers[2].1, EnvLayerState::Unset);
        assert_eq!(layers[3].0, "[workspaces.proj.roles.smith.env]");
        assert_eq!(layers[3].1, EnvLayerState::ResolvedOpRef);
    }

    #[test]
    fn build_env_layer_states_classifies_literal_at_global() {
        use crate::operator_env::EnvValue;

        let mut env = std::collections::BTreeMap::new();
        env.insert(
            "ANTHROPIC_API_KEY".into(),
            EnvValue::Plain("$ANTHROPIC_API_KEY".into()),
        );
        let cfg = AppConfig {
            env,
            ..AppConfig::default()
        };

        let layers = build_env_layer_states(&cfg, "proj", "smith", "ANTHROPIC_API_KEY");
        assert_eq!(layers[0].1, EnvLayerState::ResolvedLiteral);
        assert_eq!(layers[1].1, EnvLayerState::Unset);
        assert_eq!(layers[2].1, EnvLayerState::Unset);
        assert_eq!(layers[3].1, EnvLayerState::Unset);
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

    #[test]
    fn auth_credential_missing_displays_layer_trace() {
        let err = LaunchError::AuthCredentialMissing {
            agent: crate::agent::Agent::Claude,
            mode: crate::config::AuthForwardMode::ApiKey,
            env_var: "ANTHROPIC_API_KEY",
            workspace: "proj".into(),
            role: "smith".into(),
            mode_resolution: vec![
                (
                    "workspace × role × claude".into(),
                    Some(crate::config::AuthForwardMode::ApiKey),
                ),
                ("workspace × claude".into(), None),
                (
                    "global × claude".into(),
                    Some(crate::config::AuthForwardMode::Sync),
                ),
            ],
            env_layers: vec![
                ("[env]".into(), EnvLayerState::Unset),
                ("[roles.smith.env]".into(), EnvLayerState::Unset),
                ("[workspaces.proj.env]".into(), EnvLayerState::Unset),
                (
                    "[workspaces.proj.roles.smith.env]".into(),
                    EnvLayerState::Unset,
                ),
            ],
        };
        let s = err.to_string();
        assert!(s.contains("auth_forward is 'api_key'"), "got: {s}");
        assert!(s.contains("ANTHROPIC_API_KEY"), "got: {s}");
        assert!(
            s.contains("workspace × role × claude    -> api_key"),
            "got: {s}"
        );
        assert!(s.contains("[workspaces.proj.roles.smith.env]"), "got: {s}");
        assert!(s.contains("Open the Auth panel"), "got: {s}");
    }

    #[test]
    fn auth_credential_missing_codex_api_key_renders() {
        let err = LaunchError::AuthCredentialMissing {
            agent: crate::agent::Agent::Codex,
            mode: crate::config::AuthForwardMode::ApiKey,
            env_var: "OPENAI_API_KEY",
            workspace: "proj".into(),
            role: "smith".into(),
            mode_resolution: vec![],
            env_layers: vec![],
        };
        let s = err.to_string();
        assert!(s.contains("codex"), "got: {s}");
        assert!(s.contains("OPENAI_API_KEY"), "got: {s}");
    }

    #[test]
    fn auth_credential_missing_amp_api_key_renders() {
        let err = LaunchError::AuthCredentialMissing {
            agent: crate::agent::Agent::Amp,
            mode: crate::config::AuthForwardMode::ApiKey,
            env_var: "AMP_API_KEY",
            workspace: "proj".into(),
            role: "smith".into(),
            mode_resolution: vec![],
            env_layers: vec![],
        };
        let s = err.to_string();
        assert!(s.contains("amp"), "got: {s}");
        assert!(s.contains("AMP_API_KEY"), "got: {s}");
    }

    // ── verify_github_token_present (Token-mode pre-flight) ──────

    #[test]
    fn verify_github_token_present_ok_when_token_resolves() {
        let r = super::verify_github_token_present(
            crate::config::GithubAuthMode::Token,
            Some("ghp_real"),
            "proj",
            "smith",
        );
        assert!(r.is_ok());
    }

    #[test]
    fn verify_github_token_present_ok_for_sync_and_ignore_regardless_of_token() {
        // Sync / Ignore have no pre-flight invariant on GH_TOKEN —
        // Sync sources its token from the host, Ignore exports nothing.
        let r = super::verify_github_token_present(
            crate::config::GithubAuthMode::Sync,
            None,
            "proj",
            "smith",
        );
        assert!(r.is_ok());
        let r = super::verify_github_token_present(
            crate::config::GithubAuthMode::Ignore,
            None,
            "proj",
            "smith",
        );
        assert!(r.is_ok());
    }

    #[test]
    fn verify_github_token_present_errors_when_token_missing() {
        let err = super::verify_github_token_present(
            crate::config::GithubAuthMode::Token,
            None,
            "customer-acme",
            "release-bot",
        )
        .unwrap_err();
        let s = err.to_string();
        assert!(s.contains("auth_forward = \"token\""), "got: {s}");
        assert!(s.contains("workspace 'customer-acme'"), "got: {s}");
        assert!(s.contains("role 'release-bot'"), "got: {s}");
        assert!(s.contains("GH_TOKEN"), "got: {s}");
        // Operator-actionable remediation suggestions.
        assert!(s.contains("[github.env]"), "got: {s}");
        assert!(
            s.contains("[workspaces.customer-acme.github.env]"),
            "got: {s}"
        );
        assert!(
            s.contains("[workspaces.customer-acme.roles.release-bot.github.env]"),
            "got: {s}"
        );
        assert!(s.contains("auth_forward = \"sync\""), "got: {s}");
        assert!(s.contains("\"ignore\""), "got: {s}");
    }

    #[test]
    fn verify_github_token_present_errors_when_token_empty_string() {
        // Empty string must be rejected the same as missing — `gh`
        // reads `GH_TOKEN=""` as no token, and we don't want to
        // launch DinD just for the agent to fail at first push.
        let err = super::verify_github_token_present(
            crate::config::GithubAuthMode::Token,
            Some(""),
            "proj",
            "smith",
        )
        .unwrap_err();
        assert!(err.to_string().contains("GH_TOKEN"));
    }

    // ── resolve_github_env_map ───────────────────────────────────

    #[test]
    fn resolve_github_env_map_returns_empty_for_no_declarations() {
        use std::collections::BTreeMap;
        let decls: BTreeMap<String, crate::operator_env::EnvValue> = BTreeMap::new();
        let resolved = super::resolve_github_env_map(&decls, &LoadOptions::default()).unwrap();
        assert!(resolved.is_empty());
    }

    #[test]
    fn resolve_github_env_map_resolves_plain_values() {
        use std::collections::BTreeMap;
        let mut decls: BTreeMap<String, crate::operator_env::EnvValue> = BTreeMap::new();
        decls.insert(
            "GH_TOKEN".into(),
            crate::operator_env::EnvValue::Plain("ghp_test".into()),
        );
        decls.insert(
            "GH_HOST".into(),
            crate::operator_env::EnvValue::Plain("ghe.acme.com".into()),
        );
        let resolved = super::resolve_github_env_map(&decls, &LoadOptions::default()).unwrap();
        assert_eq!(
            resolved.get("GH_TOKEN").map(String::as_str),
            Some("ghp_test")
        );
        assert_eq!(
            resolved.get("GH_HOST").map(String::as_str),
            Some("ghe.acme.com"),
        );
    }

    #[test]
    fn resolve_github_env_map_aggregates_failures() {
        use std::collections::BTreeMap;
        // Two host-env references, both unset → both reported in
        // one structured error rather than aborting on the first.
        let mut decls: BTreeMap<String, crate::operator_env::EnvValue> = BTreeMap::new();
        decls.insert(
            "GH_TOKEN".into(),
            crate::operator_env::EnvValue::Plain("$JACKIN_TEST_MISSING_TOKEN".into()),
        );
        decls.insert(
            "GH_HOST".into(),
            crate::operator_env::EnvValue::Plain("$JACKIN_TEST_MISSING_HOST".into()),
        );
        let opts = LoadOptions {
            // Empty host-env map so `$NAME` references fail to resolve.
            host_env: Some(BTreeMap::new()),
            ..LoadOptions::default()
        };
        let err = super::resolve_github_env_map(&decls, &opts).unwrap_err();
        let s = err.to_string();
        assert!(
            s.contains("github env resolution failed for 2 var(s)"),
            "expected aggregated count, got: {s}"
        );
        assert!(s.contains("GH_TOKEN"), "got: {s}");
        assert!(s.contains("GH_HOST"), "got: {s}");
    }
}
