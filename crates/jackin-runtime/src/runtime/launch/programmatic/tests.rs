// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! `LoadOptions` programmatic-launch validation. No Docker: every case here
//! is decided before the pipeline touches a daemon.

use super::*;
use crate::runtime::LoadOptions;
use jackin_config::RoleSource;
use jackin_protocol::{ExecBinding, ExecKind};

const ROLE: &str = "donbeave/the-architect";

fn selector() -> RoleSelector {
    RoleSelector::parse(ROLE).expect("role selector must parse")
}

/// Config with the role registered and trusted, i.e. the state
/// `jackin config trust grant <selector>` leaves behind.
fn trusted_config() -> AppConfig {
    let mut config = AppConfig::default();
    config.roles.insert(
        selector().key(),
        RoleSource {
            git: "https://github.com/donbeave/the-architect".to_owned(),
            trusted: true,
            ..RoleSource::default()
        },
    );
    config
}

fn untrusted_config() -> AppConfig {
    let mut config = trusted_config();
    if let Some(source) = config.roles.get_mut(&selector().key()) {
        source.trusted = false;
    }
    config
}

fn opts() -> LoadOptions {
    LoadOptions::programmatic(Agent::Claude)
}

#[test]
fn a_fully_supplied_programmatic_launch_validates() {
    let mut options = opts();
    options.model = Some("claude-opus-5".to_owned());
    options.effort = Some(ReasoningEffort::Medium);
    options
        .env
        .insert("LINEAR_TEAM".to_owned(), "core".to_owned());
    options.on_demand_bindings.push(ExecBinding {
        name: "OP_SERVICE_ACCOUNT_TOKEN".to_owned(),
        kind: ExecKind::Op,
        source: "op://tailrocks/jackin-operator/credential".to_owned(),
    });
    assert_eq!(
        options.validate_programmatic(&trusted_config(), &selector()),
        Ok(())
    );
}

#[test]
fn an_interactive_launch_skips_every_programmatic_check() {
    // The interactive path can still answer a prompt, so an unresolved agent
    // and a missing trust grant are not validation failures there.
    let options = LoadOptions::default();
    assert_eq!(
        options.validate_programmatic(&untrusted_config(), &selector()),
        Ok(())
    );
}

#[test]
fn an_unresolved_agent_is_a_validation_failure() {
    let mut options = opts();
    options.agent = None;
    assert_eq!(
        options.validate_programmatic(&trusted_config(), &selector()),
        Err(LoadOptionsError::AgentNotResolved {
            role: selector().key()
        })
    );
}

#[test]
fn a_missing_trust_grant_is_a_validation_failure_naming_the_grant_command() {
    let error = opts()
        .validate_programmatic(&untrusted_config(), &selector())
        .expect_err("an untrusted role must not launch non-interactively");
    assert_eq!(
        error,
        LoadOptionsError::TrustNotGranted {
            role: selector().key()
        }
    );
    assert!(
        error.to_string().contains("jackin config trust grant"),
        "the error must name the command that fixes it, got {error}"
    );
}

#[test]
fn an_unregistered_role_is_treated_as_untrusted() {
    assert_eq!(
        opts().validate_programmatic(&AppConfig::default(), &selector()),
        Err(LoadOptionsError::TrustNotGranted {
            role: selector().key()
        })
    );
}

#[test]
fn a_builtin_role_needs_no_explicit_grant() {
    let (builtin, _) = jackin_config::BUILTIN_ROLES
        .first()
        .copied()
        .expect("at least one built-in role ships with jackin");
    let builtin_selector = RoleSelector::parse(builtin).expect("built-in selector must parse");
    assert_eq!(
        opts().validate_programmatic(&AppConfig::default(), &builtin_selector),
        Ok(())
    );
}

#[test]
fn a_role_branch_cannot_be_loaded_without_a_tty() {
    let mut options = opts();
    options.role_branch = Some("feat/my-pr".to_owned());
    assert_eq!(
        options.validate_programmatic(&trusted_config(), &selector()),
        Err(LoadOptionsError::RoleBranchNotAllowed {
            branch: "feat/my-pr".to_owned()
        })
    );
}

#[test]
fn a_missing_registered_account_is_a_validation_failure() {
    let mut options = opts();
    options.account = Some("missing".to_owned());
    assert_eq!(
        options.validate_programmatic(&trusted_config(), &selector()),
        Err(LoadOptionsError::AccountMissing {
            account: "missing".to_owned()
        })
    );
}

#[test]
fn launch_selection_rejects_accounts_outside_workspace_allowlist() {
    use jackin_config::{AccountConfig, AccountCredential, AiProvider, WorkspaceConfig};
    let mut config = trusted_config();
    config.accounts.insert(
        "private".to_owned(),
        AccountConfig {
            enabled: true,
            name: "Private".to_owned(),
            provider: AiProvider::OpenAi,
            credential: AccountCredential::Profile {
                agent: Agent::Codex,
                directory: "/private/codex".into(),
            },
        },
    );
    config
        .workspaces
        .insert("work".to_owned(), WorkspaceConfig::default());
    let workspace = jackin_core::WorkspaceName::parse("work").unwrap();
    with_account_selection(&config, Agent::Codex, Some(&workspace), "codex", "private")
        .unwrap_err();
    config
        .workspaces
        .get_mut("work")
        .unwrap()
        .accounts
        .push("private".to_owned());
    let selected =
        with_account_selection(&config, Agent::Codex, Some(&workspace), "codex", "private")
            .unwrap();
    assert!(
        jackin_config::resolve_account(&selected, Agent::Codex, Some(&workspace), "codex")
            .unwrap()
            .is_some()
    );
    assert!(
        config.workspaces["work"].roles.is_empty(),
        "per-launch binding must not mutate persistent config"
    );
}

