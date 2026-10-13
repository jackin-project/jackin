// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

#[test]
fn new_session_account_picker_skips_when_no_choice() {
    // Single-provider Codex must dispatch directly, mirroring Claude.
    let config = AppConfig::default();
    let tmp = tempfile::tempdir().unwrap();
    let mut state = ManagerState::from_config(&config, tmp.path());
    let outcome = commit_new_session_picker(
        &mut state,
        jackin_core::Agent::Codex,
        vec![jackin_protocol::Provider::Openai],
    );

    match outcome {
        InputOutcome::NewSessionWithAccount {
            container,
            agent,
            instance_id,
        } => {
            assert_eq!(container, "jackin-demo-architect");
            assert_eq!(agent, jackin_core::Agent::Codex);
            assert_eq!(instance_id, "openai");
        }
        other => panic!("expected direct new-session dispatch; got {other:?}"),
    }
    assert!(
        state.inline_account_picker.is_none(),
        "single-provider Codex must not open the provider picker"
    );
}

#[test]
fn new_session_account_picker_opens_for_claude() {
    let config = AppConfig::default();
    let tmp = tempfile::tempdir().unwrap();
    let mut state = ManagerState::from_config(&config, tmp.path());
    let outcome =
        commit_new_session_picker(&mut state, jackin_core::Agent::Claude, provider_choices());

    assert!(matches!(outcome, InputOutcome::Continue));
    let Some(picker) = state.inline_account_picker else {
        panic!("Claude with providers must open provider picker");
    };
    assert_eq!(picker.context, "jackin-demo-architect");
    assert_eq!(picker.agent, jackin_core::Agent::Claude);
    assert_eq!(picker.providers().len(), 2);
    assert_eq!(picker.selected(), 0);
}

#[test]
fn new_session_account_picker_opens_for_codex_with_multiple_providers() {
    // Codex with two providers configured opens the picker.
    let config = AppConfig::default();
    let tmp = tempfile::tempdir().unwrap();
    let mut state = ManagerState::from_config(&config, tmp.path());
    let outcome = commit_new_session_picker(
        &mut state,
        jackin_core::Agent::Codex,
        codex_provider_choices(),
    );

    assert!(matches!(outcome, InputOutcome::Continue));
    let Some(picker) = state.inline_account_picker else {
        panic!("Codex with multiple providers must open the provider picker");
    };
    assert_eq!(picker.context, "jackin-demo-architect");
    assert_eq!(picker.agent, jackin_core::Agent::Codex);
    assert_eq!(picker.providers().len(), 2);
    // Seeded [openai, minimax]; the picker renders stable id-ascending
    // order, so minimax sorts first.
    assert_eq!(
        picker.providers()[0].provider,
        jackin_config::AiProvider::Minimax
    );
    assert_eq!(
        picker.providers()[1].provider,
        jackin_config::AiProvider::OpenAi
    );
}

#[test]
fn new_session_picker_does_not_offer_host_config_providers_for_running_container() {
    let workdir = "/workspace/demo";
    let ws = WorkspaceConfig {
        workdir: workdir.into(),
        mounts: vec![],
        ..Default::default()
    };
    let (mut state, mut config, paths, tmp) = list_state_selecting_ws(ws);
    config.env.insert(
        "ZAI_API_KEY".into(),
        jackin_core::EnvValue::Plain("host-key-added-after-launch".into()),
    );
    state.instances = vec![instance_entry(
        "jackin-demo-architect-running",
        InstanceStatus::Running,
        workdir,
    )];
    state.expand_workspace(0);
    state.selected = state
        .index_of_row(crate::tui::state::ManagerListRow::WorkspaceInstance(0, 0))
        .expect("expanded workspace instance row exists");

    let outcome = handle_key(
        &mut state,
        &mut config,
        &paths,
        tmp.path(),
        key(KeyCode::Char('n')),
    )
    .unwrap();

    assert!(matches!(outcome, InputOutcome::Continue));
    let Some((_container, _picker, providers)) = state.inline_new_session_picker.as_ref() else {
        panic!("N on a running instance must open the agent picker");
    };
    assert!(
        providers.is_empty(),
        "host config must not offer providers for an already-running container"
    );
}

