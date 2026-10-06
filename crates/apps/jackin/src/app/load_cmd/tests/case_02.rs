// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

#[test]
fn dry_run_json_projects_resolved_models_and_effort_only_to_selected_agent() {
    use jackin_config::ResolvedInstance;

    let selector = jackin_core::RoleSelector::parse("donbeave/the-architect").unwrap();
    let mut plan = dry_run_plan_json(
        &selector,
        &dry_run_workspace(),
        "codex",
        None,
        false,
        DryRunLaunchOverrides::default(),
        &dry_run_image_plan(),
    );
    let identity = jackin_runtime::runtime::DryRunIdentity {
        account_id: None,
        model: None,
        instances: vec![
            ResolvedInstance {
                config_id: "codex-work".to_owned(),
                agent: Agent::Codex,
                account_id: "c-codex".to_owned(),
                model: Some("account-model-work".to_owned()),
                base_url: None,
                xdg_roots: None,
                label: "Codex · Work".to_owned(),
                synthesized: false,
            },
            ResolvedInstance {
                config_id: "codex-personal".to_owned(),
                agent: Agent::Codex,
                account_id: "c-codex-personal".to_owned(),
                model: Some("account-model-personal".to_owned()),
                base_url: None,
                xdg_roots: None,
                label: "Codex · Personal".to_owned(),
                synthesized: false,
            },
            ResolvedInstance {
                config_id: "claude-work".to_owned(),
                agent: Agent::Claude,
                account_id: "a-claude".to_owned(),
                model: Some("claude-role-model".to_owned()),
                base_url: None,
                xdg_roots: None,
                label: "Claude · Work".to_owned(),
                synthesized: false,
            },
        ],
        admitted_instances: Vec::new(),
    };
    apply_dry_run_identity_json(&mut plan, &identity);
    apply_dry_run_load_overrides_json(
        &mut plan,
        Agent::Codex,
        &jackin_runtime::runtime::DryRunModelProjection {
            model: None,
            instances: std::collections::BTreeMap::from([
                ("codex-work".to_owned(), "gpt-6-luna".to_owned()),
                ("codex-personal".to_owned(), "gpt-6-luna".to_owned()),
                ("claude-work".to_owned(), "claude-role-model".to_owned()),
            ]),
        },
        Some(jackin_core::ReasoningEffort::Max),
    );

    let data = &plan["data"];
    assert!(
        data["model"].is_null(),
        "instance shape has no top-level model"
    );
    assert_eq!(data["effort"], "max");
    assert_eq!(data["instances"][0]["model"], "gpt-6-luna");
    assert_eq!(data["instances"][0]["effort"], "max");
    assert_eq!(data["instances"][1]["model"], "gpt-6-luna");
    assert_eq!(data["instances"][1]["effort"], "max");
    assert_eq!(data["instances"][2]["model"], "claude-role-model");
    assert!(data["instances"][2]["effort"].is_null());
}

#[test]
fn dry_run_json_projects_single_account_model_without_a_model_override() {
    use jackin_config::ResolvedInstance;

    let selector = jackin_core::RoleSelector::parse("donbeave/the-architect").unwrap();
    let mut plan = dry_run_plan_json(
        &selector,
        &dry_run_workspace(),
        "codex",
        None,
        false,
        DryRunLaunchOverrides::default(),
        &dry_run_image_plan(),
    );
    let identity = jackin_runtime::runtime::DryRunIdentity {
        account_id: Some("c-codex".to_owned()),
        model: Some("stale-account-projection".to_owned()),
        instances: Vec::new(),
        admitted_instances: vec![ResolvedInstance {
            config_id: "codex-main@codex".to_owned(),
            agent: Agent::Codex,
            account_id: "c-codex".to_owned(),
            model: Some("account-default".to_owned()),
            base_url: None,
            xdg_roots: None,
            label: "Codex · Work".to_owned(),
            synthesized: true,
        }],
    };
    apply_dry_run_identity_json(&mut plan, &identity);
    apply_dry_run_load_overrides_json(
        &mut plan,
        Agent::Codex,
        &jackin_runtime::runtime::DryRunModelProjection {
            model: Some("role-default".to_owned()),
            instances: std::collections::BTreeMap::from([(
                "codex-main@codex".to_owned(),
                "role-default".to_owned(),
            )]),
        },
        None,
    );

    let data = &plan["data"];
    assert_eq!(data["model"], "role-default");
    assert!(data["effort"].is_null());
    assert!(data["instances"].as_array().unwrap().is_empty());
}

