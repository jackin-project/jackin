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

#![expect(
    clippy::print_stderr,
    reason = "launch flow emits operator-visible pull and spacing diagnostics"
)]

mod launch_dind;
pub use launch_dind::DIND_IMAGE;
pub use launch_dind::{
    DindSidecarPrewarm, prewarm_dind_sidecar_container, write_prewarmed_dind_state,
};
use launch_dind::{adopt_prewarmed_dind_sidecar, run_dind_sidecar_headless};

mod launch_slot;
#[cfg(test)]
pub(crate) use launch_slot::{
    claim_container_name, resolve_github_env_map, verify_credential_env_present,
    verify_github_token_present,
};

mod trust;
#[cfg(test)]
pub(crate) use trust::{
    MISE_TRUSTED_CONFIG_PATHS_ENV, inject_workspace_mise_env, seed_codex_project_trust,
    workspace_mise_trusted_config_paths,
};

mod launch_pipeline;
#[cfg(test)]
pub(crate) use crate::instance::{DockerResources, NewInstanceManifest};
#[cfg(test)]
pub(crate) use launch_pipeline::emit_auth_provision_launch_plan;
#[cfg(test)]
pub(crate) use launch_pipeline::load_role_with;
#[cfg(test)]
pub(crate) use launch_pipeline::manifest_env_timing_detail;
pub use launch_pipeline::{load_role, resolve_supported_agents_for_console};

#[cfg(test)]
use crate::instance::InstanceStatus;
use crate::instance::{InstanceIndex, InstanceManifest, PrepareResolvers, RoleState};
use anyhow::Context;
use jackin_config::AppConfig;
use jackin_core::paths::JackinPaths;
use jackin_core::selector::RoleSelector;
use jackin_core::{CommandRunner, RunOptions};
use std::path::{Path, PathBuf};

use super::attach::{ContainerState, reconnect_or_create_session_with_focus};
use super::discovery::list_running_agent_names;
use super::identity::GitIdentity;
use super::naming::{LABEL_KEEP_AWAKE, LABEL_KIND_ROLE, LABEL_MANAGED, dind_certs_volume};
use super::universe::ExitClaim;
use jackin_docker::docker_client::DockerApi;

#[expect(
    missing_debug_implementations,
    reason = "LoadOptions contains an injected OpRunner trait object that cannot expose Debug."
)]
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
    pub op_runner: Option<Box<dyn jackin_env::OpRunner>>,

    /// Optional test seam: inject a host-env lookup map. `None` (the
    /// production default) means `resolve_operator_env` reads from
    /// `std::env::var`. When `Some(map)`, `$NAME` / `${NAME}`
    /// references are resolved by looking up `name` in `map`.
    pub host_env: Option<std::collections::BTreeMap<String, String>>,

    /// CLI override for the agent. `None` defers to (in order) workspace
    /// `default_agent`, the role's single supported agent, or a rich launch
    /// dialog. A launch against a multi-agent role with no resolved choice is
    /// an error when the rich dialog is unavailable.
    pub agent: Option<jackin_core::agent::Agent>,

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

    /// Test seam for workspace `git pull` so fast-restore tests can prove the
    /// pull path did not run without mutating process-wide PATH.
    #[cfg(test)]
    pub git_program: Option<PathBuf>,
}