#[test]
fn new_session_commit_with_empty_providers_errors_instead_of_none() {
    // Zero eligible must never dispatch `NewSessionWithAccount` with
    // `account: None` — it opens an actionable error popup.
    let config = AppConfig::default();
    let tmp = tempfile::tempdir().unwrap();
    let mut state = ManagerState::from_config(&config, tmp.path());
    let mut picker = AgentChoiceState::with_choices(vec![jackin_core::Agent::Claude]);
    picker.focused = jackin_core::Agent::Claude;
    state.inline_new_session_picker = Some(("jackin-demo-architect".into(), picker, Vec::new()));

    let outcome = handle_new_session_picker(&mut state, key(KeyCode::Enter));

    assert!(
        matches!(outcome, InputOutcome::Continue),
        "empty providers must not dispatch a session; got {outcome:?}"
    );
    assert!(
        state.inline_new_session_picker.is_some(),
        "the agent picker must remain open when commit fails"
    );
    assert_no_eligible_account_popup(&state, "jackin-demo-architect");
}

#[test]
fn new_session_commit_ignores_accounts_offered_to_other_agents() {
    // A non-empty provider list where nothing serves Claude is still zero
    // eligible: error, never an account-less dispatch.
    let config = AppConfig::default();
    let tmp = tempfile::tempdir().unwrap();
    let mut state = ManagerState::from_config(&config, tmp.path());
    let mut picker = AgentChoiceState::with_choices(vec![jackin_core::Agent::Claude]);
    picker.focused = jackin_core::Agent::Claude;
    let accounts = vec![crate::services::launch::AccountChoice {
        id: "o-codex".into(),
        name: "O".into(),
        provider: jackin_config::AiProvider::OpenAi,
        agents: vec![jackin_core::Agent::Codex],
        configuration_id: None,
        instance_id: Some("codex-o".into()),
    }];
    state.inline_new_session_picker = Some(("jackin-demo-architect".into(), picker, accounts));

    let outcome = handle_new_session_picker(&mut state, key(KeyCode::Enter));

    assert!(
        matches!(outcome, InputOutcome::Continue),
        "no candidate for Claude must not dispatch; got {outcome:?}"
    );
    assert!(
        state.inline_new_session_picker.is_some(),
        "the agent picker must remain open when no candidate matches"
    );
    assert_no_eligible_account_popup(&state, "jackin-demo-architect");
}

#[test]
fn new_session_picker_lists_candidates_in_id_order() {
    let config = AppConfig::default();
    let tmp = tempfile::tempdir().unwrap();
    let mut state = ManagerState::from_config(&config, tmp.path());
    let outcome = commit_new_session_picker(
        &mut state,
        jackin_core::Agent::Claude,
        vec![
            jackin_protocol::Provider::Zai,
            jackin_protocol::Provider::Anthropic,
        ],
    );

    assert!(matches!(outcome, InputOutcome::Continue));
    let Some(picker) = &state.inline_account_picker else {
        panic!("two candidates must open the account picker");
    };
    let ids: Vec<&str> = picker
        .providers()
        .iter()
        .map(|account| account.id.as_str())
        .collect();
    assert_eq!(ids, vec!["anthropic", "zai"]);
}

#[test]
fn new_session_open_uses_live_admissions_not_workspace_default() {
    let (mut state, mut config, paths, tmp) = running_session_state(|config| {
        config
            .workspaces
            .get_mut("demo")
            .unwrap()
            .account_bindings
            .insert(jackin_core::Agent::Claude, "z-claude".into());
    });
    let outcome = handle_key(
        &mut state,
        &mut config,
        &paths,
        tmp.path(),
        key(KeyCode::Char('n')),
    )
    .unwrap();
    assert!(matches!(outcome, InputOutcome::Continue));
    let Some((_, _, providers)) = state.inline_new_session_picker.as_ref() else {
        panic!("n on a running instance must open the agent picker");
    };
    let ids: Vec<&str> = providers
        .iter()
        .map(|account| account.id.as_str())
        .collect();
    assert_eq!(ids, vec!["a-claude", "o-codex", "z-claude"]);
    let claude_offered: Vec<&str> = providers
        .iter()
        .filter(|account| account.agents.contains(&jackin_core::Agent::Claude))
        .map(|account| account.id.as_str())
        .collect();
    assert_eq!(
        claude_offered,
        vec!["a-claude", "z-claude"],
        "host defaults must not prune a live manifest admission"
    );

    let Some((_, picker, _)) = state.inline_new_session_picker.as_mut() else {
        unreachable!("picker open checked above");
    };
    picker.focused = jackin_core::Agent::Claude;
    let outcome = handle_new_session_picker(&mut state, key(KeyCode::Enter));
    match outcome {
        InputOutcome::Continue => {}
        other => panic!("live duplicate admissions must open the picker; got {other:?}"),
    }
    assert!(
        state.inline_account_picker.is_some(),
        "host defaults must not collapse live admitted instances"
    );
    let ids: Vec<&str> = state
        .inline_account_picker
        .as_ref()
        .expect("live account picker")
        .providers()
        .iter()
        .map(|account| account.instance_id.as_deref().expect("live instance ID"))
        .collect();
    assert_eq!(ids, vec!["claude-a", "claude-z"]);
}

