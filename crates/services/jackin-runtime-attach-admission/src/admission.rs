// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! Reconnect admission types and current-account/instance admission checks.

use anyhow::Context as _;
use jackin_instance::{InstanceIndex, InstanceManifest, RegistrationState};

use jackin_core::ContainerHandle;

use jackin_core::JackinPaths;

#[derive(Debug)]
pub struct ReconnectAdmissionFailure(String);

impl std::fmt::Display for ReconnectAdmissionFailure {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "reconnect admission failed: {}", self.0)
    }
}

impl std::error::Error for ReconnectAdmissionFailure {}

pub fn mark_reconnect_admission_failure(error: anyhow::Error) -> anyhow::Error {
    let summary = error.to_string();
    error.context(ReconnectAdmissionFailure(summary))
}

/// A fresh inspection proves liveness, never ownership. Only the launch-recorded
/// immutable ID authorizes use of an existing role's retained state/credentials.
pub fn validate_recorded_role_handle(
    paths: &JackinPaths,
    container_name: &str,
    container: &ContainerHandle,
) -> anyhow::Result<()> {
    let manifest = InstanceManifest::read(&paths.data_dir.join(container_name)).context(
        "Docker ownership identity unavailable; recover the original launch identity explicitly",
    )?;
    let expected_id = manifest
        .docker_identity
        .as_ref()
        .map(|identity| identity.role_container_id.as_str())
        .filter(|id| !id.is_empty())
        .ok_or_else(|| anyhow::anyhow!(
            "Docker ownership identity unavailable for {container_name}; refusing lifecycle changes; recover the original launch identity explicitly"
        ))?;
    anyhow::ensure!(
        container.name() == container_name
            && manifest.docker.role_container == container_name
            && container.id() == expected_id,
        "Docker ownership identity mismatch for {container_name}: recorded {expected_id}, found {}; refusing lifecycle changes",
        container.id()
    );
    Ok(())
}

/// Existing containers retain credential material: every attach route must
/// recheck their recorded admission against current host policy before use.
pub fn require_current_account_admission(
    paths: &JackinPaths,
    container_name: &str,
) -> anyhow::Result<jackin_runtime_launch_account_identity::account_identity::AccountConfigRevision>
{
    let admission_lease =
        jackin_runtime_launch_account_identity::account_identity::AccountConfigRevision::acquire(
            paths,
        )?;
    validate_current_account_admission(paths, container_name, &admission_lease)?;
    Ok(admission_lease)
}

pub fn validate_current_account_admission(
    paths: &JackinPaths,
    container_name: &str,
    admission_lease: &jackin_runtime_launch_account_identity::account_identity::AccountConfigRevision,
) -> anyhow::Result<()> {
    let root = paths.data_dir.join(container_name);
    let manifest = InstanceManifest::read(&root)
        .context("cannot verify this container's account policy; recreate it with `jackin load`")?;
    let manifest = refresh_registration_states(paths, &root, manifest)?;
    current_account_admission(paths, &root, &manifest)?;
    admission_lease.ensure_current(paths)?;
    Ok(())
}

pub fn refresh_registration_states(
    paths: &JackinPaths,
    root: &std::path::Path,
    mut manifest: InstanceManifest,
) -> anyhow::Result<InstanceManifest> {
    let snapshot = jackin_config::load_read_only_config_snapshot(paths)?;
    if !snapshot.diagnostics.is_empty() {
        return Ok(manifest);
    }
    let workspace = manifest
        .workspace_name
        .as_deref()
        .map(jackin_core::WorkspaceName::parse)
        .transpose()?;
    let mut changed = false;
    for admitted in manifest.admitted_instances.clone() {
        let state =
            registration_state_for_admission(&snapshot.config, workspace.as_ref(), &admitted);
        changed |= manifest.mark_registration_state(&admitted.config_id, state);
    }
    if changed {
        manifest.touch();
        manifest.write(root)?;
        InstanceIndex::update_manifest(&paths.data_dir, &manifest)?;
    }
    Ok(manifest)
}

pub fn registration_state_for_admission(
    config: &jackin_config::AppConfig,
    workspace: Option<&jackin_core::WorkspaceName>,
    admitted: &jackin_instance::AdmittedInstance,
) -> RegistrationState {
    let Some(account) = config.accounts.get(&admitted.account_id) else {
        return RegistrationState::Removed;
    };
    if !account.enabled || !account.supports_agent(admitted.agent) {
        return RegistrationState::Disabled;
    }
    if workspace.is_some_and(|workspace| {
        !config
            .workspaces
            .get(workspace.as_str())
            .is_some_and(|workspace| workspace.accounts.contains(&admitted.account_id))
    }) {
        return RegistrationState::Disabled;
    }
    match config.agent_configurations.get(&admitted.config_id) {
        Some(configuration)
            if configuration.agent == admitted.agent
                && configuration.account == admitted.account_id =>
        {
            RegistrationState::Current
        }
        Some(_) => RegistrationState::Removed,
        None if admitted.config_id
            == format!("{}@{}", admitted.account_id, admitted.agent.slug()) =>
        {
            RegistrationState::Current
        }
        None => RegistrationState::Removed,
    }
}

