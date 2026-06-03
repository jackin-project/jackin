//! `jackin load` pipeline: resolve source and trust, claim instance, build
//! image, prepare auth and mounts, launch runtime, attach, finalize.
//!
//! `load_role` is the public entry point; `load_role_with` is the pipeline
//! implementation. Key invariants:
//!
//! * Trust confirmation runs before the image build — an untrusted role may
//!   be cloned and resolved but not built until confirmed.
//! * Token-mode verification fails fast before auth state preparation or
//!   docker-in-docker launch, so a missing token never reaches container startup.
//! * Container slot claim runs before the launch summary is printed, so the
//!   name the operator sees is the final locked name that flows to the
//!   running container.
//! * Foreground-attach finalization runs before teardown classification —
//!   isolated worktrees are finalized before the preserve-vs-clean decision.
//! * `render_exit` is called on both success and error exits from
//!   `load_role_with`.

mod launch_dind;
use launch_dind::run_dind_sidecar;

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
use std::path::PathBuf;

use super::attach::{
    AgentSessionInventory, ContainerState, hardline_agent, inspect_agent_sessions,
    reconnect_or_create_session_with_focus, start_or_reconnect_capsule_client,
};
use super::cleanup::gc_orphaned_resources;
use super::discovery::list_running_agent_names;
use super::identity::{GitIdentity, load_git_identity, load_host_identity};
use super::image::{build_agent_image, prepare_runtime_binaries};
use super::naming::{
    LABEL_KEEP_AWAKE, LABEL_KIND_ROLE, LABEL_MANAGED, dind_certs_volume, image_name,
    image_name_for_branch,
};
use super::repo_cache::{RepoResolveOptions, resolve_agent_repo_with};
use super::universe::ExitClaim;
use crate::docker_client::DockerApi;

const MISE_TRUSTED_CONFIG_PATHS_ENV: &str = "MISE_TRUSTED_CONFIG_PATHS";

#[derive(Default)]
pub struct LoadOptions {
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

    /// CLI override for the agent. `None` defers to (in order) workspace
    /// `default_agent`, the role's single supported agent, or a rich launch
    /// dialog. A launch against a multi-agent role with no resolved choice is
    /// an error when the rich dialog is unavailable.
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
    /// Provider selected for the initial session (e.g. Z.AI's Anthropic
    /// redirect). When set, the first attach carries the provider's env
    /// overrides and label into the capsule's initial spawn.
    pub provider: Option<jackin_protocol::Provider>,
}

impl LoadOptions {
    pub fn initial_provider(&self) -> Option<jackin_protocol::InitialProvider> {
        // Label only: the daemon re-derives the env redirection from it and
        // backfills the token from the container's `ZAI_API_KEY`.
        self.provider
            .map(|provider| jackin_protocol::InitialProvider {
                label: provider.label().to_string(),
            })
    }

    /// Build options for `jackin load`.
    pub fn for_load(debug: bool, rebuild: bool) -> Self {
        Self {
            debug,
            rebuild,
            ..Self::default()
        }
    }

    /// Build options for the operator console (`jackin console`).
    pub fn for_launch(debug: bool) -> Self {
        Self {
            debug,
            ..Self::default()
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
    role_name: String,
    current_stage: Option<super::progress::LaunchStage>,
    progress: Option<super::progress::LaunchProgress>,
}

impl StepCounter {
    fn new(role_name: &str) -> Self {
        Self {
            current: 0,
            role_name: role_name.to_string(),
            current_stage: None,
            progress: None,
        }
    }

    fn start_progress(&mut self, progress: super::progress::LaunchProgress) {
        self.progress = Some(progress);
    }

    async fn next(&mut self, text: &str) {
        if let (Some(progress), Some(stage)) = (&mut self.progress, self.current_stage) {
            progress.stage_done(stage, completion_label(stage));
        }
        self.current += 1;
        tui::set_terminal_title(&format!("{} \u{2014} {text}", self.role_name));
        let stage = stage_for_step_text(text);
        self.current_stage = Some(stage);
        if let Some(progress) = &mut self.progress {
            progress.stage_started(stage, text);
            progress.settle_stage_visual().await;
        }
    }

    fn done(&self) {
        tui::set_terminal_title(&self.role_name);
    }

    const fn progress_mut(&mut self) -> Option<&mut super::progress::LaunchProgress> {
        self.progress.as_mut()
    }

    /// Stop the rich loading surface's render task and clear
    /// `rich_surface_active`. Call this before handing the terminal to an
    /// interactive `docker exec -it` session, otherwise the capsule attach
    /// can't own the PTY and hangs.
    fn finish_progress(&mut self) {
        if let Some(progress) = self.progress.as_mut() {
            progress.finish();
        }
        self.progress = None;
    }
}

struct LaunchEnvPrompter<'a> {
    progress: Option<std::cell::RefCell<&'a mut super::progress::LaunchProgress>>,
}

impl<'a> LaunchEnvPrompter<'a> {
    fn new(progress: Option<&'a mut super::progress::LaunchProgress>) -> Self {
        Self {
            progress: progress.map(std::cell::RefCell::new),
        }
    }
}

impl crate::env_resolver::EnvPrompter for LaunchEnvPrompter<'_> {
    fn prompt_text(
        &self,
        title: &str,
        default: Option<&str>,
        skippable: bool,
    ) -> anyhow::Result<crate::env_resolver::PromptResult> {
        if let Some(progress) = &self.progress {
            return progress.borrow_mut().prompt_text(title, default, skippable);
        }
        anyhow::bail!("manifest env text prompt requires the rich launch dialog")
    }

    fn prompt_select(
        &self,
        title: &str,
        options: &[String],
        default: Option<&str>,
        skippable: bool,
    ) -> anyhow::Result<crate::env_resolver::PromptResult> {
        if let Some(progress) = &self.progress {
            return progress
                .borrow_mut()
                .prompt_select(title, options, default, skippable);
        }
        anyhow::bail!("manifest env select prompt requires the rich launch dialog")
    }
}

fn sensitive_mount_prompt(sensitive: &[crate::workspace::SensitiveMount]) -> String {
    let mut lines = vec![
        "Sensitive host paths are mounted into this role container.".to_string(),
        "Continue only if this role should see these credentials.".to_string(),
        String::new(),
    ];
    for hit in sensitive {
        lines.push(format!("{} — {}", hit.src, hit.reason));
    }
    lines.push(String::new());
    lines.push("Continue with these mounts?".to_string());
    lines.join("\n")
}

fn stage_for_step_text(text: &str) -> super::progress::LaunchStage {
    match text {
        "Resolving role identity" => super::progress::LaunchStage::Role,
        "Preparing runtime binaries" => super::progress::LaunchStage::AgentBinaries,
        "Preparing derived image" => super::progress::LaunchStage::DerivedImage,
        "Starting Docker-in-Docker" => super::progress::LaunchStage::Sidecar,
        "Launching role" => super::progress::LaunchStage::Capsule,
        _ => super::progress::LaunchStage::Identity,
    }
}

const fn completion_label(stage: super::progress::LaunchStage) -> &'static str {
    match stage {
        super::progress::LaunchStage::Identity | super::progress::LaunchStage::Credentials => {
            "resolved"
        }
        super::progress::LaunchStage::Role => "trusted source",
        super::progress::LaunchStage::Construct => "online",
        super::progress::LaunchStage::AgentBinaries => "cached",
        super::progress::LaunchStage::DerivedImage | super::progress::LaunchStage::Capsule => {
            "ready"
        }
        super::progress::LaunchStage::Workspace => "materialized",
        super::progress::LaunchStage::Network => "isolated",
        super::progress::LaunchStage::Sidecar => "awake",
        super::progress::LaunchStage::Hardline => "open",
    }
}

const fn launch_target_kind(workspace_name: Option<&str>) -> super::progress::LaunchTargetKind {
    if workspace_name.is_some() {
        super::progress::LaunchTargetKind::Workspace
    } else {
        super::progress::LaunchTargetKind::Directory
    }
}

fn launch_target_label(
    workspace_name: Option<&str>,
    workspace: &crate::workspace::ResolvedWorkspace,
) -> String {
    workspace_name.map_or_else(|| tui::shorten_home(&workspace.workdir), str::to_string)
}