impl LoadOptions {
    pub fn initial_provider(&self) -> Option<jackin_protocol::InitialProvider> {
        // Label only: the daemon re-derives the env redirection from it and
        // backfills the token from the container's provider key env var.
        self.provider
            .map(|provider| jackin_protocol::InitialProvider {
                label: provider.label().to_owned(),
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
pub(super) fn validate_agent_supported(
    selector: &RoleSelector,
    manifest: &jackin_manifest::RoleManifest,
    agent: jackin_core::agent::Agent,
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

mod progress_helpers;
use progress_helpers::{
    LaunchEnvPrompter, StepCounter, launch_mount_lines, launch_target_kind, launch_target_label,
    sensitive_mount_prompt,
};

/// Returns the per-agent mount strings in jackin's `src:dst[:ro]`
/// idiom for `docker run -v`.
///
/// Every provisioned agent is represented on `state.auth`, so the mount block
/// checks `auth.*` flags rather than matching the selected-agent variant. The
/// foreground launch path provisions all manifest-supported agents so sibling
/// tabs opened via `hardline --new --agent <other>` find their homes
/// bind-mounted from the start.
/// Read-only bind-mount specs (`host:container:ro`) for every agent CLI binary
/// cached on the host. The agent binaries are mounted at `docker run` instead of
/// baked into the derived image, so an agent version bump no longer rebuilds the
/// image — the newest cached binary is mounted onto the PATH location the image's
/// `ENV PATH` already covers. Agents with no cached binary are skipped.
async fn agent_binary_mount_specs(paths: &JackinPaths, supported: &[String]) -> Vec<String> {
    // Resolve straight from the authoritative supported-slug list (`FromStr`
    // surfaces a junk slug as a skip, vs. scanning every `Agent::ALL` and string
    // comparing). The per-agent cache lookup is blocking filesystem IO, so run the
    // whole resolution off the async reactor.
    let paths = paths.clone();
    let agents: Vec<jackin_core::Agent> = supported.iter().filter_map(|s| s.parse().ok()).collect();
    tokio::task::spawn_blocking(move || {
        agents
            .into_iter()
            .filter_map(|agent| {
                let host = jackin_image::agent_binary::runtime_mount_binary_path(&paths, agent)?
                    .to_str()?
                    .to_owned();
                Some((agent, host))
            })
            .flat_map(|(agent, host)| {
                agent
                    .runtime()
                    .container_binary_paths()
                    .iter()
                    .map(move |path| format!("{host}:{path}:ro"))
            })
            .collect()
    })
    .await
    .unwrap_or_default()
}

fn agent_mounts(state: &RoleState) -> Vec<String> {
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

    if let Some(grok) = &state.auth.grok {
        mounts.push(format!(
            "{}:/home/agent/.grok",
            state.root.join("home/.grok").display()
        ));
        if let Some(auth_json) = &grok.auth_json {
            mounts.push(format!("{}:/jackin/grok/auth.json", auth_json.display()));
        }
    }

    mounts
}

fn github_config_mount(state: &RoleState) -> Option<String> {
    if matches!(
        state.gh_provision_outcome,
        crate::instance::GithubProvisionOutcome::Skipped
    ) && !state.gh_config_dir.exists()
    {
        None
    } else {
        Some(format!(
            "{}:/home/agent/.config/gh",
            state.gh_config_dir.display()
        ))
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

pub(super) struct LaunchContext<'a> {
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
    agent: jackin_core::agent::Agent,
    capsule_config: &'a jackin_protocol::CapsuleConfig,
    resolved_env: &'a jackin_env::ResolvedEnv,
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
    selected_image_refresh: Option<SelectedImageRefresh<'a>>,
    sibling_prewarm: SiblingPrewarm<'a>,
    sibling_auth_prewarm: SiblingAuthPrewarm<'a>,
}

pub(super) struct SelectedImageRefresh<'a> {
    role_git: &'a str,
    branch_override: Option<&'a str>,
    reason: crate::runtime::image::ImageInvalidationReason,
}

pub(super) struct SiblingPrewarm<'a> {
    role_git: &'a str,
    branch_override: Option<&'a str>,
    validated_repo: &'a jackin_manifest::repo::ValidatedRoleRepo,
    selected_image_reused: bool,
}

pub(super) struct SiblingAuthPrewarm<'a> {
    manifest: &'a jackin_manifest::RoleManifest,
    config: &'a AppConfig,
    workspace_name: &'a str,
    role_key: &'a str,
}

fn spawn_sibling_auth_prewarm(
    paths: &JackinPaths,
    container_name: &str,
    prewarm: &SiblingAuthPrewarm<'_>,
    selected_agent: jackin_core::agent::Agent,
) {
    let sibling_agents = prewarm
        .manifest
        .supported_agents()
        .into_iter()
        .filter(|agent| *agent != selected_agent)
        .collect::<Vec<_>>();
    if sibling_agents.is_empty() {
        if let Some(run) = jackin_diagnostics::active_run() {
            run.compact(
                "sibling_auth_prewarm_skipped",
                &format!("no sibling agents for selected agent {selected_agent}"),
            );
        }
        return;
    }

    let paths_owned = paths.clone();
    let home_dir = paths.home_dir.clone();
    let container_name = container_name.to_owned();
    let manifest = prewarm.manifest.clone();
    let config = prewarm.config.clone();
    let workspace_name = prewarm.workspace_name.to_owned();
    let role_key = prewarm.role_key.to_owned();
    let agents = sibling_agents
        .iter()
        .map(ToString::to_string)
        .collect::<Vec<_>>();
    if let Some(run) = jackin_diagnostics::active_run() {
        run.compact(
            "sibling_auth_prewarm_started",
            &format!(
                "prewarming {} sibling auth slots for selected agent {selected_agent}: {}",
                sibling_agents.len(),
                agents.join(", ")
            ),
        );
    }
    let timing_detail = agents.join(",");
    emit_prewarm_launch_plan(&format!("sibling_auth_prewarm:{timing_detail}"));
    jackin_diagnostics::active_timing_started(
        "credentials",
        "sibling_auth_prewarm",
        Some(&timing_detail),
    );

    tokio::task::spawn_blocking(move || {
        let resolve_mode = |a: jackin_core::agent::Agent| {
            jackin_config::resolve_mode(&config, a, &workspace_name, &role_key)
        };
        let resolve_sync_src = |a: jackin_core::agent::Agent| {
            jackin_config::resolve_sync_source_dir(&config, a, &workspace_name, &role_key)
        };
        let result = RoleState::prewarm_auth_for_agents(
            &paths_owned,
            &container_name,
            &manifest,
            &PrepareResolvers {
                auth_modes: &resolve_mode,
                sync_source_dirs: &resolve_sync_src,
            },
            &home_dir,
            &sibling_agents,
        );
        let timing_done = match &result {
            Ok(count) => format!("{count} slots"),
            Err(error) => format!("failed: {error}"),
        };
        jackin_diagnostics::active_timing_done(
            "credentials",
            "sibling_auth_prewarm",
            Some(&timing_done),
        );

        if let Some(run) = jackin_diagnostics::active_run() {
            match result {
                Ok(count) => run.compact(
                    "sibling_auth_prewarm_done",
                    &format!(
                        "prewarmed {count} sibling auth slots for selected agent {selected_agent}"
                    ),
                ),
                Err(error) => run.compact(
                    "sibling_auth_prewarm_failed",
                    &format!(
                        "sibling auth prewarm failed for selected agent {selected_agent}: {error}"
                    ),
                ),
            }
        }
    });
}

pub(super) fn capsule_config(
    selector: &RoleSelector,
    workdir: &str,
    manifest: &jackin_manifest::RoleManifest,
    initial_provider: Option<jackin_protocol::InitialProvider>,
) -> jackin_protocol::CapsuleConfig {
    let mut agents = Vec::new();
    let mut models = std::collections::BTreeMap::new();
    let mut provider_models = std::collections::BTreeMap::new();
    for agent in manifest.supported_agents() {
        agents.push(agent.slug().to_owned());
        let model = manifest.agent_model(agent);
        if let Some(model) = model {
            models.insert(agent.slug().to_owned(), model.to_owned());
        }
        let per_provider = manifest.agent_provider_models(agent);
        if !per_provider.is_empty() {
            let inner = per_provider
                .into_iter()
                .map(|(id, model)| (id.to_owned(), model.to_owned()))
                .collect();
            provider_models.insert(agent.slug().to_owned(), inner);
        }
    }
    let (claude_marketplaces, claude_plugins) = manifest.claude.as_ref().map_or_else(
        || (Vec::new(), Vec::new()),
        |claude| {
            let marketplaces = claude
                .marketplaces
                .iter()
                .map(|m| jackin_protocol::ClaudeMarketplace {
                    source: m.source.clone(),
                    sparse: m.sparse.clone(),
                })
                .collect();
            (marketplaces, claude.plugins.clone())
        },
    );
    jackin_protocol::CapsuleConfig {
        role: selector.key(),
        workdir: workdir.to_owned(),
        agents,
        models,
        provider_models,
        initial_provider,
        claude_marketplaces,
        claude_plugins,
        // Populated by the launch pipeline once the operator env is known; the
        // manifest alone does not carry on-demand workspace credentials.
        exec_bindings: Vec::new(),
    }
}

/// Comma-join the on-demand credential binding names for the
/// `JACKIN_EXEC_BINDINGS` env var. Shared by the Docker and apple-container
/// launch paths so the two cannot format the list differently.
#[must_use]
pub(super) fn exec_binding_names(bindings: &[jackin_protocol::ExecBinding]) -> String {
    bindings
        .iter()
        .map(|b| b.name.as_str())
        .collect::<Vec<_>>()
        .join(",")
}

/// Create the per-container socket dir and write Capsule's launch config
/// (`agent.toml`) into it. The dir is bind-mounted to `/jackin/run`, so the
/// in-container capsule reads `agent.toml` at startup and the host.sock
/// credential-resolver socket lands beside it. Used by the apple-container
/// launch path; the Docker path inlines an equivalent write alongside its
/// extrausers passwd setup. Directory permissions are locked to `0o700` by the
/// `exec_host` listener when it binds the socket.
pub(super) fn prepare_socket_dir(
    socket_dir: &Path,
    capsule_config: &jackin_protocol::CapsuleConfig,
) -> std::io::Result<()> {
    std::fs::create_dir_all(socket_dir)?;
    let contents = toml::to_string(capsule_config).map_err(std::io::Error::other)?;
    std::fs::write(
        socket_dir.join(jackin_protocol::CAPSULE_CONFIG_FILENAME),
        contents,
    )
}

/// Launch the role container after the caller has prepared the private network
/// and `DinD` sidecar.
#[expect(
    clippy::too_many_lines,
    reason = "pending extraction — tracked in codebase-readability roadmap"
)]
pub(super) async fn launch_role_runtime(
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
        selected_image_refresh,
        sibling_prewarm,
        sibling_auth_prewarm,
    } = ctx;

    let certs_volume = dind_certs_volume(container_name);

    let docker_run_opts = RunOptions {
        quiet: !debug,
        ..RunOptions::default()
    };

    // Step 4: Mount volumes and launch
    steps.next("Launching role").await?;
    steps.done();

    if steps.progress.is_none() {
        jackin_tui::output::print_deploying(agent_display_name).await;
    }

