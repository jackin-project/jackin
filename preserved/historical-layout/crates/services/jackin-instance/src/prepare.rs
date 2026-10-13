// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! `RoleState` preparation for agents and bindings.

use super::{
    AgentRuntimeState, AuthProvisionOutcome, DEFAULT_ACCOUNT_ID, GithubAuthContext,
    GithubProvisionOutcome, InstanceAuthBinding, InstanceError, PrepareResolvers, ProvisionedAuth,
    RoleState, auth, capture_selected_account_sources, emit_agent_auth_provision,
    github_ignore_can_skip_state_prepare, slot_suffixes,
};

use jackin_core::JackinPaths;
use jackin_manifest::RoleManifest;
use std::collections::BTreeMap;

use std::path::Path;

impl RoleState {
    /// Provision auth state for every agent in `manifest.supported_agents()` by
    /// delegating to [`Self::prepare_for_agents`] with the full supported set.
    ///
    /// `resolvers.auth_modes` is invoked once per agent — pass
    /// `jackin_config::resolve_mode(config, a, ws, role)` so each agent gets
    /// its own configured forward mode. Reusing the *selected* agent's mode for
    /// sibling agents silently wipes their durable state when modes diverge
    /// (e.g. `claude.auth_forward = sync` next to `codex.auth_forward =
    /// api_key`).
    ///
    /// `resolvers.sync_source_dirs` returns an optional override source
    /// directory for each agent's auth sync, overriding `host_home`.
    pub fn prepare(
        paths: &JackinPaths,
        container_name: &str,
        manifest: &RoleManifest,
        resolvers: &PrepareResolvers<'_>,
        github: &GithubAuthContext,
        host_home: &Path,
        agent: jackin_core::Agent,
    ) -> anyhow::Result<(Self, AuthProvisionOutcome)> {
        Self::prepare_for_agents(
            paths,
            container_name,
            manifest,
            resolvers,
            github,
            host_home,
            agent,
            &manifest.supported_agents(),
        )
    }

