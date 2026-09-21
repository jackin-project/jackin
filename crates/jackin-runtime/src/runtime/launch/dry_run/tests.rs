// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! `--dry-run` identity agrees with launch admission by construction: every
//! case asserts the helper against the exact `resolve_launch` call the
//! pipeline provisions from. Mirrors the S5 acceptance matrix (bindings at
//! each scope, lists at each scope, coexistence, ambiguity, unknown entries,
//! explicit picks) plus the F7 byte-exact model round-trip.

use super::*;
use jackin_config::{
    AccountConfig, AccountCredential, AgentConfiguration, AiProvider, WorkspaceConfig,
};

const ROLE: &str = "agent-smith";
const WS: &str = "myapp";

fn claude_profile_account(name: &str) -> AccountConfig {
    AccountConfig {
        enabled: true,
        name: name.to_owned(),
        provider: AiProvider::Anthropic,
        credential: AccountCredential::Profile {
            agent: Agent::Claude,
            directory: format!("/profiles/{name}").into(),
            xdg_roots: None,
            source_selector: None,
        },
    }
}

fn configuration(agent: Agent, account: &str) -> AgentConfiguration {
    AgentConfiguration {
        agent,
        account: account.to_owned(),
        model: None,
        base_url: None,
        display_label: None,
        invoked_via_wrapper: None,
    }
}

/// S5 matrix base: three Claude accounts allowlisted in `myapp`, one
/// configuration per account, no bindings or lists.
fn matrix_config() -> (AppConfig, WorkspaceName) {
    let mut config = AppConfig::default();
    for id in ["c-lab", "c-home", "c-work"] {
        config
            .accounts
            .insert(id.to_owned(), claude_profile_account(id));
    }
    for (id, account) in [
        ("cfg-r", "c-lab"),
        ("cfg-r2", "c-home"),
        ("cfg-w", "c-work"),
        ("cfg-g", "c-home"),
    ] {
        config
            .agent_configurations
            .insert(id.to_owned(), configuration(Agent::Claude, account));
    }
    config.workspaces.insert(
        WS.to_owned(),
        WorkspaceConfig {
            workdir: "/app".to_owned(),
            accounts: vec!["c-home".to_owned(), "c-lab".to_owned(), "c-work".to_owned()],
            ..WorkspaceConfig::default()
        },
    );
    let workspace = WorkspaceName::parse(WS).unwrap();
    (config, workspace)
}

fn set_role_list(config: &mut AppConfig, ids: &[&str]) {
    config
        .workspaces
        .get_mut(WS)
        .unwrap()
        .roles
        .entry(ROLE.to_owned())
        .or_default()
        .default_launch = Some(ids.iter().map(ToString::to_string).collect());
}

fn identity(config: &AppConfig, workspace: &WorkspaceName) -> anyhow::Result<DryRunIdentity> {
    resolve_dry_run_identity(config, Agent::Claude, Some(workspace), ROLE, false)
}

/// The admission set the launch pipeline would provision.
fn admission(
    config: &AppConfig,
    workspace: &WorkspaceName,
) -> anyhow::Result<Vec<ResolvedInstance>> {
    Ok(jackin_config::resolve_launch(
        config,
        Some(workspace),
        ROLE,
        None,
        Some(Agent::Claude),
    )?)
}

fn admitted_pairs(instances: &[ResolvedInstance]) -> Vec<(&str, &str)> {
    instances
        .iter()
        .map(|instance| (instance.config_id.as_str(), instance.account_id.as_str()))
        .collect()
}

#[test]
fn role_list_beats_global_binding() {
    // Case V: binding names c-work, role list admits cfg-r/c-lab. The plan
    // must name the admitted instance, not the binding.
    let (mut config, workspace) = matrix_config();
    config
        .account_bindings
        .insert(Agent::Claude, "c-work".to_owned());
    set_role_list(&mut config, &["cfg-r"]);

    let plan = identity(&config, &workspace).unwrap();
    assert_eq!(plan.account_id, None);
    assert_eq!(admitted_pairs(&plan.instances), [("cfg-r", "c-lab")]);
    assert_eq!(plan.instances, admission(&config, &workspace).unwrap());
}

