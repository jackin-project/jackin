// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

#[test]
fn single_account_plan_carries_the_exact_pinned_model() {
    // F7 plan leg: an `account add --model <exact-id>` pin must reach the
    // plan byte-exact.
    const MODEL: &str = "openrouter/anthropic/claude-sonnet-4";
    let mut config = AppConfig::default();
    config.accounts.insert(
        "or-model".to_owned(),
        AccountConfig {
            enabled: true,
            name: "or-model".to_owned(),
            provider: AiProvider::OpenRouter,
            credential: AccountCredential::ApiKey {
                value: jackin_core::EnvValue::Plain("fixture-or-key".into()),
                base_url: None,
                model: Some(MODEL.to_owned()),
            },
        },
    );
    config.workspaces.insert(
        WS.to_owned(),
        WorkspaceConfig {
            workdir: "/workspace".to_owned(),
            accounts: vec!["or-model".to_owned()],
            ..WorkspaceConfig::default()
        },
    );
    config
        .workspaces
        .get_mut(WS)
        .unwrap()
        .account_bindings
        .insert(Agent::Opencode, "or-model".to_owned());
    let workspace = WorkspaceName::parse(WS).unwrap();

    let plan =
        resolve_dry_run_identity(&config, Agent::Opencode, Some(&workspace), ROLE, false).unwrap();
    assert_eq!(plan.account_id.as_deref(), Some("or-model"));
    assert_eq!(plan.model.as_deref(), Some(MODEL));
    assert!(plan.instances.is_empty());

    // The instances shape carries the same bytes per instance, with a
    // configuration override winning over the account pin.
    config.agent_configurations.insert(
        "oc-pinned".to_owned(),
        AgentConfiguration {
            agent: Agent::Opencode,
            account: "or-model".to_owned(),
            model: Some("openrouter/x-ai/grok-4".to_owned()),
            base_url: None,
            display_label: None,
            invoked_via_wrapper: None,
        },
    );
    config.agent_configurations.insert(
        "oc-plain".to_owned(),
        configuration(Agent::Opencode, "or-model"),
    );
    // Opencode admits one instance per container, so each list entry
    // resolves on its own.
    set_role_list(&mut config, &["oc-pinned"]);
    let plan =
        resolve_dry_run_identity(&config, Agent::Opencode, Some(&workspace), ROLE, false).unwrap();
    assert_eq!(plan.account_id, None);
    assert_eq!(plan.instances.len(), 1);
    assert_eq!(
        plan.instances[0].model.as_deref(),
        Some("openrouter/x-ai/grok-4")
    );
    set_role_list(&mut config, &["oc-plain"]);
    let plan =
        resolve_dry_run_identity(&config, Agent::Opencode, Some(&workspace), ROLE, false).unwrap();
    assert_eq!(plan.account_id, None);
    assert_eq!(plan.instances.len(), 1);
    assert_eq!(plan.instances[0].model.as_deref(), Some(MODEL));
}

#[test]
fn model_projection_matches_launch_for_mixed_providers_and_trimmed_override() {
    let (config, workspace) = mixed_model_projection_config();
    let identity =
        resolve_dry_run_identity(&config, Agent::Codex, Some(&workspace), ROLE, false).unwrap();
    assert_eq!(identity.instances.len(), 2);

    let defaults = resolve_dry_run_model_projection(
        &config,
        &mixed_role_model_defaults(),
        &identity,
        Agent::Opencode,
        None,
    )
    .unwrap();
    assert_eq!(
        defaults.model, None,
        "multi-instance plans have no top model"
    );
    assert_eq!(defaults.instances["codex-main"], "codex-role-default");
    assert_eq!(
        defaults.instances["opencode-zai"], "zai-coding-plan/glm-account-default",
        "account model defaults override the role and use the owning provider"
    );

    let overridden = resolve_dry_run_model_projection(
        &config,
        &mixed_role_model_defaults(),
        &identity,
        Agent::Opencode,
        Some("  gpt-6-luna  "),
    )
    .unwrap();
    assert_eq!(overridden.instances["codex-main"], "codex-role-default");
    assert_eq!(
        overridden.instances["opencode-zai"],
        "zai-coding-plan/gpt-6-luna"
    );
}

#[test]
fn model_projection_matches_single_account_selection_and_trims_override() {
    let (mut config, workspace) = mixed_model_projection_config();
    config.default_launch = Some(vec!["codex-main".to_owned(), "opencode-openai".to_owned()]);
    let scoped = programmatic::with_account_selection(
        &config,
        Agent::Opencode,
        Some(&workspace),
        ROLE,
        "openai",
    )
    .unwrap();
    let identity =
        resolve_dry_run_identity(&scoped, Agent::Opencode, Some(&workspace), ROLE, true).unwrap();
    assert_eq!(identity.account_id.as_deref(), Some("openai"));
    assert!(identity.instances.is_empty());
    assert_eq!(identity.admitted_instances.len(), 2);

    let defaults = resolve_dry_run_model_projection(
        &scoped,
        &mixed_role_model_defaults(),
        &identity,
        Agent::Opencode,
        None,
    )
    .unwrap();
    assert_eq!(
        defaults.model.as_deref(),
        Some("openai/opencode-role-default")
    );

    let projection = resolve_dry_run_model_projection(
        &scoped,
        &mixed_role_model_defaults(),
        &identity,
        Agent::Opencode,
        Some("  gpt-6-luna  "),
    )
    .unwrap();
    assert_eq!(projection.model.as_deref(), Some("openai/gpt-6-luna"));
    assert_eq!(projection.instances["opencode-openai"], "openai/gpt-6-luna");
}
