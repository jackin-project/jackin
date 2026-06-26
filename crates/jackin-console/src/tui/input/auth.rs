//! Auth-tab input handling: open the form modal, route keystrokes to
//! the form, and persist commits back to `editor.pending`.
//!
//! Mirrors the Secrets tab's pattern of "form mutates `editor.pending`
//! in memory; the editor's existing save flow (`edit_workspace`)
//! serializes the whole `WorkspaceConfig` block back to disk on save".
//!
//! Kind-keyed: Claude / Codex / Github route through the same shared
//! [`AuthForm`] widget, with the kind dispatching the persistence path
//! (Claude/Codex write `[workspaces.<ws>(.roles.<role>)?].<agent>`;
//! Github writes `[workspaces.<ws>(.roles.<role>)?].github`).

use crossterm::event::{KeyCode, KeyEvent};
use std::path::PathBuf;

#[cfg(test)]
use crate::tui::auth::AuthMode;
use crate::tui::components::auth_panel::{
    AuthFormKeyPlan, auth_credential_input_state, auth_form_key_plan_with_source_folder,
    auth_source_picker_state, generated_token_source_picker_state,
};
use crate::tui::op_picker::OpPickerState;
use crate::tui::state::RolePickerState;
use crate::tui::state::{
    AuthForm, AuthFormFocus, EditorState, FileBrowserTarget, Modal, TextInputTarget,
};
use jackin_config::AppConfig;
#[cfg(test)]
use jackin_core::EnvValue;
use jackin_env::OpCache;

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