/// Human-readable lines for the mounts whose host source differs from the
/// container destination. Same-path mounts (the current-directory launch
/// case) carry no information for the operator and are omitted entirely, so
/// a directory launch shows no mount line at all.
fn launch_mount_lines(workspace: &crate::workspace::ResolvedWorkspace) -> Vec<String> {
    workspace
        .mounts
        .iter()
        .filter(|mount| mount.src.trim_end_matches('/') != mount.dst.trim_end_matches('/'))
        .map(|mount| {
            let ro = if mount.readonly { " (ro)" } else { "" };
            format!("{} → {}{ro}", tui::shorten_home(&mount.src), mount.dst)
        })
        .collect()
}

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
            "{}:/home/agent/.kimi-code",
            state.root.join("home/.kimi-code").display()
        ));
        if kimi.forward_auth {
            mounts.push(format!(
                "{}:/jackin/kimi-code",
                state.root.join("kimi-code").display()
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

fn workspace_trusted_project_paths(
    workspace: &crate::workspace::ResolvedWorkspace,
) -> std::collections::BTreeSet<String> {
    let mut paths = std::collections::BTreeSet::new();
    if !workspace.workdir.trim().is_empty() {
        paths.insert(workspace.workdir.clone());
    }
    for mount in &workspace.mounts {
        if !mount.dst.trim().is_empty() {
            paths.insert(mount.dst.clone());
        }
    }
    paths
}

fn workspace_mise_trusted_config_paths(
    workspace: &crate::workspace::ResolvedWorkspace,
) -> Option<String> {
    let paths = workspace_trusted_project_paths(workspace);
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

/// Coerce `key` to a table, overwriting any non-table value — the trailing
/// `as_table_mut` is infallible only because of this normalization.
fn ensure_table<'a>(table: &'a mut toml_edit::Table, key: &str) -> &'a mut toml_edit::Table {
    let item = table
        .entry(key)
        .or_insert_with(|| toml_edit::Item::Table(toml_edit::Table::new()));
    if !item.is_table() {
        *item = toml_edit::Item::Table(toml_edit::Table::new());
    }
    item.as_table_mut()
        .expect("item was just normalized to a table")
}

/// Codex's per-folder trust prompt is separate from approval/sandbox bypass,
/// so the launch flag alone does not suppress it — each workspace path is
/// marked `trusted` in the container's `config.toml`.
fn seed_codex_project_trust(
    state: &crate::instance::RoleState,
    workspace: &crate::workspace::ResolvedWorkspace,
) -> anyhow::Result<()> {
    if state.auth.codex.is_none() {
        return Ok(());
    }

    let trusted_paths = workspace_trusted_project_paths(workspace);
    if trusted_paths.is_empty() {
        return Ok(());
    }

    let config_path = state.root.join("home/.codex/config.toml");
    let raw = match std::fs::read_to_string(&config_path) {
        Ok(raw) => raw,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => String::new(),
        Err(err) => {
            return Err(err)
                .with_context(|| format!("reading Codex config at {}", config_path.display()));
        }
    };
    let mut doc: toml_edit::DocumentMut = if raw.trim().is_empty() {
        toml_edit::DocumentMut::new()
    } else {
        raw.parse()
            .with_context(|| format!("parsing Codex config at {}", config_path.display()))?
    };

    crate::debug_log!(
        "codex-trust",
        "seeding trust_level=trusted for {} workspace path(s) in {}",
        trusted_paths.len(),
        config_path.display()
    );
    let projects = ensure_table(doc.as_table_mut(), "projects");
    for path in trusted_paths {
        ensure_table(projects, &path).insert("trust_level", toml_edit::value("trusted"));
    }

    if let Some(parent) = config_path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("creating Codex config directory at {}", parent.display()))?;
    }
    std::fs::write(&config_path, doc.to_string())
        .with_context(|| format!("writing Codex config at {}", config_path.display()))?;
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
    git_dco: bool,
    agent: crate::agent::Agent,
    capsule_config: &'a jackin_protocol::CapsuleConfig,
    resolved_env: &'a crate::env_resolver::ResolvedEnv,
    /// Resolved `[…github.env]` map (post `op://` + `$NAME`
    /// resolution). `GH_TOKEN` carries the token in the launcher's
    /// preferred env-injection path; `GH_HOST` and
    /// `GH_ENTERPRISE_TOKEN` are forwarded as-is when set so GHE
    /// targets work end to end.
    github_env: &'a std::collections::BTreeMap<String, String>,
    /// Required so `launch_role_runtime` can fire the `keep_awake`
    /// reconciler between `docker run -d` and the foreground `docker
    /// attach`. Without that mid-flight call, caffeinate would never
    /// spawn for an interactive `jackin load`: the post-launch
    /// reconcile in `app::Command::Load` only runs after attach
    /// returns, by which time the container has stopped and the
    /// `keep_awake` count is back to zero.
    paths: &'a JackinPaths,
}

fn capsule_config(
    selector: &RoleSelector,
    workdir: &str,
    manifest: &crate::manifest::RoleManifest,
    initial_provider: Option<jackin_protocol::InitialProvider>,
) -> jackin_protocol::CapsuleConfig {
    let mut agents = Vec::new();
    let mut models = std::collections::BTreeMap::new();
    for agent in manifest.supported_agents() {
        agents.push(agent.slug().to_string());
        let model = match agent {
            crate::agent::Agent::Claude => manifest
                .claude
                .as_ref()
                .and_then(|cfg| cfg.model.as_deref()),
            crate::agent::Agent::Codex => {
                manifest.codex.as_ref().and_then(|cfg| cfg.model.as_deref())
            }
            crate::agent::Agent::Amp => None,
            crate::agent::Agent::Kimi => {
                manifest.kimi.as_ref().and_then(|cfg| cfg.model.as_deref())
            }
            crate::agent::Agent::Opencode => manifest
                .opencode
                .as_ref()
                .and_then(|cfg| cfg.model.as_deref()),
        };
        if let Some(model) = model {
            models.insert(agent.slug().to_string(), model.to_string());
        }
    }
    jackin_protocol::CapsuleConfig {
        role: selector.key(),
        workdir: workdir.to_string(),
        agents,
        models,
        initial_provider,
    }
}