    /// Provision auth state only for the provided agents.
    ///
    /// Accepts an explicit `provision_agents` list so callers that intentionally
    /// need only a subset (such as tests) can pass a narrower slice. The
    /// foreground launch path passes the full `manifest.supported_agents()` set;
    /// [`Self::prepare`] is a convenience wrapper that does the same.
    ///
    /// Each agent resolves to one [`InstanceAuthBinding`] with a
    /// placeholder account id; multi-instance callers use
    /// [`Self::prepare_for_bindings`] directly so two instances of the
    /// same agent provision independently.
    #[expect(
        clippy::too_many_arguments,
        reason = "Per-agent prepare carries every per-agent + per-container input \
                  the role-materialize path needs: paths, container identity, \
                  role selectors, validated repo, agent list, env resolver, \
                  workspace. Bundling is a parallel-pass refactor."
    )]
    pub fn prepare_for_agents(
        paths: &JackinPaths,
        container_name: &str,
        manifest: &RoleManifest,
        resolvers: &PrepareResolvers<'_>,
        github: &GithubAuthContext,
        host_home: &Path,
        agent: jackin_core::Agent,
        provision_agents: &[jackin_core::Agent],
    ) -> anyhow::Result<(Self, AuthProvisionOutcome)> {
        let supported = manifest.supported_agents();
        let bindings: Vec<InstanceAuthBinding> = provision_agents
            .iter()
            .copied()
            .filter(|provision_agent| supported.contains(provision_agent))
            .map(|provisioned| {
                InstanceAuthBinding::new(
                    DEFAULT_ACCOUNT_ID,
                    provisioned,
                    (resolvers.auth_modes)(provisioned),
                    (resolvers.sync_source_dirs)(provisioned),
                )
            })
            .collect();
        Self::prepare_for_bindings(
            paths,
            container_name,
            manifest,
            &bindings,
            github,
            host_home,
            agent,
        )
    }

    /// Provision auth state for one explicit instance binding each.
    ///
    /// Unlike [`Self::prepare_for_agents`], bindings carry their own
    /// account/mode/key, so two bindings for the same agent provision
    /// independently and merge as separate [`ProvisionedAuth::slots`]
    /// entries. Binding keys must be unique within `bindings`.
    pub fn prepare_for_bindings(
        paths: &JackinPaths,
        container_name: &str,
        manifest: &RoleManifest,
        bindings: &[InstanceAuthBinding],
        github: &GithubAuthContext,
        host_home: &Path,
        agent: jackin_core::Agent,
    ) -> anyhow::Result<(Self, AuthProvisionOutcome)> {
        let root = paths.data_dir.join(container_name);
        let gh_config_dir = root.join(".config/gh");
        let home_dir = root.join("home");
        let jackin_state_dir = root.join("state");

        std::fs::create_dir_all(&home_dir)?;
        // Owned by the host operator; the container runs as that same UID
        // (`--user` on docker run), so `agent` can write state files here
        // with no special directory mode.
        std::fs::create_dir_all(&jackin_state_dir)?;

        let hosts_yml = gh_config_dir.join("hosts.yml");
        let github_context = github.clone();
        let bindings = capture_selected_account_sources(bindings, host_home, &root)?;

        let host_home_path = host_home.to_path_buf();
        let root_path = root.clone();
        let home_path = home_dir.clone();

        let suffixes = slot_suffixes(&bindings);
        let (gh_provision_outcome, auth_provisions) = std::thread::scope(|scope| {
            let mut handles = Vec::with_capacity(bindings.len());
            for (binding, suffix) in bindings.iter().zip(suffixes) {
                let root = root_path.clone();
                let home_dir = home_path.clone();
                let host_home = host_home_path.clone();
                let provisioned = binding.agent;
                let mode = binding.mode;
                let binding = binding.clone();
                let handle = jackin_telemetry::spawn::thread_scoped_joined(scope, move || {
                    Self::provision_agent_auth_slot(
                        &root,
                        &home_dir,
                        &host_home,
                        &binding,
                        suffix.as_deref(),
                    )
                });
                handles.push((provisioned, mode, handle));
            }

            let gh_provision_outcome =
                if github_ignore_can_skip_state_prepare(&github_context, &hosts_yml)? {
                    jackin_diagnostics::active_timing_started(
                        jackin_diagnostics::DiagnosticStage::Credentials,
                        "role_state_prepare:github_auth",
                        Some(&github_context.mode.to_string()),
                    );
                    jackin_diagnostics::active_timing_done(
                        jackin_diagnostics::DiagnosticStage::Credentials,
                        "role_state_prepare:github_auth",
                        Some("skipped_no_state"),
                    );
                    GithubProvisionOutcome::Skipped
                } else {
                    let gh_handle = jackin_telemetry::spawn::thread_scoped_joined(scope, {
                        let hosts_yml = hosts_yml.clone();
                        let host_home = host_home_path.clone();
                        move || Self::provision_github_slot(&hosts_yml, &github_context, &host_home)
                    });
                    gh_handle
                        .join()
                        .map_err(|_| InstanceError::GithubAuthTaskPanicked)??
                };

            let mut auth_provisions = Vec::with_capacity(handles.len());
            for (agent, mode, handle) in handles {
                match handle.join() {
                    Ok(Ok(provision)) => {
                        emit_agent_auth_provision(agent, mode, Ok(provision.outcome));
                        auth_provisions.push(provision);
                    }
                    Ok(Err(error)) => {
                        emit_agent_auth_provision(
                            agent,
                            mode,
                            Err(jackin_telemetry::schema::enums::ErrorType::IoError),
                        );
                        return Err(error);
                    }
                    Err(_) => {
                        emit_agent_auth_provision(
                            agent,
                            mode,
                            Err(jackin_telemetry::schema::enums::ErrorType::Panic),
                        );
                        return Err(InstanceError::AuthProvisionTaskPanicked {
                            agent: agent.slug().to_owned(),
                        }
                        .into());
                    }
                }
            }

            anyhow::Ok((gh_provision_outcome, auth_provisions))
        })?;

        let mut auth = ProvisionedAuth::default();
        let mut auth_outcomes = BTreeMap::new();
        let mut selected_outcome = AuthProvisionOutcome::Skipped;

        for provision in auth_provisions {
            if provision.auth.agent == agent {
                selected_outcome = provision.outcome;
            }
            auth_outcomes.insert(provision.auth.agent, provision.outcome);
            auth.slots.insert(provision.key, provision.auth);
        }

        let (auth_mount_paths, auth_mount_leases) = auth::admit_auth_mounts(&auth)?;

        // Single struct construction — no per-variant dispatch needed.
        let agent_runtime = AgentRuntimeState {
            agent,
            model: manifest.agent_model(agent).map(str::to_owned),
        };

        Ok((
            Self {
                root,
                gh_config_dir,
                gh_provision_outcome,
                agent_runtime,
                auth,
                auth_outcomes,
                auth_mount_paths,
                auth_mount_leases,
                provider_config_mounts: Vec::new(),
            },
            selected_outcome,
        ))
    }
}