#[test]
fn dry_run_json_projects_effort_only_without_replacing_resolved_models() {
    use jackin_config::ResolvedInstance;

    let selector = jackin_core::RoleSelector::parse("donbeave/the-architect").unwrap();
    let mut plan = dry_run_plan_json(
        &selector,
        &dry_run_workspace(),
        "codex",
        None,
        false,
        DryRunLaunchOverrides::default(),
        &dry_run_image_plan(),
    );
    let instances = vec![
        ResolvedInstance {
            config_id: "codex-main".to_owned(),
            agent: Agent::Codex,
            account_id: "c-codex".to_owned(),
            model: Some("account-model".to_owned()),
            base_url: None,
            xdg_roots: None,
            label: "Codex · Work".to_owned(),
            synthesized: false,
        },
        ResolvedInstance {
            config_id: "oc-zai".to_owned(),
            agent: Agent::Opencode,
            account_id: "zai".to_owned(),
            model: Some("glm-default".to_owned()),
            base_url: None,
            xdg_roots: None,
            label: "OpenCode · Z.ai".to_owned(),
            synthesized: false,
        },
    ];
    let identity = jackin_runtime::runtime::DryRunIdentity {
        account_id: None,
        model: None,
        instances: instances.clone(),
        admitted_instances: instances,
    };
    apply_dry_run_identity_json(&mut plan, &identity);
    apply_dry_run_load_overrides_json(
        &mut plan,
        Agent::Codex,
        &jackin_runtime::runtime::DryRunModelProjection {
            model: None,
            instances: std::collections::BTreeMap::from([
                ("codex-main".to_owned(), "account-model".to_owned()),
                (
                    "oc-zai".to_owned(),
                    "zai-coding-plan/glm-default".to_owned(),
                ),
            ]),
        },
        Some(jackin_core::ReasoningEffort::Max),
    );

    let data = &plan["data"];
    assert!(data["model"].is_null());
    assert_eq!(data["instances"][0]["model"], "account-model");
    assert_eq!(data["instances"][0]["effort"], "max");
    assert_eq!(data["instances"][1]["model"], "zai-coding-plan/glm-default");
    assert!(data["instances"][1]["effort"].is_null());
}

#[test]
fn dry_run_json_reports_explicit_model_and_effort_overrides() {
    let selector = jackin_core::RoleSelector::parse("donbeave/the-architect").unwrap();
    let plan = dry_run_plan_json(
        &selector,
        &dry_run_workspace(),
        "codex",
        None,
        false,
        DryRunLaunchOverrides {
            model: Some("provider/model-id"),
            effort: Some(jackin_core::ReasoningEffort::Max),
        },
        &dry_run_image_plan(),
    );
    assert_eq!(plan["data"]["model_override"], "provider/model-id");
    assert_eq!(plan["data"]["effort"], "max");
}

#[test]
fn normal_load_dispatch_preserves_absent_model_and_effort_defaults() {
    let mut options = jackin_runtime::runtime::LoadOptions::for_load(false, false);
    apply_load_model_effort(&mut options, None, None);
    assert_eq!(options.model, None);
    assert_eq!(options.effort, None);
}

#[test]
fn normal_load_dispatch_forwards_model_and_effort_overrides() {
    let mut options = jackin_runtime::runtime::LoadOptions::for_load(false, false);
    apply_load_model_effort(
        &mut options,
        Some("provider/model-id".to_owned()),
        Some(jackin_core::ReasoningEffort::High),
    );
    assert_eq!(options.model.as_deref(), Some("provider/model-id"));
    assert_eq!(options.effort, Some(jackin_core::ReasoningEffort::High));
}
