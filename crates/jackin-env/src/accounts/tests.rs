// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;
use jackin_config::{AccountConfig, AccountCredential, AiProvider, WorkspaceConfig};
use jackin_core::EnvValue;

struct NoSecrets;
impl OpRunner for NoSecrets {
    fn read(&self, _: &str) -> anyhow::Result<String> {
        anyhow::bail!("unexpected secret lookup")
    }
}
fn account(value: &str) -> AccountConfig {
    AccountConfig {
        enabled: true,
        name: "Test".into(),
        provider: AiProvider::OpenAi,
        credential: AccountCredential::ApiKey {
            value: EnvValue::from(value),
            base_url: None,
            model: None,
        },
    }
}
#[test]
fn workspace_does_not_inherit_global_credentials() {
    let mut cfg = AppConfig::default();
    cfg.accounts
        .insert("personal".into(), account("test-personal"));
    cfg.account_bindings.insert(Agent::Codex, "personal".into());
    cfg.workspaces
        .insert("work".into(), WorkspaceConfig::default());
    let ws = WorkspaceName::parse("work").unwrap();
    let env = resolve_account_env_with(
        &cfg,
        &[Agent::Codex],
        Some(&ws),
        "codex",
        &NoSecrets,
        |_| Err(std::env::VarError::NotPresent),
    )
    .unwrap();
    assert!(env.is_empty());
}
#[test]
fn assigned_key_resolves_host_reference_without_reading_other_accounts() {
    let mut cfg = AppConfig::default();
    cfg.accounts.insert("work".into(), account("$WORK_TOKEN"));
    cfg.accounts
        .insert("unselected".into(), account("$ABSENT_TOKEN"));
    cfg.account_bindings.insert(Agent::Codex, "work".into());
    let env = resolve_account_env_with(&cfg, &[Agent::Codex], None, "codex", &NoSecrets, |name| {
        assert_eq!(name, "WORK_TOKEN");
        Ok("test-key".into())
    })
    .unwrap();
    assert_eq!(env["codex"]["OPENAI_API_KEY"], "test-key");
}
#[test]
fn different_accounts_resolve_into_separate_agent_environments() {
    let mut cfg = AppConfig::default();
    cfg.accounts.insert("codex".into(), account("test-one"));
    cfg.accounts.insert("opencode".into(), account("test-two"));
    cfg.account_bindings.insert(Agent::Codex, "codex".into());
    cfg.account_bindings
        .insert(Agent::Opencode, "opencode".into());
    let env = resolve_account_env_with(
        &cfg,
        &[Agent::Codex, Agent::Opencode],
        None,
        "role",
        &NoSecrets,
        |_| Err(std::env::VarError::NotPresent),
    )
    .unwrap();
    assert_eq!(env["codex"]["OPENAI_API_KEY"], "test-one");
    assert_eq!(env["opencode"]["OPENAI_API_KEY"], "test-two");
}
#[test]
fn generic_env_cannot_bypass_account_admission() {
    let mut cfg = AppConfig::default();
    cfg.env
        .insert("OPENAI_API_KEY".into(), EnvValue::from("test-bypass"));
    cfg.env
        .insert("MOONSHOT_API_KEY".into(), EnvValue::from("test-bypass"));
    for key in [
        "KIMI_AUTH_TOKEN",
        "kimi_auth_token",
        "MINIMAX_CODING_API_KEY",
        "Z_AI_API_KEY",
    ] {
        cfg.env.insert(key.into(), EnvValue::from("test-bypass"));
    }
    cfg.env.insert("EDITOR".into(), EnvValue::from("vim"));
    let env = crate::resolve_operator_env_with(&cfg, None, None, &NoSecrets, |_| {
        Err(std::env::VarError::NotPresent)
    })
    .unwrap();
    assert_eq!(env, BTreeMap::from([("EDITOR".into(), "vim".into())]));
}
