// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

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
    let scoped = programmatic::with_account_selection(
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
    let scoped = programmatic::with_account_selection(
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
    let error = programmatic::with_account_selection(
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
