// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

#[test]
fn default_launch_scope_precedence_replaces_without_union() {
    let (mut config, ws) = launch_config();
    config.default_launch = Some(vec!["claude-a".into()]);
    set_workspace_default(&mut config, ws.as_str(), &["claude-z"]);
    set_role_default(&mut config, ws.as_str(), ROLE, &["claude-a"]);

    let eligible = accounts_for_launch(&config, Some(&ws), Agent::Claude);
    assert_eq!(
        select_launch_account(&config, Some(&ws), ROLE, Agent::Claude, eligible).unwrap(),
        LaunchAccountSelection::Launch(jackin_core::LaunchSelection::Configuration(
            "claude-a".into()
        ))
    );

    // Clearing the role scope falls through to the workspace scope, not a
    // union of both.
    config
        .workspaces
        .get_mut(ws.as_str())
        .unwrap()
        .roles
        .clear();
    let eligible = accounts_for_launch(&config, Some(&ws), Agent::Claude);
    assert_eq!(
        select_launch_account(&config, Some(&ws), ROLE, Agent::Claude, eligible).unwrap(),
        LaunchAccountSelection::Launch(jackin_core::LaunchSelection::Configuration(
            "claude-z".into()
        ))
    );
}

#[test]
fn global_default_filters_unauthorized_candidates() {
    let (mut config, ws) = launch_config();
    config.default_launch = Some(vec!["claude-out".into(), "claude-a".into()]);
    let eligible = accounts_for_launch(&config, Some(&ws), Agent::Claude);

    assert_eq!(
        select_launch_account(&config, Some(&ws), ROLE, Agent::Claude, eligible).unwrap(),
        LaunchAccountSelection::Launch(jackin_core::LaunchSelection::Configuration(
            "claude-a".into()
        ))
    );

    // A global default that filters down to nothing admits nothing for
    // the agent: an error, never a silent fallback to the eligible list.
    config.default_launch = Some(vec!["claude-out".into()]);
    let eligible = accounts_for_launch(&config, Some(&ws), Agent::Claude);
    assert!(!eligible.is_empty());
    let error =
        select_launch_account(&config, Some(&ws), ROLE, Agent::Claude, eligible).unwrap_err();
    assert!(error.to_string().contains("admits"), "got {error:?}");
}

#[test]
fn unknown_configuration_in_default_fails_atomically() {
    let (mut config, ws) = launch_config();
    set_role_default(&mut config, ws.as_str(), ROLE, &["ghost"]);
    let eligible = accounts_for_launch(&config, Some(&ws), Agent::Claude);
    assert!(!eligible.is_empty());

    let error =
        select_launch_account(&config, Some(&ws), ROLE, Agent::Claude, eligible).unwrap_err();
    let message = error.to_string();
    assert!(
        message.contains("unknown agent configuration"),
        "got {message:?}"
    );
    assert!(
        !message.contains("test-key"),
        "resolver errors must never leak credential values; got {message:?}"
    );
}

#[test]
fn workspace_default_outside_allowlist_hard_errors() {
    let (mut config, ws) = launch_config();
    set_workspace_default(&mut config, ws.as_str(), &["claude-out"]);
    let eligible = accounts_for_launch(&config, Some(&ws), Agent::Claude);
    assert!(!eligible.is_empty());

    let error =
        select_launch_account(&config, Some(&ws), ROLE, Agent::Claude, eligible).unwrap_err();
    assert!(error.to_string().contains("not assigned"), "got {error:?}");
}

#[test]
fn incompatible_default_configuration_errors() {
    let (mut config, ws) = launch_config();
    // `o-codex` is an OpenAI account: it cannot authenticate Claude.
    config.agent_configurations.insert(
        "claude-bogus".into(),
        agent_configuration(Agent::Claude, "o-codex"),
    );
    set_workspace_default(&mut config, ws.as_str(), &["claude-bogus"]);
    let eligible = accounts_for_launch(&config, Some(&ws), Agent::Claude);

    let error =
        select_launch_account(&config, Some(&ws), ROLE, Agent::Claude, eligible).unwrap_err();
    assert!(
        error.to_string().contains("not authorized"),
        "got {error:?}"
    );
}

#[test]
fn several_admitted_instances_open_picker_in_id_order() {
    let (mut config, ws) = launch_config();
    set_workspace_default(&mut config, ws.as_str(), &["claude-z", "claude-a"]);
    let eligible = accounts_for_launch(&config, Some(&ws), Agent::Claude);

    let selection =
        select_launch_account(&config, Some(&ws), ROLE, Agent::Claude, eligible).unwrap();
    assert_eq!(eligible_ids(&selection), vec!["a-claude", "z-claude"]);
}

