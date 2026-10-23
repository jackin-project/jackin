// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

fn usage_header_visible(
    state: &mut ManagerState<'_>,
    config: &AppConfig,
    cwd: &std::path::Path,
) -> bool {
    let buffer = render_manager_buffer(state, config, cwd, 120, 40);
    row_text(&buffer, 0).contains("usage")
}

fn assert_workspace_usage_not_painted(
    state: &mut ManagerState<'_>,
    config: &AppConfig,
    cwd: &std::path::Path,
) {
    assert!(
        !console_usage_surface_painted(state.stage.route(), state.usage.visible),
        "Usage eligibility follows the actual List route"
    );
    assert!(
        !usage_header_visible(state, config, cwd),
        "the workspace renderer must not paint stale Usage on this route"
    );
    assert_ne!(
        state.painted_input_owner(),
        crate::tui::model::ConsolePaintedInputOwner::Usage,
        "input ownership must use the same List-only Usage eligibility"
    );
}

#[test]
fn usage_paint_and_input_ownership_share_list_only_eligibility() {
    let config = AppConfig::default();
    let cwd = test_cwd();
    let mut state = ManagerState::from_config(&config, &cwd);
    state.usage.visible = true;
    state.usage.screen = Some(crate::tui::state::UsageScreenState::default());

    assert!(console_usage_surface_painted(
        state.stage.route(),
        state.usage.visible
    ));
    assert!(usage_header_visible(&mut state, &config, &cwd));
    assert_eq!(
        state.painted_input_owner(),
        crate::tui::model::ConsolePaintedInputOwner::Usage
    );

    state.stage = ManagerStage::CreatePrelude(crate::tui::state::CreatePreludeState::default());
    assert_workspace_usage_not_painted(&mut state, &config, &cwd);
    assert!(matches!(
        state.painted_input_owner(),
        crate::tui::model::ConsolePaintedInputOwner::Dispatch(
            crate::tui::model::ConsoleInputDispatchPlan::Stage(
                ConsoleManagerStageRoute::CreatePrelude
            )
        )
    ));

    let prelude = crate::tui::state::CreatePreludeState {
        modal: Some(Modal::ErrorPopup {
            state: crate::tui::components::ErrorPopupState::new("Error", "Modal is visible"),
        }),
        ..Default::default()
    };
    state.stage = ManagerStage::CreatePrelude(prelude);
    assert_workspace_usage_not_painted(&mut state, &config, &cwd);
    assert!(matches!(
        state.painted_input_owner(),
        crate::tui::model::ConsolePaintedInputOwner::Dispatch(
            crate::tui::model::ConsoleInputDispatchPlan::CreatePreludeModal
        )
    ));

    state.stage = ManagerStage::ConfirmDelete {
        name: "workspace".to_owned(),
        state: crate::tui::components::ConfirmState::new("Delete workspace?"),
    };
    assert_workspace_usage_not_painted(&mut state, &config, &cwd);
    state.stage = ManagerStage::ConfirmInstancePurge {
        container: "container".to_owned(),
        label: "label".to_owned(),
        state: crate::tui::components::ConfirmState::new("Purge instance?"),
    };
    assert_workspace_usage_not_painted(&mut state, &config, &cwd);

    state.stage = ManagerStage::Editor(EditorState::new_edit(
        "workspace".to_owned(),
        WorkspaceConfig::default(),
    ));
    assert_workspace_usage_not_painted(&mut state, &config, &cwd);
    state.stage = ManagerStage::Settings(SettingsState::from_config(&config));
    assert_workspace_usage_not_painted(&mut state, &config, &cwd);
    state.stage =
        ManagerStage::Workflows(crate::tui::screens::workflows::model::WorkflowScreen::default());
    assert_workspace_usage_not_painted(&mut state, &config, &cwd);
}
