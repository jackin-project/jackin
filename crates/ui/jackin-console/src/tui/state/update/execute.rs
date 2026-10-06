// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! Op-commit validation and URL opening.

use super::{ManagerMessage, update_manager};

use super::super::{ManagerStage, ManagerState, Modal};

/// Wire up an OP reference commit-validation subscription into manager state.
pub fn execute_op_commit_validation(
    state: &mut ManagerState<'_>,
    op_ref: jackin_core::OpRef,
    is_settings: bool,
) {
    let rx = crate::tui::op_picker::start_ref_validation(op_ref.clone());
    if is_settings {
        if let ManagerStage::Settings(settings) = &mut state.stage {
            settings
                .auth
                .set_pending_op_commit(super::super::PendingOpCommit::new(op_ref, rx));
        }
    } else if let ManagerStage::Editor(editor) = &mut state.stage {
        editor.pending_op_commit = Some(super::super::PendingOpCommit::new(op_ref, rx));
    }
}

/// Open a URL in the system browser; on failure route error popup to the active stage.
pub fn execute_open_url(state: &mut ManagerState<'_>, url: &str) -> bool {
    match crate::services::browser::open_url(url) {
        Ok(()) => false,
        Err(error) => {
            report_open_url_error(state, error);
            true
        }
    }
}

/// Apply a URL-open failure to manager state (opens an error popup).
pub fn report_open_url_error(state: &mut ManagerState<'_>, error: anyhow::Error) {
    use crate::tui::components::error_popup;
    match &mut state.stage {
        ManagerStage::Editor(editor) => {
            editor.modal = Some(Modal::ErrorPopup {
                state: error_popup::failed_to_open_url_error_popup_state(error),
            });
        }
        ManagerStage::Settings(_) => {
            update_manager(
                state,
                ManagerMessage::OpenSettingsErrorPopup {
                    title: error_popup::failed_to_open_url_error_title().into(),
                    message: error.to_string(),
                },
            );
        }
        _ => {
            update_manager(
                state,
                ManagerMessage::OpenListErrorPopup {
                    title: error_popup::failed_to_open_url_error_title().into(),
                    message: error.to_string(),
                },
            );
        }
    }
}
