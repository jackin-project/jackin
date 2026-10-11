// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

#[test]
fn role_binding_beats_workspace_and_global() {
    let (mut config, ws) = test_config();
    let workspace = config.workspaces.get_mut(ws.as_str()).unwrap();
    workspace
        .account_bindings
        .insert(Agent::Claude, "a-claude".into());
    config
        .account_bindings
        .insert(Agent::Claude, "a-claude".into());
    set_role_binding(&mut config, ws.as_str(), ROLE, Agent::Claude, "z-claude");

    assert_eq!(
        resolve_agent_default(&config, Some(&ws), ROLE, Agent::Claude),
        AgentDefaultResolution::Launch("z-claude".into())
    );
}

#[test]
fn workspace_binding_beats_global() {
    let (mut config, ws) = test_config();
    config
        .workspaces
        .get_mut(ws.as_str())
        .unwrap()
        .account_bindings
        .insert(Agent::Claude, "z-claude".into());
    config
        .account_bindings
        .insert(Agent::Claude, "a-claude".into());

    assert_eq!(
        resolve_agent_default(&config, Some(&ws), ROLE, Agent::Claude),
        AgentDefaultResolution::Launch("z-claude".into())
    );
}

#[test]
fn global_binding_honored_with_several_accounts() {
    let (mut config, ws) = test_config();
    config
        .account_bindings
        .insert(Agent::Claude, "z-claude".into());
    let eligible = accounts_for_launch(&config, Some(&ws), Agent::Claude);
    assert!(eligible.len() > 1);

    assert_eq!(
        select_launch_account(&config, Some(&ws), ROLE, Agent::Claude, eligible).unwrap(),
        LaunchAccountSelection::Launch(jackin_core::LaunchSelection::Account("z-claude".into()))
    );
}

#[test]
fn role_binding_for_other_role_is_ignored() {
    let (mut config, ws) = test_config();
    set_role_binding(
        &mut config,
        ws.as_str(),
        "other-role",
        Agent::Claude,
        "z-claude",
    );

    assert_eq!(
        resolve_agent_default(&config, Some(&ws), ROLE, Agent::Claude),
        AgentDefaultResolution::NoDefault
    );
}

#[test]
fn sole_eligible_candidate_launches_without_binding() {
    let (mut config, ws) = test_config();
    config.workspaces.get_mut(ws.as_str()).unwrap().accounts = vec!["z-claude".into()];
    let eligible = accounts_for_launch(&config, Some(&ws), Agent::Claude);
    assert_eq!(eligible.len(), 1);

    assert_eq!(
        select_launch_account(&config, Some(&ws), ROLE, Agent::Claude, eligible).unwrap(),
        LaunchAccountSelection::Launch(jackin_core::LaunchSelection::Account("z-claude".into()))
    );
}

#[test]
fn no_binding_with_several_candidates_opens_picker_in_id_order() {
    let (config, ws) = test_config();
    let mut eligible = accounts_for_launch(&config, Some(&ws), Agent::Claude);
    eligible.reverse();
    assert_eq!(eligible.len(), 2);

    let selection =
        select_launch_account(&config, Some(&ws), ROLE, Agent::Claude, eligible).unwrap();
    assert_eq!(eligible_ids(&selection), vec!["a-claude", "z-claude"]);
}

#[test]
fn picker_order_is_stable_regardless_of_input_order() {
    let (config, ws) = test_config();
    let forward = accounts_for_launch(&config, Some(&ws), Agent::Claude);
    let mut backward = forward.clone();
    backward.reverse();

    let from_forward =
        select_launch_account(&config, Some(&ws), ROLE, Agent::Claude, forward).unwrap();
    let from_backward =
        select_launch_account(&config, Some(&ws), ROLE, Agent::Claude, backward).unwrap();
    assert_eq!(eligible_ids(&from_forward), eligible_ids(&from_backward));
}

#[test]
fn zero_eligible_candidates_error_actionably() {
    let (mut config, ws) = test_config();
    config.workspaces.get_mut(ws.as_str()).unwrap().accounts = vec!["o-codex".into()];
    let eligible = accounts_for_launch(&config, Some(&ws), Agent::Claude);
    assert!(eligible.is_empty());

    let error =
        select_launch_account(&config, Some(&ws), ROLE, Agent::Claude, eligible).unwrap_err();
    let message = error.to_string();
    assert!(
        message.contains("claude"),
        "error must name the agent; got {message:?}"
    );
    assert!(
        message.contains("demo"),
        "error must name the workspace; got {message:?}"
    );
    assert!(
        message.contains("binding"),
        "error must point at the binding remedy; got {message:?}"
    );
}

#[test]
fn zero_eligible_candidates_without_workspace_errors() {
    let (config, _) = test_config();
    let error = select_launch_account(&config, None, ROLE, Agent::Muse, Vec::new()).unwrap_err();
    assert!(
        error.to_string().contains("muse"),
        "ad-hoc error must name the agent; got {error:?}"
    );
}

