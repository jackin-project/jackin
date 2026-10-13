// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Workspace account assignment and shared credential picker round trips.

use crossterm::event::{KeyCode, KeyEvent};
use std::path::PathBuf;

use crate::tui::components::auth_panel::{
    AuthFormKeyPlan, auth_credential_input_state, auth_form_key_plan_with_source_folder,
    auth_source_picker_state,
};
use crate::tui::op_picker::OpPickerState;
use crate::tui::state::{
    AuthForm, AuthFormFocus, EditorState, FileBrowserTarget, Modal, TextInputTarget,
};
use jackin_config::AppConfig;
use jackin_env::OpCache;

/// Open the auth-edit form modal for the row currently under the
/// cursor on the Auth tab. Pre-populates the form from the row's
/// effective mode + credential so editing an existing entry shows
/// what's there.
pub fn open_auth_form_modal(editor: &mut EditorState<'_>, config: &AppConfig) {
    let Some((target, form)) = editor.focused_auth_form(config) else {
        return;
    };
    let literal_buffer = form.literal_buffer();
    editor.modal = Some(Modal::AuthForm {
        target,
        state: Box::new(form),
        focus: AuthFormFocus::Mode,
        literal_buffer,
    });
}

/// Handle `D`/`d` on the Auth tab.
///
/// - `RoleHeader` → clear the selected auth kind's role override.
/// - `RoleMode` → silently clear the selected auth kind's role-level override.
/// - `WorkspaceMode` → clear the workspace-level override for the selected auth kind.
/// - Anything else (`AuthKindRow`, `AddSentinel`, `Spacer`) → no-op.
pub fn handle_d_on_auth_row(editor: &mut EditorState<'_>, config: &AppConfig) {
    editor.clear_auth_row_at_cursor(config);
}

pub fn open_auth_source_folder_browser_from_form_with_state(
    editor: &mut EditorState<'_>,
    state: crate::tui::components::file_browser::FileBrowserState,
) -> bool {
    match crate::tui::auth_config::ModalAuthSourceFolderBrowserOpen::open_auth_source_folder_browser(
        &mut editor.modal,
        &mut editor.modal_parents,
        AuthFormFocus::SourceFolder,
        FileBrowserTarget::AuthFormSourceFolder,
        || Ok::<_, std::convert::Infallible>(state),
    ) {
        crate::tui::auth_config::AuthSourceFolderBrowserOpenResult::Opened => true,
        crate::tui::auth_config::AuthSourceFolderBrowserOpenResult::NotAvailable => false,
        crate::tui::auth_config::AuthSourceFolderBrowserOpenResult::BrowserError(error) => {
            match error {}
        }
    }
}

fn record_missing_return_path() {
    let _recorded = jackin_telemetry::record_error(
        jackin_telemetry::schema::enums::ErrorType::TelemetryInstrumentationFault,
    );
}

/// Commit branch for `Modal::AuthSourcePicker` when the operator picks
/// the plain-text source. Re-stashes the auth form's context with the
/// focus pinned to `CredentialSource`, then mounts a `Modal::TextInput`
/// pre-filled from the round-trip's literal buffer.
pub fn apply_plain_source_picker_to_auth_form(editor: &mut EditorState<'_>) {
    if !crate::tui::auth_config::ModalAuthPlainSourceOpen::open_auth_plain_source_text_input(
        &mut editor.modal,
        &mut editor.modal_parents,
        AuthFormFocus::CredentialSource,
        TextInputTarget::AuthCredential,
        auth_credential_input_state,
    ) {
        record_missing_return_path();
    }
}

/// Commit branch for the credential `Modal::TextInput`. Lifts the
/// stashed auth form back, applies the typed value via `set_literal`,
/// and re-mounts the form with focus on Save. Also the post-mint
/// re-mount target for the plain-text generate path in the
/// `run_console` loop, hence the wider visibility.
pub fn apply_plain_text_to_auth_form(editor: &mut EditorState<'_>, value: &str) {
    if !crate::tui::auth_config::ModalAuthFormCredentialApply::apply_auth_plain_text(
        &mut editor.modal,
        &mut editor.modal_parents,
        AuthFormFocus::Save,
        value,
    ) {
        record_missing_return_path();
    }
}

pub fn apply_source_folder_to_auth_form(editor: &mut EditorState<'_>, value: PathBuf) {
    if !crate::tui::auth_config::ModalAuthFormCredentialApply::apply_auth_source_folder(
        &mut editor.modal,
        &mut editor.modal_parents,
        AuthFormFocus::Save,
        value,
    ) {
        record_missing_return_path();
    }
}

/// Commit branch for `Modal::AuthSourcePicker` when the operator picks
/// the 1Password source. Pins the stashed return-path focus to
/// `CredentialSource` (so cancel/error paths land back on the source
/// row) and mounts a fresh `Modal::OpPicker`.
pub fn open_op_picker_from_auth_source(
    editor: &mut EditorState<'_>,
    op_cache: std::rc::Rc<std::cell::RefCell<OpCache>>,
) {
    if !crate::tui::auth_config::ModalAuthOpPickerOpen::open_auth_op_picker(
        &mut editor.modal,
        &mut editor.modal_parents,
        AuthFormFocus::CredentialSource,
        || OpPickerState::new_with_cache(op_cache),
    ) {
        record_missing_return_path();
    }
}

