// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! Dedicated-provisioner agent slot methods.

use super::{
    AuthProvisionOutcome, InstanceAuthBinding, ProvisionedInstanceAuth, RoleState, agent_slot_dirs,
    auth, slot_home_rel, slot_layout,
};

use jackin_config::AuthForwardMode;

use std::path::Path;

impl RoleState {
    pub(crate) fn provision_claude_slot(
        root: &Path,
        home_dir: &Path,
        host_home: &Path,
        binding: &InstanceAuthBinding,
        suffix: Option<&str>,
    ) -> anyhow::Result<(ProvisionedInstanceAuth, AuthProvisionOutcome)> {
        let mode = binding.mode;
        let sync_source_dir = binding.provision_source_dir();
        let (store, home_rel) = agent_slot_dirs(binding.agent);
        let layout = slot_layout(binding.agent, store, home_rel, suffix);
        let claude_dir = root.join(&layout.store_rel);
        let claude_home_dir = home_dir.join(&layout.home_rel);
        std::fs::create_dir_all(&claude_dir)?;
        std::fs::create_dir_all(&claude_home_dir)?;
        // 0o600 because the Claude CLI may later persist OAuth state
        // into this file once the container runs.
        let claude_account_home = claude_home_dir.join(".claude.json");
        auth::create_private_file_if_absent(&claude_account_home, b"{}")?;
        let account_json = claude_dir.join("account.json");
        let credentials_json = claude_dir.join("credentials.json");
        let (outcome, forward_auth) = if let Some(source_dir) = sync_source_dir {
            Self::provision_claude_auth_from_config_dir(
                &account_json,
                &credentials_json,
                mode,
                host_home,
                source_dir,
            )?
        } else {
            Self::provision_claude_auth(&account_json, &credentials_json, mode, host_home)?
        };
        let slot = ProvisionedInstanceAuth::new(
            binding,
            Some(claude_home_dir),
            vec![account_json, credentials_json],
            forward_auth,
            layout,
        );
        Ok((slot, outcome))
    }

    pub(crate) fn provision_codex_slot(
        root: &Path,
        home_dir: &Path,
        host_home: &Path,
        binding: &InstanceAuthBinding,
        suffix: Option<&str>,
    ) -> anyhow::Result<(ProvisionedInstanceAuth, AuthProvisionOutcome)> {
        let mode = binding.mode;
        let sync_source_dir = binding.provision_source_dir();
        let (store, home_rel) = agent_slot_dirs(binding.agent);
        let layout = slot_layout(binding.agent, store, home_rel, suffix);
        let codex_dir = root.join(&layout.store_rel);
        let codex_home_dir = home_dir.join(&layout.home_rel);
        std::fs::create_dir_all(&codex_dir)?;
        std::fs::create_dir_all(&codex_home_dir)?;
        let auth_json_path = codex_dir.join("auth.json");
        let (outcome, auth_json) = if let Some(source_dir) = sync_source_dir {
            Self::provision_codex_auth_from_source_dir(&auth_json_path, mode, source_dir)?
        } else {
            Self::provision_codex_auth(&auth_json_path, mode, host_home)?
        };
        let credential_paths = auth_json.into_iter().collect::<Vec<_>>();
        let forward_auth = !credential_paths.is_empty();
        let slot = ProvisionedInstanceAuth::new(
            binding,
            Some(codex_home_dir),
            credential_paths,
            forward_auth,
            layout,
        );
        Ok((slot, outcome))
    }