/// Open the auth-edit form modal for the row currently under the
/// cursor on the Auth tab. Pre-populates the form from the row's
/// effective mode + credential so editing an existing entry shows
/// what's there.
pub fn open_auth_form_modal(editor: &mut EditorState<'_>, config: &AppConfig) {
    let Some((target, form)) = editor.focused_auth_form(config) else {
        jackin_diagnostics::debug_log!(
            "auth_form",
            "open_auth_form_modal: no target for cursor row (cursor may be on Spacer / AddSentinel / out-of-range)"
        );
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

/// Mount the Auth-tab role picker for the "+ Add per-role override"
/// flow. Filters `eligible_agents_for_override` down to roles that
/// don't yet carry an override for the focused kind — re-mounting the
/// picker for an already-overridden role would just duplicate the
/// existing rows. Silent no-op when no candidates remain (the row is
/// rendered dimmed in that state).
pub fn open_auth_role_picker(editor: &mut EditorState<'_>, config: &AppConfig) {
    let Some(candidates) = editor.auth_role_override_selectors(config.roles.keys()) else {
        jackin_diagnostics::debug_log!(
            "auth_role_picker",
            "open_auth_role_picker: no auth kind selected (root view or stale state)"
        );
        return;
    };
    if candidates.is_empty() {
        return;
    }
    let state = RolePickerState::new(candidates);
    editor.modal = Some(Modal::AuthRolePicker { state });
}

/// Toggle the expanded/collapsed state of a role section on the Auth tab.
/// If the role is currently expanded, collapse it; otherwise expand it.
pub fn toggle_role_expand(editor: &mut EditorState<'_>, role: String) {
    editor.toggle_auth_role_expanded(role);
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

/// Drive a single keystroke into an open `Modal::AuthForm`. Returns
/// `true` when the modal was closed (committed, cancelled, or
/// transitioned away from the form via `Modal::AuthSourcePicker`).
///
/// `op_available` gates the 1Password choice rendered inside
/// `Modal::AuthSourcePicker` — passed through from `EditorState` so
/// the form doesn't reprobe the `op` binary on every keypress.
///
/// The `g`/`G` generate trigger opens the shared source picker (which
/// needs only `op_available` to dim the disabled 1Password choice); the
/// Create-mode `OpPicker` for the 1Password generate branch is mounted
/// later from the source-picker commit handler in `editor.rs`, which
/// owns `op_cache`.
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

    // `g`/`G` at any focus mints a Claude OAuth token. It opens the
    // shared source picker (plain literal vs. 1Password) as the first
    // step. Gated to the workspace-level Claude oauth_token slot in Edit
    // mode; a no-op everywhere else.
    if matches!(key.code, KeyCode::Char('g' | 'G'))
        && try_start_token_generate(editor, op_available)
    {
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

/// Mint-path trigger: when the gate holds, stash the open form (so the
/// post-mint re-mount lands the operator back on the same Edit-auth
/// dialog with the minted credential staged, focus Save — exactly like
/// the provide path) and mount the shared source picker so the operator
/// first chooses where the freshly minted token is stored (plain
/// literal vs. 1Password). The source picker's commit (in `editor.rs`)
/// routes to GENERATE because `generating_token_target` is set. Returns
/// `false` (a no-op) when the gate fails.
fn try_start_token_generate(editor: &mut EditorState<'_>, op_available: bool) -> bool {
    editor.start_auth_token_generate(generated_token_source_picker_state(op_available))
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

/// Detach the open `Modal::AuthForm` into `pending_auth_form_return`
/// and mount a `Modal::AuthSourcePicker` for the form's required env
/// var. Returns `true` when the swap happened (the picker took over).
///
/// Three early-returns put the form back unchanged when the swap
/// can't proceed: no form open, mode not yet picked, or selected
/// mode requires no credential. The form is restored verbatim so
/// the operator's keypress on the credential row is a quiet no-op
/// rather than a state desync.
fn open_auth_source_picker_from_form(editor: &mut EditorState<'_>, op_available: bool) -> bool {
    crate::tui::auth_config::ModalAuthSourcePickerOpen::open_auth_source_picker(
        &mut editor.modal,
        &mut editor.modal_parents,
        |env_var| auth_source_picker_state(env_var, op_available),
    )
}

/// Stash-miss debug-log codes. Emitted when a side modal commits or
/// cancels and the auth form's `pending_auth_form_return` slot is
/// unexpectedly empty — the side handler fired without a paired
/// open. Codes are stable so grepping `--debug` output stays cheap.
const AUTH_MISSING_PLAIN_SOURCE: &str = "AUTH001";
const AUTH_MISSING_PLAIN_TEXT: &str = "AUTH002";
const AUTH_MISSING_OP_SOURCE: &str = "AUTH003";
const AUTH_MISSING_OP_CANCEL: &str = "AUTH004";
const AUTH_MISSING_OP_COMMIT: &str = "AUTH005";
const AUTH_MISSING_FOLDER_COMMIT: &str = "AUTH006";

fn log_missing_return_path(code: &'static str, fn_name: &'static str, suffix: &str) {
    jackin_diagnostics::debug_log!(
        "auth",
        "{} {}: pending_auth_form_return missing{}",
        code,
        fn_name,
        suffix
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
        log_missing_return_path(
            AUTH_MISSING_PLAIN_SOURCE,
            "apply_plain_source_picker_to_auth_form",
            "",
        );
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
        log_missing_return_path(
            AUTH_MISSING_PLAIN_TEXT,
            "apply_plain_text_to_auth_form",
            " — typed credential dropped",
        );
    }
}

pub fn apply_source_folder_to_auth_form(editor: &mut EditorState<'_>, value: PathBuf) {
    if !crate::tui::auth_config::ModalAuthFormCredentialApply::apply_auth_source_folder(
        &mut editor.modal,
        &mut editor.modal_parents,
        AuthFormFocus::Save,
        value,
    ) {
        log_missing_return_path(
            AUTH_MISSING_FOLDER_COMMIT,
            "apply_source_folder_to_auth_form",
            " — selected folder dropped",
        );
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
        log_missing_return_path(
            AUTH_MISSING_OP_SOURCE,
            "open_op_picker_from_auth_source",
            " — closing modal",
        );
    }
}

/// Re-mount the auth-form modal with a freshly-picked `OpRef` applied
/// against the production `OpCli` runner. Called from the `OpPicker`'s
/// commit handler in `editor.rs` when `pending_auth_form_return` was
/// set (i.e. the picker was opened from the auth form, not from the
/// Secrets tab).
///
/// On vault read error, the form is re-stashed into
/// `pending_auth_form_return` and `Modal::ErrorPopup` is mounted;
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
        log_missing_return_path(
            AUTH_MISSING_OP_COMMIT,
            "apply_op_picker_to_auth_form_committed",
            " — async OpRef commit dropped",
        );
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
        log_missing_return_path(
            AUTH_MISSING_OP_CANCEL,
            "restore_auth_form_after_op_picker_cancel",
            "",
        );
    }
}

/// Inner helper split out so tests can inject a fake `OpRunner`
/// without touching the real `op` binary.
#[cfg(test)]
fn apply_op_picker_to_auth_form_with_runner<R: jackin_env::OpRunner + ?Sized>(
    editor: &mut EditorState<'_>,
    op_ref: jackin_core::OpRef,
    runner: &R,
) {
    apply_op_picker_to_auth_form_with_validator(editor, op_ref, |op_ref| {
        runner.read(&op_ref.op).map(|_| ())
    });
}

#[cfg(test)]
fn apply_op_picker_to_auth_form_with_validator(
    editor: &mut EditorState<'_>,
    op_ref: jackin_core::OpRef,
    validate: impl FnOnce(&jackin_core::OpRef) -> anyhow::Result<()>,
) {
    if !editor.has_auth_form_parent() {
        log_missing_return_path(
            AUTH_MISSING_OP_COMMIT,
            "apply_op_picker_to_auth_form",
            " — OpRef commit dropped",
        );
        return;
    }
    let read_result = validate(&op_ref);
    if let Err(e) = read_result {
        editor.open_error_popup(
            crate::tui::components::error_popup::op_read_failed_error_popup_state(e),
        );
        return;
    }
    apply_op_picker_to_auth_form_committed(editor, op_ref);
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

#[cfg(test)]
mod tests;
