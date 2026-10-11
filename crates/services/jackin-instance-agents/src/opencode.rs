// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! `OpenCode` credential provisioning.

use jackin_instance_credentials::AuthProvisionOutcome;

use anyhow::Context;
use jackin_config::{AiProvider, AuthForwardMode};

use std::path::{Path, PathBuf};

use crate::{
    private_file_exists, provision_single_file_credential, read_source_text,
    select_opencode_auth_entry, wipe_agent_file_state,
};
use jackin_instance_credentials::{reject_auth_path, repair_permissions, write_private_bytes};

/// Provision `OpenCode`'s host-side `auth.json` per the chosen mode.
///
/// Source: `~/.local/share/opencode/auth.json` (`XDG_DATA`).
/// `OpenCode` stores provider credentials (e.g. Z.AI Coding Plan API
/// keys) in this file.
///
/// Follows the same semantics as `provision_amp_auth`.
pub fn provision_opencode_auth(
    auth_json: &Path,
    mode: AuthForwardMode,
    host_home: &Path,
) -> anyhow::Result<(AuthProvisionOutcome, Option<PathBuf>)> {
    provision_opencode_auth_from_path(
        auth_json,
        mode,
        &host_home.join(".local/share/opencode/auth.json"),
        None,
    )
}

pub fn provision_opencode_auth_from_source_dir(
    auth_json: &Path,
    mode: AuthForwardMode,
    source_dir: &Path,
    provider: Option<AiProvider>,
) -> anyhow::Result<(AuthProvisionOutcome, Option<PathBuf>)> {
    provision_opencode_auth_from_path(auth_json, mode, &source_dir.join("auth.json"), provider)
}

fn provision_opencode_auth_from_path(
    auth_json: &Path,
    mode: AuthForwardMode,
    host_auth_json: &Path,
    provider: Option<AiProvider>,
) -> anyhow::Result<(AuthProvisionOutcome, Option<PathBuf>)> {
    if mode == AuthForwardMode::Sync {
        reject_auth_path(auth_json)?;
        let Some(content) = read_source_text(host_auth_json, "OpenCode auth.json")? else {
            repair_permissions(auth_json)?;
            return Ok((
                AuthProvisionOutcome::HostMissing,
                private_file_exists(auth_json)?.then(|| auth_json.to_path_buf()),
            ));
        };
        if content.trim().is_empty() {
            // A present-but-blank ambient file is invalid input, not an
            // absent host login. Preserve the documented missing-file
            // behavior (an in-container login may survive), but never
            // carry stale role-state credentials across this invalidation.
            wipe_agent_file_state(auth_json, "OpenCode auth.json")?;
            return Ok((AuthProvisionOutcome::HostMissing, None));
        }
        let value = serde_json::from_str::<serde_json::Value>(&content)
            .map_err(|_| anyhow::anyhow!("OpenCode auth.json is malformed"))?;
        let (key, entry) = select_opencode_auth_entry(&value, provider).map_err(|reason| {
            anyhow::anyhow!("OpenCode auth.json cannot be selected safely: {reason}")
        })?;
        let mut selected = serde_json::Map::new();
        selected.insert(key.to_owned(), entry.clone());
        let selected = serde_json::to_vec(&serde_json::Value::Object(selected))
            .context("serializing selected OpenCode credential")?;
        write_private_bytes(auth_json, &selected)
            .context("writing selected OpenCode credential")?;
        return Ok((AuthProvisionOutcome::Synced, Some(auth_json.to_path_buf())));
    }
    provision_single_file_credential(
        auth_json,
        host_auth_json,
        mode,
        "OpenCode auth.json",
        "OpenCode",
        true,
        true,
        true,
    )
}
