// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;
pub(super) fn instance_config(instances: &[(&str, &str, &str)]) -> CapsuleConfig {
    let mut config = CapsuleConfig {
        workdir: "/workspace".to_owned(),
        instances: instances
            .iter()
            .map(|(id, _, _)| (*id).to_owned())
            .collect(),
        agents: instances
            .iter()
            .map(|(id, _, slug)| ((*id).to_owned(), (*slug).to_owned()))
            .collect(),
        auth_modes: instances
            .iter()
            .map(|(id, mode, _)| ((*id).to_owned(), (*mode).to_owned()))
            .collect(),
        credential_provider_surfaces: instances
            .iter()
            .filter_map(|(id, _, agent)| {
                let surface = match *agent {
                    "claude" => "claude",
                    "codex" => "codex",
                    "amp" => "amp",
                    "grok" => "grok",
                    "kimi" => "kimi",
                    "opencode" | "omp" | "hermes" => "opencode",
                    "antigravity" | "gemini" => "google",
                    "cursor" => "cursor",
                    "muse" => "meta",
                    _ => return None,
                };
                Some(((*id).to_owned(), surface.to_owned()))
            })
            .collect(),
        accounts: instances
            .iter()
            .map(|(id, _, _)| {
                let account = match *id {
                    "claude-work" => "acc-work",
                    "claude-personal" => "acc-personal",
                    "codex-work" => "acc-codex",
                    _ => id,
                };
                ((*id).to_owned(), account.to_owned())
            })
            .collect(),
        ..CapsuleConfig::default()
    };
    for (index, (id, _, _)) in instances.iter().enumerate() {
        let index = u32::try_from(index).unwrap_or(u32::MAX);
        config
            .instance_home_dirs
            .insert((*id).to_owned(), format!("/home/agent/.slot-{index}"));
        config
            .instance_forwarded_dirs
            .insert((*id).to_owned(), format!("/jackin/slot-{index}"));
        config.instance_credential_files.insert(
            (*id).to_owned(),
            jackin_protocol::account_credentials_container_path(id),
        );
        config.instance_mount_paths.insert(
            (*id).to_owned(),
            vec![
                format!("/home/agent/.slot-{index}"),
                format!("/jackin/slot-{index}"),
            ],
        );
        config.instance_identities.insert(
            (*id).to_owned(),
            jackin_protocol::SessionIdentity {
                uid: 2_000 + index,
                gid: 2_000 + index,
            },
        );
    }
    config.shell_identity = Some(jackin_protocol::SessionIdentity {
        uid: 2_000 + u32::try_from(instances.len()).unwrap_or(u32::MAX),
        gid: 2_000 + u32::try_from(instances.len()).unwrap_or(u32::MAX),
    });
    config
}

pub(super) fn v2_credentials(value: serde_json::Value) -> jackin_protocol::AgentCredentialEnv {
    serde_json::from_value(value).expect("v2 fixture must decode")
}
