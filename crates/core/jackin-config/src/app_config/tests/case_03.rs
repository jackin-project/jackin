// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

#[test]
fn deserializes_per_agent_env_map() {
    let toml_str = r#"
[roles.agent-smith]
git = "https://github.com/jackin-project/jackin-agent-smith.git"

[roles.agent-smith.env]
AGENT_TOKEN = "op://Shared/smith/token"
"#;
    let config: AppConfig = toml::from_str(toml_str).unwrap();
    let role = config.roles.get("agent-smith").unwrap();
    assert_eq!(
        role.env.get("AGENT_TOKEN").unwrap().as_persisted_str(),
        "op://Shared/smith/token"
    );
}

#[test]
fn deserializes_per_workspace_env_map() {
    let toml_str = r#"
[roles.agent-smith]
git = "https://github.com/jackin-project/jackin-agent-smith.git"

[workspaces.big-monorepo]
workdir = "/workspace/project"

[[workspaces.big-monorepo.mounts]]
src = "/tmp/src"
dst = "/workspace/project"

[workspaces.big-monorepo.env]
WORKSPACE_VAR = "literal"
"#;
    let config: AppConfig = toml::from_str(toml_str).unwrap();
    let ws = config.workspaces.get("big-monorepo").unwrap();
    assert_eq!(
        ws.env.get("WORKSPACE_VAR").unwrap().as_persisted_str(),
        "literal"
    );
}

#[test]
fn deserializes_workspace_agent_override_env() {
    let toml_str = r#"
[roles.agent-smith]
git = "https://github.com/jackin-project/jackin-agent-smith.git"

[workspaces.big-monorepo]
workdir = "/workspace/project"

[[workspaces.big-monorepo.mounts]]
src = "/tmp/src"
dst = "/workspace/project"

[workspaces.big-monorepo.roles.agent-smith.env]
PER_WORKSPACE_PER_AGENT = "specific"
"#;
    let config: AppConfig = toml::from_str(toml_str).unwrap();
    let ws = config.workspaces.get("big-monorepo").unwrap();
    let override_ = ws.roles.get("agent-smith").unwrap();
    assert_eq!(
        override_
            .env
            .get("PER_WORKSPACE_PER_AGENT")
            .unwrap()
            .as_persisted_str(),
        "specific"
    );
}

#[test]
fn env_maps_default_to_empty_when_omitted() {
    let toml_str = r#"
[roles.agent-smith]
git = "https://github.com/jackin-project/jackin-agent-smith.git"
"#;
    let config: AppConfig = toml::from_str(toml_str).unwrap();
    assert!(config.env.is_empty());
    assert!(config.roles.get("agent-smith").unwrap().env.is_empty());
}

#[test]
fn deserializes_agent_with_slash_in_name_using_quoted_keys() {
    // The spec calls out `[roles."chainargos/agent-jones".env]`
    // and `[workspaces.<ws>.roles."chainargos/agent-jones".env]`
    // as the TOML shape for third-party role selectors that
    // include a `/`. Standard TOML quoted keys suffice — this
    // test locks in that shape so a future refactor does not
    // accidentally require un-quoted identifiers.
    let toml_str = r#"
[roles."chainargos/agent-jones"]
git = "https://github.com/chainargos/jackin-agent-jones.git"

[roles."chainargos/agent-jones".env]
DATABASE_URL = "op://Work/agent-jones/db"

[workspaces.big-monorepo]
workdir = "/workspace/project"

[[workspaces.big-monorepo.mounts]]
src = "/tmp/src"
dst = "/workspace/project"

[workspaces.big-monorepo.roles."chainargos/agent-jones".env]
OPENAI_API_KEY = "op://Work/big-monorepo/OpenAI"
"#;
    let config: AppConfig = toml::from_str(toml_str).unwrap();
    let role = config.roles.get("chainargos/agent-jones").unwrap();
    assert_eq!(
        role.env.get("DATABASE_URL").unwrap().as_persisted_str(),
        "op://Work/agent-jones/db"
    );
    let ws = config.workspaces.get("big-monorepo").unwrap();
    let override_ = ws.roles.get("chainargos/agent-jones").unwrap();
    assert_eq!(
        override_
            .env
            .get("OPENAI_API_KEY")
            .unwrap()
            .as_persisted_str(),
        "op://Work/big-monorepo/OpenAI"
    );
}

#[test]
fn git_config_coauthor_trailer_round_trips() {
    let toml_str = "[git]\ncoauthor_trailer = true\n";
    let config: AppConfig = toml::from_str(toml_str).unwrap();
    assert!(config.git.coauthor_trailer);
    let serialized = toml::to_string(&config).unwrap();
    assert!(
        serialized.contains("coauthor_trailer = true"),
        "{serialized}"
    );
}

#[test]
fn git_config_default_omits_git_table_from_serialized_output() {
    let config = AppConfig::default();
    assert!(!config.git.coauthor_trailer);
    assert!(!config.git.dco);
    let serialized = toml::to_string(&config).unwrap();
    assert!(!serialized.contains("[git]"), "{serialized}");
    assert!(!serialized.contains("coauthor_trailer"), "{serialized}");
    assert!(!serialized.contains("dco"), "{serialized}");
}

#[test]
fn named_account_roundtrip_retains_workspace_authorization() {
    let text = r#"
[accounts.work]
name = "Work"
provider = "anthropic"
[accounts.work.credential]
type = "profile"
agent = "claude"
directory = "/home/operator/.claude-work"
[workspaces.project]
workdir = "/workspace/project"
accounts = ["work"]
[workspaces.project.account_bindings]
claude = "work"
"#;
    let config: AppConfig = toml::from_str(text).unwrap();
    config.validate_accounts().unwrap();
    let serialized = toml::to_string(&config).unwrap();
    let restored: AppConfig = toml::from_str(&serialized).unwrap();
    assert_eq!(restored.accounts, config.accounts);
    assert_eq!(restored.workspaces["project"].accounts, ["work"]);
    assert_eq!(
        restored.workspaces["project"].account_bindings[&Agent::Claude],
        "work"
    );
}

#[test]
fn new_config_has_no_implicit_account_grants() {
    let config = AppConfig::default();
    assert!(config.accounts.is_empty());
    assert!(WorkspaceConfig::default().accounts.is_empty());
    assert!(
        crate::resolve_account(&config, Agent::Claude, None, "smith")
            .unwrap()
            .is_none()
    );
}

#[test]
fn workspace_cannot_bind_unassigned_global_account() {
    let text = r#"
[accounts.work]
name = "Work"
provider = "anthropic"
[accounts.work.credential]
type = "profile"
agent = "claude"
directory = "/home/operator/.claude-work"
[workspaces.project]
workdir = "/workspace/project"
[workspaces.project.account_bindings]
claude = "work"
"#;
    let config: AppConfig = toml::from_str(text).unwrap();
    assert!(config.validate_accounts().is_err());
    crate::resolve_account(&config, Agent::Claude, Some(&wn("project")), "smith").unwrap_err();
}