#[test]
fn unauthorized_global_binding_hard_errors_without_fallback() {
    // The saved workspace authorizes a different account. The global
    // selection is explicit and must not be filtered into a fallback.
    let (mut config, ws) = test_config();
    config.workspaces.get_mut(ws.as_str()).unwrap().accounts = vec!["z-claude".into()];
    config
        .account_bindings
        .insert(Agent::Claude, "a-claude".into());
    let eligible = accounts_for_launch(&config, Some(&ws), Agent::Claude);
    assert_eq!(eligible.len(), 1);

    let resolution = resolve_agent_default(&config, Some(&ws), ROLE, Agent::Claude);
    assert!(
        matches!(resolution, AgentDefaultResolution::Invalid(_)),
        "unauthorized global selection must be invalid; got {resolution:?}"
    );
    let error =
        select_launch_account(&config, Some(&ws), ROLE, Agent::Claude, eligible).unwrap_err();
    assert!(error.to_string().contains("not assigned"), "{error:?}");
}

#[test]
fn missing_workspace_falls_back_to_global_accounts() {
    // Missing selection: ad-hoc launches have no allowlist, so the
    // global binding applies unfiltered and bare candidates stay
    // eligible.
    let (mut config, _) = test_config();
    config
        .account_bindings
        .insert(Agent::Claude, "z-claude".into());
    let eligible = accounts_for_launch(&config, None, Agent::Claude);
    assert!(eligible.len() > 1);

    assert_eq!(
        select_launch_account(&config, None, ROLE, Agent::Claude, eligible).unwrap(),
        LaunchAccountSelection::Launch(jackin_core::LaunchSelection::Account("z-claude".into()))
    );

    let (config, _) = test_config();
    let eligible = accounts_for_launch(&config, None, Agent::Claude);
    let selection = select_launch_account(&config, None, ROLE, Agent::Claude, eligible).unwrap();
    assert_eq!(
        eligible_ids(&selection),
        vec!["a-claude", "outside", "z-claude"]
    );
}

#[test]
fn unknown_workspace_name_errors() {
    let (config, _) = test_config();
    let ghost = WorkspaceName::parse("ghost").unwrap();
    let eligible = accounts_for_launch(&config, None, Agent::Claude);

    assert!(matches!(
        resolve_agent_default(&config, Some(&ghost), ROLE, Agent::Claude),
        AgentDefaultResolution::Invalid(_)
    ));
    select_launch_account(&config, Some(&ghost), ROLE, Agent::Claude, eligible).unwrap_err();
}

#[test]
fn unauthorized_role_binding_hard_errors_without_fallback() {
    let (mut config, ws) = test_config();
    set_role_binding(&mut config, ws.as_str(), ROLE, Agent::Claude, "outside");
    let eligible = accounts_for_launch(&config, Some(&ws), Agent::Claude);
    assert!(!eligible.is_empty());

    let resolution = resolve_agent_default(&config, Some(&ws), ROLE, Agent::Claude);
    assert!(
        matches!(resolution, AgentDefaultResolution::Invalid(_)),
        "unauthorized role default must be invalid; got {resolution:?}"
    );
    let error =
        select_launch_account(&config, Some(&ws), ROLE, Agent::Claude, eligible).unwrap_err();
    assert!(error.to_string().contains("not assigned"), "got {error:?}");
}

#[test]
fn unauthorized_workspace_binding_hard_errors_without_fallback() {
    let (mut config, ws) = test_config();
    config
        .workspaces
        .get_mut(ws.as_str())
        .unwrap()
        .account_bindings
        .insert(Agent::Claude, "outside".into());
    let eligible = accounts_for_launch(&config, Some(&ws), Agent::Claude);
    assert!(!eligible.is_empty());

    assert!(matches!(
        resolve_agent_default(&config, Some(&ws), ROLE, Agent::Claude),
        AgentDefaultResolution::Invalid(_)
    ));
    select_launch_account(&config, Some(&ws), ROLE, Agent::Claude, eligible).unwrap_err();
}

#[test]
fn unauthorized_global_binding_is_not_filtered_into_fallback() {
    // Global defaults can never widen workspace access: an unauthorized
    // global binding is an error, not a reason to choose another account.
    let (mut config, ws) = test_config();
    config
        .account_bindings
        .insert(Agent::Claude, "outside".into());
    let eligible = accounts_for_launch(&config, Some(&ws), Agent::Claude);
    assert_eq!(eligible.len(), 2);

    assert!(matches!(
        resolve_agent_default(&config, Some(&ws), ROLE, Agent::Claude),
        AgentDefaultResolution::Invalid(_)
    ));
    select_launch_account(&config, Some(&ws), ROLE, Agent::Claude, eligible).unwrap_err();
}