pub fn current_account_admission(
    paths: &JackinPaths,
    root: &std::path::Path,
    manifest: &InstanceManifest,
) -> anyhow::Result<(jackin_config::AppConfig, Option<jackin_core::WorkspaceName>)> {
    let snapshot = jackin_config::load_read_only_config_snapshot(paths)
        .context("cannot read current account policy")?;
    anyhow::ensure!(
        snapshot.diagnostics.is_empty(),
        "current account configuration is unavailable or invalid; reconnect denied"
    );
    let workspace = manifest
        .workspace_name
        .as_deref()
        .map(jackin_core::WorkspaceName::parse)
        .transpose()?;
    anyhow::ensure!(
        jackin_runtime_launch_account_identity::account_identity::account_admission_matches(
            root,
            &snapshot.config,
            workspace.as_ref(),
            &manifest.role_key
        )?,
        "container account policy changed or cannot be verified; recreate it with `jackin load`"
    );
    Ok((snapshot.config, workspace))
}

/// Revalidate a requested new-session target against the manifest captured by
/// the live container and the current host account policy. Every v3 manifest
/// must carry an explicit admission set; the function never resolves an
/// account by provider/name and never silently substitutes an unqualified
/// same-agent row.
pub fn require_current_instance_admission(
    paths: &JackinPaths,
    container_name: &str,
    agent: jackin_core::Agent,
    requested_instance_id: Option<&str>,
    admission_lease: &jackin_runtime_launch_account_identity::account_identity::AccountConfigRevision,
) -> anyhow::Result<(InstanceManifest, Option<String>)> {
    let root = paths.data_dir.join(container_name);
    let manifest = InstanceManifest::read(&root).context(
        "cannot verify this container's live instance admission; recreate it with `jackin load`",
    )?;
    let manifest = refresh_registration_states(paths, &root, manifest)?;

    let target = if let Some(requested) = requested_instance_id {
        manifest
            .admitted_instances
            .iter()
            .find(|admitted| admitted.config_id == requested)
            .ok_or_else(|| {
                anyhow::anyhow!(
                    "instance {requested:?} is not admitted by the live container manifest"
                )
            })?
    } else {
        let mut matches = manifest
            .admitted_instances
            .iter()
            .filter(|admitted| admitted.agent == agent);
        let Some(target) = matches.next() else {
            anyhow::bail!("agent {agent} is not admitted by the live container manifest");
        };
        anyhow::ensure!(
            matches.next().is_none(),
            "agent {agent} has multiple live instances; select an exact instance ID"
        );
        target
    };
    anyhow::ensure!(
        target.agent == agent,
        "instance {:?} is admitted for {}, not {}",
        target.config_id,
        target.agent,
        agent
    );
    let target_id = target.config_id.clone();

    if target.registration_state != RegistrationState::Current {
        anyhow::bail!(
            "container account policy changed: admitted account {:?} registration is {}; stop and recreate the instance before requesting a new session",
            target.account_id,
            target.registration_state.label()
        );
    }

    let (config, workspace) = current_account_admission(paths, &root, &manifest)?;

    let account = config.accounts.get(&target.account_id).ok_or_else(|| {
        anyhow::anyhow!(
            "admitted account {:?} is no longer registered",
            target.account_id
        )
    })?;
    anyhow::ensure!(
        account.enabled && account.supports_agent(agent),
        "admitted account {:?} no longer authorizes {}",
        target.account_id,
        agent
    );
    if let Some(workspace) = workspace.as_ref() {
        anyhow::ensure!(
            config
                .workspaces
                .get(workspace.as_str())
                .is_some_and(|workspace| workspace.accounts.contains(&target.account_id)),
            "admitted account {:?} is no longer assigned to workspace {:?}",
            target.account_id,
            workspace
        );
    }
    if let Some(configuration) = config.agent_configurations.get(&target_id) {
        anyhow::ensure!(
            configuration.agent == agent && configuration.account == target.account_id,
            "live instance {target_id:?} no longer matches the current agent/account configuration"
        );
    } else {
        anyhow::ensure!(
            target_id == format!("{}@{}", target.account_id, agent.slug()),
            "live instance {target_id:?} no longer exists in the current account configuration"
        );
    }

    // The capsule receives the same exact ID and performs the final immutable
    // launch-config admission check before creating the PTY.
    admission_lease.ensure_current(paths)?;
    Ok((manifest, Some(target_id)))
}
