// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;
pub(super) fn api_key_account(provider: AiProvider, model: Option<&str>) -> AccountConfig {
    AccountConfig {
        enabled: true,
        name: "Test".into(),
        provider,
        credential: AccountCredential::ApiKey {
            value: "fixture-key".into(),
            base_url: None,
            model: model.map(str::to_owned),
        },
    }
}

pub(super) fn instance(
    config_id: &str,
    agent: Agent,
    account_id: &str,
) -> jackin_config::ResolvedInstance {
    jackin_config::ResolvedInstance {
        config_id: config_id.into(),
        agent,
        account_id: account_id.into(),
        model: None,
        base_url: None,
        xdg_roots: None,
        label: config_id.into(),
        synthesized: true,
    }
}

pub(super) fn manifest_with(
    temp: &tempfile::TempDir,
    agents: &[&str],
) -> jackin_manifest::RoleManifest {
    let mut role = format!(
        "version = \"v1alpha5\"\ndockerfile = \"Dockerfile\"\nagents = [{}]\n",
        agents
            .iter()
            .map(|agent| format!("\"{agent}\""))
            .collect::<Vec<_>>()
            .join(", ")
    );
    for agent in agents {
        role.push_str(&format!("\n[{agent}]\n"));
        if *agent == "claude" {
            role.push_str("model = \"sonnet\"\n");
        }
    }
    std::fs::write(temp.path().join("jackin.role.toml"), role).unwrap();
    std::fs::write(
        temp.path().join("Dockerfile"),
        "FROM projectjackin/construct:0.1-trixie\n",
    )
    .unwrap();
    jackin_manifest::load_role_manifest(temp.path()).unwrap()
}