#[test]
fn workspace_list_beats_role_binding() {
    let (mut config, workspace) = matrix_config();
    config
        .workspaces
        .get_mut(WS)
        .unwrap()
        .roles
        .entry(ROLE.to_owned())
        .or_default()
        .account_bindings
        .insert(Agent::Claude, "c-lab".to_owned());
    config.workspaces.get_mut(WS).unwrap().default_launch = Some(vec!["cfg-w".to_owned()]);

    let plan = identity(&config, &workspace).unwrap();
    assert_eq!(plan.account_id, None);
    assert_eq!(admitted_pairs(&plan.instances), [("cfg-w", "c-work")]);
    assert_eq!(plan.instances, admission(&config, &workspace).unwrap());
}

#[test]
fn global_list_beats_workspace_binding() {
    let (mut config, workspace) = matrix_config();
    config
        .workspaces
        .get_mut(WS)
        .unwrap()
        .account_bindings
        .insert(Agent::Claude, "c-home".to_owned());
    config.default_launch = Some(vec!["cfg-g".to_owned()]);

    let plan = identity(&config, &workspace).unwrap();
    assert_eq!(plan.account_id, None);
    assert_eq!(admitted_pairs(&plan.instances), [("cfg-g", "c-home")]);
    assert_eq!(plan.instances, admission(&config, &workspace).unwrap());
}

#[test]
fn role_list_without_binding_lists_its_instance() {
    // Cases B/RESTORED.
    let (mut config, workspace) = matrix_config();
    set_role_list(&mut config, &["cfg-r"]);

    let plan = identity(&config, &workspace).unwrap();
    assert_eq!(plan.account_id, None);
    assert_eq!(admitted_pairs(&plan.instances), [("cfg-r", "c-lab")]);
    assert_eq!(plan.instances, admission(&config, &workspace).unwrap());
}

#[test]
fn workspace_list_without_binding_lists_its_instance() {
    // Case C shape.
    let (mut config, workspace) = matrix_config();
    config.workspaces.get_mut(WS).unwrap().default_launch = Some(vec!["cfg-w".to_owned()]);

    let plan = identity(&config, &workspace).unwrap();
    assert_eq!(plan.account_id, None);
    assert_eq!(admitted_pairs(&plan.instances), [("cfg-w", "c-work")]);
    assert_eq!(plan.instances, admission(&config, &workspace).unwrap());
}

#[test]
fn global_list_without_binding_lists_its_instance() {
    // Case D shape.
    let (mut config, workspace) = matrix_config();
    config.default_launch = Some(vec!["cfg-g".to_owned()]);

    let plan = identity(&config, &workspace).unwrap();
    assert_eq!(plan.account_id, None);
    assert_eq!(admitted_pairs(&plan.instances), [("cfg-g", "c-home")]);
    assert_eq!(plan.instances, admission(&config, &workspace).unwrap());
}

#[test]
fn global_binding_without_list_reports_single_account() {
    // Case E shape.
    let (mut config, workspace) = matrix_config();
    config
        .account_bindings
        .insert(Agent::Claude, "c-lab".to_owned());

    let plan = identity(&config, &workspace).unwrap();
    assert_eq!(plan.account_id.as_deref(), Some("c-lab"));
    assert!(plan.instances.is_empty());
    let admitted = admission(&config, &workspace).unwrap();
    assert_eq!(admitted_pairs(&admitted), [("c-lab@claude", "c-lab")]);
}

#[test]
fn workspace_binding_without_list_reports_single_account() {
    // Case A1 shape.
    let (mut config, workspace) = matrix_config();
    config
        .workspaces
        .get_mut(WS)
        .unwrap()
        .account_bindings
        .insert(Agent::Claude, "c-home".to_owned());

    let plan = identity(&config, &workspace).unwrap();
    assert_eq!(plan.account_id.as_deref(), Some("c-home"));
    assert!(plan.instances.is_empty());
}