#[test]
fn an_empty_model_override_is_a_validation_failure() {
    let mut options = opts();
    options.model = Some("   ".to_owned());
    assert_eq!(
        options.validate_programmatic(&trusted_config(), &selector()),
        Err(LoadOptionsError::EmptyModel)
    );
}

#[test]
fn a_reserved_env_name_is_a_validation_failure() {
    let (reserved, _) = jackin_core::RESERVED_RUNTIME_ENV_VARS
        .first()
        .copied()
        .expect("the runtime reserves at least one env name");
    let mut options = opts();
    options.env.insert(reserved.to_owned(), "x".to_owned());
    assert_eq!(
        options.validate_programmatic(&trusted_config(), &selector()),
        Err(LoadOptionsError::ReservedEnvName {
            name: reserved.to_owned()
        })
    );
}

#[test]
fn an_empty_env_name_is_a_validation_failure() {
    let mut options = opts();
    options.env.insert(String::new(), "x".to_owned());
    assert_eq!(
        options.validate_programmatic(&trusted_config(), &selector()),
        Err(LoadOptionsError::EmptyEnvName)
    );
}

#[test]
fn a_duplicate_pre_approved_on_demand_binding_is_a_validation_failure() {
    let binding = ExecBinding {
        name: "OP_TOKEN".to_owned(),
        kind: ExecKind::Op,
        source: "op://vault/item/credential".to_owned(),
    };
    let mut options = opts();
    options.on_demand_bindings = vec![binding.clone(), binding];
    assert_eq!(
        options.validate_programmatic(&trusted_config(), &selector()),
        Err(LoadOptionsError::DuplicateOnDemandBinding {
            name: "OP_TOKEN".to_owned()
        })
    );
}

#[test]
fn an_on_demand_binding_without_a_source_is_a_validation_failure() {
    let mut options = opts();
    options.on_demand_bindings = vec![ExecBinding {
        name: "OP_TOKEN".to_owned(),
        kind: ExecKind::Op,
        source: String::new(),
    }];
    assert_eq!(
        options.validate_programmatic(&trusted_config(), &selector()),
        Err(LoadOptionsError::IncompleteOnDemandBinding {
            name: "OP_TOKEN".to_owned()
        })
    );
}

#[test]
fn the_identity_sink_records_the_first_claimed_container_only() {
    let options = opts();
    assert_eq!(options.launched_instance(), None);
    options.record_launched_instance("jk-k7p9m2xq-the-architect-claude");
    options.record_launched_instance("jk-zzzzzzzz-the-architect-claude");
    let launched = options
        .launched_instance()
        .expect("the sink must hold the claimed identity");
    assert_eq!(launched.instance_id, "k7p9m2xq");
    assert_eq!(launched.container_base, "jk-k7p9m2xq-the-architect-claude");
}

#[test]
fn an_unparseable_container_base_falls_back_to_the_full_name() {
    let launched = LaunchedInstance::from_container_base("legacy_container");
    assert_eq!(launched.instance_id, "legacy_container");
    assert_eq!(launched.container_base, "legacy_container");
}

#[test]
fn an_interactive_launch_installs_no_identity_sink() {
    assert!(LoadOptions::default().identity_sink.is_none());
    assert_eq!(LoadOptions::default().launched_instance(), None);
}

#[test]
fn codex_model_and_effort_travel_as_the_role_hook_config_keys() {
    assert_eq!(
        lane_agent_env(
            Agent::Codex,
            Some("gpt-5.6-terra"),
            Some(ReasoningEffort::High)
        ),
        vec![
            (CODEX_LANE_MODEL_ENV.to_owned(), "gpt-5.6-terra".to_owned()),
            (CODEX_LANE_EFFORT_ENV.to_owned(), "high".to_owned()),
        ]
    );
}

#[test]
fn claude_model_and_effort_travel_as_claude_code_env() {
    assert_eq!(
        lane_agent_env(
            Agent::Claude,
            Some("claude-opus-5"),
            Some(ReasoningEffort::Medium)
        ),
        vec![
            (CLAUDE_MODEL_ENV.to_owned(), "claude-opus-5".to_owned()),
            (CLAUDE_EFFORT_ENV.to_owned(), "medium".to_owned()),
        ]
    );
}

#[test]
fn an_absent_model_or_effort_emits_no_lane_env() {
    assert!(lane_agent_env(Agent::Codex, None, None).is_empty());
    assert!(lane_agent_env(Agent::Claude, Some("  "), None).is_empty());
}

#[test]
fn an_agent_without_an_env_model_knob_emits_no_lane_env() {
    assert!(
        lane_agent_env(Agent::Amp, Some("some-model"), Some(ReasoningEffort::Low)).is_empty(),
        "runtimes that take their model on argv must not grow a silent env knob"
    );
}
