// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;
use crate::{ConfigEditor, WorkspaceConfig, WorkspaceRoleOverride};
use jackin_core::JackinPaths;

fn profile(name: &str) -> AccountConfig {
    AccountConfig {
        enabled: true,
        name: name.into(),
        provider: AiProvider::Anthropic,
        credential: AccountCredential::Profile {
            agent: Agent::Claude,
            directory: PathBuf::from("/profiles").join(name),
        },
    }
}
fn config() -> (AppConfig, WorkspaceName) {
    let mut cfg = AppConfig::default();
    cfg.accounts.insert("personal".into(), profile("Personal"));
    cfg.accounts.insert("work".into(), profile("Work"));
    let ws = WorkspaceName::parse("project").unwrap();
    cfg.workspaces
        .insert(ws.as_str().into(), WorkspaceConfig::default());
    (cfg, ws)
}
#[test]
fn workspace_empty_allowlist_never_inherits_global_credentials() {
    let (mut cfg, ws) = config();
    cfg.account_bindings
        .insert(Agent::Claude, "personal".into());
    assert!(
        resolve_account(&cfg, Agent::Claude, Some(&ws), "")
            .unwrap()
            .is_none()
    );
    assert_eq!(
        resolve_account(&cfg, Agent::Claude, None, "")
            .unwrap()
            .unwrap()
            .name,
        "Personal"
    );
}
#[test]
fn sole_allowed_account_selected_and_ambiguity_rejected() {
    let (mut cfg, ws) = config();
    cfg.workspaces.get_mut(ws.as_str()).unwrap().accounts = vec!["personal".into()];
    assert_eq!(
        resolve_account(&cfg, Agent::Claude, Some(&ws), "")
            .unwrap()
            .unwrap()
            .name,
        "Personal"
    );
    cfg.workspaces
        .get_mut(ws.as_str())
        .unwrap()
        .accounts
        .push("work".into());
    resolve_account(&cfg, Agent::Claude, Some(&ws), "").unwrap_err();
}
#[test]
fn role_binding_wins_but_cannot_escape_allowlist() {
    let (mut cfg, ws) = config();
    let workspace = cfg.workspaces.get_mut(ws.as_str()).unwrap();
    workspace.accounts = vec!["personal".into(), "work".into()];
    workspace
        .account_bindings
        .insert(Agent::Claude, "personal".into());
    workspace.roles.insert(
        "role".into(),
        WorkspaceRoleOverride {
            account_bindings: BTreeMap::from([(Agent::Claude, "work".into())]),
            ..Default::default()
        },
    );
    assert_eq!(
        resolve_account(&cfg, Agent::Claude, Some(&ws), "role")
            .unwrap()
            .unwrap()
            .name,
        "Work"
    );
    cfg.workspaces.get_mut(ws.as_str()).unwrap().accounts = vec!["personal".into()];
    assert!(cfg.validate_accounts().is_err());
    resolve_account(&cfg, Agent::Claude, Some(&ws), "role").unwrap_err();
}
#[test]
fn secrets_are_redacted_and_provider_routing_is_explicit() {
    let account = AccountConfig {
        enabled: true,
        name: "Work".into(),
        provider: AiProvider::Moonshot,
        credential: AccountCredential::ApiKey {
            value: EnvValue::from("SECRET-SENTINEL"),
            base_url: None,
            model: Some("k3".into()),
        },
    };
    assert!(!format!("{account:?}").contains("SECRET-SENTINEL"));
    let env = account.credential_env(Agent::Claude).unwrap();
    assert_eq!(
        env.get("ANTHROPIC_BASE_URL"),
        Some(&EnvValue::from("https://api.kimi.com/coding"))
    );
    assert_eq!(
        env.get("ANTHROPIC_AUTH_TOKEN"),
        Some(&EnvValue::from("SECRET-SENTINEL"))
    );
    assert!(!account.supports_agent(Agent::Amp));
}
#[test]
fn invalid_ids_and_on_demand_credentials_rejected() {
    for id in ["", "../x", "X", "-start", "with space"] {
        assert!(validate_account_id(id).is_err());
    }
    let mut cfg = AppConfig::default();
    cfg.accounts.insert(
        "bad".into(),
        AccountConfig {
            enabled: true,
            name: "Bad".into(),
            provider: AiProvider::Anthropic,
            credential: AccountCredential::ApiKey {
                value: EnvValue::from(""),
                base_url: None,
                model: None,
            },
        },
    );
    assert!(cfg.validate_accounts().is_err());
}
#[test]
fn first_start_discovers_once_and_does_not_grant_workspace_access() {
    let temp = tempfile::TempDir::new().unwrap();
    let paths = JackinPaths::resolve_with_env(temp.path(), None, None);
    std::fs::create_dir_all(temp.path().join(".codex")).unwrap();
    std::fs::write(
        temp.path().join(".codex/auth.json"),
        r#"{"tokens":{"access_token":"fixture-token"}}"#,
    )
    .unwrap();
    let cfg = AppConfig::load_or_init(&paths).unwrap();
    assert!(cfg.accounts.contains_key("default-codex"));
    assert!(cfg.account_bindings.is_empty());
    let mut editor = ConfigEditor::open(&paths).unwrap();
    editor.remove_account("default-codex").unwrap();
    editor.save().unwrap();
    assert!(
        !AppConfig::load_or_init(&paths)
            .unwrap()
            .accounts
            .contains_key("default-codex")
    );
}