    let class_label = format!("jackin.class={}", selector.key());
    let display_label = format!("jackin.display.name={agent_display_name}");
    let docker_host = format!("DOCKER_HOST=tcp://{dind}:2376");
    let dind_hostname = format!(
        "{}={dind}",
        jackin_core::env_model::JACKIN_DIND_HOSTNAME_ENV_NAME
    );
    let role_container_name_env = format!(
        "{}={container_name}",
        jackin_core::env_model::JACKIN_CONTAINER_NAME_ENV_NAME
    );
    let instance_id = if let Some(id) =
        crate::instance::naming::instance_id_from_container_base(container_name)
    {
        id
    } else {
        jackin_diagnostics::emit_compact_line(
            "warning",
            &format!(
                "warning: instance_id_from_container_base could not parse {container_name:?}; falling back to full container name as JACKIN_INSTANCE_ID — chrome chip will render the full name"
            ),
        );
        container_name
    };
    let instance_id_env = format!(
        "{}={instance_id}",
        jackin_core::env_model::JACKIN_INSTANCE_ID_ENV_NAME
    );
    let testcontainers_host_override = format!(
        "{}={dind}",
        jackin_core::env_model::TESTCONTAINERS_HOST_OVERRIDE_ENV_NAME
    );
    let git_author_name = format!("GIT_AUTHOR_NAME={}", git.user_name);
    let git_author_email = format!("GIT_AUTHOR_EMAIL={}", git.user_email);
    let agent_specific_mounts = agent_mounts(state);
    let agent_binary_mounts = agent_binary_mount_specs(paths, &capsule_config.agents).await;
    let gh_config_mount = github_config_mount(state);
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

    // Run the container as the host operator's UID (group 0). The image is
    // UID-agnostic (built once, shared); matching the host UID at runtime is
    // what makes every host-owned bind-mount transparently read/write for the
    // `agent` user — see `identity::host_run_as_user`. `HOME` is set
    // explicitly so shells and the agent CLIs resolve the bind-mounted home
    // even before any passwd lookup.
    let run_as_user = crate::runtime::identity::host_run_as_user();
    if let Some(ref user) = run_as_user {
        run_args.extend_from_slice(&["--user", user.as_str(), "-e", "HOME=/home/agent"]);
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
        jackin_diagnostics::active_run().map(|r| format!("JACKIN_RUN_ID={}", r.run_id()))
    } else {
        None
    };
    if let Some(ref env) = debug_run_id_env {
        run_args.extend_from_slice(&["-e", env.as_str()]);
    }
    // Always pass the host jackin version so the capsule ContainerInfo dialog
    // can surface which host binary launched the container.
    let host_version_env = format!("JACKIN_HOST_VERSION={}", env!("JACKIN_VERSION"));
    run_args.extend_from_slice(&["-e", host_version_env.as_str()]);

    let git_coauthor_trailer_env = git_coauthor_trailer.then(|| {
        format!(
            "{}=1",
            jackin_core::env_model::JACKIN_GIT_COAUTHOR_TRAILER_ENV_NAME
        )
    });
    if let Some(ref env) = git_coauthor_trailer_env {
        run_args.extend_from_slice(&["-e", env.as_str()]);
    }
    let git_dco_env =
        git_dco.then(|| format!("{}=1", jackin_core::env_model::JACKIN_GIT_DCO_ENV_NAME));
    if let Some(ref env) = git_dco_env {
        run_args.extend_from_slice(&["-e", env.as_str()]);
    }