#[test]
fn binding_to_unknown_account_errors_at_every_scope() {
    let (mut config, ws) = test_config();
    let eligible = accounts_for_launch(&config, Some(&ws), Agent::Claude);

    set_role_binding(&mut config, ws.as_str(), ROLE, Agent::Claude, "ghost");
    assert!(matches!(
        resolve_agent_default(&config, Some(&ws), ROLE, Agent::Claude),
        AgentDefaultResolution::Invalid(_)
    ));
    // Role scope wins, so the error below pins the role binding; clear
    // it to exercise the workspace scope, then the global scope.
    config
        .workspaces
        .get_mut(ws.as_str())
        .unwrap()
        .roles
        .clear();
    config
        .workspaces
        .get_mut(ws.as_str())
        .unwrap()
        .account_bindings
        .insert(Agent::Claude, "ghost".into());
    // The workspace binding names an id outside the allowlist, so the
    // authorization check fires before the unknown-id check — either way
    // the selection fails atomically.
    select_launch_account(&config, Some(&ws), ROLE, Agent::Claude, eligible.clone()).unwrap_err();
    config
        .workspaces
        .get_mut(ws.as_str())
        .unwrap()
        .account_bindings
        .clear();
    config
        .workspaces
        .get_mut(ws.as_str())
        .unwrap()
        .accounts
        .push("ghost".into());
    // The allowlist now references the ghost id, so the dangling-id
    // check fires even without any binding.
    select_launch_account(&config, Some(&ws), ROLE, Agent::Claude, eligible).unwrap_err();
}

#[test]
fn global_binding_to_unknown_id_errors_without_fallback() {
    // A global binding remains an explicit selection when inherited by a
    // workspace, so an unknown ID is an atomic configuration error.
    let (mut config, ws) = test_config();
    config
        .account_bindings
        .insert(Agent::Claude, "ghost".into());
    let eligible = accounts_for_launch(&config, Some(&ws), Agent::Claude);

    assert!(matches!(
        resolve_agent_default(&config, Some(&ws), ROLE, Agent::Claude),
        AgentDefaultResolution::Invalid(_)
    ));
    select_launch_account(&config, Some(&ws), ROLE, Agent::Claude, eligible).unwrap_err();
}

#[test]
fn binding_to_incompatible_account_errors() {
    let (mut config, ws) = test_config();
    set_role_binding(&mut config, ws.as_str(), ROLE, Agent::Claude, "o-codex");
    let eligible = accounts_for_launch(&config, Some(&ws), Agent::Claude);

    let error =
        select_launch_account(&config, Some(&ws), ROLE, Agent::Claude, eligible).unwrap_err();
    assert!(
        error.to_string().contains("does not support"),
        "got {error:?}"
    );
}

#[test]
fn binding_to_disabled_account_errors() {
    let (mut config, ws) = test_config();
    config.accounts.get_mut("z-claude").unwrap().enabled = false;
    set_role_binding(&mut config, ws.as_str(), ROLE, Agent::Claude, "z-claude");
    let eligible = accounts_for_launch(&config, Some(&ws), Agent::Claude);

    select_launch_account(&config, Some(&ws), ROLE, Agent::Claude, eligible).unwrap_err();
}

#[test]
fn empty_string_binding_fails_atomically() {
    // An explicit empty selection is invalid (unknown id), never a
    // trigger to fall back to the eligible candidates.
    let (mut config, ws) = test_config();
    set_role_binding(&mut config, ws.as_str(), ROLE, Agent::Claude, "");
    let eligible = accounts_for_launch(&config, Some(&ws), Agent::Claude);
    assert!(!eligible.is_empty());

    select_launch_account(&config, Some(&ws), ROLE, Agent::Claude, eligible).unwrap_err();
}

#[test]
fn binding_for_other_agent_does_not_leak() {
    let (mut config, ws) = test_config();
    set_role_binding(&mut config, ws.as_str(), ROLE, Agent::Claude, "z-claude");

    assert_eq!(
        resolve_agent_default(&config, Some(&ws), ROLE, Agent::Codex),
        AgentDefaultResolution::NoDefault
    );
    let eligible = accounts_for_launch(&config, Some(&ws), Agent::Codex);
    assert_eq!(
        select_launch_account(&config, Some(&ws), ROLE, Agent::Codex, eligible).unwrap(),
        LaunchAccountSelection::Launch(jackin_core::LaunchSelection::Account("o-codex".into()))
    );
}

#[test]
fn role_default_launch_honored_with_several_accounts() {
    let (mut config, ws) = launch_config();
    set_role_default(&mut config, ws.as_str(), ROLE, &["claude-z"]);
    // The passed eligible list is ignored in the defaults regime: the
    // admitted set resolves through `resolve_launch` instead.
    let eligible = accounts_for_launch(&config, Some(&ws), Agent::Claude);
    assert!(eligible.len() > 1);

    assert_eq!(
        select_launch_account(&config, Some(&ws), ROLE, Agent::Claude, eligible).unwrap(),
        LaunchAccountSelection::Launch(jackin_core::LaunchSelection::Configuration(
            "claude-z".into()
        ))
    );
    assert_eq!(
        select_launch_account(&config, Some(&ws), ROLE, Agent::Claude, Vec::new()).unwrap(),
        LaunchAccountSelection::Launch(jackin_core::LaunchSelection::Configuration(
            "claude-z".into()
        ))
    );
}