    pub(crate) fn provision_amp_slot(
        root: &Path,
        home_dir: &Path,
        host_home: &Path,
        binding: &InstanceAuthBinding,
        suffix: Option<&str>,
    ) -> anyhow::Result<(ProvisionedInstanceAuth, AuthProvisionOutcome)> {
        let mode = binding.mode;
        let sync_source_dir = binding.provision_source_dir();
        let xdg_data_dir = binding
            .xdg_roots
            .as_ref()
            .map(|roots| roots.data.join("amp"));
        let credential_source_dir = binding
            .selected_source
            .as_ref()
            .map(auth::SelectedAuthSourceSnapshot::materialized_source_dir)
            .or(xdg_data_dir.as_deref())
            .or(sync_source_dir);
        let (store, home_rel) = agent_slot_dirs(binding.agent);
        let layout = slot_layout(binding.agent, store, home_rel, suffix);
        let amp_dir = root.join(&layout.store_rel);
        let amp_home_dir = home_dir.join(&layout.home_rel);
        let amp_config_dir = home_dir.join(slot_home_rel(".config/amp", suffix));
        std::fs::create_dir_all(&amp_dir)?;
        std::fs::create_dir_all(&amp_home_dir)?;
        std::fs::create_dir_all(&amp_config_dir)?;
        if mode == AuthForwardMode::Sync {
            let settings = binding
                .xdg_roots
                .as_ref()
                .map(|roots| roots.config.join("amp/settings.json"))
                .or_else(|| sync_source_dir.map(|source| source.join("config/amp/settings.json")));
            if let Some(settings) = settings.filter(|path| path.is_file()) {
                std::fs::copy(settings, amp_config_dir.join("settings.json"))?;
            }
        }
        let secrets_json_path = amp_dir.join("secrets.json");
        let (outcome, secrets_json) = if let Some(source_dir) = credential_source_dir {
            Self::provision_amp_auth_from_source_dir(&secrets_json_path, mode, source_dir)?
        } else {
            Self::provision_amp_auth(&secrets_json_path, mode, host_home)?
        };
        let credential_paths = secrets_json.into_iter().collect::<Vec<_>>();
        let forward_auth = !credential_paths.is_empty();
        let slot = ProvisionedInstanceAuth::new(
            binding,
            Some(amp_home_dir),
            credential_paths,
            forward_auth,
            layout,
        )
        .with_xdg_cache(home_dir, binding, suffix)?;
        Ok((slot, outcome))
    }

    pub(crate) fn provision_kimi_slot(
        root: &Path,
        home_dir: &Path,
        host_home: &Path,
        binding: &InstanceAuthBinding,
        suffix: Option<&str>,
    ) -> anyhow::Result<(ProvisionedInstanceAuth, AuthProvisionOutcome)> {
        let mode = binding.mode;
        let sync_source_dir = binding.provision_source_dir();
        let (store, home_rel) = agent_slot_dirs(binding.agent);
        let layout = slot_layout(binding.agent, store, home_rel, suffix);
        let kimi_dir = root.join(&layout.store_rel);
        let kimi_home_dir = home_dir.join(&layout.home_rel);
        std::fs::create_dir_all(&kimi_dir)?;
        std::fs::create_dir_all(&kimi_home_dir)?;
        let (outcome, forward_auth) = if let Some(source_dir) = sync_source_dir {
            Self::provision_kimi_auth_from_source_dir(&kimi_dir, mode, source_dir)?
        } else {
            Self::provision_kimi_auth(&kimi_dir, mode, host_home)?
        };
        let slot = ProvisionedInstanceAuth::new(
            binding,
            Some(kimi_home_dir),
            vec![kimi_dir],
            forward_auth,
            layout,
        );
        Ok((slot, outcome))
    }