/// Re-mount the auth-form modal with a freshly-picked `OpRef` applied
/// against the production `OpCli` runner. Called from the `OpPicker`'s
/// commit handler in `editor.rs` when the auth form is the modal parent
/// set (i.e. the picker was opened from the auth form, not from the
/// Secrets tab).
///
/// On vault read error, the form stays on the modal parent stack and
/// `Modal::ErrorPopup` is mounted;
/// dismissing the popup invokes `restore_auth_form_after_op_picker_cancel`
/// so the operator lands back on the form with the prior credential
/// unchanged. Root input validates with `op read` before mutating the
/// form, so a broken reference never lands in `editor.pending`.
/// Apply a committed op picker selection after the 1Password read has already
/// succeeded on the `spawn_blocking` thread. Called from the `run_console`
/// poll loop — the read was verified asynchronously so Touch ID / the 1Password
/// desktop dialog did not freeze the TUI reactor.
///
/// The auth form is on `editor.modal_parents` (it was stashed when the
/// `OpPicker` opened) — pop it, set the `OpRef` without re-reading, and
/// re-mount with focus on Save.
pub fn apply_op_picker_to_auth_form_committed(
    editor: &mut EditorState<'_>,
    op_ref: jackin_core::OpRef,
) {
    if !crate::tui::auth_config::ModalAuthFormOpRefApply::apply_auth_op_ref(
        &mut editor.modal,
        &mut editor.modal_parents,
        AuthFormFocus::Save,
        op_ref,
    ) {
        record_missing_return_path();
    }
}

/// Restore the auth-form modal unchanged after the operator cancels
/// the `OpPicker` or the literal `TextInput`. Both side modals share
/// the same recovery shape, so the same helper handles both.
pub fn restore_auth_form_after_op_picker_cancel(editor: &mut EditorState<'_>) {
    if !crate::tui::auth_config::ModalAuthFormCredentialApply::restore_auth_form_modal(
        &mut editor.modal,
        &mut editor.modal_parents,
    ) {
        record_missing_return_path();
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AuthFormKeyOutcome {
    Continue,
    Changed,
    OpenSourceFolderBrowser,
}

impl AuthFormKeyOutcome {
    #[cfg(test)]
    pub const fn is_dirty(self) -> bool {
        !matches!(self, Self::Continue)
    }
}

pub fn handle_auth_form_key(
    editor: &mut EditorState<'_>,
    key: KeyEvent,
    op_available: bool,
) -> AuthFormKeyOutcome {
    let Some(current_focus) = editor.active_auth_form_focus() else {
        return AuthFormKeyOutcome::Continue;
    };

    // Esc cancels at every focus. Drain the auth-form return stash too so
    // a stale OpPicker round-trip can't be re-applied to a future modal —
    // every other exit path (Save / Cancel / Reset commit, OpPicker
    // commit/cancel) drains it explicitly; Esc must too.
    if key.code == KeyCode::Esc {
        editor.clear_modal_chain();
        return AuthFormKeyOutcome::Changed;
    }

    let Some(Modal::AuthForm { state, .. }) = editor.modal.as_ref() else {
        return AuthFormKeyOutcome::Continue;
    };
    let plan = auth_form_key_plan_with_source_folder(
        current_focus,
        key.code,
        state.shows_source_folder(),
        state.shows_credential_block(),
        state.can_save(),
    );

    match plan {
        AuthFormKeyPlan::Stay => AuthFormKeyOutcome::Continue,
        AuthFormKeyPlan::Focus(next) => {
            if let Some(Modal::AuthForm { focus, .. }) = editor.modal.as_mut() {
                *focus = next;
            }
            AuthFormKeyOutcome::Changed
        }
        AuthFormKeyPlan::CycleMode => {
            if let Some(Modal::AuthForm { state, focus, .. }) = editor.modal.as_mut() {
                state.cycle_mode();
                if *focus == AuthFormFocus::SourceFolder && !state.shows_source_folder() {
                    *focus = AuthFormFocus::Mode;
                }
            }
            AuthFormKeyOutcome::Changed
        }
        AuthFormKeyPlan::OpenSourceFolderBrowser => AuthFormKeyOutcome::OpenSourceFolderBrowser,
        AuthFormKeyPlan::OpenCredentialSource => {
            if open_auth_source_picker_from_form(editor, op_available) {
                AuthFormKeyOutcome::Changed
            } else {
                AuthFormKeyOutcome::Continue
            }
        }
        AuthFormKeyPlan::Save => {
            if commit_auth_form_save(editor) {
                AuthFormKeyOutcome::Changed
            } else {
                AuthFormKeyOutcome::Continue
            }
        }
        AuthFormKeyPlan::Cancel => {
            editor.clear_modal_chain();
            AuthFormKeyOutcome::Changed
        }
        AuthFormKeyPlan::Reset => {
            if reset_auth_form_layer(editor) {
                AuthFormKeyOutcome::Changed
            } else {
                AuthFormKeyOutcome::Continue
            }
        }
    }
}

fn open_auth_source_picker_from_form(editor: &mut EditorState<'_>, op_available: bool) -> bool {
    crate::tui::auth_config::ModalAuthSourcePickerOpen::open_auth_source_picker(
        &mut editor.modal,
        &mut editor.modal_parents,
        |env_var| auth_source_picker_state(env_var, op_available),
    )
}

fn commit_auth_form_save(editor: &mut EditorState<'_>) -> bool {
    let Some(Modal::AuthForm { target, state, .. }) = editor.modal.as_mut() else {
        return false;
    };
    let committed_target = target.clone();
    let kind = state.kind;
    let form = std::mem::replace(state.as_mut(), AuthForm::new(kind));
    editor.clear_modal_chain();
    editor.persist_auth_form(&committed_target, &form);
    true
}

fn reset_auth_form_layer(editor: &mut EditorState<'_>) -> bool {
    let Some(Modal::AuthForm { target, .. }) = editor.modal.as_mut() else {
        return false;
    };
    let committed_target = target.clone();
    editor.clear_modal_chain();
    editor.clear_auth_form_layer(&committed_target);
    true
}