#[test]
fn role_binding_without_list_reports_single_account() {
    // Case A2 shape.
    let (mut config, workspace) = matrix_config();
    config
        .workspaces
        .get_mut(WS)
        .unwrap()
        .roles
        .entry(ROLE.to_owned())
        .or_default()
        .account_bindings
        .insert(Agent::Claude, "c-home".to_owned());

    let plan = identity(&config, &workspace).unwrap();
    assert_eq!(plan.account_id.as_deref(), Some("c-home"));
    assert!(plan.instances.is_empty());
}

#[test]
fn multi_entry_list_reports_every_instance() {
    // Case G.
    let (mut config, workspace) = matrix_config();
    set_role_list(&mut config, &["cfg-r", "cfg-r2"]);

    let plan = identity(&config, &workspace).unwrap();
    assert_eq!(plan.account_id, None);
    assert_eq!(
        admitted_pairs(&plan.instances),
        [("cfg-r", "c-lab"), ("cfg-r2", "c-home")]
    );
    assert_eq!(plan.instances, admission(&config, &workspace).unwrap());
}

#[test]
fn sole_eligible_account_without_defaults_reports_single_account() {
    let (mut config, workspace) = matrix_config();
    config.workspaces.get_mut(WS).unwrap().accounts = vec!["c-lab".to_owned()];

    let plan = identity(&config, &workspace).unwrap();
    assert_eq!(plan.account_id.as_deref(), Some("c-lab"));
    assert!(plan.instances.is_empty());
}

#[test]
fn ambiguous_fallback_errors_exactly_like_admission() {
    let (config, workspace) = matrix_config();
    let plan = identity(&config, &workspace).unwrap_err().to_string();
    let launch = admission(&config, &workspace).unwrap_err().to_string();
    assert_eq!(plan, launch);
    assert!(plan.contains("multiple accounts support claude"));
}

#[test]
fn unknown_configuration_in_list_errors() {
    // Case H5.
    let (mut config, workspace) = matrix_config();
    set_role_list(&mut config, &["cfg-nope"]);

    let error = identity(&config, &workspace).unwrap_err().to_string();
    assert!(
        error.contains("unknown agent configuration \"cfg-nope\""),
        "unexpected error: {error}"
    );
    assert_eq!(
        error,
        admission(&config, &workspace).unwrap_err().to_string()
    );
}

#[test]
fn unknown_binding_errors_exactly_like_admission() {
    let (mut config, workspace) = matrix_config();
    config
        .account_bindings
        .insert(Agent::Claude, "c-ghost".to_owned());

    let error = identity(&config, &workspace).unwrap_err().to_string();
    assert_eq!(
        error,
        admission(&config, &workspace).unwrap_err().to_string()
    );
}

#[test]
fn explicit_pick_without_ambient_list_reports_single_account() {
    // Case F shape: `--account c-work` with no ambient list synthesizes an
    // ephemeral one-entry default; the plan still names the pick.
    let (config, workspace) = matrix_config();
    let scoped = super::super::programmatic::with_account_selection(
        &config,
        Agent::Claude,
        Some(&workspace),
        ROLE,
        "c-work",
    )
    .unwrap();

    let plan =
        resolve_dry_run_identity(&scoped, Agent::Claude, Some(&workspace), ROLE, true).unwrap();
    assert_eq!(plan.account_id.as_deref(), Some("c-work"));
    assert!(plan.instances.is_empty());
}

#[test]
fn explicit_pick_with_ambient_list_reports_single_account() {
    let (mut config, workspace) = matrix_config();
    set_role_list(&mut config, &["cfg-r", "cfg-r2"]);
    let scoped = super::super::programmatic::with_account_selection(
        &config,
        Agent::Claude,
        Some(&workspace),
        ROLE,
        "c-home",
    )
    .unwrap();

    let plan =
        resolve_dry_run_identity(&scoped, Agent::Claude, Some(&workspace), ROLE, true).unwrap();
    assert_eq!(plan.account_id.as_deref(), Some("c-home"));
    assert!(plan.instances.is_empty());
    // Substance still agrees: admission provisions exactly the pick.
    let admitted = admission(&scoped, &workspace).unwrap();
    assert!(
        admitted
            .iter()
            .all(|instance| instance.account_id == "c-home")
    );
}

