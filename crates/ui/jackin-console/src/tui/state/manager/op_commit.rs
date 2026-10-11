// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! `ManagerState` op-picker commit application.

use super::super::{ManagerStage, ManagerState};

impl ManagerState<'_> {
    pub fn apply_op_picker_op_ref_committed_for_editor(&mut self, op_ref: jackin_core::OpRef) {
        let ManagerStage::Editor(editor) = &mut self.stage else {
            return;
        };
        if !crate::tui::auth_config::ModalAuthFormOpRefApply::apply_auth_op_ref(
            &mut editor.modal,
            &mut editor.modal_parents,
            crate::tui::screens::settings::model::AuthFormFocus::Save,
            op_ref,
        ) {
            super::super::record_console_error(
                jackin_telemetry::schema::enums::ErrorType::TelemetryInstrumentationFault,
            );
        }
    }

    pub fn apply_op_picker_commit_failed_for_editor(&mut self, error: &anyhow::Error) {
        let ManagerStage::Editor(editor) = &mut self.stage else {
            return;
        };
        super::super::record_console_error(jackin_telemetry::schema::enums::ErrorType::IoError);
        editor.open_error_popup(
            crate::tui::components::error_popup::op_read_failed_error_popup_state(error),
        );
    }

    pub fn apply_op_picker_op_ref_committed_for_settings(&mut self, op_ref: jackin_core::OpRef) {
        let ManagerStage::Settings(settings) = &mut self.stage else {
            return;
        };
        let Some(super::super::SettingsModal::AuthForm {
            target,
            mut state,
            literal_buffer,
            ..
        }) = settings.auth.pop_parent_modal()
        else {
            super::super::record_console_error(
                jackin_telemetry::schema::enums::ErrorType::TelemetryInstrumentationFault,
            );
            return;
        };
        state.set_op_ref(op_ref);
        settings
            .auth
            .set_modal(super::super::SettingsModal::AuthForm {
                target,
                state,
                focus: crate::tui::screens::settings::model::AuthFormFocus::Save,
                literal_buffer,
            });
    }

    pub fn apply_op_picker_commit_failed_for_settings(&mut self, error: &anyhow::Error) {
        let ManagerStage::Settings(settings) = &mut self.stage else {
            return;
        };
        super::super::record_console_error(jackin_telemetry::schema::enums::ErrorType::IoError);
        settings.auth.set_error(
            crate::tui::screens::settings::view::settings_auth_op_read_failed_message(error),
        );
    }
}
