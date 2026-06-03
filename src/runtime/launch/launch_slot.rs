//! Container name slot management: claim, lock, and credential verification.

use fs2::FileExt;

use super::super::attach::{ContainerState, docker_unavailable_msg};
use crate::docker_client::DockerApi;
use crate::paths::JackinPaths;
use crate::selector::RoleSelector;

/// Cap retries so a filesystem without working flock (NFS without
/// lockd, exotic mount) surfaces as an actionable error instead of an
/// unbounded spin. 64 attempts at 40 bits of ID entropy is enough that
/// a genuine collision-space exhaustion is astronomically unlikely;
/// hitting the cap signals an environmental fault, not bad luck.
const CLAIM_MAX_ATTEMPTS: u32 = 64;

/// Claim a unique DNS-safe container name by acquiring an exclusive lock file.
/// Random IDs avoid deterministic role slots; the lock still protects the
/// vanishingly small random-collision window and concurrent launch races.
pub(super) async fn claim_container_name(
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
                    docker_unavailable_msg(&format!("claim container name `{name}`"), &reason,)
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

pub(super) async fn claim_known_container_name(
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
                docker_unavailable_msg(&format!("restore `{container_name}`"), &reason,)
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

/// Token-mode pre-flight for the `[github]` axis: `GH_TOKEN` must
/// resolve to a non-empty value before launch proceeds. The other
/// modes (`Sync` / `Ignore`) have nothing to verify here.
///
/// Extracted from `load_role_with` so the bail-message shape and
/// trigger condition can be unit-pinned without orchestrating the
/// full launch flow.
pub(super) fn verify_github_token_present(
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
pub(super) fn resolve_github_env_map(
    declarations: &std::collections::BTreeMap<String, crate::operator_env::EnvValue>,
    opts: &super::LoadOptions,
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
    env_layers: &[(String, super::EnvLayerState)],
    workspace: &str,
    role: &str,
) -> Result<(), super::LaunchError> {
    let Some(env_var) = agent.required_env_var(mode) else {
        return Ok(());
    };
    let value = merged_env.get(env_var).map_or("", String::as_str);
    if !value.is_empty() {
        return Ok(());
    }

    Err(super::LaunchError::AuthCredentialMissing {
        agent,
        mode,
        env_var,
        workspace: workspace.to_string(),
        role: role.to_string(),
        mode_resolution: mode_resolution.to_vec(),
        env_layers: env_layers.to_vec(),
    })
}
