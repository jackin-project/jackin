// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

#[test]
fn list_o_on_row_zero_is_silent_noop() {
    // Row 0 is "Current directory" — O must be a silent no-op.
    let tmp = tempfile::tempdir().unwrap();
    let paths = JackinPaths::for_tests(tmp.path());
    paths.ensure_base_dirs().unwrap();
    let mut config = AppConfig::default();
    config
        .workspaces
        .insert("demo".into(), WorkspaceConfig::default());
    let mut state = ManagerState::from_config(&config, tmp.path());
    state.selected = 0;

    handle_key(
        &mut state,
        &mut config,
        &paths,
        tmp.path(),
        key(KeyCode::Char('o')),
    )
    .unwrap();

    assert!(
        state.list_modal.is_none(),
        "O on row 0 must not open a modal"
    );
}

#[test]
fn picker_commit_closes_list_modal_and_clears_state() {
    // Seed the state directly with an open GithubPicker, then commit.
    // The input layer must not open the browser. It closes the modal and
    // returns a typed URL-open outcome for the run loop.
    use crate::{github_mounts::GithubChoice, tui::components::github_picker::GithubPickerState};
    let tmp = tempfile::tempdir().unwrap();
    let paths = JackinPaths::for_tests(tmp.path());
    paths.ensure_base_dirs().unwrap();
    let mut config = AppConfig::default();
    let mut state = ManagerState::from_config(&config, tmp.path());
    state.list_modal = Some(Modal::GithubPicker {
        state: GithubPickerState::new(vec![GithubChoice {
            src: "/tmp/a".into(),
            branch: "main".into(),
            url: "file:///dev/null".into(),
        }]),
    });

    let outcome = handle_key(
        &mut state,
        &mut config,
        &paths,
        tmp.path(),
        key(KeyCode::Enter),
    )
    .unwrap();

    // Browser side effects are not executed in the input handler. The
    // modal closes and queues a URL-open effect for the run loop.
    assert!(
        !matches!(state.list_modal, Some(Modal::GithubPicker { .. })),
        "GithubPicker must be gone after Enter"
    );
    assert!(matches!(outcome, InputOutcome::Continue));
    let effects = state.drain_effects();
    match effects.as_slice() {
        [ManagerEffect::OpenUrl(url)] => assert_eq!(url, "file:///dev/null"),
        other => panic!("expected OpenUrl effect, got {other:?}"),
    }
}

#[test]
fn container_info_enter_copies_default_value_without_dismissing() {
    let tmp = tempfile::tempdir().unwrap();
    let paths = JackinPaths::for_tests(tmp.path());
    paths.ensure_base_dirs().unwrap();
    let mut config = AppConfig::default();
    let mut state = ManagerState::from_config(&config, tmp.path());
    state.list_modal = Some(Modal::ContainerInfo {
        state: crate::tui::components::container_info_surface::ContainerInfoState::new(
            "Debug info",
            vec![
                crate::tui::components::container_info_surface::ContainerInfoRow::new(
                    "jackin version",
                    "0.6.0-dev",
                ),
                crate::tui::components::container_info_surface::ContainerInfoRow::new(
                    "Run ID",
                    "jk-run-123",
                )
                .copyable()
                .emphasised(),
            ],
        ),
    });

    let outcome = handle_key(
        &mut state,
        &mut config,
        &paths,
        tmp.path(),
        key(KeyCode::Enter),
    )
    .unwrap();

    assert!(matches!(outcome, InputOutcome::Continue));
    assert!(
        matches!(state.list_modal, Some(Modal::ContainerInfo { .. })),
        "Enter copies but keeps Debug info open for copied feedback"
    );
    match state.drain_effects().as_slice() {
        [ManagerEffect::CopyContainerInfoValue { row, payload }] => {
            assert_eq!(*row, 1);
            assert_eq!(payload, "jk-run-123");
        }
        other => panic!("expected CopyContainerInfoValue effect, got {other:?}"),
    }
}

#[test]
fn picker_esc_closes_without_opening_url() {
    use crate::{github_mounts::GithubChoice, tui::components::github_picker::GithubPickerState};
    let tmp = tempfile::tempdir().unwrap();
    let paths = JackinPaths::for_tests(tmp.path());
    paths.ensure_base_dirs().unwrap();
    let mut config = AppConfig::default();
    let mut state = ManagerState::from_config(&config, tmp.path());
    state.list_modal = Some(Modal::GithubPicker {
        state: GithubPickerState::new(vec![GithubChoice {
            src: "/tmp/a".into(),
            branch: "main".into(),
            url: "https://github.com/owner/repo/tree/main".into(),
        }]),
    });

    handle_key(
        &mut state,
        &mut config,
        &paths,
        tmp.path(),
        key(KeyCode::Esc),
    )
    .unwrap();

    assert!(state.list_modal.is_none());
}

#[test]
fn configured_launch_picker_dispatches_exact_configuration_only() {
    let config = AppConfig::default();
    let tmp = tempfile::tempdir().unwrap();
    let mut state = ManagerState::from_config(&config, tmp.path());
    let selector = jackin_core::RoleSelector::new(None, "architect");
    let choices = ["claude-fast", "claude-deep"]
        .into_iter()
        .map(|id| crate::services::launch::AccountChoice {
            id: "shared-account".into(),
            name: "Shared account".into(),
            provider: jackin_config::AiProvider::Anthropic,
            agents: vec![jackin_core::Agent::Claude],
            configuration_id: Some(id.into()),
            instance_id: None,
        })
        .collect();
    let mut picker = crate::tui::components::account_picker::AccountPickerState::new(
        selector.clone(),
        jackin_core::Agent::Claude,
        choices,
    );
    picker.move_down();
    state.launch_account_picker = Some(picker);

    match handle_launch_account_picker(&mut state, key(KeyCode::Enter)) {
        InputOutcome::LaunchWithAccount {
            selector: selected,
            agent,
            selection,
        } => {
            assert_eq!(selected, selector);
            assert_eq!(agent, jackin_core::Agent::Claude);
            assert_eq!(
                selection,
                jackin_core::LaunchSelection::Configuration("claude-deep".into())
            );
        }
        other => panic!("expected exact configured launch, got {other:?}"),
    }
    assert!(state.launch_account_picker.is_none());
}