    let passthrough_strings = host_runtime_passthrough_env(std::env::vars());
    for env_str in &passthrough_strings {
        run_args.push("-e");
        run_args.push(env_str);
    }
    let mut env_strings: Vec<String> = Vec::new();
    env_strings.push(format!(
        "{}={}",
        jackin_core::env_model::JACKIN_ENV_NAME,
        jackin_core::env_model::JACKIN_ENV_VALUE
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
        if jackin_core::env_model::is_reserved(key) {
            continue;
        }
        if key == NO_PROXY_UPPER || key == NO_PROXY_LOWER {
            // Synthesized below from merged casing — skip the inline emit.
            continue;
        }
        env_strings.push(format!("{key}={value}"));
    }

    // Grok's CLI accepts either XAI_API_KEY (personal) or GROK_DEPLOYMENT_KEY
    // (enterprise deployment key) for api-key auth. When the operator
    // configures a credential for Grok (via the api_key slot, which stores
    // under XAI_API_KEY, or has XAI_API_KEY in any env layer), also expose
    // it under GROK_DEPLOYMENT_KEY so the in-container `grok` sees the
    // credential under the name it prefers. Explicit GROK_DEPLOYMENT_KEY
    // in the layers takes precedence.
    if *agent == jackin_core::agent::Agent::Grok
        && let Some((_, value)) = resolved_env.vars.iter().find(|(k, _)| k == "XAI_API_KEY")
        && !resolved_env
            .vars
            .iter()
            .any(|(k, _)| k == "GROK_DEPLOYMENT_KEY")
    {
        env_strings.push(format!("GROK_DEPLOYMENT_KEY={value}"));
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
        jackin_core::env_model::GH_TOKEN_ENV_NAME,
        gh_token,
    );
    push_env_if_present(
        &mut env_strings,
        jackin_core::env_model::GITHUB_TOKEN_ENV_NAME,
        gh_token,
    );
    push_env_if_present(
        &mut env_strings,
        jackin_core::env_model::GH_HOST_ENV_NAME,
        github_env
            .get(jackin_core::env_model::GH_HOST_ENV_NAME)
            .map(String::as_str),
    );
    push_env_if_present(
        &mut env_strings,
        jackin_core::env_model::GH_ENTERPRISE_TOKEN_ENV_NAME,
        github_env
            .get(jackin_core::env_model::GH_ENTERPRISE_TOKEN_ENV_NAME)
            .map(String::as_str),
    );

    for env_str in &env_strings {
        run_args.push("-e");
        run_args.push(env_str);
    }

    // OTLP cross-process propagation: hand the container the launch trace
    // context (W3C traceparent) and a container-reachable endpoint, so the
    // capsule's telemetry links back to this launch trace and shares the run.
    // host.docker.internal must be wired to the host gateway for the rewritten
    // loopback endpoint to resolve on Linux engines.
    let container_otlp = jackin_diagnostics::container_otlp();
    let mut otlp_propagation: Vec<String> = Vec::new();
    if let Some(otlp) = &container_otlp {
        otlp_propagation.push(format!("OTEL_EXPORTER_OTLP_ENDPOINT={}", otlp.endpoint));
        if let Some(traceparent) = jackin_diagnostics::current_traceparent() {
            otlp_propagation.push(format!("TRACEPARENT={traceparent}"));
        }
        // Share parallax.run.id so capsule telemetry groups with the host run.
        // In debug runs JACKIN_RUN_ID is already injected above; avoid a dupe.
        if debug_run_id_env.is_none()
            && let Some(run) = jackin_diagnostics::active_run()
        {
            otlp_propagation.push(format!("JACKIN_RUN_ID={}", run.run_id()));
        }
    }
    for env_str in &otlp_propagation {
        run_args.push("-e");
        run_args.push(env_str);
    }
    if container_otlp
        .as_ref()
        .is_some_and(|otlp| otlp.needs_host_gateway)
    {
        run_args.extend_from_slice(&["--add-host", "host.docker.internal:host-gateway"]);
    }

    run_args.extend_from_slice(&["-v", &certs_agent_mount]);
    if let Some(gh_config_mount) = gh_config_mount.as_deref() {
        run_args.extend_from_slice(&["-v", gh_config_mount]);
    }
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
    // root:root 0755. The dir is owned by the host operator (this process)
    // and the container runs as that same UID (`--user`), so the `agent`
    // user creates jackin.sock with no special directory mode. The socket
    // file itself gets 0o600 from inside the capsule. The same directory
    // carries Capsule's normalized launch config.
    let socket_dir = paths.jackin_home.join("sockets").join(*container_name);
    let capsule_config_contents = toml::to_string(capsule_config)
        .context("serializing Capsule launch config for /jackin/run/agent.toml")?;
    // Runtime passwd entry for the host UID so `getpwuid`/`$HOME` resolve to
    // the `agent` user inside the container even though the image only bakes
    // UID 1000. Consumed via `libnss-extrausers` (see docker/construct). One
    // shared file, content depends only on the host UID; written atomically
    // (per-container temp + rename) so a concurrent launch can't read a torn
    // file at mount time, and only when the bytes actually change (see the
    // no-churn guard below) so the rename can't swap the inode out from under
    // a live `:ro` bind mount in an already-running container.
    let extrausers_passwd = paths.jackin_home.join("extrausers").join("passwd");
    let extrausers_line = crate::runtime::identity::host_uid()
        .map(|uid| format!("agent:x:{uid}:0:agent:/home/agent:/bin/zsh\n"));
    let extrausers_tmp = extrausers_passwd.with_file_name(format!("passwd.{container_name}.tmp"));
    // Run the filesystem syscalls on the blocking pool — the tokio
    // runtime is built without the `fs` feature here, and blocking on
    // a slow / NFS host parks the worker driving the docker-run RPC
    // for every other future scheduled on it.
    let socket_dir_for_mkdir = socket_dir.clone();
    let capsule_config_contents_for_write = capsule_config_contents.clone();
    let extrausers_passwd_for_write = extrausers_passwd.clone();
    let extrausers_line_for_write = extrausers_line.clone();
    jackin_diagnostics::active_timing_started(
        "capsule",
        "prepare_socket_dir",
        Some(container_name),
    );
    let prepare_socket_dir_result = tokio::task::spawn_blocking(move || -> std::io::Result<()> {
        std::fs::create_dir_all(&socket_dir_for_mkdir)?;
        std::fs::write(
            socket_dir_for_mkdir.join(jackin_protocol::CAPSULE_CONFIG_FILENAME),
            capsule_config_contents_for_write,
        )?;
        if let Some(line) = extrausers_line_for_write {
            if let Some(parent) = extrausers_passwd_for_write.parent() {
                std::fs::create_dir_all(parent)?;
            }
            // No-churn guard: this shared passwd file is bind-mounted `:ro`
            // into every running container. Its content depends only on the
            // host UID, so a concurrent launch produces identical bytes — but
            // the temp+rename below replaces the inode, which on macOS
            // invalidates the live single-file bind mount in containers that
            // are already running, breaking `getpwuid`/`$HOME` for the agent.
            // Skip the rename when the bytes already match. Byte compare (not
            // `read_to_string`) so a transient non-UTF-8 read can't silently
            // fall through to the inode-swapping rewrite. Mirrors the auth
            // provisioner's no-churn guard (see `instance::auth`).
            let unchanged = std::fs::read(&extrausers_passwd_for_write)
                .is_ok_and(|existing| existing == line.as_bytes());
            if !unchanged {
                std::fs::write(&extrausers_tmp, &line)?;
                std::fs::rename(&extrausers_tmp, &extrausers_passwd_for_write)?;
            }
        }
        Ok(())
    })
    .await
    .context("socket dir mkdir worker join")
    .and_then(|result| {
        result.with_context(|| {
            format!(
                "creating host-side socket dir {} for container {container_name}",
                socket_dir.display(),
            )
        })
    });
    jackin_diagnostics::active_timing_done(
        "capsule",
        "prepare_socket_dir",
        if prepare_socket_dir_result.is_ok() {
            Some("prepared")
        } else {
            Some("error")
        },
    );
    prepare_socket_dir_result?;
    // Start the jackin-exec host credential resolver for this container's
    // on-demand bindings. Its socket lands in the dir just prepared (bind-
    // mounted to /jackin/run), so the in-container capsule reaches it at
    // /jackin/run/host.sock. Spawned detached: the task runs independently of
    // this handle, for the host process's lifetime alongside the interactive
    // attach. No-op when the workspace declares no on-demand credentials.
    if !ctx.capsule_config.exec_bindings.is_empty() {
        drop(crate::exec_host::start_for_container(
            &ctx.paths.jackin_home,
            ctx.container_name,
            &ctx.capsule_config.exec_bindings,
        ));
    }
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
    // Mount each cached agent CLI binary read-only onto its PATH location. The
    // binaries are not baked into the image (see `render_derived_dockerfile`), so
    // an agent version bump is picked up here without an image rebuild.
    for mount in &agent_binary_mounts {
        run_args.extend_from_slice(&["-v", mount]);
    }
    // Mount the host-UID passwd line where libnss-extrausers reads it.
    let extrausers_mount = extrausers_line.as_ref().and_then(|_| {
        extrausers_passwd
            .to_str()
            .map(|p| format!("{p}:/var/lib/extrausers/passwd:ro"))
    });
    if let Some(ref mount) = extrausers_mount {
        run_args.extend_from_slice(&["-v", mount]);
    }
    jackin_diagnostics::debug_log!(
        "launch",
        "prepared host socket dir {socket_dir_str} (owned by host UID, default umask) and Capsule config for bind-mount at /jackin/run",
    );
    run_args.push(image);
    // Pass the initial agent as the container command argument. The
    // daemon uses it only to choose the first tab; per-session
    // `JACKIN_AGENT` is set later when spawning an actual agent PTY.
    run_args.push(agent.slug());
    jackin_diagnostics::active_timing_started("capsule", "docker_run_role", Some(container_name));
    let run_role = runner.run("docker", &run_args, None, &docker_run_opts);
    let run_role_result = if let Some(progress) = steps.progress_mut() {
        progress.while_waiting(run_role).await
    } else {
        run_role.await
    };
    jackin_diagnostics::active_timing_done(
        "capsule",
        "docker_run_role",
        if run_role_result.is_ok() {
            Some("started")
        } else {
            Some("error")
        },
    );
    run_role_result?;

    // Reconcile keep_awake AFTER the role container is running but
    // BEFORE the foreground session blocks. This is the only window in
    // which an interactive `jackin load` can spawn caffeinate.
    super::caffeinate::reconcile(paths, docker, runner).await;

    // Emit a structured container_started event so the run JSONL points at
    // the capsule log regardless of whether the session succeeds (Defect 41).
    let capsule_log_path = capsule_multiplexer_log_path(paths, container_name);
    let capsule_log_str = capsule_log_path.display().to_string();
    if let Some(run) = jackin_diagnostics::active_run() {
        run.container_started(container_name, &capsule_log_str);
    }

    // Pre-session safety check: if jackin-capsule exited immediately
    // (missing binary, bad image), surface the container logs rather than
    // failing with a cryptic docker exec error.
    jackin_diagnostics::active_timing_started(
        "capsule",
        "pre_attach_exit_check",
        Some(container_name),
    );
    if let Some(err) = diagnose_premature_exit(
        docker,
        runner,
        container_name,
        ExitPhase::PreAttach,
        Some(&capsule_log_str),
    )
    .await
    {
        jackin_diagnostics::active_timing_done("capsule", "pre_attach_exit_check", Some("exited"));
        return Err(err);
    }
    jackin_diagnostics::active_timing_done("capsule", "pre_attach_exit_check", Some("running"));

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
    if let Some(refresh) = selected_image_refresh {
        crate::runtime::image::spawn_selected_image_refresh(
            paths,
            selector,
            refresh.role_git,
            refresh.branch_override,
            *agent,
            refresh.reason,
            *debug,
        );
    }
    crate::runtime::image::spawn_sibling_runtime_prewarm(
        paths,
        sibling_prewarm.validated_repo,
        *agent,
        sibling_prewarm.selected_image_reused,
    );
    crate::runtime::image::spawn_sibling_image_prewarm(
        paths,
        selector,
        sibling_prewarm.role_git,
        sibling_prewarm.branch_override,
        sibling_prewarm.validated_repo,
        *agent,
        sibling_prewarm.selected_image_reused,
    );
    spawn_sibling_auth_prewarm(paths, container_name, sibling_auth_prewarm, *agent);
    let session_result =
        reconnect_or_create_session_with_focus(paths, container_name, None, docker, runner).await;
    // Ensure cleanup debug logs start on a fresh line after the interactive session
    eprintln!();
    if let Err(err) = session_result {
        // Single inspect — the previous two-call shape opened a TOCTOU
        // window where the container could transition Running→Stopped(0)
        // between the diagnose and swallow checks. If the attach command
        // itself returned Err, propagate it even when PID 1 exited cleanly:
        // the capsule attach protocol uses that path to report failed final
        // sessions while the daemon still shuts down as init with exit 0.
        let inspect = docker.inspect_container_state(container_name).await;
        if let Some(diag) = diagnose_with_state(
            runner,
            container_name,
            &inspect,
            ExitPhase::PostAttach,
            Some(&capsule_log_str),
        )
        .await
        {
            return Err(diag);
        }
        let attach_error =
            attach_failure_error(container_name, &err, &capsule_log_path, &capsule_log_str);
        if let Some(run) = jackin_diagnostics::active_run() {
            run.error("attach_error", &attach_error.to_string());
        }
        return Err(attach_error);
    }
    if let Some(progress) = steps.progress_mut() {
        progress.stage_done(super::progress::LaunchStage::Hardline, "open");
    }

    Ok(())
}

fn host_runtime_passthrough_env(vars: impl IntoIterator<Item = (String, String)>) -> Vec<String> {
    vars.into_iter()
        .filter_map(|(key, value)| {
            if key.starts_with("JACKIN_DISABLE_")
                || matches!(
                    key.as_str(),
                    "JACKIN_DHAT_ALLOC_LOG" | "JACKIN_CAPSULE_FORCE_PANIC" | "TZ"
                )
            {
                Some(format!("{key}={value}"))
            } else {
                None
            }
        })
        .collect()
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
    runner: &mut impl CommandRunner,
    container_name: &str,
    phase: ExitPhase,
    capsule_log_path: Option<&str>,
) -> Option<anyhow::Error> {
    let state = docker.inspect_container_state(container_name).await;
    diagnose_with_state(runner, container_name, &state, phase, capsule_log_path).await
}

/// Same diagnostic logic as `diagnose_premature_exit` but with the
/// inspected state passed in — callers that already inspected the
/// container can avoid a second `docker inspect` round-trip (and the
/// TOCTOU window between the two).
async fn diagnose_with_state(
    runner: &mut impl CommandRunner,
    container_name: &str,
    state: &ContainerState,
    phase: ExitPhase,
    capsule_log_path: Option<&str>,
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
                    let trimmed = text.trim().to_owned();
                    if trimmed.is_empty() {
                        None
                    } else {
                        Some(trimmed)
                    }
                }
                Err(e) => Some(format!("(docker logs failed: {e:#})")),
            };
            let reason = if *oom_killed {
                "OOM killed".to_owned()
            } else {
                format!("exit {exit_code}")
            };
            let phase_label = match phase {
                ExitPhase::PreAttach => "exited before attach",
                ExitPhase::PostAttach => "exited during session",
            };
            let body = if let Some(text) = logs.as_deref() {
                format!(
                    "container {container_name} {phase_label} ({reason}); last 40 log lines:\n{text}"
                )
            } else if let Some(mux_tail) = capsule_log_path
                .map(Path::new)
                .and_then(|path| read_text_tail(path, 40).ok().flatten())
            {
                // `docker logs` is empty when the capsule daemon routes its
                // diagnostics to multiplexer.log rather than stderr. Surface
                // that file's tail so a pre-attach daemon crash is reported
                // with its real error instead of an opaque "no log output".
                format!(
                    "container {container_name} {phase_label} ({reason}); docker logs empty — last 40 multiplexer.log lines:\n{mux_tail}"
                )
            } else {
                format!(
                    "container {container_name} {phase_label} ({reason}) and produced no log output"
                )
            };
            // Emit a structured container exit event with the crash evidence so
            // the run JSONL is self-contained (Defect 41).
            if let Some(run) = jackin_diagnostics::active_run() {
                run.container_exited(
                    container_name,
                    (*exit_code).into(),
                    *oom_killed,
                    capsule_log_path.unwrap_or("(path unknown)"),
                    logs.as_deref(),
                );
            }
            Some(anyhow::anyhow!(body))
        }
    }
}