    pub(crate) fn provision_opencode_slot(
        root: &Path,
        home_dir: &Path,
        host_home: &Path,
        binding: &InstanceAuthBinding,
        suffix: Option<&str>,
    ) -> anyhow::Result<(ProvisionedInstanceAuth, AuthProvisionOutcome)> {
        let mode = binding.mode;
        let sync_source_dir = binding.provision_source_dir();
        let xdg_data_dir = binding
            .xdg_roots
            .as_ref()
            .map(|roots| roots.data.join("opencode"));
        let credential_source_dir = binding
            .selected_source
            .as_ref()
            .map(auth::SelectedAuthSourceSnapshot::materialized_source_dir)
            .or(xdg_data_dir.as_deref())
            .or(sync_source_dir);
        let (store, home_rel) = agent_slot_dirs(binding.agent);
        let layout = slot_layout(binding.agent, store, home_rel, suffix);
        let opencode_dir = root.join(&layout.store_rel);
        let opencode_home_dir = home_dir.join(&layout.home_rel);
        std::fs::create_dir_all(&opencode_dir)?;
        std::fs::create_dir_all(&opencode_home_dir)?;
        std::fs::create_dir_all(home_dir.join(slot_home_rel(".config/opencode", suffix)))?;
        let auth_json_path = opencode_dir.join("auth.json");
        let (outcome, auth_json) = if let Some(source_dir) = credential_source_dir {
            Self::provision_opencode_auth_from_source_dir(
                &auth_json_path,
                mode,
                source_dir,
                binding.source_provider,
            )?
        } else {
            Self::provision_opencode_auth(&auth_json_path, mode, host_home)?
        };
        let credential_paths = auth_json.into_iter().collect::<Vec<_>>();
        let forward_auth = !credential_paths.is_empty();
        let slot = ProvisionedInstanceAuth::new(
            binding,
            Some(opencode_home_dir),
            credential_paths,
            forward_auth,
            layout,
        )
        .with_xdg_cache(home_dir, binding, suffix)?;
        Ok((slot, outcome))
    }

    pub(crate) fn provision_omp_slot(
        root: &Path,
        home_dir: &Path,
        host_home: &Path,
        binding: &InstanceAuthBinding,
        suffix: Option<&str>,
    ) -> anyhow::Result<(ProvisionedInstanceAuth, AuthProvisionOutcome)> {
        let mode = binding.mode;
        let sync_source_dir = binding.provision_source_dir();
        let (store, home_rel) = agent_slot_dirs(binding.agent);
        let layout = slot_layout(binding.agent, store, home_rel, suffix);
        let omp_dir = root.join(&layout.store_rel);
        let omp_home_dir = home_dir.join(&layout.home_rel);
        std::fs::create_dir_all(&omp_dir)?;
        std::fs::create_dir_all(&omp_home_dir)?;
        let agent_db_path = omp_dir.join("agent.db");
        let (outcome, agent_db) = if let Some(source_dir) = sync_source_dir {
            Self::provision_omp_auth_from_source_dir(
                &agent_db_path,
                mode,
                source_dir,
                binding.source_provider,
                binding.source_selector.as_ref(),
            )?
        } else {
            Self::provision_omp_auth(
                &agent_db_path,
                mode,
                host_home,
                binding.source_provider,
                binding.source_selector.as_ref(),
            )?
        };
        let credential_paths = agent_db.into_iter().collect::<Vec<_>>();
        let forward_auth = !credential_paths.is_empty();
        let slot = ProvisionedInstanceAuth::new(
            binding,
            Some(omp_home_dir),
            credential_paths,
            forward_auth,
            layout,
        );
        Ok((slot, outcome))
    }

    pub(crate) fn provision_hermes_slot(
        root: &Path,
        home_dir: &Path,
        host_home: &Path,
        binding: &InstanceAuthBinding,
        suffix: Option<&str>,
    ) -> anyhow::Result<(ProvisionedInstanceAuth, AuthProvisionOutcome)> {
        let mode = binding.mode;
        let sync_source_dir = binding.provision_source_dir();
        let (store, home_rel) = agent_slot_dirs(binding.agent);
        let layout = slot_layout(binding.agent, store, home_rel, suffix);
        let hermes_dir = root.join(&layout.store_rel);
        let hermes_home_dir = home_dir.join(&layout.home_rel);
        std::fs::create_dir_all(&hermes_dir)?;
        std::fs::create_dir_all(&hermes_home_dir)?;
        let (outcome, forward_auth) = if let Some(source_dir) = sync_source_dir {
            Self::provision_hermes_auth_from_source_dir(
                &hermes_dir,
                mode,
                source_dir,
                binding.source_provider,
                binding.source_selector.as_ref(),
            )?
        } else {
            Self::provision_hermes_auth(
                &hermes_dir,
                mode,
                host_home,
                binding.source_provider,
                binding.source_selector.as_ref(),
            )?
        };
        let slot = ProvisionedInstanceAuth::new(
            binding,
            Some(hermes_home_dir),
            vec![hermes_dir],
            forward_auth,
            layout,
        );
        Ok((slot, outcome))
    }
}
