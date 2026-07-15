// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Mount construction helpers extracted from the launch coordinator.
//! All items re-exported from the parent to preserve `super::` call sites
//! in `launch_role_runtime` and `launch_pipeline.rs`.

use std::path::{Path, PathBuf};

use jackin_config::AppConfig;

use crate::isolation::materialize::MaterializedWorkspace;

/// Emit the durable-home bind mounts for `agent`, derived from its
/// [`AgentStatePaths`](jackin_core::AgentStatePaths) so the
/// per-agent home layout (data root, paired config root, standalone home files)
/// lives only in the agent enum. Auth-handoff mounts are agent-specific and stay
/// inline in [`agent_mounts`].
fn push_agent_home_mounts(mounts: &mut Vec<String>, root: &Path, agent: jackin_core::Agent) {
    let paths = agent.runtime().state_paths();
    let home = root.join("home");
    for entry in paths.home_dirs().chain(paths.home_files.iter().copied()) {
        mounts.push(format!(
            "{}:/home/agent/{entry}",
            home.join(entry).display()
        ));
    }
}

/// Returns the per-agent mount strings in jackin❯'s `src:dst[:ro]` idiom for
/// `docker run -v`.
///
/// Every provisioned agent is represented on `state.auth`, so the mount block
/// checks `auth.*` flags rather than matching the selected-agent variant. The
/// foreground launch path provisions all manifest-supported agents so sibling
/// tabs opened via `hardline --new --agent <other>` find their homes
/// bind-mounted from the start.
pub(crate) fn agent_mounts(state: &crate::instance::RoleState) -> Vec<String> {
    use jackin_core::Agent;
    let mut mounts = vec![format!(
        "{}:/jackin/state",
        state.root.join("state").display()
    )];

    if let Some(claude) = &state.auth.claude {
        push_agent_home_mounts(&mut mounts, &state.root, Agent::Claude);
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
        push_agent_home_mounts(&mut mounts, &state.root, Agent::Codex);
        if let Some(auth_json) = &codex.auth_json {
            mounts.push(format!("{}:/jackin/codex/auth.json", auth_json.display()));
        }
    }

    if let Some(amp) = &state.auth.amp {
        push_agent_home_mounts(&mut mounts, &state.root, Agent::Amp);
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
        push_agent_home_mounts(&mut mounts, &state.root, Agent::Kimi);
        if kimi.forward_auth {
            mounts.push(format!(
                "{}:/jackin/kimi-code",
                state.root.join("kimi-code").display()
            ));
        }
    }

    if let Some(opencode) = &state.auth.opencode {
        push_agent_home_mounts(&mut mounts, &state.root, Agent::Opencode);
        if let Some(auth_json) = &opencode.auth_json {
            mounts.push(format!(
                "{}:/jackin/opencode/auth.json",
                auth_json.display()
            ));
        }
    }

    if let Some(grok) = &state.auth.grok {
        push_agent_home_mounts(&mut mounts, &state.root, Agent::Grok);
        if let Some(auth_json) = &grok.auth_json {
            mounts.push(format!("{}:/jackin/grok/auth.json", auth_json.display()));
        }
    }

    mounts
}

pub(crate) fn github_config_mount(state: &crate::instance::RoleState) -> Option<String> {
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
pub(crate) fn build_workspace_mount_strings(workspace: &MaterializedWorkspace) -> Vec<String> {
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

/// The container backend selected for a launch.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Backend {
    Docker,
    AppleContainer,
}

/// Resolve the container backend for a launch. A per-workspace
/// `[runtime].backend` overrides the host-wide `[runtime].default_backend`,
/// which defaults to Docker when unset.
///
/// The backend fields are free-text strings in the config schema, so an
/// unrecognised value is rejected here rather than silently falling through to
/// Docker — a typo must fail closed, not launch the wrong (weaker-isolation)
/// backend behind the operator's back.
pub(crate) fn resolve_backend(
    config: &AppConfig,
    workspace_name: Option<&str>,
) -> anyhow::Result<Backend> {
    let selected = workspace_name
        .and_then(|name| config.workspaces.get(name))
        .and_then(|ws| ws.runtime.backend.as_deref())
        .or(config.runtime.default_backend.as_deref());
    match selected {
        None | Some(crate::apple_container_client::DOCKER_BACKEND_NAME) => Ok(Backend::Docker),
        Some(crate::apple_container_client::BACKEND_NAME) => Ok(Backend::AppleContainer),
        Some(other) => anyhow::bail!(
            "unknown runtime backend {other:?}: expected `{}` or `{}`",
            crate::apple_container_client::DOCKER_BACKEND_NAME,
            crate::apple_container_client::BACKEND_NAME,
        ),
    }
}

/// Translate a [`MaterializedWorkspace`] into `(host, guest)` mount pairs for
/// the apple-container backend (which formats its own `-v host:container`
/// flags via the `container` CLI). Mirrors [`build_workspace_mount_strings`]
/// but yields typed path pairs. Read-only flags and the worktree-mode `.git`
/// override entries are not yet carried — tracked as apple-container Phase 0
/// work, since they need empirical validation inside an apple/container VM.
pub(crate) fn build_workspace_mount_pairs(
    workspace: &MaterializedWorkspace,
) -> Vec<(PathBuf, PathBuf)> {
    crate::isolation::materialize::mount_order_for_docker(workspace)
        .into_iter()
        .map(|mount| (PathBuf::from(&mount.bind_src), PathBuf::from(&mount.dst)))
        .collect()
}