/// Create the Docker network, start `DinD`, and launch the role container.
#[expect(
    clippy::too_many_lines,
    reason = "pending extraction — tracked in codebase-readability roadmap"
)]
async fn launch_role_runtime(
    ctx: &LaunchContext<'_>,
    steps: &mut StepCounter,
    docker: &impl DockerApi,
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
        git_dco,
        agent,
        capsule_config,
        resolved_env,
        github_env,
        paths,
    } = ctx;

    let certs_volume = dind_certs_volume(container_name);

    let docker_run_opts = RunOptions {
        quiet: !debug,
        ..RunOptions::default()
    };

    if let Some(progress) = steps.progress_mut() {
        progress.stage_started(
            super::progress::LaunchStage::Network,
            "wiring private network",
        );
    }
    run_dind_sidecar(
        container_name,
        network,
        dind,
        &certs_volume,
        docker,
        runner,
        steps,
        &docker_run_opts,
    )
    .await?;

    // Step 4: Mount volumes and launch
    steps.next("Launching role").await;
    steps.done();

    if steps.progress.is_none() {
        tui::print_deploying(agent_display_name).await;
    }

    let class_label = format!("jackin.class={}", selector.key());
    let display_label = format!("jackin.display_name={agent_display_name}");
    let docker_host = format!("DOCKER_HOST=tcp://{dind}:2376");
    let dind_hostname = format!("{}={dind}", crate::env_model::JACKIN_DIND_HOSTNAME_ENV_NAME);
    let role_container_name_env = format!(
        "{}={container_name}",
        crate::env_model::JACKIN_CONTAINER_NAME_ENV_NAME
    );
    let instance_id = if let Some(id) =
        crate::instance::naming::instance_id_from_container_base(container_name)
    {
        id
    } else {
        crate::tui::emit_compact_line(
            "warning",
            &format!(
                "warning: instance_id_from_container_base could not parse {container_name:?}; falling back to full container name as JACKIN_INSTANCE_ID — chrome chip will render the full name"
            ),
        );
        container_name
    };
    let instance_id_env = format!(
        "{}={instance_id}",
        crate::env_model::JACKIN_INSTANCE_ID_ENV_NAME
    );
    let testcontainers_host_override = format!(
        "{}={dind}",
        crate::env_model::TESTCONTAINERS_HOST_OVERRIDE_ENV_NAME
    );
    let git_author_name = format!("GIT_AUTHOR_NAME={}", git.user_name);
    let git_author_email = format!("GIT_AUTHOR_EMAIL={}", git.user_email);
    let agent_specific_mounts = agent_mounts(state);
    let gh_config_mount = format!("{}:/home/agent/.config/gh", state.gh_config_dir.display());
    let certs_agent_mount = format!("{certs_volume}:/certs/client:ro");

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
        &role_container_name_env,
        "-e",
        &instance_id_env,
        "-e",
        &testcontainers_host_override,
        "-e",
        &git_author_name,
        "-e",
        &git_author_email,
    ]);
    let debug_run_id_env = if *debug {
        run_args.extend_from_slice(&["-e", "JACKIN_DEBUG=1"]);
        crate::diagnostics::active_run().map(|r| format!("JACKIN_RUN_ID={}", r.run_id()))
    } else {
        None
    };
    if let Some(ref env) = debug_run_id_env {
        run_args.extend_from_slice(&["-e", env.as_str()]);
    }
    // Always pass the host jackin version so the capsule ContainerInfo dialog
    // can surface which host binary launched the container.
    let host_version_env = format!("JACKIN_HOST_VERSION={}", env!("CARGO_PKG_VERSION"));
    run_args.extend_from_slice(&["-e", host_version_env.as_str()]);

    let git_coauthor_trailer_env = git_coauthor_trailer.then(|| {
        format!(
            "{}=1",
            crate::env_model::JACKIN_GIT_COAUTHOR_TRAILER_ENV_NAME
        )
    });
    if let Some(ref env) = git_coauthor_trailer_env {
        run_args.extend_from_slice(&["-e", env.as_str()]);
    }
    let git_dco_env = git_dco.then(|| format!("{}=1", crate::env_model::JACKIN_GIT_DCO_ENV_NAME));
    if let Some(ref env) = git_dco_env {
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
    run_args.extend_from_slice(&["-v", &certs_agent_mount, "-v", &gh_config_mount]);
    for mount in &agent_specific_mounts {
        run_args.push("-v");
        run_args.push(mount);
    }

    let mount_strings = build_workspace_mount_strings(workspace);
    for ms in &mount_strings {
        run_args.push("-v");
        run_args.push(ms);
    }
    let image_label = format!("jackin.image={image}");
    run_args.extend_from_slice(&["--label", &image_label]);
    // Host-side bind-mount of the daemon's socket directory. Pre-create
    // host-side so Docker does not materialise the target itself as
    // root:root 0755 — that would block the in-container `agent` user
    // (whose UID matches the host user post-`usermod` in the derived
    // image) from creating and chmod'ing `jackin.sock`. The same
    // directory carries Capsule's normalized launch config.
    let socket_dir = paths.jackin_home.join("sockets").join(*container_name);
    let capsule_config_contents = toml::to_string(capsule_config)
        .context("serializing Capsule launch config for /jackin/run/agent.toml")?;
    // Run the filesystem syscalls on the blocking pool — the tokio
    // runtime is built without the `fs` feature here, and blocking on
    // a slow / NFS host parks the worker driving the docker-run RPC
    // for every other future scheduled on it.
    let socket_dir_for_mkdir = socket_dir.clone();
    let capsule_config_contents_for_write = capsule_config_contents.clone();
    tokio::task::spawn_blocking(move || -> std::io::Result<()> {
        std::fs::create_dir_all(&socket_dir_for_mkdir)?;
        std::fs::write(
            socket_dir_for_mkdir.join(jackin_protocol::CAPSULE_CONFIG_FILENAME),
            capsule_config_contents_for_write,
        )?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(
                &socket_dir_for_mkdir,
                std::fs::Permissions::from_mode(0o700),
            )?;
        }
        Ok(())
    })
    .await
    .context("socket dir mkdir worker join")?
    .with_context(|| {
        format!(
            "creating host-side socket dir {} for container {container_name}",
            socket_dir.display(),
        )
    })?;
    // `Display` is lossy on non-UTF-8 paths — docker would silently mount a
    // different host dir than the one we just created. Bail rather than
    // smuggle U+FFFD into a `-v` argument.
    let socket_dir_str = socket_dir.to_str().ok_or_else(|| {
        anyhow::anyhow!(
            "socket dir {} contains non-UTF-8 bytes; cannot pass to docker -v",
            socket_dir.display(),
        )
    })?;
    let socket_mount = format!("{socket_dir_str}:/jackin/run");
    run_args.extend_from_slice(&["-v", &socket_mount]);
    crate::debug_log!(
        "launch",
        "prepared host socket dir {socket_dir_str} (0o700) and Capsule config for bind-mount at /jackin/run",
    );
    run_args.push(image);
    // Pass the initial agent as the container command argument. The
    // daemon uses it only to choose the first tab; per-session
    // `JACKIN_AGENT` is set later when spawning an actual agent PTY.
    run_args.push(agent.slug());
    let run_role = runner.run("docker", &run_args, None, &docker_run_opts);
    if let Some(progress) = steps.progress_mut() {
        progress.while_waiting(run_role).await?;
    } else {
        run_role.await?;
    }

    // Reconcile keep_awake AFTER the role container is running but
    // BEFORE the foreground session blocks. This is the only window in
    // which an interactive `jackin load` can spawn caffeinate.
    super::caffeinate::reconcile(paths, docker, runner).await;

    // Pre-session safety check: if jackin-capsule exited immediately
    // (missing binary, bad image), surface the container logs rather than
    // failing with a cryptic docker exec error.
    if let Some(err) =
        diagnose_premature_exit(docker, runner, container_name, ExitPhase::PreAttach).await
    {
        return Err(err);
    }

    // Connect the operator's terminal to the running jackin-capsule multiplexer.
    // The shared reconnect helper first waits for `/jackin/run/jackin.sock`
    // to answer `status`; jackin-capsule detects PID != 1 and then runs in
    // client mode, connecting to that daemon socket inside the container.
    if let Some(progress) = steps.progress_mut() {
        progress.stage_done(super::progress::LaunchStage::Capsule, "ready");
        progress.opening_hardline();
        progress.settle_stage_visual().await;
    }
    // Tear down the loading cockpit before the interactive attach: the
    // capsule's `docker exec -it` must own a clean terminal, and leaving the
    // rich surface active would force-capture its PTY and hang the handoff.
    steps.finish_progress();
    let session_result =
        reconnect_or_create_session_with_focus(paths, container_name, None, docker, runner).await;
    // Ensure cleanup debug logs start on a fresh line after the interactive session
    eprintln!();
    if let Err(err) = session_result {
        // Single inspect — the previous two-call shape opened a TOCTOU
        // window where the container could transition Running→Stopped(0)
        // between the diagnose and swallow checks. `diagnose_premature_exit`
        // returns a synthesized error for surfaceable exits; otherwise
        // the post-attach happy path is `Stopped(exit 0, !oom)` from a
        // clean multiplexer shutdown — swallow `docker exec`'s broken
        // pipe in that case. External `docker rm` (NotFound) is rare
        // and must propagate the real exec error so the operator sees
        // why the container vanished mid-session.
        let inspect = docker.inspect_container_state(container_name).await;
        if let Some(diag) =
            diagnose_with_state(runner, container_name, &inspect, ExitPhase::PostAttach).await
        {
            return Err(diag);
        }
        if matches!(
            inspect,
            ContainerState::Stopped {
                exit_code: 0,
                oom_killed: false,
            }
        ) {
            return Ok(());
        }
        return Err(err);
    }
    if let Some(progress) = steps.progress_mut() {
        progress.stage_done(super::progress::LaunchStage::Hardline, "open");
    }

    Ok(())
}

/// Whether `diagnose_premature_exit` is firing before the operator's
/// terminal was attached or after. The treatment of `exit 0` differs
/// between the two: pre-attach it's PID 1 exiting before the client
/// attaches (still worth surfacing — most likely a bad image or
/// missing binary), post-attach it's the multiplexer shutting the
/// container down because no live sessions remain (the
/// container-lifecycle-policy happy path — swallow it).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ExitPhase {
    PreAttach,
    PostAttach,
}

/// inspect + log fetch so the surfaced error names the exit code, OOM
/// flag, and the last lines of the container's combined stdout/stderr.
///
/// Returns `None` when the container is still running (the normal
/// happy path) so the caller can proceed to the session exec.
async fn diagnose_premature_exit(
    docker: &impl DockerApi,
    runner: &mut impl crate::docker::CommandRunner,
    container_name: &str,
    phase: ExitPhase,
) -> Option<anyhow::Error> {
    let state = docker.inspect_container_state(container_name).await;
    diagnose_with_state(runner, container_name, &state, phase).await
}