#[test]
fn unadmitted_explicit_pick_errors_before_identity() {
    // Case H4: the pick never reaches identity resolution.
    let (mut config, workspace) = matrix_config();
    set_role_list(&mut config, &["cfg-r"]);
    let error = super::super::programmatic::with_account_selection(
        &config,
        Agent::Claude,
        Some(&workspace),
        ROLE,
        "c-work",
    )
    .unwrap_err()
    .to_string();
    assert!(
        error.contains("not admitted for claude by the configured default launch set"),
        "unexpected error: {error}"
    );
}

#[test]
fn plan_matches_admission_across_binding_list_combinations() {
    // No-drift sweep: every binding scope × every list scope. The plan's
    // substance (admitted account ids in order) always equals admission's.
    let bindings: [Option<(&str, &str)>; 4] = [
        None,
        Some(("global", "c-work")),
        Some(("workspace", "c-home")),
        Some(("role", "c-lab")),
    ];
    let lists: [Option<(&str, &[&str])>; 4] = [
        None,
        Some(("global", &["cfg-g"])),
        Some(("workspace", &["cfg-w"])),
        Some(("role", &["cfg-r"])),
    ];
    for binding in bindings {
        for list in lists {
            let (mut config, workspace) = matrix_config();
            if let Some((scope, account)) = binding {
                match scope {
                    "global" => {
                        config
                            .account_bindings
                            .insert(Agent::Claude, account.to_owned());
                    }
                    "workspace" => {
                        config
                            .workspaces
                            .get_mut(WS)
                            .unwrap()
                            .account_bindings
                            .insert(Agent::Claude, account.to_owned());
                    }
                    _ => {
                        config
                            .workspaces
                            .get_mut(WS)
                            .unwrap()
                            .roles
                            .entry(ROLE.to_owned())
                            .or_default()
                            .account_bindings
                            .insert(Agent::Claude, account.to_owned());
                    }
                }
            }
            if let Some((scope, ids)) = list {
                let ids: Vec<String> = ids.iter().map(ToString::to_string).collect();
                match scope {
                    "global" => config.default_launch = Some(ids),
                    "workspace" => {
                        config.workspaces.get_mut(WS).unwrap().default_launch = Some(ids);
                    }
                    _ => {
                        config
                            .workspaces
                            .get_mut(WS)
                            .unwrap()
                            .roles
                            .entry(ROLE.to_owned())
                            .or_default()
                            .default_launch = Some(ids);
                    }
                }
            }
            let plan = identity(&config, &workspace);
            let admitted = admission(&config, &workspace);
            if binding.is_none() && list.is_none() {
                // Ambiguous fallback: both sides fail, identically.
                assert_eq!(
                    plan.unwrap_err().to_string(),
                    admitted.unwrap_err().to_string()
                );
                continue;
            }
            let plan = plan.unwrap();
            let admitted = admitted.unwrap();
            let planned_accounts: Vec<&str> = plan.account_id.as_deref().map_or_else(
                || {
                    plan.instances
                        .iter()
                        .map(|instance| instance.account_id.as_str())
                        .collect()
                },
                |single| vec![single],
            );
            let admitted_accounts: Vec<&str> = admitted
                .iter()
                .map(|instance| instance.account_id.as_str())
                .collect();
            assert_eq!(
                planned_accounts, admitted_accounts,
                "drift with binding={binding:?} list={list:?}"
            );
            // Shape rule: single-account display only for the synthesized
            // fallback; list-backed admissions always surface as instances.
            if admitted.len() == 1 && admitted[0].synthesized {
                assert_eq!(
                    plan.account_id.as_deref(),
                    Some(admitted[0].account_id.as_str())
                );
                assert!(plan.instances.is_empty());
            } else {
                assert_eq!(plan.account_id, None);
                assert_eq!(plan.instances, admitted);
            }
        }
    }
}

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
