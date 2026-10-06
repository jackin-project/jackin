// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! Auth slot provisioning and prewarm.

use crate::{PrepareResolvers, RoleState};
use anyhow::Context;
use jackin_config::AuthForwardMode;
use jackin_core::JackinPaths;
use jackin_instance_agents::{
    AgentAuthProvision, DEFAULT_ACCOUNT_ID, InstanceAuthBinding, ProvisionedInstanceAuth,
    agent_ignore_can_skip_state_prepare, capture_selected_account_sources,
    emit_agent_auth_provision, skipped_ignore_instance_auth, slot_suffixes,
};
use jackin_instance_credentials::{
    AuthProvisionOutcome, GithubAuthContext, GithubProvisionOutcome, InstanceError,
};
use jackin_manifest::RoleManifest;

use std::path::Path;

impl RoleState {
    pub(crate) fn provision_github_slot(
        hosts_yml: &Path,
        github: &GithubAuthContext,
        host_home: &Path,
    ) -> anyhow::Result<GithubProvisionOutcome> {
        jackin_diagnostics::active_timing_started(
            jackin_diagnostics::DiagnosticStage::Credentials,
            "role_state_prepare:github_auth",
            Some(&github.mode.to_string()),
        );
        if let Some(parent) = hosts_yml.parent() {
            std::fs::create_dir_all(parent).with_context(|| {
                format!(
                    "failed to create GitHub role-state directory at {}",
                    parent.display()
                )
            })?;
        }
        let result = jackin_instance_agents::provision_github_auth(hosts_yml, github, host_home);
        jackin_diagnostics::active_timing_done(
            jackin_diagnostics::DiagnosticStage::Credentials,
            "role_state_prepare:github_auth",
            Some(if result.is_ok() { "prepared" } else { "error" }),
        );
        result
    }

    /// Background-prewarm auth state for non-selected agents only.
    ///
    /// This intentionally skips the GitHub-auth axis and returns no launch
    /// `RoleState`: foreground launch already prepared the selected agent and
    /// GitHub context needed for the current `docker run`. Background sibling
    /// prep may create/update only jackin-owned per-agent state under the
    /// instance data dir so opening a later sibling runtime has less work.
    pub fn prewarm_auth_for_agents(
        paths: &JackinPaths,
        container_name: &str,
        manifest: &RoleManifest,
        resolvers: &PrepareResolvers<'_>,
        host_home: &Path,
        agents: &[jackin_core::Agent],
    ) -> anyhow::Result<usize> {
        let supported = manifest.supported_agents();
        let bindings: Vec<InstanceAuthBinding> = agents
            .iter()
            .copied()
            .filter(|agent| supported.contains(agent))
            .map(|provisioned| {
                InstanceAuthBinding::new(
                    DEFAULT_ACCOUNT_ID,
                    provisioned,
                    (resolvers.auth_modes)(provisioned),
                    (resolvers.sync_source_dirs)(provisioned),
                )
            })
            .collect();
        Self::prewarm_auth_for_bindings(paths, container_name, &bindings, host_home)
    }

    /// Background-prewarm auth state for one explicit instance binding
    /// each. Binding-driven counterpart of
    /// [`Self::prewarm_auth_for_agents`]; see its docs for the
    /// skip-GitHub contract. Bindings are already resolved, so no
    /// manifest filter applies here.
    pub fn prewarm_auth_for_bindings(
        paths: &JackinPaths,
        container_name: &str,
        bindings: &[InstanceAuthBinding],
        host_home: &Path,
    ) -> anyhow::Result<usize> {
        let root = paths.data_dir.join(container_name);
        let home_dir = root.join("home");
        let jackin_state_dir = root.join("state");

        std::fs::create_dir_all(&home_dir)?;
        std::fs::create_dir_all(&jackin_state_dir)?;

        let bindings = capture_selected_account_sources(bindings, host_home, &root)?;

        let host_home_path = host_home.to_path_buf();
        let root_path = root.clone();
        let home_path = home_dir.clone();

        let suffixes = slot_suffixes(&bindings);
        let prepared_auth = std::thread::scope(|scope| {
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

            let mut prepared = Vec::with_capacity(handles.len());
            for (agent, mode, handle) in handles {
                match handle.join() {
                    Ok(Ok(provision)) => {
                        emit_agent_auth_provision(agent, mode, Ok(provision.outcome));
                        prepared.push(provision);
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
                        return Err(InstanceError::BackgroundAuthTaskPanicked.into());
                    }
                }
            }
            anyhow::Ok(prepared)
        })?;

        Ok(prepared_auth.len())
    }