/// Same diagnostic logic as `diagnose_premature_exit` but with the
/// inspected state passed in — callers that already inspected the
/// container can avoid a second `docker inspect` round-trip (and the
/// TOCTOU window between the two).
async fn diagnose_with_state(
    runner: &mut impl crate::docker::CommandRunner,
    container_name: &str,
    state: &ContainerState,
    phase: ExitPhase,
) -> Option<anyhow::Error> {
    match state {
        // Default to letting the `docker exec` attempt proceed when state is
        // ambiguous: the daemon's own error from a true `NotFound`
        // (`No such container`) is just as actionable as anything we
        // could synthesize, and a transient inspect hiccup must not
        // hijack an otherwise-healthy launch.
        ContainerState::Running
        | ContainerState::Paused
        | ContainerState::Restarting
        | ContainerState::Created
        | ContainerState::Removing
        | ContainerState::Dead
        | ContainerState::NotFound
        | ContainerState::InspectUnavailable(_) => None,
        ContainerState::Stopped {
            exit_code,
            oom_killed,
        } => {
            // Post-attach clean exit (exit 0, no OOM) is the normal
            // shutdown path: the operator typed `/exit` in the agent,
            // the multiplexer drained the last live session, and the
            // container shut itself down. The container-lifecycle
            // policy treats this as the happy path — return None so
            // the caller does not synthesize a misleading "exited
            // before attach" error. Pre-attach exit 0 is still
            // surfaced because PID 1 died before the
            // client connected indicates a bad image / missing binary
            // even when the exit code looks clean.
            if phase == ExitPhase::PostAttach && *exit_code == 0 && !oom_killed {
                return None;
            }
            // Distinguish "docker logs succeeded but was empty" from
            // "docker logs CLI failed" — the latter is a post-mortem
            // signal the operator needs (daemon down, container gone)
            // rather than the empty body the prose body falls back to.
            let logs = match runner
                .capture("docker", &["logs", "--tail", "40", container_name], None)
                .await
            {
                Ok(text) => {
                    let trimmed = text.trim().to_string();
                    if trimmed.is_empty() {
                        None
                    } else {
                        Some(trimmed)
                    }
                }
                Err(e) => Some(format!("(docker logs failed: {e:#})")),
            };
            let reason = if *oom_killed {
                "OOM killed".to_string()
            } else {
                format!("exit {exit_code}")
            };
            let phase_label = match phase {
                ExitPhase::PreAttach => "exited before attach",
                ExitPhase::PostAttach => "exited during session",
            };
            let body = logs.map_or_else(
                || {
                    format!(
                        "container {container_name} {phase_label} ({reason}) and produced no log output"
                    )
                },
                |text| {
                    format!(
                        "container {container_name} {phase_label} ({reason}); last 40 log lines:\n{text}"
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
pub(super) async fn inspect_attach_outcome(
    docker: &impl DockerApi,
    container: &str,
) -> anyhow::Result<crate::isolation::finalize::AttachOutcome> {
    use crate::isolation::finalize::AttachOutcome;
    // Only `Stopped` with a clean or non-zero exit legitimately routes through
    // finalize_clean_exit. Paused/Restarting/Created/Removing are transient
    // active states — treating them as still_running is the conservative choice
    // that prevents finalize_clean_exit from auto-deleting worktrees of
    // containers that may resume. Dead is rare (daemon failed to deinitialize)
    // and also preserved for operator inspection.
    Ok(match docker.inspect_container_state(container).await {
        ContainerState::Running
        | ContainerState::Paused
        | ContainerState::Restarting
        | ContainerState::Created
        | ContainerState::Removing => AttachOutcome::still_running(),
        ContainerState::Dead => {
            crate::debug_log!(
                "isolation",
                "inspect_attach_outcome: container {container} status=dead; treating as still_running to preserve records for inspection",
            );
            AttachOutcome::still_running()
        }
        ContainerState::Stopped {
            oom_killed: true, ..
        } => AttachOutcome::oom_killed(),
        ContainerState::Stopped { exit_code, .. } => AttachOutcome::stopped(exit_code),
        ContainerState::NotFound | ContainerState::InspectUnavailable(_) => {
            crate::debug_log!(
                "isolation",
                "inspect_attach_outcome: docker inspect failed for {container}; treating as still_running (conservative — finalize_clean_exit's auto-cleanup never fires)",
            );
            AttachOutcome::still_running()
        }
    })
}

enum GitPullResult {
    Success { src: String, stdout: String },
    Failure { src: String, stderr: String },
    SpawnError { src: String, error: std::io::Error },
    JoinError { src: String },
}

#[cfg(test)]
fn pull_workspace_repos_with_git(
    workspace: &crate::workspace::ResolvedWorkspace,
    debug: bool,
    git_program: &std::path::Path,
) -> Vec<GitPullResult> {
    pull_git_sources_with_git(git_pull_sources(workspace), debug, git_program, true)
}

fn git_pull_sources(workspace: &crate::workspace::ResolvedWorkspace) -> Vec<String> {
    let mut sources = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for mount in &workspace.mounts {
        if std::path::Path::new(&mount.src).join(".git").exists() && seen.insert(mount.src.clone())
        {
            sources.push(mount.src.clone());
        }
    }
    sources
}

fn pull_git_sources_with_git(
    sources: Vec<String>,
    debug: bool,
    git_program: &std::path::Path,
    print_starts: bool,
) -> Vec<GitPullResult> {
    let mut pulls = Vec::new();

    for src in sources {
        if debug {
            crate::diagnostics::active_debug("git_pull", &format!("git pull in {src}"));
            if crate::diagnostics::active_run().is_none() {
                eprintln!("[jackin debug] git pull in {src}");
            }
        }
        if print_starts {
            eprintln!("  Pulling {} …", crate::tui::shorten_home(&src));
        }
        let git_program = git_program.to_path_buf();
        pulls.push((
            src.clone(),
            std::thread::spawn(move || {
                let mut command = std::process::Command::new(git_program);
                command
                    .args(["-C", &src, "pull"])
                    .env("GIT_TERMINAL_PROMPT", "0")
                    .stdin(std::process::Stdio::null());
                match command.output() {
                    Ok(out) if out.status.success() => GitPullResult::Success {
                        src,
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

    pulls
        .into_iter()
        .map(|(src, handle)| handle.join().unwrap_or(GitPullResult::JoinError { src }))
        .collect()
}

fn print_git_pull_results(results: &[GitPullResult]) {
    for result in results {
        match result {
            GitPullResult::Success { stdout, .. } => {
                print_git_pull_stdout(stdout);
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

fn print_git_pull_stdout(stdout: &str) {
    let trimmed = stdout.trim();
    if !trimmed.is_empty() {
        eprintln!("    {trimmed}");
    }
}

fn record_git_pull_results(results: &[GitPullResult]) -> (usize, usize) {
    let mut ok = 0;
    let mut failed = 0;
    for result in results {
        match result {
            GitPullResult::Success { src, stdout } => {
                ok += 1;
                crate::diagnostics::active_debug(
                    "git_pull",
                    &format!("git pull in {src} succeeded: {}", stdout.trim()),
                );
            }
            GitPullResult::Failure { src, stderr } => {
                failed += 1;
                if let Some(run) = crate::diagnostics::active_run() {
                    run.compact("git_pull", &format!("git pull failed in {src}"));
                }
                crate::diagnostics::active_debug(
                    "git_pull",
                    &format!("git pull in {src} failed: {}", stderr.trim()),
                );
            }
            GitPullResult::SpawnError { src, error } => {
                failed += 1;
                if let Some(run) = crate::diagnostics::active_run() {
                    run.compact(
                        "git_pull",
                        &format!("could not run git pull in {src}: {error}"),
                    );
                }
            }
            GitPullResult::JoinError { src } => {
                failed += 1;
                if let Some(run) = crate::diagnostics::active_run() {
                    run.compact("git_pull", &format!("git pull thread panicked in {src}"));
                }
            }
        }
    }
    (ok, failed)
}

// Boxed future required: load_role calls itself recursively via
// RestoreResolution::RebuildRelatedRole — async fn recursion is not allowed.
pub fn load_role<'a>(
    paths: &'a JackinPaths,
    config: &'a mut AppConfig,
    selector: &'a RoleSelector,
    workspace: &'a crate::workspace::ResolvedWorkspace,
    docker: &'a impl DockerApi,
    runner: &'a mut impl CommandRunner,
    opts: &'a LoadOptions,
) -> std::pin::Pin<Box<dyn std::future::Future<Output = anyhow::Result<()>> + 'a>> {
    Box::pin(load_role_with(
        paths,
        config,
        selector,
        workspace,
        docker,
        runner,
        opts,
        |_, _| anyhow::bail!("role trust prompt requires the rich launch dialog"),
        |_, _, _| anyhow::bail!("branch trust prompt requires the rich launch dialog"),
    ))
}

pub async fn resolve_supported_agents_for_console(
    paths: &JackinPaths,
    config: &AppConfig,
    selector: &RoleSelector,
    runner: &mut impl CommandRunner,
) -> anyhow::Result<Vec<crate::agent::Agent>> {
    // Lookup-only: the actual launch path uses
    // `AppConfig::resolve_role_source` which synthesizes + inserts a
    // RoleSource for unregistered namespaced selectors. That mutation
    // is for the launch (which persists trust), not for a transient
    // agent-list query that discards the config.
    let source = config
        .roles
        .get(&selector.key())
        .cloned()
        .ok_or_else(|| anyhow::anyhow!("unknown role selector {}", selector.key()))?;
    // Cached manifest is sufficient because the supported-agent set
    // rarely changes between fetches; the real launch re-fetches and
    // re-validates. Saves a git round trip per role-row Enter.
    let cached = crate::repo::CachedRepo::new(paths, selector);
    if cached.repo_dir.join(".git").is_dir() {
        match crate::manifest::load_role_manifest(&cached.repo_dir) {
            Ok(manifest) => return Ok(manifest.supported_agents()),
            Err(error) => crate::debug_log!(
                "console",
                "cached manifest for {} present but failed to parse ({error:#}); refetching",
                selector.key()
            ),
        }
    } else {
        crate::debug_log!(
            "console",
            "no cached repo for {}; falling back to git fetch",
            selector.key()
        );
    }
    let (_, validated_repo, _repo_lock) = super::repo_cache::resolve_agent_repo_with(
        paths,
        selector,
        &source.git,
        runner,
        super::repo_cache::RepoResolveOptions::non_interactive(),
        || Ok(false),
    )
    .await?;
    Ok(validated_repo.manifest.supported_agents())
}

#[expect(
    clippy::too_many_lines,
    clippy::too_many_arguments,
    reason = "pending extraction — tracked in codebase-readability roadmap"
)]
async fn load_role_with(
    paths: &JackinPaths,
    config: &mut AppConfig,
    selector: &RoleSelector,
    workspace: &crate::workspace::ResolvedWorkspace,
    docker: &impl DockerApi,
    runner: &mut impl CommandRunner,
    opts: &LoadOptions,
    confirm_trust_for_test: impl FnOnce(&RoleSelector, &crate::config::RoleSource) -> anyhow::Result<()>,
    confirm_branch_for_test: impl FnOnce(
        &RoleSelector,
        &crate::config::RoleSource,
        &str,
    ) -> anyhow::Result<()>,
) -> anyhow::Result<()> {
    // Pre-launch garbage collection is independent from host identity probes.
    let ((), (git, host)) = tokio::join!(gc_orphaned_resources(docker), async {
        let git = load_git_identity(runner).await;
        let host = load_host_identity(runner).await;
        (git, host)
    });

    // `app::run` claims the first-entry boundary immediately before a real
    // launch so the two-screen intro only plays from an empty construct. Direct
    // test/internal callers still need the elapsed-time marker for the last-exit
    // outro, so this idempotently writes one if the app layer did not.
    super::universe::mark_start(paths, super::universe::StartKind::ResumeExisting);

    // `load_role` receives a `ResolvedWorkspace` (mounts + workdir),
    // not a name. Recover the name by matching workdir, mirroring the
    // identification rule used by `jackin workspace show`.
    let workspace_name = config
        .workspaces
        .iter()
        .find(|(_, w)| w.workdir == workspace.workdir)
        .map(|(name, _)| name.clone());

    let mut steps = StepCounter::new(&selector.name);
    if let Some(run) = crate::diagnostics::active_run() {
        let mut progress = super::progress::LaunchProgress::new(
            run,
            std::env::var_os("JACKIN_NO_MOTION").is_some(),
            super::progress::host_terminal(),
            env!("JACKIN_VERSION"),
        )?;
        progress.started(super::progress::LaunchIdentity {
            role: selector.name.clone(),
            agent: opts
                .agent
                .or(workspace.default_agent)
                .map_or_else(|| "resolving".to_string(), |agent| agent.slug().to_string()),
            target_kind: launch_target_kind(workspace_name.as_deref()),
            target_label: launch_target_label(workspace_name.as_deref(), workspace),
            mounts: launch_mount_lines(workspace),
            image: None,
            container: None,
        });
        progress.stage_done(super::progress::LaunchStage::Identity, "resolved operator");
        steps.start_progress(progress);
    }

    let sensitive = crate::workspace::find_sensitive_mounts(&workspace.mounts);
    if !sensitive.is_empty() {
        let prompt = sensitive_mount_prompt(&sensitive);
        let confirmed = if let Some(progress) = steps.progress_mut() {
            progress.confirm_prompt(prompt)?
        } else {
            anyhow::bail!("sensitive mount confirmation requires the rich launch dialog")
        };
        if !confirmed {
            anyhow::bail!("aborted — sensitive mount paths were not confirmed");
        }
    }

    if workspace.git_pull_on_entry {
        let sources = git_pull_sources(workspace);
        if let Some(progress) = steps.progress_mut() {
            if sources.is_empty() {
                progress.stage_skipped(
                    super::progress::LaunchStage::Workspace,
                    "no mounted git repositories",
                );
            } else {
                progress.stage_started(
                    super::progress::LaunchStage::Workspace,
                    format!("polling {} workspace repositories", sources.len()),
                );
                let debug = opts.debug;
                let git_program = std::path::PathBuf::from("git");
                let pull = tokio::task::spawn_blocking(move || {
                    pull_git_sources_with_git(sources, debug, &git_program, false)
                });
                let results = progress
                    .while_waiting(async move {
                        pull.await
                            .map_err(|error| anyhow::anyhow!("joining git pull worker: {error}"))
                    })
                    .await?;
                let (ok, failed) = record_git_pull_results(&results);
                let detail = if failed == 0 {
                    format!("{ok} repositories current")
                } else {
                    format!("{ok} repositories current; {failed} failed")
                };
                progress.stage_done(super::progress::LaunchStage::Workspace, detail);
            }
        } else if !sources.is_empty() {
            // Run the blocking git pulls on a blocking-pool thread so the
            // single-threaded executor is never parked on the join.
            let debug = opts.debug;
            let git_program = std::path::PathBuf::from("git");
            let results = tokio::task::spawn_blocking(move || {
                pull_git_sources_with_git(sources, debug, &git_program, true)
            })
            .await
            .map_err(|error| anyhow::anyhow!("joining git pull worker: {error}"))?;
            print_git_pull_results(&results);
        }
    }

    let (source, is_new, restore_source_override) =
        resolve_launch_role_source(config, selector, opts.restore_role_source_git.as_deref())?;

    // Step 1: Resolve role identity (clone or update repo)
    steps.next("Resolving role identity").await;

    let mut confirm_repo_removal = || {
        if let Some(progress) = steps.progress_mut() {
            return progress
                .confirm_prompt("Remove the cached repo and re-clone from the configured source?");
        }
        anyhow::bail!("cached repo recovery prompt requires the rich launch dialog")
    };
    let (cached_repo, validated_repo, repo_lock) = resolve_agent_repo_with(
        paths,
        selector,
        &source.git,
        runner,
        RepoResolveOptions::interactive(opts.debug).with_branch(opts.role_branch.as_deref()),
        &mut confirm_repo_removal,
    )
    .await?;

    // Trust gate: prompt the operator before running an untrusted third-party role
    let newly_trusted = if source.trusted {
        false
    } else {
        let confirmed = if let Some(progress) = steps.progress_mut() {
            progress.confirm_role_trust(selector.key(), source.git.clone())?
        } else {
            confirm_trust_for_test(selector, &source)?;
            true
        };
        if !confirmed {
            anyhow::bail!(
                "role source \"{selector}\" not trusted — aborting.\n\
                 To trust it later, run `jackin config trust grant {selector}` or try loading again."
            );
        }
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

    let supported_agents = validated_repo.manifest.supported_agents();
    let agent = match opts.agent.or(workspace.default_agent) {
        Some(a) => a,
        None if supported_agents.len() == 1 => supported_agents[0],
        None if supported_agents.len() >= 2 => {
            let labels: Vec<String> = supported_agents
                .iter()
                .map(|a| a.slug().to_string())
                .collect();
            if let Some(progress) = steps.progress_mut() {
                let selection = progress.select_choice("Choose launch agent", labels)?;
                supported_agents[selection]
            } else {
                anyhow::bail!(
                    "role \"{}\" supports multiple agents ({:?}); load requires the rich launch dialog for agent selection, or pass --agent / set workspace `default_agent`",
                    selector.key(),
                    supported_agents
                        .iter()
                        .map(|a| a.slug())
                        .collect::<Vec<_>>()
                )
            }
        }
        None if supported_agents.is_empty() => anyhow::bail!(
            "role \"{}\" declares no supported agents in its manifest",
            selector.key()
        ),
        None => anyhow::bail!(
            "role \"{}\" supports multiple agents ({:?}); pass --agent, set workspace `default_agent`, or use the rich launch dialog",
            selector.key(),
            supported_agents
                .iter()
                .map(|a| a.slug())
                .collect::<Vec<_>>()
        ),
    };
    validate_agent_supported(selector, &validated_repo.manifest, agent)?;

    // Branch trust gate: fires even for already-trusted roles because the
    // operator trusted the default branch, not this unreviewed PR branch.
    if let Some(branch) = opts.role_branch.as_deref() {
        let prompt = format!(
            "Role `{selector}` is being loaded from unmerged branch `{branch}`.\n\
             Its Dockerfile and scripts may differ from the trusted main branch.\n\
             Have you reviewed the branch diff and verified it is safe to build?"
        );
        let confirmed = if let Some(progress) = steps.progress_mut() {
            progress.confirm_prompt(prompt)?
        } else {
            confirm_branch_for_test(selector, &source, branch)?;
            true
        };
        if !confirmed {
            anyhow::bail!(
                "branch \"{branch}\" not confirmed — aborting.\n\
                 Review the Dockerfile and scripts on that branch before loading it."
            );
        }
    }

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
            docker,
            steps.progress_mut(),
        )
        .await?
        {
            RestoreResolution::StartFresh => None,
            RestoreResolution::RestoreCurrentRole(container) => Some(container),
            RestoreResolution::RecoverRelatedRole(container) => {
                steps.finish_progress();
                let load_result = hardline_agent(paths, &container, docker, runner)
                    .await
                    .map(|()| container);
                match load_result {
                    Ok(_) => {
                        render_exit(paths, docker).await;
                        return Ok(());
                    }
                    Err(error) => {
                        render_exit(paths, docker).await;
                        return Err(error);
                    }
                }
            }
            RestoreResolution::RebuildRelatedRole(manifest) => {
                steps.finish_progress();
                let selector = RoleSelector::parse(&manifest.role_key)?;
                let related_opts = related_restore_load_options(opts, &manifest)?;
                let load_result = load_role(
                    paths,
                    config,
                    &selector,
                    workspace,
                    docker,
                    runner,
                    &related_opts,
                )
                .await
                .map(|()| manifest.container_base);
                match load_result {
                    Ok(_) => {
                        render_exit(paths, docker).await;
                        return Ok(());
                    }
                    Err(error) => {
                        render_exit(paths, docker).await;
                        return Err(error);
                    }
                }
            }
        }
    };
    let restoring = restore_container.is_some();
    let (container_name, _name_lock) = if let Some(container_name) = restore_container {
        claim_known_container_name(paths, &container_name, docker).await?
    } else {
        claim_container_name(paths, workspace_name.as_deref(), selector, docker).await?
    };

    let image_tag = opts.role_branch.as_deref().map_or_else(
        || image_name(selector),
        |b| image_name_for_branch(selector, b),
    );
    if let Some(progress) = steps.progress_mut() {
        progress.update_identity(super::progress::LaunchIdentity {
            role: agent_display_name.clone(),
            agent: agent.slug().to_string(),
            target_kind: launch_target_kind(workspace_name.as_deref()),
            target_label: launch_target_label(workspace_name.as_deref(), workspace),
            mounts: launch_mount_lines(workspace),
            image: Some(image_tag.clone()),
            container: Some(container_name.clone()),
        });
        progress.stage_done(super::progress::LaunchStage::Role, "trusted source");
    }

    if let Some(progress) = steps.progress_mut() {
        progress.stage_started(
            super::progress::LaunchStage::Credentials,
            "resolving launch credentials",
        );
    }

    // Resolve operator env layers (global / role / workspace /
    // workspace × role) before manifest env. Operator-provided values
    // preseed matching manifest variables, so a configured value does
    // not ask the operator the same question again.
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

    // Resolve env vars (interactive prompts happen here, before build)
    let manifest_resolved = if validated_repo.manifest.env.is_empty() {
        crate::env_resolver::ResolvedEnv { vars: vec![] }
    } else {
        let prompter = LaunchEnvPrompter::new(steps.progress_mut());
        crate::env_resolver::resolve_env_with_overrides(
            &validated_repo.manifest.env,
            &prompter,
            &operator_env,
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
    if let Some(progress) = steps.progress_mut() {
        progress.stage_done(super::progress::LaunchStage::Credentials, "resolved");
    }

    let load_result: anyhow::Result<String> = async {
        // Step 2: Prepare runtime assets and build the derived image.
        let rebuild = opts.rebuild;
        let agent_update = !rebuild && {
            let img = image_name(selector);
            let needs_update = version_check::needs_agent_update(paths, &img, agent).await;
            if needs_update {
                let name = agent.slug();
                if let Some(progress) = steps.progress_mut() {
                    progress.stage_progress(
                        super::progress::LaunchStage::DerivedImage,
                        format!("{name} update available; refreshing agent layer"),
                    );
                }
            }
            needs_update
        };
        if let Some(progress) = steps.progress_mut() {
            progress.stage_started(
                super::progress::LaunchStage::Construct,
                "verifying construct",
            );
            progress.stage_done(super::progress::LaunchStage::Construct, "online");
        }
        steps.next("Preparing runtime binaries").await;
        let runtime_binaries = if let Some(progress) = steps.progress_mut() {
            prepare_runtime_binaries(paths, &validated_repo, Some(progress)).await?
        } else {
            prepare_runtime_binaries(paths, &validated_repo, None).await?
        };
        steps.next("Preparing derived image").await;
        let image = if let Some(progress) = steps.progress_mut() {
            build_agent_image(
                paths,
                selector,
                &cached_repo,
                &validated_repo,
                &host,
                agent,
                runtime_binaries,
                rebuild,
                agent_update,
                opts.debug,
                opts.role_branch.as_deref(),
                docker,
                runner,
                repo_lock,
                Some(progress),
            )
            .await?
        } else {
            build_agent_image(
                paths,
                selector,
                &cached_repo,
                &validated_repo,
                &host,
                agent,
                runtime_binaries,
                rebuild,
                agent_update,
                opts.debug,
                opts.role_branch.as_deref(),
                docker,
                runner,
                repo_lock,
                None,
            )
            .await?
        };

        let container_state = paths.data_dir.join(&container_name);
        let resources = crate::instance::DockerResources::from_container_name(&container_name);
        let network = resources.network.clone();
        let dind = resources.dind_container.clone();
        let certs_volume = resources.certs_volume.clone();
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
        let (state, _auth_outcome) = RoleState::prepare(
            paths,
            &container_name,
            &validated_repo.manifest,
            &resolve_supported_mode,
            &github_ctx,
            &paths.home_dir,
            agent,
        )?;
        seed_codex_project_trust(&state, workspace)?;

        if agent != crate::agent::Agent::Codex {
            let _expiry_days = workspace_name
                .as_deref()
                .filter(|_| auth_mode == crate::config::AuthForwardMode::OAuthToken)
                .and_then(|ws| {
                    match crate::workspace::token_setup::expiry_days_for_launch(paths, ws) {
                        Ok(days) => days,
                        Err(e) => {
                            let message = format!(
                                "[jackin] note: token expiry cache for workspace {ws:?} \
                                 is unreadable ({e}); re-run \
                                 `jackin workspace claude-token setup {ws}` to refresh."
                            );
                            if let Some(run) = crate::diagnostics::active_run() {
                                run.compact("auth", &message);
                            }
                            None
                        }
                    }
                });
        }
        if let Some(run) = crate::diagnostics::active_run() {
            run.compact("auth", &format!("{agent} auth resolved via {auth_mode}"));
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
            if let Some(run) = crate::diagnostics::active_run() {
                run.compact(
                    "github_auth",
                    &format!("resolved GitHub auth from {token_breadcrumb}"),
                );
            }
        }

        // Materialize workspace mounts: shared mounts pass through;
        // worktree-isolated mounts get a per-container `git worktree`
        // staged on the host. Must run AFTER `RoleState::prepare` (so the
        // per-container state directory exists) and BEFORE the docker run
        // command is assembled (so the docker `-v` flags reflect the
        // per-mount bind sources).
        let interactive = true;
        let workspace_label = workspace.label.as_str();
        crate::debug_log!(
            "isolation",
            "load_role: invoking materialize_workspace for container {container_name} (interactive={interactive}, force={force})",
            force = opts.force,
        );
        if let Some(progress) = steps.progress_mut() {
            progress.stage_started(
                super::progress::LaunchStage::Workspace,
                "materializing workspace",
            );
        }
        let materialize_preflight = crate::isolation::materialize::PreflightContext {
            workspace_name: workspace_label.to_string(),
            force: opts.force,
            interactive,
        };
        let materialize = crate::isolation::materialize::materialize_workspace(
            workspace,
            &container_state,
            &role_key,
            &container_name,
            workspace_label,
            &materialize_preflight,
            runner,
        );
        let materialized = if let Some(progress) = steps.progress_mut() {
            progress.while_waiting(materialize).await?
        } else {
            materialize.await?
        };
        if let Some(progress) = steps.progress_mut() {
            progress.stage_done(super::progress::LaunchStage::Workspace, "materialized");
        }

        // Step 3: Create network and start Docker-in-Docker
        steps.next("Starting Docker-in-Docker").await;

        let launch_config = capsule_config(
            selector,
            &workspace.workdir,
            &validated_repo.manifest,
            opts.initial_provider(),
        );
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
            git_coauthor_trailer: config.git.coauthor_trailer,
            git_dco: config.git.dco,
            agent,
            capsule_config: &launch_config,
            resolved_env: &resolved_env,
            github_env: &github_resolved_env,
            paths,
        };
        let socket_dir = paths.jackin_home.join("sockets").join(&container_name);
        let mut cleanup = LoadCleanup::new(
            container_name.clone(),
            dind.clone(),
            certs_volume,
            network.clone(),
            socket_dir,
        );
        let launch_result = launch_role_runtime(&ctx, &mut steps, docker, runner).await;
        if launch_result.is_err() {
            // FailedSetup write error must not abort cleanup; surface to stderr
            // so the operator sees the on-disk status is stale (Active) and
            // that `jackin inspect` / `hardline` may report misleading state.
            if let Err(status_err) = write_instance_status(
                paths,
                &container_state,
                &mut instance_manifest,
                InstanceStatus::FailedSetup,
            ) {
                let message = format!(
                    "jackin: warning: failed to mark FailedSetup for {container_name} \
                     after launch error: {status_err:#}; on-disk status may be stale"
                );
                if let Some(run) = crate::diagnostics::active_run() {
                    run.compact("status", &message);
                }
            }
            cleanup.run(docker).await;
        }
        launch_result?;
        // Launch succeeded. From here on the cleanup struct is reused
        // to tear down docker resources at session end (clean exit,
        // crash, NotFound, etc.); the host-side socket dir + Capsule
        // launch config stay behind for operator inspection and get
        // swept by the next explicit `jackin eject` / Purge.
        cleanup.keep_socket_dir();
        write_instance_status(
            paths,
            &container_state,
            &mut instance_manifest,
            InstanceStatus::Running,
        )?;

        // Finalize per-mount isolation worktrees BEFORE the container teardown
        // decision below: clean exits without dirty/unpushed state get their
        // worktrees swept; dirty state is preserved through the rich cleanup
        // dialog. A `ReturnToAgent` choice restarts + re-attaches the container
        // exactly once so the operator can address the dirty state inside the
        // role, then the safe cleanup is retried.
        let interactive_finalize = true;
        let mut prompt = crate::isolation::finalize::RichCleanupPrompt;
        let outcome = inspect_attach_outcome(docker, &container_name).await?;
        write_instance_attach_outcome(paths, &container_state, &mut instance_manifest, outcome)?;
        let mut decision = crate::isolation::finalize::finalize_foreground_session(
            &container_name,
            &paths.data_dir.join(&container_name),
            outcome,
            interactive_finalize,
            &mut prompt,
            docker,
            runner,
        ).await?;
        write_preserved_status_if_applicable(
            decision,
            paths,
            &container_state,
            &mut instance_manifest,
        )?;
        if matches!(
            decision,
            crate::isolation::finalize::FinalizeDecision::ReturnToAgent
        ) {
            // Restart detached, then attach through the jackin-capsule client
            // socket. Attaching `docker start -ai` to PID 1 would only show
            // daemon logs, not the multiplexer UI the operator needs to fix
            // the preserved worktree. We do not loop further: if the operator
            // still leaves dirty state, the second pass will fall back to
            // Preserved and exit normally.
            start_or_reconnect_capsule_client(paths, &container_name, docker, runner).await?;
            let outcome2 = inspect_attach_outcome(docker, &container_name).await?;
            write_instance_attach_outcome(
                paths,
                &container_state,
                &mut instance_manifest,
                outcome2,
            )?;
            decision = crate::isolation::finalize::finalize_foreground_session(
                &container_name,
                &paths.data_dir.join(&container_name),
                outcome2,
                interactive_finalize,
                &mut prompt,
                docker,
                runner,
            ).await?;
            write_preserved_status_if_applicable(
                decision,
                paths,
                &container_state,
                &mut instance_manifest,
            )?;
        }

        // Classify how the interactive session ended and tear down DinD/network
        // unless the container is still running with active sessions (detach):
        //  - Running + active sessions → user detached (Ctrl-B D). Keep DinD so
        //                               `jackin hardline` can reconnect.
        //  - Running + no sessions → agent exited; Capsule cleanup lag or stale socket.
        //                            Tear down same as Stopped/0 regardless of
        //                            preserved isolation state — worktrees live on
        //                            the host and are accessible without DinD.
        //  - Stopped / 0 → user exited cleanly. Tear down.
        //  - Stopped / ≠0 or OOM-killed → crash. Tear down; DinD is no longer
        //                                  needed once the container has exited.
        //  - NotFound + Preserved → removed externally during finalization.
        //                           Tear down DinD/network; status on disk stands.
        //  - NotFound → removed externally. Tear down.
        //  - InspectUnavailable → Docker unreachable; keep everything alive.
        let is_preserved = matches!(
            decision,
            crate::isolation::finalize::FinalizeDecision::Preserved
        );
        #[allow(clippy::match_same_arms)]
        match docker.inspect_container_state(&container_name).await {
            ContainerState::Running | ContainerState::Paused | ContainerState::Restarting => {
                if is_preserved {
                    // Finalize saw sessions at check-time (detach). Re-check: sessions
                    // may have ended in the interval between finalize and this inspect.
                    let sessions =
                        inspect_agent_sessions(docker, &container_name, &ContainerState::Running).await;
                    if let AgentSessionInventory::Unavailable(ref reason) = sessions {
                        crate::debug_log!(
                            "instance",
                            "inspect_agent_sessions unavailable for {container_name}: {reason}; \
                             treating conservatively as sessions-present (container preserved)",
                        );
                    }
                    let no_sessions =
                        matches!(&sessions, AgentSessionInventory::Sessions(v) if v.is_empty());
                    if no_sessions {
                        write_instance_status(
                            paths,
                            &container_state,
                            &mut instance_manifest,
                            InstanceStatus::CleanExited,
                        )?;
                        cleanup.run(docker).await;
                    } else {
                        cleanup.disarm();
                    }
                } else {
                    // Finalize already confirmed no sessions (Capsule still running after
                    // clean exit). Skip the redundant re-query and tear down.
                    write_instance_status(
                        paths,
                        &container_state,
                        &mut instance_manifest,
                        InstanceStatus::CleanExited,
                    )?;
                    cleanup.run(docker).await;
                }
            }
            ContainerState::Stopped {
                exit_code: 0,
                oom_killed: false,
            } if is_preserved => {
                cleanup.run(docker).await;
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
                cleanup.run(docker).await;
            }
            ContainerState::Stopped { .. }
            | ContainerState::Created
            | ContainerState::Removing
            | ContainerState::Dead => {
                write_instance_status(
                    paths,
                    &container_state,
                    &mut instance_manifest,
                    InstanceStatus::Crashed,
                )?;
                cleanup.run(docker).await;
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
            ContainerState::NotFound if is_preserved => {
                crate::debug_log!(
                    "instance",
                    "container {container_name} not found after session with Preserved decision; \
                     removed externally during finalization — tearing down DinD/network, \
                     preserved status on disk stands",
                );
                cleanup.run(docker).await;
            }
            ContainerState::NotFound => {
                write_instance_status(
                    paths,
                    &container_state,
                    &mut instance_manifest,
                    InstanceStatus::CleanExited,
                )?;
                cleanup.run(docker).await;
            }
        }

        Ok(container_name)
    }.await;

    match load_result {
        Ok(_) => {
            render_exit(paths, docker).await;
            Ok(())
        }
        Err(error) => {
            let failed_stage = steps
                .current_stage
                .unwrap_or(super::progress::LaunchStage::Capsule);
            let run = crate::diagnostics::active_run();
            let final_error = launch_failure_cli_error(failed_stage, &error, run.as_deref());
            if let Some(progress) = steps.progress_mut() {
                progress
                    .stage_failed(super::progress::LaunchFailure {
                        title: launch_failure_title(failed_stage, &error, run.as_deref()),
                        summary: short_launch_diagnosis(failed_stage, &error, run.as_deref()),
                        detail: Some(format!("{error:#}")),
                        next_step: None,
                        stage: failed_stage,
                        diagnostics_path: None,
                        command_output_path: None,
                    })
                    .await;
            }
            // Stop the cockpit render task and release the rich surface before
            // the exit warp writes to the terminal. A pre-attach failure returns
            // before the success path's pre-handoff teardown runs, so without
            // this the background task keeps drawing frames over the warp.
            steps.finish_progress();
            render_exit(paths, docker).await;
            Err(final_error)
        }
    }
}

fn launch_failure_title(
    stage: super::progress::LaunchStage,
    error: &anyhow::Error,
    run: Option<&crate::diagnostics::RunDiagnostics>,
) -> String {
    if stage == super::progress::LaunchStage::DerivedImage
        && run.and_then(docker_build_output_artifact).is_some()
    {
        return "Docker build failed".to_string();
    }
    let text = error.to_string().to_ascii_lowercase();
    if text.contains("docker") {
        "Docker unavailable".to_string()
    } else if text.contains("credential") || text.contains("token") || text.contains("auth") {
        "Credential check failed".to_string()
    } else {
        "Launch failed".to_string()
    }
}

fn short_launch_diagnosis(
    stage: super::progress::LaunchStage,
    error: &anyhow::Error,
    run: Option<&crate::diagnostics::RunDiagnostics>,
) -> String {
    if stage == super::progress::LaunchStage::DerivedImage
        && run.and_then(docker_build_output_artifact).is_some()
    {
        return "Building the Docker container failed.".to_string();
    }
    error.chain().next().map_or_else(
        || "launch did not complete".to_string(),
        ToString::to_string,
    )
}

fn docker_build_output_artifact(run: &crate::diagnostics::RunDiagnostics) -> Option<PathBuf> {
    let docker_output = run.command_output_path("docker-build");
    docker_output.exists().then_some(docker_output)
}

fn launch_failure_cli_error(
    stage: super::progress::LaunchStage,
    error: &anyhow::Error,
    run: Option<&crate::diagnostics::RunDiagnostics>,
) -> anyhow::Error {
    if stage != super::progress::LaunchStage::DerivedImage {
        return anyhow::anyhow!("{error:#}");
    }
    let Some(run) = run else {
        return anyhow::anyhow!("{error:#}");
    };
    let Some(docker_output) = docker_build_output_artifact(run) else {
        return anyhow::anyhow!("{error:#}");
    };
    let mut report = String::from("Docker build command failed");
    let mut table = tabled::Table::builder([
        ["run id", run.run_id()],
        ["run diagnostics", &run.path().display().to_string()],
        ["docker output", &docker_output.display().to_string()],
    ])
    .build();
    table
        .with(tabled::settings::Style::modern_rounded())
        .with(tabled::settings::Remove::row(
            tabled::settings::object::Rows::first(),
        ));
    report.push_str("\n\n");
    report.push_str(&table.to_string());
    anyhow::anyhow!("{report}")
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

async fn render_exit(paths: &JackinPaths, docker: &impl DockerApi) {
    let force_outro = super::universe::force_boundary_outro_enabled();
    let running = match list_running_agent_names(docker).await {
        Ok(names) => names,
        Err(e) => {
            if let Some(run) = crate::diagnostics::active_run() {
                run.compact(
                    "exit_summary",
                    &format!("skipping boundary outro; running-container list failed: {e:#}"),
                );
            }
            return;
        }
    };

    if !running.is_empty() {
        if let Some(run) = crate::diagnostics::active_run() {
            let index = InstanceIndex::read_or_rebuild(&paths.data_dir).unwrap_or(InstanceIndex {
                version: 0,
                instances: Vec::new(),
            });
            let (headline, rows) = super::exit_summary::summary(&running, &index);
            run.compact(
                "exit_summary",
                &format!("{headline}; boundary outro skipped"),
            );
            for row in rows {
                run.compact("exit_summary", &row);
            }
        }
        if !force_outro {
            return;
        }
    }

    // Last container left the construct: clear the session marker and show the
    // two-screen outro (decelerating warp, then closing caption). Exits that
    // leave other instances running skip this entirely because the operator is
    // still inside the Construct.
    let elapsed = if force_outro && !running.is_empty() {
        None
    } else {
        match super::universe::take_exit_claim(paths) {
            ExitClaim::Claimed { elapsed } => elapsed,
            ExitClaim::Missing if force_outro => None,
            ExitClaim::Missing => return,
        }
    };
    if !super::progress::rich_terminal_supported() {
        return;
    }
    // Defensive: the attach paths already re-assert the alt screen the moment
    // the capsule exec returns, so the post-attach work never flashes the
    // shell. Re-assert once more before the rich outro in case render_exit is
    // reached by a path that did not go through the attach.
    crate::tui::reassert_alt_screen();
    tui::warp_out();
    tui::warp_end_caption(elapsed);
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum RestoreResolution {
    StartFresh,
    RestoreCurrentRole(String),
    RecoverRelatedRole(String),
    RebuildRelatedRole(Box<InstanceManifest>),
}

#[allow(clippy::too_many_arguments)]
async fn resolve_restore_candidate(
    paths: &JackinPaths,
    workspace_name: Option<&str>,
    workspace_label: &str,
    workdir: &str,
    role_key: &str,
    agent: crate::agent::Agent,
    docker: &impl DockerApi,
    progress: Option<&mut super::progress::LaunchProgress>,
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
        let docker_state = docker
            .inspect_container_state(&manifest.container_base)
            .await;
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
        docker,
    )
    .await?;

    if candidates.is_empty() && related.is_empty() {
        return Ok(RestoreResolution::StartFresh);
    }

    // One dialog for every stale-state decision — same-role candidates and
    // related-role candidates alike — so the operator sees a single rich
    // forced-choice picker inside the TUI.
    present_restore_choice(
        progress,
        paths,
        workspace_label,
        role_key,
        candidates,
        &related,
    )
}

/// Present the stale-instance decision. "Start fresh" is always the
/// default first option; recoverable instances follow. The rich launch
/// surface renders it as a forced-choice picker (no cancel). The operator
/// must pick.
fn present_restore_choice(
    progress: Option<&mut super::progress::LaunchProgress>,
    paths: &JackinPaths,
    workspace_label: &str,
    role_key: &str,
    candidates: Vec<InstanceManifest>,
    related: &[RelatedRestoreCandidate],
) -> anyhow::Result<RestoreResolution> {
    let mut labels = vec!["Start fresh instance".to_string()];
    labels.extend(
        candidates
            .iter()
            .map(|manifest| restore_candidate_label(paths, manifest)),
    );
    labels.extend(related.iter().map(|candidate| {
        format!(
            "Recover other role with hardline {}",
            related_restore_candidate_label(paths, candidate)
        )
    }));

    let Some(progress) = progress else {
        let hint = candidates.first().map_or_else(
            || format!("role `{role_key}`"),
            |manifest| format!("`jackin hardline {}`", manifest.container_base),
        );
        anyhow::bail!(
            "unfinished jackin instances exist for workspace `{workspace_label}` and role `{role_key}` but the rich launch dialog is unavailable; run {hint} to inspect or recover, or purge stale instances before a fresh load"
        );
    };
    let choice = progress.select_choice("Unfinished jackin instances", labels)?;

    if choice == 0 {
        supersede_restore_candidates(paths, candidates)?;
        Ok(RestoreResolution::StartFresh)
    } else if choice <= candidates.len() {
        Ok(RestoreResolution::RestoreCurrentRole(
            candidates[choice - 1].container_base.clone(),
        ))
    } else {
        recover_related_restore_candidate(&related[choice - 1 - candidates.len()])
    }
}

#[derive(Debug)]
struct RelatedRestoreCandidate {
    manifest: InstanceManifest,
    docker_state: ContainerState,
}

async fn related_restore_candidates(
    paths: &JackinPaths,
    workspace_name: Option<&str>,
    workspace_label: &str,
    workdir: &str,
    role_key: &str,
    agent: crate::agent::Agent,
    docker: &impl DockerApi,
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
        let docker_state = docker
            .inspect_container_state(&manifest.container_base)
            .await;
        let should_prompt = match docker_state {
            ContainerState::InspectUnavailable(_) | ContainerState::NotFound => true,
            ContainerState::Running
            | ContainerState::Paused
            | ContainerState::Restarting
            | ContainerState::Stopped { .. }
            | ContainerState::Created
            | ContainerState::Removing
            | ContainerState::Dead => false,
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

fn recover_related_restore_candidate(
    candidate: &RelatedRestoreCandidate,
) -> anyhow::Result<RestoreResolution> {
    match candidate.docker_state {
        ContainerState::Running
        | ContainerState::Paused
        | ContainerState::Restarting
        | ContainerState::Stopped { .. } => Ok(RestoreResolution::RecoverRelatedRole(
            candidate.manifest.container_base.clone(),
        )),
        ContainerState::NotFound
        | ContainerState::Created
        | ContainerState::Removing
        | ContainerState::Dead => Ok(RestoreResolution::RebuildRelatedRole(Box::new(
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

pub(super) fn write_instance_status(
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
    if matches!(
        outcome,
        crate::isolation::finalize::AttachOutcome::StillRunning
    ) {
        manifest.mark_status(InstanceStatus::Running);
    } else {
        manifest.touch();
    }
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
    use crate::isolation::finalize::AttachOutcome;
    match outcome {
        AttachOutcome::OomKilled => "oom_killed".to_string(),
        AttachOutcome::StillRunning => "running".to_string(),
        AttachOutcome::Stopped(code) => format!("exit:{code}"),
    }
}

/// Persist `Preserved`-tier status when `finalize_foreground_session`
/// decides to keep the isolation state. No-op for any other decision;
/// both the first finalize pass and the post-restart retry pass call
/// this so a future field added under the `Preserved` arm cannot drift
/// between them.
fn write_preserved_status_if_applicable(
    decision: crate::isolation::finalize::FinalizeDecision,
    paths: &JackinPaths,
    state_dir: &std::path::Path,
    manifest: &mut InstanceManifest,
) -> anyhow::Result<()> {
    if !matches!(
        decision,
        crate::isolation::finalize::FinalizeDecision::Preserved
    ) {
        return Ok(());
    }
    let status = preserved_instance_status(state_dir)?;
    write_instance_status(paths, state_dir, manifest, status)
}

pub(super) fn preserved_instance_status(
    state_dir: &std::path::Path,
) -> anyhow::Result<InstanceStatus> {
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
async fn claim_container_name(
    paths: &JackinPaths,
    workspace_name: Option<&str>,
    selector: &RoleSelector,
    docker: &impl DockerApi,
) -> anyhow::Result<(String, std::fs::File)> {
    std::fs::create_dir_all(&paths.data_dir)?;

    let mut last_lock_err: Option<std::io::Error> = None;
    let mut last_unlink_err: Option<std::io::Error> = None;
    let mut occupied_attempts = 0u32;

    for attempt in 0..CLAIM_MAX_ATTEMPTS {
        let name = crate::instance::new_container_name(workspace_name, selector);

        let slot_free = match docker.inspect_container_state(&name).await {
            ContainerState::Stopped {
                exit_code: 0,
                oom_killed: false,
            } => match docker.remove_container(&name).await {
                Ok(()) => true,
                Err(error) => {
                    return Err(error.context(format!(
                        "removing stale container `{name}` before reclaiming its name"
                    )));
                }
            },
            ContainerState::Running
            | ContainerState::Paused
            | ContainerState::Restarting
            | ContainerState::Stopped { .. }
            | ContainerState::Created
            | ContainerState::Removing
            | ContainerState::Dead => false,
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

async fn claim_known_container_name(
    paths: &JackinPaths,
    container_name: &str,
    docker: &impl DockerApi,
) -> anyhow::Result<(String, std::fs::File)> {
    match docker.inspect_container_state(container_name).await {
        ContainerState::NotFound => {}
        ContainerState::Running
        | ContainerState::Paused
        | ContainerState::Restarting
        | ContainerState::Stopped { .. }
        | ContainerState::Created
        | ContainerState::Removing
        | ContainerState::Dead => {
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
pub(super) enum EnvLayerState {
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
pub(super) enum LaunchError {
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
pub(super) fn verify_credential_env_present(
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
    crate::config::resolve_mode_with_trace(cfg, agent, workspace, role).1
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

/// Printable source reference for the credential env var `env_var` (e.g.
/// `"CLAUDE_CODE_OAUTH_TOKEN"`, `"ANTHROPIC_API_KEY"`) given the raw
/// (unresolved) declaration value from the operator env config (e.g.
/// `"Private/Claude/security/auth token"` or `"$CLAUDE_CODE_OAUTH_TOKEN"`).
/// Produces the `"KEY ← value"` form; falls back to the bare env-var name
/// when `raw` is `None` or empty.
fn auth_token_source_reference(env_var: &str, raw: Option<&str>) -> String {
    match raw {
        None | Some("") => env_var.to_string(),
        Some(value) => format!("{env_var} \u{2190} {value}"),
    }
}

struct LoadCleanup {
    container_name: String,
    dind: String,
    certs_volume: String,
    network: String,
    /// Host-side bind-mount dir (`~/.jackin/sockets/<container>/`).
    /// Removed only when `armed` is true AND the cleanup fires on the
    /// launch-failure path — `clean_socket_dir` distinguishes that from
    /// post-session teardown where the operator may still want to
    /// inspect the just-written Capsule launch config. Post-session
    /// teardown paths flip `clean_socket_dir = false` before
    /// `cleanup.run()` (or call `disarm`); explicit cleanup commands
    /// (`jackin eject`, Purge from the console) sweep the directory via
    /// `cleanup::eject_role` / `purge_container_filesystem`.
    socket_dir: PathBuf,
    clean_socket_dir: bool,
    armed: bool,
}

impl LoadCleanup {
    const fn new(
        container_name: String,
        dind: String,
        certs_volume: String,
        network: String,
        socket_dir: PathBuf,
    ) -> Self {
        Self {
            container_name,
            dind,
            certs_volume,
            network,
            socket_dir,
            clean_socket_dir: true,
            armed: true,
        }
    }

    const fn disarm(&mut self) {
        self.armed = false;
    }

    /// Switch off socket-dir cleanup for post-session teardown.
    /// docker-resource removal still runs (`cleanup.run` is reused for
    /// "session ended cleanly, tear down DinD/network/volume"); the
    /// host-side bind-mount dir is left for the operator to inspect
    /// and gets reaped by the next explicit eject / purge.
    const fn keep_socket_dir(&mut self) {
        self.clean_socket_dir = false;
    }

    async fn run(&self, docker: &impl DockerApi) {
        if !self.armed {
            return;
        }

        if let Err(e) = docker.remove_container(&self.container_name).await {
            tui::step_fail(&format!("cleanup failed (container): {e}"));
        }
        if let Err(e) = docker.remove_container(&self.dind).await {
            tui::step_fail(&format!("cleanup failed (dind): {e}"));
        }
        if let Err(e) = docker.remove_volume(&self.certs_volume).await {
            tui::step_fail(&format!("cleanup failed (certs volume): {e}"));
        }
        if let Err(e) = docker.remove_network(&self.network).await {
            tui::step_fail(&format!("cleanup failed (network): {e}"));
        }
        if self.clean_socket_dir {
            match std::fs::remove_dir_all(&self.socket_dir) {
                Ok(()) => {}
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                Err(error) => tui::step_fail(&format!(
                    "cleanup failed (socket dir {}): {error}",
                    self.socket_dir.display()
                )),
            }
        }
    }
}

#[cfg(test)]
mod tests;