#[test]
fn shared_account_instances_open_picker_with_exact_configurations() {
    let (mut config, ws) = launch_config();
    // Two instances, one account (different models): keep both configuration
    // identities in the picker instead of collapsing them to the account.
    for (id, model) in [("claude-a-fast", "fast"), ("claude-a-deep", "deep")] {
        let mut configuration = agent_configuration(Agent::Claude, "a-claude");
        configuration.model = Some(model.into());
        config.agent_configurations.insert(id.into(), configuration);
    }
    set_workspace_default(
        &mut config,
        ws.as_str(),
        &["claude-a-fast", "claude-a-deep"],
    );
    let eligible = accounts_for_launch(&config, Some(&ws), Agent::Claude);

    let selection =
        select_launch_account(&config, Some(&ws), ROLE, Agent::Claude, eligible).unwrap();
    let LaunchAccountSelection::Pick(accounts) = selection else {
        panic!("expected a picker for two configurations sharing one account");
    };
    assert_eq!(
        accounts
            .iter()
            .map(|account| account.configuration_id.as_deref())
            .collect::<Vec<_>>(),
        vec![Some("claude-a-deep"), Some("claude-a-fast")]
    );
}

#[test]
fn default_admitting_only_other_agent_errors_actionably() {
    let (mut config, ws) = launch_config();
    set_workspace_default(&mut config, ws.as_str(), &["codex-o"]);
    let eligible = accounts_for_launch(&config, Some(&ws), Agent::Claude);
    assert!(!eligible.is_empty());

    let error =
        select_launch_account(&config, Some(&ws), ROLE, Agent::Claude, eligible).unwrap_err();
    let message = error.to_string();
    assert!(message.contains("claude"), "got {message:?}");
    assert!(message.contains("demo"), "got {message:?}");
    assert!(message.contains("default_launch"), "got {message:?}");
}

#[test]
fn explicit_empty_default_errors_for_agent_launch() {
    let (mut config, ws) = launch_config();
    set_workspace_default(&mut config, ws.as_str(), &[]);
    let eligible = accounts_for_launch(&config, Some(&ws), Agent::Claude);

    select_launch_account(&config, Some(&ws), ROLE, Agent::Claude, eligible).unwrap_err();
}

#[test]
fn defaults_override_bindings_without_fallback() {
    let (mut config, ws) = launch_config();
    set_role_binding(&mut config, ws.as_str(), ROLE, Agent::Claude, "a-claude");
    set_role_default(&mut config, ws.as_str(), ROLE, &["claude-z"]);
    let eligible = accounts_for_launch(&config, Some(&ws), Agent::Claude);

    // The binding points elsewhere, but the admitted set wins.
    assert_eq!(
        select_launch_account(&config, Some(&ws), ROLE, Agent::Claude, eligible).unwrap(),
        LaunchAccountSelection::Launch(jackin_core::LaunchSelection::Configuration(
            "claude-z".into()
        ))
    );

    // An invalid binding is ignored entirely while a valid default
    // applies: the defaults regime never consults bindings.
    update_role_binding(&mut config, ws.as_str(), ROLE, Agent::Claude, "ghost");
    let eligible = accounts_for_launch(&config, Some(&ws), Agent::Claude);
    assert_eq!(
        select_launch_account(&config, Some(&ws), ROLE, Agent::Claude, eligible).unwrap(),
        LaunchAccountSelection::Launch(jackin_core::LaunchSelection::Configuration(
            "claude-z".into()
        ))
    );
}

#[test]
fn invalid_default_beats_valid_binding() {
    let (mut config, ws) = launch_config();
    set_role_binding(&mut config, ws.as_str(), ROLE, Agent::Claude, "a-claude");
    set_role_default(&mut config, ws.as_str(), ROLE, &["ghost"]);
    let eligible = accounts_for_launch(&config, Some(&ws), Agent::Claude);

    // The binding is valid, but the invalid explicit default fails the
    // whole selection instead of falling back to it.
    let error =
        select_launch_account(&config, Some(&ws), ROLE, Agent::Claude, eligible).unwrap_err();
    assert!(
        error.to_string().contains("unknown agent configuration"),
        "got {error:?}"
    );
}

#[test]
fn ad_hoc_default_launch_resolves_without_workspace() {
    let (mut config, _) = launch_config();
    config.default_launch = Some(vec!["claude-z".into()]);
    let eligible = accounts_for_launch(&config, None, Agent::Claude);
    assert!(eligible.len() > 1);

    assert_eq!(
        select_launch_account(&config, None, ROLE, Agent::Claude, eligible).unwrap(),
        LaunchAccountSelection::Launch(jackin_core::LaunchSelection::Configuration(
            "claude-z".into()
        ))
    );
}