    pub(crate) fn provision_agent_auth_slot(
        root: &Path,
        home_dir: &Path,
        host_home: &Path,
        binding: &InstanceAuthBinding,
        suffix: Option<&str>,
    ) -> anyhow::Result<AgentAuthProvision> {
        let agent = binding.agent;
        let mode = binding.mode;
        let timing_name = format!("role_state_prepare:{}_auth", agent.slug());
        jackin_diagnostics::active_timing_started(
            jackin_diagnostics::DiagnosticStage::Credentials,
            &timing_name,
            Some(&mode.to_string()),
        );
        let ignore_can_skip = if mode == AuthForwardMode::Ignore && binding.xdg_roots.is_none() {
            agent_ignore_can_skip_state_prepare(root, agent, suffix)?
        } else {
            false
        };
        if ignore_can_skip {
            jackin_diagnostics::active_timing_done(
                jackin_diagnostics::DiagnosticStage::Credentials,
                &timing_name,
                Some("skipped_no_state"),
            );
            let provision = AgentAuthProvision {
                key: binding.key.clone(),
                auth: skipped_ignore_instance_auth(root, binding, suffix),
                outcome: AuthProvisionOutcome::Skipped,
            };
            return Ok(provision);
        }
        let provision_result: anyhow::Result<(ProvisionedInstanceAuth, AuthProvisionOutcome)> =
            match agent {
                jackin_core::Agent::Claude => jackin_instance_agents::provision_claude_slot(
                    root, home_dir, host_home, binding, suffix,
                ),
                jackin_core::Agent::Codex => jackin_instance_agents::provision_codex_slot(
                    root, home_dir, host_home, binding, suffix,
                ),
                jackin_core::Agent::Amp => jackin_instance_agents::provision_amp_slot(
                    root, home_dir, host_home, binding, suffix,
                ),
                jackin_core::Agent::Kimi => jackin_instance_agents::provision_kimi_slot(
                    root, home_dir, host_home, binding, suffix,
                ),
                jackin_core::Agent::Opencode => jackin_instance_agents::provision_opencode_slot(
                    root, home_dir, host_home, binding, suffix,
                ),
                jackin_core::Agent::Grok => jackin_instance_agents::provision_grok_slot(
                    root, home_dir, host_home, binding, suffix,
                ),
                jackin_core::Agent::Antigravity => {
                    jackin_instance_agents::provision_antigravity_slot(
                        root, home_dir, host_home, binding, suffix,
                    )
                }
                jackin_core::Agent::Gemini => jackin_instance_agents::provision_gemini_slot(
                    root, home_dir, host_home, binding, suffix,
                ),
                jackin_core::Agent::Cursor => jackin_instance_agents::provision_cursor_slot(
                    root, home_dir, host_home, binding, suffix,
                ),
                jackin_core::Agent::Muse => jackin_instance_agents::provision_muse_slot(
                    root, home_dir, host_home, binding, suffix,
                ),
                jackin_core::Agent::Omp => jackin_instance_agents::provision_omp_slot(
                    root, home_dir, host_home, binding, suffix,
                ),
                jackin_core::Agent::Hermes => jackin_instance_agents::provision_hermes_slot(
                    root, home_dir, host_home, binding, suffix,
                ),
            };
        let timing_detail = provision_result
            .as_ref()
            .map_or("error".to_owned(), |(_, outcome)| format!("{outcome:?}"));
        jackin_diagnostics::active_timing_done(
            jackin_diagnostics::DiagnosticStage::Credentials,
            &timing_name,
            Some(&timing_detail),
        );
        let (slot, outcome) = provision_result?;
        anyhow::ensure!(
            !(mode == AuthForwardMode::Sync
                && (binding.sync_source_dir.is_some() || binding.xdg_roots.is_some())
                && outcome == AuthProvisionOutcome::HostMissing),
            "selected {agent} account credentials disappeared during provisioning"
        );
        Ok(AgentAuthProvision {
            key: binding.key.clone(),
            auth: slot,
            outcome,
        })
    }
}