fn read_text_tail(path: &Path, max_lines: usize) -> anyhow::Result<Option<String>> {
    let lines = super::logs::read_tail(path, max_lines)?;
    if lines.is_empty() {
        Ok(None)
    } else {
        Ok(Some(lines.join("\n")))
    }
}

fn attach_failure_error(
    container_name: &str,
    err: &anyhow::Error,
    capsule_log_path: &Path,
    capsule_log_str: &str,
) -> anyhow::Error {
    let evidence = match read_text_tail(capsule_log_path, 40) {
        Ok(Some(tail)) => format!("last 40 capsule log lines:\n{tail}"),
        Ok(None) => format!("capsule log {capsule_log_str} had no output"),
        Err(error) => format!("failed to read capsule log {capsule_log_str}: {error:#}"),
    };
    anyhow::anyhow!(
        "capsule attach failed for {container_name}: {err}\ncapsule log: {capsule_log_str}\n{evidence}"
    )
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
            jackin_diagnostics::debug_log!(
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
            jackin_diagnostics::debug_log!(
                "isolation",
                "inspect_attach_outcome: docker inspect failed for {container}; treating as still_running (conservative — finalize_clean_exit's auto-cleanup never fires)",
            );
            AttachOutcome::still_running()
        }
    })
}

pub(super) enum GitPullResult {
    Success { src: String, stdout: String },
    Failure { src: String, stderr: String },
    SpawnError { src: String, error: std::io::Error },
    JoinError { src: String },
}

#[cfg(test)]
fn pull_workspace_repos_with_git(
    workspace: &jackin_config::ResolvedWorkspace,
    debug: bool,
    git_program: &Path,
) -> Vec<GitPullResult> {
    pull_git_sources_with_git(git_pull_sources(workspace), debug, git_program, true)
}

pub(super) fn git_pull_sources(workspace: &jackin_config::ResolvedWorkspace) -> Vec<String> {
    let mut sources = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for mount in &workspace.mounts {
        if Path::new(&mount.src).join(".git").exists() && seen.insert(mount.src.clone()) {
            sources.push(mount.src.clone());
        }
    }
    sources
}

