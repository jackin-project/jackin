// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! Single-file-credential agent slot methods.

use super::{
    AuthProvisionOutcome, InstanceAuthBinding, ProvisionedInstanceAuth, RoleState, agent_slot_dirs,
    slot_layout,
};

use std::path::Path;

impl RoleState {
    pub(crate) fn provision_grok_slot(
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
        let grok_dir = root.join(&layout.store_rel);
        let grok_home_dir = home_dir.join(&layout.home_rel);
        std::fs::create_dir_all(&grok_dir)?;
        std::fs::create_dir_all(&grok_home_dir)?;
        let auth_json_path = grok_dir.join("auth.json");
        let (outcome, auth_json) = if let Some(source_dir) = sync_source_dir {
            Self::provision_grok_auth_from_source_dir(&auth_json_path, mode, source_dir)?
        } else {
            Self::provision_grok_auth(&auth_json_path, mode, host_home)?
        };

        let credential_paths = auth_json.into_iter().collect::<Vec<_>>();
        let forward_auth = !credential_paths.is_empty();
        let slot = ProvisionedInstanceAuth::new(
            binding,
            Some(grok_home_dir),
            credential_paths,
            forward_auth,
            layout,
        );
        Ok((slot, outcome))
    }

    pub(crate) fn provision_antigravity_slot(
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
        let antigravity_dir = root.join(&layout.store_rel);
        let antigravity_home_dir = home_dir.join(&layout.home_rel);
        std::fs::create_dir_all(&antigravity_dir)?;
        std::fs::create_dir_all(&antigravity_home_dir)?;
        let settings_json_path = antigravity_dir.join("settings.json");
        let (outcome, settings_json) = if let Some(source_dir) = sync_source_dir {
            Self::provision_antigravity_auth_from_source_dir(&settings_json_path, mode, source_dir)?
        } else {
            Self::provision_antigravity_auth(&settings_json_path, mode, host_home)?
        };
        let credential_paths = settings_json.into_iter().collect::<Vec<_>>();
        let forward_auth = !credential_paths.is_empty();
        let slot = ProvisionedInstanceAuth::new(
            binding,
            Some(antigravity_home_dir),
            credential_paths,
            forward_auth,
            layout,
        );
        Ok((slot, outcome))
    }

    pub(crate) fn provision_gemini_slot(
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
        let gemini_dir = root.join(&layout.store_rel);
        let gemini_home_dir = home_dir.join(&layout.home_rel);
        std::fs::create_dir_all(&gemini_dir)?;
        std::fs::create_dir_all(&gemini_home_dir)?;
        let oauth_creds_path = gemini_dir.join("oauth_creds.json");
        let (outcome, oauth_creds) = if let Some(source_dir) = sync_source_dir {
            Self::provision_gemini_auth_from_source_dir(&oauth_creds_path, mode, source_dir)?
        } else {
            Self::provision_gemini_auth(&oauth_creds_path, mode, host_home)?
        };
        let credential_paths = oauth_creds.into_iter().collect::<Vec<_>>();
        let forward_auth = !credential_paths.is_empty();
        let slot = ProvisionedInstanceAuth::new(
            binding,
            Some(gemini_home_dir),
            credential_paths,
            forward_auth,
            layout,
        );
        Ok((slot, outcome))
    }

    pub(crate) fn provision_cursor_slot(
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
        let cursor_dir = root.join(&layout.store_rel);
        let cursor_home_dir = home_dir.join(&layout.home_rel);
        std::fs::create_dir_all(&cursor_dir)?;
        std::fs::create_dir_all(&cursor_home_dir)?;
        let auth_json_path = cursor_dir.join("auth.json");
        let (outcome, auth_json) = if let Some(source_dir) = sync_source_dir {
            Self::provision_cursor_auth_from_source_dir(&auth_json_path, mode, source_dir)?
        } else {
            Self::provision_cursor_auth(&auth_json_path, mode, host_home)?
        };
        let credential_paths = auth_json.into_iter().collect::<Vec<_>>();
        let forward_auth = !credential_paths.is_empty();
        let slot = ProvisionedInstanceAuth::new(
            binding,
            Some(cursor_home_dir),
            credential_paths,
            forward_auth,
            layout,
        );
        Ok((slot, outcome))
    }

    pub(crate) fn provision_muse_slot(
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
        let muse_dir = root.join(&layout.store_rel);
        let muse_home_dir = home_dir.join(&layout.home_rel);
        std::fs::create_dir_all(&muse_dir)?;
        std::fs::create_dir_all(&muse_home_dir)?;
        let auth_json_path = muse_dir.join("auth.json");
        let (outcome, auth_json) = if let Some(source_dir) = sync_source_dir {
            Self::provision_muse_auth_from_source_dir(&auth_json_path, mode, source_dir)?
        } else {
            Self::provision_muse_auth(&auth_json_path, mode, host_home)?
        };
        let credential_paths = auth_json.into_iter().collect::<Vec<_>>();
        let forward_auth = !credential_paths.is_empty();
        let slot = ProvisionedInstanceAuth::new(
            binding,
            Some(muse_home_dir),
            credential_paths,
            forward_auth,
            layout,
        );
        Ok((slot, outcome))
    }
}
