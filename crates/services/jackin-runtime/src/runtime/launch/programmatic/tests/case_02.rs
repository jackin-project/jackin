// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

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

#[test]
fn selected_launch_keeps_only_authorized_global_siblings() {
    let (mut config, workspace) = two_account_config();
    config.accounts.insert(
        "foreign".into(),
        jackin_config::AccountConfig {
            enabled: true,
            name: "Foreign".into(),
            provider: jackin_config::AiProvider::Anthropic,
            credential: jackin_config::AccountCredential::ApiKey {
                value: "test-key".into(),
                base_url: None,
                model: None,
            },
        },
    );
    config.agent_configurations.insert(
        "claude-foreign".into(),
        AgentConfiguration {
            agent: Agent::Claude,
            account: "foreign".into(),
            model: None,
            base_url: None,
            display_label: None,
            invoked_via_wrapper: None,
        },
    );
    config
        .accounts
        .insert("allowed".into(), config.accounts["foreign"].clone());
    config
        .workspaces
        .get_mut("work")
        .unwrap()
        .accounts
        .push("allowed".into());
    let mut sibling = config.agent_configurations["claude-foreign"].clone();
    sibling.account = "allowed".into();
    config
        .agent_configurations
        .insert("claude-allowed".into(), sibling);
    config.default_launch = Some(vec![
        "codex-main".into(),
        "claude-foreign".into(),
        "claude-allowed".into(),
        "codex-alt".into(),
    ]);

    for selected in [
        with_account_selection(&config, Agent::Codex, Some(&workspace), "codex", "private")
            .unwrap(),
        with_configuration_selection(
            &config,
            Agent::Codex,
            Some(&workspace),
            "codex",
            "codex-main",
        )
        .unwrap(),
    ] {
        let instances =
            jackin_config::resolve_launch(&selected, Some(&workspace), "codex", None, None)
                .unwrap();
        assert_eq!(instances.len(), 2);
        assert_eq!(instances[1].config_id, "claude-allowed");
        assert_eq!(instances[0].config_id, "codex-main");
        assert_eq!(
            selected.workspaces["work"].roles["codex"].default_launch,
            Some(vec!["codex-main".into(), "claude-allowed".into()])
        );
    }
    assert!(config.workspaces["work"].roles.is_empty());
    assert_eq!(config.default_launch.as_ref().unwrap().len(), 4);
}