#[test]
fn minimax_codex_account_routes_its_key_and_requires_a_model() {
    let mut account = AccountConfig {
        enabled: true,
        name: "MiniMax coding".into(),
        provider: AiProvider::Minimax,
        credential: AccountCredential::ApiKey {
            value: EnvValue::from("fixture-minimax-key"),
            base_url: None,
            model: Some("MiniMax-M3".into()),
        },
    };
    assert!(account.supports_agent(Agent::Codex));
    let env = account.credential_env(Agent::Codex).unwrap();
    assert_eq!(
        env.get("MINIMAX_API_KEY"),
        Some(&EnvValue::from("fixture-minimax-key"))
    );
    assert_eq!(
        env.get("OPENAI_BASE_URL"),
        Some(&EnvValue::from("https://api.minimax.io/v1"))
    );
    assert!(!env.contains_key("OPENAI_API_KEY"));
    if let AccountCredential::ApiKey { model, .. } = &mut account.credential {
        *model = None;
    }
    account.credential_env(Agent::Codex).unwrap_err();
}

#[test]
fn disabled_accounts_keep_configuration_but_cannot_authenticate() {
    let (mut cfg, ws) = config();
    cfg.workspaces
        .get_mut(ws.as_str())
        .unwrap()
        .accounts
        .push("work".into());
    cfg.accounts.get_mut("work").unwrap().enabled = false;
    cfg.validate_accounts().unwrap();
    assert!(!cfg.accounts["work"].supports_agent(Agent::Claude));
    cfg.accounts["work"]
        .credential_env(Agent::Claude)
        .unwrap_err();
    assert!(
        resolve_account(&cfg, Agent::Claude, Some(&ws), "smith")
            .unwrap()
            .is_none()
    );
    cfg.workspaces
        .get_mut(ws.as_str())
        .unwrap()
        .account_bindings
        .insert(Agent::Claude, "work".into());
    cfg.validate_accounts().unwrap();
    resolve_account(&cfg, Agent::Claude, Some(&ws), "smith").unwrap_err();
    let serialized = toml::to_string(&cfg.accounts["work"]).unwrap();
    assert!(serialized.contains("enabled = false"));
    let restored: AccountConfig = toml::from_str(&serialized).unwrap();
    assert!(!restored.enabled);
    assert!(
        toml::from_str::<AccountConfig>(&serialized.replace("enabled = false\n", ""))
            .unwrap()
            .enabled
    );
}