pub(super) fn pull_git_sources_with_git(
    sources: Vec<String>,
    debug: bool,
    git_program: &Path,
    print_starts: bool,
) -> Vec<GitPullResult> {
    let mut pulls = Vec::new();

    for src in sources {
        if debug {
            jackin_diagnostics::active_debug("git_pull", &format!("git pull in {src}"));
            if jackin_diagnostics::active_run().is_none() {
                tracing::debug!(src, "git pull in workspace");
            }
        }
        if print_starts {
            let src_display = jackin_diagnostics::shorten_home(&src);
            tracing::info!(src = src_display.as_str(), "pulling workspace");
            eprintln!("  Pulling {src_display} …");
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
                #[expect(
                    clippy::disallowed_methods,
                    reason = "git pull runs on a dedicated OS thread, not the launch render runtime thread"
                )]
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

pub(super) fn print_git_pull_results(results: &[GitPullResult]) {
    for result in results {
        match result {
            GitPullResult::Success { stdout, .. } => {
                print_git_pull_stdout(stdout);
            }
            GitPullResult::Failure { src, stderr } => {
                tracing::warn!(src, stderr = stderr.trim(), "git pull failed");
                eprintln!("  Warning: git pull failed in {}: {}", src, stderr.trim());
            }
            GitPullResult::SpawnError { src, error } => {
                tracing::warn!(src, %error, "git pull spawn error");
                eprintln!("  Warning: could not run git pull in {src}: {error}");
            }
            GitPullResult::JoinError { src } => {
                tracing::warn!(src, "git pull thread panicked");
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

pub(super) fn record_git_pull_results(results: &[GitPullResult]) -> (usize, usize) {
    let mut ok = 0;
    let mut failed = 0;
    for result in results {
        match result {
            GitPullResult::Success { src, stdout } => {
                ok += 1;
                jackin_diagnostics::active_debug(
                    "git_pull",
                    &format!("git pull in {src} succeeded: {}", stdout.trim()),
                );
            }
            GitPullResult::Failure { src, stderr } => {
                failed += 1;
                if let Some(run) = jackin_diagnostics::active_run() {
                    run.compact("git_pull", &format!("git pull failed in {src}"));
                }
                jackin_diagnostics::active_debug(
                    "git_pull",
                    &format!("git pull in {src} failed: {}", stderr.trim()),
                );
            }
            GitPullResult::SpawnError { src, error } => {
                failed += 1;
                if let Some(run) = jackin_diagnostics::active_run() {
                    run.compact(
                        "git_pull",
                        &format!("could not run git pull in {src}: {error}"),
                    );
                }
            }
            GitPullResult::JoinError { src } => {
                failed += 1;
                if let Some(run) = jackin_diagnostics::active_run() {
                    run.compact("git_pull", &format!("git pull thread panicked in {src}"));
                }
            }
        }
    }
    (ok, failed)
}

pub(super) fn launch_failure_title(
    stage: super::progress::LaunchStage,
    error: &anyhow::Error,
    run: Option<&jackin_diagnostics::RunDiagnostics>,
) -> String {
    if stage == super::progress::LaunchStage::DerivedImage
        && run.and_then(docker_build_output_artifact).is_some()
    {
        return "Docker build failed".to_owned();
    }
    let text = error.to_string().to_ascii_lowercase();
    if text.contains("docker") {
        "Docker unavailable".to_owned()
    } else if text.contains("credential") || text.contains("token") || text.contains("auth") {
        "Credential check failed".to_owned()
    } else {
        "Launch failed".to_owned()
    }
}

pub(super) fn short_launch_diagnosis(
    stage: super::progress::LaunchStage,
    error: &anyhow::Error,
    run: Option<&jackin_diagnostics::RunDiagnostics>,
) -> String {
    if stage == super::progress::LaunchStage::DerivedImage
        && run.and_then(docker_build_output_artifact).is_some()
    {
        return "Building the Docker container failed.".to_owned();
    }
    error
        .chain()
        .next()
        .map_or_else(|| "launch did not complete".to_owned(), ToString::to_string)
}

fn docker_build_output_artifact(run: &jackin_diagnostics::RunDiagnostics) -> Option<PathBuf> {
    let docker_output = run.command_output_path("docker-build");
    docker_output.exists().then_some(docker_output)
}

pub(super) fn launch_failure_cli_error(
    stage: super::progress::LaunchStage,
    error: &anyhow::Error,
    run: Option<&jackin_diagnostics::RunDiagnostics>,
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

pub(super) fn resolve_launch_role_source(
    config: &mut AppConfig,
    selector: &RoleSelector,
    restore_role_source_git: Option<&str>,
) -> anyhow::Result<(jackin_config::RoleSource, bool, bool)> {
    if let Some(git) = restore_role_source_git {
        let mut source = config
            .roles
            .get(&selector.key())
            .cloned()
            .unwrap_or_default();
        source.git = git.to_owned();
        source.trusted = true;
        return Ok((source, false, true));
    }
    let (source, is_new) = config.resolve_role_source(selector)?;
    Ok((source, is_new, false))
}

pub(super) async fn render_exit(paths: &JackinPaths, docker: &impl DockerApi) {
    let force_outro = super::universe::force_boundary_outro_enabled();
    let running = match list_running_agent_names(docker).await {
        Ok(names) => names,
        Err(e) => {
            if let Some(run) = jackin_diagnostics::active_run() {
                run.compact(
                    "exit_summary",
                    &format!("skipping boundary outro; running-container list failed: {e:#}"),
                );
            }
            return;
        }
    };

    if !running.is_empty() {
        if let Some(run) = jackin_diagnostics::active_run() {
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
    jackin_diagnostics::reassert_alt_screen();
    let host_owned = jackin_diagnostics::host_screen_owned();
    jackin_tui::animation::warp_out(host_owned);
    jackin_tui::animation::warp_end_caption(elapsed, host_owned);
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum RestoreResolution {
    StartFresh,
    AttachCurrentRole(String),
    StartCurrentRole(String),
    RecreateCurrentRole(String),
    RestoreCurrentRole(String),
    RecoverRelatedRole(String),
    RebuildRelatedRole(Box<InstanceManifest>),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum LaunchPlan {
    AttachExisting,
    StartStopped,
    CreateFromValidImage,
    BuildAndCreate,
    PrewarmOnly,
}

impl LaunchPlan {
    const fn as_str(self) -> &'static str {
        match self {
            Self::AttachExisting => "AttachExisting",
            Self::StartStopped => "StartStopped",
            Self::CreateFromValidImage => "CreateFromValidImage",
            Self::BuildAndCreate => "BuildAndCreate",
            Self::PrewarmOnly => "PrewarmOnly",
        }
    }
}

pub(super) fn emit_launch_plan(plan: LaunchPlan, reason: &str, container: Option<&str>) {
    let plan = plan.as_str();
    let detail = serde_json::json!({
        "plan": plan,
        "reason": reason,
        "container": container,
    })
    .to_string();
    if let Some(run) = jackin_diagnostics::active_run() {
        run.stage(
            "launch_plan",
            "restore",
            &format!("selected launch plan {plan}"),
            Some(&detail),
        );
    }
}

pub(super) fn emit_prewarm_launch_plan(reason: &str) {
    emit_launch_plan(LaunchPlan::PrewarmOnly, reason, None);
}

pub(super) fn emit_image_materialization_plan(
    image_reused: bool,
    reason: &str,
    restoring: bool,
    container: &str,
) {
    if image_reused {
        let base_reason = if restoring {
            "restore_container_missing_valid_image"
        } else {
            "no_restore_candidate_valid_image"
        };
        let plan_reason = if reason == "recipe_hash_match" {
            base_reason.to_owned()
        } else {
            format!("{base_reason}:{reason}")
        };
        emit_launch_plan(
            LaunchPlan::CreateFromValidImage,
            &plan_reason,
            Some(container),
        );
    } else {
        emit_launch_plan(LaunchPlan::BuildAndCreate, reason, Some(container));
    }
}

fn emit_rejected_launch_plan(
    plan: LaunchPlan,
    reason: &str,
    container: Option<&str>,
    state: Option<&str>,
) {
    let plan = plan.as_str();
    let detail = serde_json::json!({
        "plan": plan,
        "reason": reason,
        "container": container,
        "state": state,
    })
    .to_string();
    if let Some(run) = jackin_diagnostics::active_run() {
        run.stage(
            "launch_plan_rejected",
            "restore",
            &format!("rejected launch plan {plan}"),
            Some(&detail),
        );
    }
}

#[allow(clippy::too_many_arguments)]
pub(super) async fn resolve_restore_candidate(
    paths: &JackinPaths,
    workspace_name: Option<&str>,
    workspace_label: &str,
    workdir: &str,
    role_key: &str,
    agent: jackin_core::agent::Agent,
    docker: &impl DockerApi,
    progress: Option<&mut super::progress::LaunchProgress>,
) -> anyhow::Result<RestoreResolution> {
    let current = resolve_current_restore_candidate_timed(
        paths,
        workspace_name,
        workspace_label,
        workdir,
        role_key,
        agent,
        docker,
    )
    .await?;
    if let Some(current) = current {
        return Ok(current);
    }

    jackin_diagnostics::active_timing_started(
        "restore",
        "related_restore_candidates",
        Some(role_key),
    );
    let related_result = related_restore_candidates(
        paths,
        workspace_name,
        workspace_label,
        workdir,
        role_key,
        agent,
        docker,
    )
    .await;
    let related = match related_result {
        Ok(related) => {
            jackin_diagnostics::active_timing_done(
                "restore",
                "related_restore_candidates",
                Some(&format!("{} candidates", related.len())),
            );
            related
        }
        Err(error) => {
            jackin_diagnostics::active_timing_done(
                "restore",
                "related_restore_candidates",
                Some("error"),
            );
            return Err(error);
        }
    };

    if related.is_empty() {
        emit_rejected_launch_plan(
            LaunchPlan::AttachExisting,
            "no_current_role_candidate",
            None,
            None,
        );
        emit_rejected_launch_plan(
            LaunchPlan::StartStopped,
            "no_current_role_candidate",
            None,
            None,
        );
        emit_rejected_launch_plan(
            LaunchPlan::CreateFromValidImage,
            "no_current_role_candidate",
            None,
            None,
        );
        return Ok(RestoreResolution::StartFresh);
    }

    // Related stale-state decisions still require an explicit rich prompt so
    // launching one role never silently recovers or supersedes another role.
    present_restore_choice(
        progress,
        paths,
        workspace_label,
        role_key,
        Vec::new(),
        &related,
    )
}

#[allow(clippy::too_many_arguments)]
pub(super) async fn resolve_current_restore_candidate_timed(
    paths: &JackinPaths,
    workspace_name: Option<&str>,
    workspace_label: &str,
    workdir: &str,
    role_key: &str,
    agent: jackin_core::agent::Agent,
    docker: &impl DockerApi,
) -> anyhow::Result<Option<RestoreResolution>> {
    jackin_diagnostics::active_timing_started(
        "restore",
        "current_restore_candidate",
        Some(role_key),
    );
    let result = resolve_current_restore_candidate(
        paths,
        workspace_name,
        workspace_label,
        workdir,
        role_key,
        agent,
        docker,
    )
    .await;
    match result {
        Ok(current) => {
            let detail = current
                .as_ref()
                .map_or("none", current_restore_timing_detail);
            jackin_diagnostics::active_timing_done(
                "restore",
                "current_restore_candidate",
                Some(detail),
            );
            Ok(current)
        }
        Err(error) => {
            jackin_diagnostics::active_timing_done(
                "restore",
                "current_restore_candidate",
                Some("error"),
            );
            Err(error)
        }
    }
}

#[cfg(test)]
#[allow(clippy::too_many_arguments)]
pub(super) async fn resolve_unselected_current_restore_candidate_timed(
    paths: &JackinPaths,
    workspace_name: Option<&str>,
    workspace_label: &str,
    workdir: &str,
    role_key: &str,
    docker: &impl DockerApi,
) -> anyhow::Result<Option<RestoreResolution>> {
    Ok(
        resolve_unselected_current_restore_candidate_with_agent_timed(
            paths,
            workspace_name,
            workspace_label,
            workdir,
            role_key,
            docker,
        )
        .await?
        .map(|candidate| candidate.resolution),
    )
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct UnselectedCurrentRestoreResolution {
    pub resolution: RestoreResolution,
    pub agent: jackin_core::agent::Agent,
}

#[allow(clippy::too_many_arguments)]
pub(super) async fn resolve_unselected_current_restore_candidate_with_agent_timed(
    paths: &JackinPaths,
    workspace_name: Option<&str>,
    workspace_label: &str,
    workdir: &str,
    role_key: &str,
    docker: &impl DockerApi,
) -> anyhow::Result<Option<UnselectedCurrentRestoreResolution>> {
    jackin_diagnostics::active_timing_started(
        "restore",
        "current_restore_candidate_unselected_agent",
        Some(role_key),
    );
    let result = resolve_unselected_current_restore_candidate_with_agent(
        paths,
        workspace_name,
        workspace_label,
        workdir,
        role_key,
        docker,
    )
    .await;
    match result {
        Ok(current) => {
            let detail = current.as_ref().map_or("none", |candidate| {
                current_restore_timing_detail(&candidate.resolution)
            });
            jackin_diagnostics::active_timing_done(
                "restore",
                "current_restore_candidate_unselected_agent",
                Some(detail),
            );
            Ok(current)
        }
        Err(error) => {
            jackin_diagnostics::active_timing_done(
                "restore",
                "current_restore_candidate_unselected_agent",
                Some("error"),
            );
            Err(error)
        }
    }
}

fn current_restore_timing_detail(resolution: &RestoreResolution) -> &'static str {
    match resolution {
        RestoreResolution::AttachCurrentRole(_) => "attach_existing",
        RestoreResolution::StartCurrentRole(_) => "start_stopped",
        RestoreResolution::RecreateCurrentRole(_) => "create_from_valid_image",
        _ => "other",
    }
}

#[allow(clippy::too_many_arguments)]
async fn resolve_unselected_current_restore_candidate_with_agent(
    paths: &JackinPaths,
    workspace_name: Option<&str>,
    workspace_label: &str,
    workdir: &str,
    role_key: &str,
    docker: &impl DockerApi,
) -> anyhow::Result<Option<UnselectedCurrentRestoreResolution>> {
    let candidates =
        matching_current_role_manifests(paths, workspace_name, workspace_label, workdir, role_key)?
            .into_iter()
            .filter(InstanceManifest::is_restore_candidate)
            .collect::<Vec<_>>();

    if candidates.is_empty() {
        return Ok(None);
    }

    let multiple_candidates = candidates.len() > 1;
    let mut runnable = Vec::new();
    let mut recreatable = Vec::new();
    for manifest in candidates {
        let agent = manifest.agent()?;
        jackin_diagnostics::active_timing_started(
            "restore",
            "inspect_current_container",
            Some(&manifest.container_base),
        );
        let docker_state = docker
            .inspect_container_state(&manifest.container_base)
            .await;
        jackin_diagnostics::active_timing_done(
            "restore",
            "inspect_current_container",
            Some(docker_state.short_label().as_str()),
        );
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
        match docker_state {
            ContainerState::Running | ContainerState::Paused | ContainerState::Restarting => {
                runnable.push(UnselectedCurrentRestoreResolution {
                    resolution: RestoreResolution::AttachCurrentRole(manifest.container_base),
                    agent,
                });
            }
            ContainerState::Stopped { .. } | ContainerState::Created => {
                runnable.push(UnselectedCurrentRestoreResolution {
                    resolution: RestoreResolution::StartCurrentRole(manifest.container_base),
                    agent,
                });
            }
            ContainerState::NotFound => {
                emit_rejected_launch_plan(
                    LaunchPlan::AttachExisting,
                    if multiple_candidates {
                        "current_role_agent_container_missing"
                    } else {
                        "single_current_role_agent_container_missing"
                    },
                    Some(&manifest.container_base),
                    Some(docker_state.short_label().as_str()),
                );
                emit_rejected_launch_plan(
                    LaunchPlan::StartStopped,
                    if multiple_candidates {
                        "current_role_agent_container_missing"
                    } else {
                        "single_current_role_agent_container_missing"
                    },
                    Some(&manifest.container_base),
                    Some(docker_state.short_label().as_str()),
                );
                recreatable.push(UnselectedCurrentRestoreResolution {
                    resolution: RestoreResolution::RecreateCurrentRole(manifest.container_base),
                    agent,
                });
            }
            ContainerState::Removing
            | ContainerState::Dead
            | ContainerState::InspectUnavailable(_) => {
                emit_rejected_launch_plan(
                    LaunchPlan::AttachExisting,
                    if multiple_candidates {
                        "current_role_agent_container_not_attachable"
                    } else {
                        "single_current_role_agent_container_not_attachable"
                    },
                    Some(&manifest.container_base),
                    Some(docker_state.short_label().as_str()),
                );
                emit_rejected_launch_plan(
                    LaunchPlan::StartStopped,
                    if multiple_candidates {
                        "current_role_agent_container_not_startable"
                    } else {
                        "single_current_role_agent_container_not_startable"
                    },
                    Some(&manifest.container_base),
                    Some(docker_state.short_label().as_str()),
                );
            }
        }
    }

    match runnable.as_slice() {
        [
            UnselectedCurrentRestoreResolution {
                resolution: RestoreResolution::AttachCurrentRole(container),
                agent,
            },
        ] => {
            emit_launch_plan(
                LaunchPlan::AttachExisting,
                if multiple_candidates {
                    "only_viable_current_role_agent_container_running"
                } else {
                    "single_current_role_agent_container_running"
                },
                Some(container),
            );
            Ok(Some(UnselectedCurrentRestoreResolution {
                resolution: RestoreResolution::AttachCurrentRole(container.clone()),
                agent: *agent,
            }))
        }
        [
            UnselectedCurrentRestoreResolution {
                resolution: RestoreResolution::StartCurrentRole(container),
                agent,
            },
        ] => {
            emit_launch_plan(
                LaunchPlan::StartStopped,
                if multiple_candidates {
                    "only_viable_current_role_agent_container_startable"
                } else {
                    "single_current_role_agent_container_startable"
                },
                Some(container),
            );
            Ok(Some(UnselectedCurrentRestoreResolution {
                resolution: RestoreResolution::StartCurrentRole(container.clone()),
                agent: *agent,
            }))
        }
        [] => match recreatable.as_slice() {
            [candidate] => Ok(Some(candidate.clone())),
            [] => Ok(None),
            _ => {
                emit_rejected_launch_plan(
                    LaunchPlan::CreateFromValidImage,
                    "multiple_current_role_agents_need_selection",
                    None,
                    None,
                );
                Ok(None)
            }
        },
        _ => {
            emit_rejected_launch_plan(
                LaunchPlan::AttachExisting,
                "multiple_current_role_agents_need_selection",
                None,
                None,
            );
            emit_rejected_launch_plan(
                LaunchPlan::StartStopped,
                "multiple_current_role_agents_need_selection",
                None,
                None,
            );
            Ok(None)
        }
    }
}

#[allow(clippy::too_many_arguments)]
pub(super) async fn resolve_current_restore_candidate(
    paths: &JackinPaths,
    workspace_name: Option<&str>,
    workspace_label: &str,
    workdir: &str,
    role_key: &str,
    agent: jackin_core::agent::Agent,
    docker: &impl DockerApi,
) -> anyhow::Result<Option<RestoreResolution>> {
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
        jackin_diagnostics::active_timing_started(
            "restore",
            "inspect_current_container",
            Some(&manifest.container_base),
        );
        let docker_state = docker
            .inspect_container_state(&manifest.container_base)
            .await;
        jackin_diagnostics::active_timing_done(
            "restore",
            "inspect_current_container",
            Some(docker_state.short_label().as_str()),
        );
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
        match docker_state {
            ContainerState::Running | ContainerState::Paused | ContainerState::Restarting => {
                emit_launch_plan(
                    LaunchPlan::AttachExisting,
                    "current_role_container_running",
                    Some(&manifest.container_base),
                );
                return Ok(Some(RestoreResolution::AttachCurrentRole(
                    manifest.container_base.clone(),
                )));
            }
            ContainerState::Stopped { .. } | ContainerState::Created => {
                emit_launch_plan(
                    LaunchPlan::StartStopped,
                    "current_role_container_startable",
                    Some(&manifest.container_base),
                );
                return Ok(Some(RestoreResolution::StartCurrentRole(
                    manifest.container_base.clone(),
                )));
            }
            ContainerState::NotFound => {
                emit_rejected_launch_plan(
                    LaunchPlan::AttachExisting,
                    "current_role_container_missing",
                    Some(&manifest.container_base),
                    Some(docker_state.short_label().as_str()),
                );
                emit_rejected_launch_plan(
                    LaunchPlan::StartStopped,
                    "current_role_container_missing",
                    Some(&manifest.container_base),
                    Some(docker_state.short_label().as_str()),
                );
                return Ok(Some(RestoreResolution::RecreateCurrentRole(
                    manifest.container_base.clone(),
                )));
            }
            ContainerState::Removing
            | ContainerState::Dead
            | ContainerState::InspectUnavailable(_) => {
                emit_rejected_launch_plan(
                    LaunchPlan::AttachExisting,
                    "current_role_container_not_attachable",
                    Some(&manifest.container_base),
                    Some(docker_state.short_label().as_str()),
                );
                emit_rejected_launch_plan(
                    LaunchPlan::StartStopped,
                    "current_role_container_not_startable",
                    Some(&manifest.container_base),
                    Some(docker_state.short_label().as_str()),
                );
            }
        }
    }
    Ok(None)
}

/// Present the stale-instance decision. "Start fresh" is always the
/// default first option; recoverable instances follow. The rich launch
/// surface renders it as a forced-choice picker (no cancel). The operator
/// must pick.
mod restore;
#[cfg(test)]
use restore::{
    RelatedRestoreCandidate, format_attach_outcome, recover_related_restore_candidate,
    restore_candidate_label, supersede_restore_candidates,
};
use restore::{
    capsule_multiplexer_log_path, manifest_host_workdir_fingerprint,
    matching_current_role_manifests, matching_instance_manifests, present_restore_choice,
    related_restore_candidates, related_restore_load_options, write_instance_attach_outcome,
    write_preserved_status_if_applicable,
};
pub(in crate::runtime) use restore::{
    preserved_instance_status, record_instance_attach_outcome, write_instance_status,
};

mod auth_error;
use auth_error::{
    EnvLayerState, NO_PROXY_LOWER, NO_PROXY_UPPER, auth_token_source_reference,
    build_env_layer_states, build_mode_resolution, is_proxy_env_name, push_env_if_present,
};
#[cfg(test)]
use auth_error::{LaunchError, append_no_proxy_host};
#[cfg(not(test))]
use auth_error::{LaunchError, append_no_proxy_host};

pub(super) struct LoadCleanup {
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
            jackin_tui::output::step_fail(&format!("cleanup failed (container): {e}"));
        }
        if let Err(e) = docker.remove_container(&self.dind).await {
            jackin_tui::output::step_fail(&format!("cleanup failed (dind): {e}"));
        }
        if let Err(e) = docker.remove_volume(&self.certs_volume).await {
            jackin_tui::output::step_fail(&format!("cleanup failed (certs volume): {e}"));
        }
        if let Err(e) = docker.remove_network(&self.network).await {
            jackin_tui::output::step_fail(&format!("cleanup failed (network): {e}"));
        }
        if self.clean_socket_dir {
            match std::fs::remove_dir_all(&self.socket_dir) {
                Ok(()) => {}
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                Err(error) => jackin_tui::output::step_fail(&format!(
                    "cleanup failed (socket dir {}): {error}",
                    self.socket_dir.display()
                )),
            }
        }
    }
}

#[cfg(test)]
mod tests;