#[test]
fn new_session_open_without_default_opens_sorted_picker() {
    let (mut state, mut config, paths, tmp) = running_session_state(|_| {});
    let outcome = open_and_commit_new_session(
        &mut state,
        &mut config,
        &paths,
        tmp.path(),
        jackin_core::Agent::Claude,
    );

    assert!(matches!(outcome, InputOutcome::Continue));
    let Some(picker) = &state.inline_account_picker else {
        panic!("two candidates without a default must open the account picker");
    };
    let ids: Vec<&str> = picker
        .providers()
        .iter()
        .map(|account| account.id.as_str())
        .collect();
    assert_eq!(ids, vec!["a-claude", "z-claude"]);

    // Selecting the second same-agent row must retain its exact config ID,
    // not collapse back to the shared account/provider identity.
    assert!(matches!(
        handle_inline_account_picker(&mut state, key(KeyCode::Down)),
        InputOutcome::Continue
    ));
    match handle_inline_account_picker(&mut state, key(KeyCode::Enter)) {
        InputOutcome::NewSessionWithAccount {
            container,
            agent,
            instance_id,
        } => {
            assert_eq!(container, "jackin-demo-architect-running");
            assert_eq!(agent, jackin_core::Agent::Claude);
            assert_eq!(instance_id, "claude-z");
        }
        other => panic!("expected exact second live instance ID; got {other:?}"),
    }
}

#[test]
fn new_session_open_ignores_mutable_host_bindings() {
    // The role binding names an unknown id while the live manifest admits two
    // Claude instances: the host binding must not rewrite the live rows.
    let (mut state, mut config, paths, tmp) = running_session_state(|config| {
        config.workspaces.get_mut("demo").unwrap().roles.insert(
            "the-architect".into(),
            jackin_config::WorkspaceRoleOverride {
                account_bindings: std::collections::BTreeMap::from([(
                    jackin_core::Agent::Claude,
                    "ghost".into(),
                )]),
                ..Default::default()
            },
        );
    });
    let outcome = open_and_commit_new_session(
        &mut state,
        &mut config,
        &paths,
        tmp.path(),
        jackin_core::Agent::Claude,
    );

    assert!(
        matches!(outcome, InputOutcome::Continue),
        "invalid binding must not dispatch; got {outcome:?}"
    );
    assert!(matches!(outcome, InputOutcome::Continue));
    assert!(state.inline_account_picker.is_some());
    assert!(state.inline_new_session_picker.is_none());
}

#[test]
fn right_on_current_directory_parent_expands_even_with_live_snapshot() {
    let tmp = tempfile::tempdir().unwrap();
    let paths = JackinPaths::for_tests(tmp.path());
    paths.ensure_base_dirs().unwrap();
    let cwd = tmp.path();
    let workdir = cwd.display().to_string();
    let container = "jackin-current-dir-the-architect-live";

    let mut config = AppConfig::default();
    let mut state = ManagerState::from_config(&config, cwd);
    state.instances = vec![current_dir_instance_entry(
        container,
        InstanceStatus::Running,
        &workdir,
    )];
    state
        .instance_snapshots
        .insert(container.into(), live_snapshot());

    let outcome = handle_key(&mut state, &mut config, &paths, cwd, key(KeyCode::Right)).unwrap();

    assert!(matches!(outcome, InputOutcome::Continue));
    assert!(
        state.current_dir_expanded,
        "→ on the Current directory parent must expand the tree"
    );
    assert!(
        !state.preview_focused,
        "preview focus is only reachable from instance child rows"
    );
    assert!(matches!(
        state.row_at(1),
        Some(crate::tui::state::ManagerListRow::CurrentDirectoryInstance(
            0
        ))
    ));
}
