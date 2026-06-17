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

use crate::config::AppConfig;
use crate::console::domain::{
    apply_role_auth_commit, apply_workspace_auth_commit, auth_kind_agent, clear_role_auth_layer,
    clear_workspace_auth_layer, role_auth_mode_and_credential, role_override_present,
    set_role_sync_source_dir, set_workspace_sync_source_dir, workspace_auth_mode_and_credential,
};
use crate::console::tui::components::auth_panel::{AuthForm, editor_source_folder_display};
use crate::console::tui::op_picker::OpPickerState;
use crate::console::tui::state::{
    AuthFormFocus, AuthFormTarget, AuthRow, EditorState, FieldFocus, FileBrowserTarget, Modal,
    TextInputTarget, auth_flat_rows, eligible_agents_for_override, resolve_auth_row_target,
    synthesize_appconfig_for_auth, workspace_name_for_panel,
};
use crate::operator_env::EnvValue;
use crate::operator_env::OpCache;
use crate::selector::RolePickerState;
use jackin_console::tui::auth::{AuthKind, AuthMode, can_generate_claude_oauth_token};
use jackin_console::tui::components::auth_panel::{
    AuthFormKeyPlan, auth_credential_input_state, auth_form_key_plan_with_source_folder,
    auth_source_picker_state, generated_token_source_picker_state,
};

/// Open the auth-edit form modal for the row currently under the
/// cursor on the Auth tab. Pre-populates the form from the row's
/// effective mode + credential so editing an existing entry shows
/// what's there.
pub(super) fn open_auth_form_modal(editor: &mut EditorState<'_>, config: &AppConfig) {
    let FieldFocus::Row(n) = editor.active_field;
    let Some(target) = resolve_auth_row_target(editor, config, n) else {
        crate::debug_log!(
            "auth_form",
            "open_auth_form_modal: no target for row {n} (cursor may be on Spacer / AddSentinel / out-of-range)"
        );
        return;
    };
    let kind = *target.kind();
    let (existing_mode, existing_cred) = current_mode_and_credential(editor, &target);
    let form = existing_mode
        .map_or_else(
            || AuthForm::new(kind),
            |mode| AuthForm::from_existing(kind, mode, existing_cred),
        )
        .with_source_folder(
            current_source_folder(editor, &target),
            current_source_folder_fallback(editor, config, &target),
        );
    let literal_buffer = form.literal_buffer();
    editor.modal = Some(Modal::AuthForm {
        target,
        state: Box::new(form),
        focus: AuthFormFocus::Mode,
        literal_buffer,
    });
}

fn current_source_folder(editor: &EditorState<'_>, target: &AuthFormTarget) -> Option<PathBuf> {
    let agent = auth_kind_agent(*target.kind())?;
    match target {
        AuthFormTarget::Workspace { .. } => editor.pending.sync_source_dir_for(agent),
        AuthFormTarget::WorkspaceRole { role, .. } => editor
            .pending
            .roles
            .get(role)
            .and_then(|role| role.sync_source_dir_for(agent)),
    }
}

fn current_source_folder_fallback(
    editor: &EditorState<'_>,
    config: &AppConfig,
    target: &AuthFormTarget,
) -> Option<jackin_console::tui::components::editor_rows::AuthSourceFolderDisplay> {
    auth_kind_agent(*target.kind())?;
    let synthesized = synthesize_appconfig_for_auth(editor, config);
    let workspace_name = workspace_name_for_panel(editor);
    let role = match target {
        AuthFormTarget::Workspace { .. } => "",
        AuthFormTarget::WorkspaceRole { role, .. } => role.as_str(),
    };
    Some(editor_source_folder_display(
        &synthesized,
        &workspace_name,
        role,
        *target.kind(),
    ))
}

/// Mount the Auth-tab role picker for the "+ Add per-role override"
/// flow. Filters `eligible_agents_for_override` down to roles that
/// don't yet carry an override for the focused kind — re-mounting the
/// picker for an already-overridden role would just duplicate the
/// existing rows. Silent no-op when no candidates remain (the row is
/// rendered dimmed in that state).
pub(super) fn open_auth_role_picker(editor: &mut EditorState<'_>, config: &AppConfig) {
    let Some(kind) = editor.auth_selected_kind else {
        crate::debug_log!(
            "auth_role_picker",
            "open_auth_role_picker: no auth kind selected (root view or stale state)"
        );
        return;
    };
    let eligible = eligible_agents_for_override(editor, config);
    let already_overridden: std::collections::BTreeSet<String> = editor
        .pending
        .roles
        .iter()
        .filter(|(_, ro)| role_override_present(kind, ro))
        .map(|(name, _)| name.clone())
        .collect();
    let candidates: Vec<crate::selector::RoleSelector> = eligible
        .into_iter()
        .filter(|r| !already_overridden.contains(r))
        .filter_map(|r| match crate::selector::RoleSelector::parse(&r) {
            Ok(sel) => Some(sel),
            Err(e) => {
                crate::debug_log!(
                    "auth_role_picker",
                    "skipping role {r:?} from override picker (parse failed: {e})"
                );
                None
            }
        })
        .collect();
    if candidates.is_empty() {
        return;
    }
    let state = RolePickerState::new(candidates);
    editor.modal = Some(Modal::AuthRolePicker { state });
}

/// Toggle the expanded/collapsed state of a role section on the Auth tab.
/// If the role is currently expanded, collapse it; otherwise expand it.
pub(super) fn toggle_role_expand(editor: &mut EditorState<'_>, role: String) {
    if !editor.auth_expanded.remove(&role) {
        editor.auth_expanded.insert(role);
    }
}

/// Handle `D`/`d` on the Auth tab.
///
/// - `RoleHeader` → clear the selected auth kind's role override.
/// - `RoleMode` → silently clear the selected auth kind's role-level override.
/// - `WorkspaceMode` → clear the workspace-level override for the selected auth kind.
/// - Anything else (`AuthKindRow`, `AddSentinel`, `Spacer`) → no-op.
pub(super) fn handle_d_on_auth_row(editor: &mut EditorState<'_>, config: &AppConfig) {
    let FieldFocus::Row(n) = editor.active_field;
    let rows = auth_flat_rows(editor, config);
    match rows.get(n).cloned() {
        Some(AuthRow::RoleHeader { role, .. }) => {
            if let Some(kind) = editor.auth_selected_kind {
                clear_role_kind(editor, &role, kind);
            }
        }
        Some(AuthRow::RoleMode { role, kind }) => {
            clear_role_kind(editor, &role, kind);
        }
        Some(AuthRow::WorkspaceMode { kind }) => {
            clear_workspace_kind(&mut editor.pending, kind);
        }
        _ => {}
    }
}

fn clear_role_kind(editor: &mut EditorState<'_>, role: &str, kind: AuthKind) {
    if let Some(ro) = editor.pending.roles.get_mut(role) {
        match kind {
            AuthKind::Claude => ro.claude = None,
            AuthKind::Codex => ro.codex = None,
            AuthKind::Amp => ro.amp = None,
            AuthKind::Kimi => {
                ro.kimi = None;
                ro.env.remove(crate::env_model::KIMI_CODE_API_KEY_ENV_NAME);
            }
            AuthKind::Opencode => ro.opencode = None,
            AuthKind::Grok => ro.grok = None,
            AuthKind::Github => ro.github = None,
            AuthKind::Zai => {
                ro.env.remove(crate::env_model::ZAI_API_KEY_ENV_NAME);
            }
            AuthKind::Minimax => {
                ro.env.remove(crate::env_model::MINIMAX_API_KEY_ENV_NAME);
            }
        }
    }
}

fn clear_workspace_kind(ws: &mut crate::workspace::WorkspaceConfig, kind: AuthKind) {
    match kind {
        AuthKind::Claude => ws.claude = None,
        AuthKind::Codex => ws.codex = None,
        AuthKind::Amp => ws.amp = None,
        AuthKind::Kimi => {
            ws.kimi = None;
            ws.env.remove(crate::env_model::KIMI_CODE_API_KEY_ENV_NAME);
        }
        AuthKind::Opencode => ws.opencode = None,
        AuthKind::Grok => ws.grok = None,
        AuthKind::Github => ws.github = None,
        AuthKind::Zai => {
            ws.env.remove(crate::env_model::ZAI_API_KEY_ENV_NAME);
        }
        AuthKind::Minimax => {
            ws.env.remove(crate::env_model::MINIMAX_API_KEY_ENV_NAME);
        }
    }
}

/// Read the current mode + credential for the form's target out of
/// `editor.pending`. Returns `(None, _)` when the layer has no explicit
/// mode set yet — the form opens with the mode picker unset.
fn current_mode_and_credential(
    editor: &EditorState<'_>,
    target: &AuthFormTarget,
) -> (Option<AuthMode>, Option<EnvValue>) {
    match target {
        AuthFormTarget::Workspace { kind } => {
            workspace_auth_mode_and_credential(&editor.pending, *kind)
        }
        AuthFormTarget::WorkspaceRole { role, kind } => {
            role_auth_mode_and_credential(editor.pending.roles.get(role), *kind)
        }
    }
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
pub(super) fn handle_auth_form_key(
    editor: &mut EditorState<'_>,
    key: KeyEvent,
    op_available: bool,
) -> bool {
    if !matches!(editor.modal, Some(Modal::AuthForm { .. })) {
        return false;
    }

    let Some(Modal::AuthForm { focus, .. }) = editor.modal.as_ref() else {
        unreachable!("guarded above");
    };
    let current_focus = *focus;

    // Esc cancels at every focus. Drain the auth-form return stash too so
    // a stale OpPicker round-trip can't be re-applied to a future modal —
    // every other exit path (Save / Cancel / Reset commit, OpPicker
    // commit/cancel) drains it explicitly; Esc must too.
    if key.code == KeyCode::Esc {
        editor.modal = None;
        editor.modal_parents.clear();
        return true;
    }

    // `g`/`G` at any focus mints a Claude OAuth token. It opens the
    // shared source picker (plain literal vs. 1Password) as the first
    // step. Gated to the workspace-level Claude oauth_token slot in Edit
    // mode; a no-op everywhere else.
    if matches!(key.code, KeyCode::Char('g' | 'G'))
        && try_start_token_generate(editor, op_available)
    {
        return true;
    }

    let Some(Modal::AuthForm { state, .. }) = editor.modal.as_ref() else {
        return false;
    };
    let plan = auth_form_key_plan_with_source_folder(
        current_focus,
        key.code,
        state.shows_source_folder(),
        state.shows_credential_block(),
        state.can_save(),
    );

    match plan {
        AuthFormKeyPlan::Stay => false,
        AuthFormKeyPlan::Focus(next) => {
            if let Some(Modal::AuthForm { focus, .. }) = editor.modal.as_mut() {
                *focus = next;
            }
            false
        }
        AuthFormKeyPlan::CycleMode => {
            if let Some(Modal::AuthForm { state, focus, .. }) = editor.modal.as_mut() {
                state.cycle_mode();
                if *focus == AuthFormFocus::SourceFolder && !state.shows_source_folder() {
                    *focus = AuthFormFocus::Mode;
                }
            }
            false
        }
        AuthFormKeyPlan::OpenSourceFolderBrowser => {
            open_auth_source_folder_browser_from_form(editor)
        }
        AuthFormKeyPlan::OpenCredentialSource => {
            open_auth_source_picker_from_form(editor, op_available)
        }
        AuthFormKeyPlan::Save => commit_auth_form_save(editor),
        AuthFormKeyPlan::Cancel => {
            editor.modal = None;
            true
        }
        AuthFormKeyPlan::Reset => reset_auth_form_layer(editor),
    }
}

/// Whether the open auth form is eligible for the `g`/`G` generate
/// trigger: a Claude `oauth_token` slot in an existing (Edit-mode)
/// workspace, at either the workspace layer or a per-role override. The
/// scope is taken from the form's target — the operator picks the role
/// by opening that role's auth form, so generate needs no role step.
pub(crate) fn auth_form_can_generate_token(editor: &EditorState<'_>) -> bool {
    if !matches!(
        editor.mode,
        crate::console::tui::state::EditorMode::Edit { .. }
    ) {
        return false;
    }
    let Some(Modal::AuthForm { target, state, .. }) = editor.modal.as_ref() else {
        return false;
    };
    can_generate_claude_oauth_token(state.kind, state.mode)
        && matches!(
            target,
            AuthFormTarget::Workspace {
                kind: AuthKind::Claude
            } | AuthFormTarget::WorkspaceRole {
                kind: AuthKind::Claude,
                ..
            }
        )
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
    if !auth_form_can_generate_token(editor) {
        return false;
    }
    if !matches!(
        editor.mode,
        crate::console::tui::state::EditorMode::Edit { .. }
    ) {
        return false;
    }
    let Some(Modal::AuthForm {
        target,
        state,
        focus,
        literal_buffer,
    }) = editor.modal.take()
    else {
        return false;
    };
    // Stash the form so the mint completion re-mounts it via the same
    // helpers the provide path uses. The generate vs. provide
    // disambiguation is the `generating_token_target` marker, which the
    // source-picker / op-picker commit arms check first.
    editor.generating_token_target = Some(target.clone());
    editor.modal_parents.push(Modal::AuthForm {
        target,
        state,
        focus,
        literal_buffer,
    });
    editor.modal = Some(Modal::AuthSourcePicker {
        state: generated_token_source_picker_state(op_available),
    });
    true
}

fn open_auth_source_folder_browser_from_form(editor: &mut EditorState<'_>) -> bool {
    let Some(Modal::AuthForm {
        target,
        state,
        focus,
        literal_buffer,
    }) = editor.modal.take()
    else {
        return false;
    };

    if !state.shows_source_folder() {
        editor.modal = Some(Modal::AuthForm {
            target,
            state,
            focus,
            literal_buffer,
        });
        return false;
    }

    match jackin_console::services::file_browser::state_from_home_with_hidden() {
        Ok(browser) => {
            editor.modal_parents.push(Modal::AuthForm {
                target,
                state,
                focus: AuthFormFocus::SourceFolder,
                literal_buffer,
            });
            editor.modal = Some(Modal::FileBrowser {
                target: FileBrowserTarget::AuthFormSourceFolder,
                state: browser,
            });
            true
        }
        Err(error) => {
            editor.modal = Some(Modal::AuthForm {
                target,
                state,
                focus,
                literal_buffer,
            });
            crate::console::tui::state::open_editor_action_error(editor, &error);
            true
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
    let Some(Modal::AuthForm {
        target,
        state,
        focus,
        literal_buffer,
    }) = editor.modal.take()
    else {
        return false;
    };

    let Some(env_var) = state.mode.and_then(|m| state.kind.required_env_var(m)) else {
        editor.modal = Some(Modal::AuthForm {
            target,
            state,
            focus,
            literal_buffer,
        });
        return false;
    };

    editor.modal_parents.push(Modal::AuthForm {
        target,
        state,
        focus,
        literal_buffer,
    });
    editor.modal = Some(Modal::AuthSourcePicker {
        state: auth_source_picker_state(env_var, op_available),
    });
    true
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
    crate::debug_log!(
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
pub(super) fn apply_plain_source_picker_to_auth_form(editor: &mut EditorState<'_>) {
    let Some(Modal::AuthForm {
        target,
        state,
        literal_buffer,
        ..
    }) = editor.modal_parents.pop()
    else {
        log_missing_return_path(
            AUTH_MISSING_PLAIN_SOURCE,
            "apply_plain_source_picker_to_auth_form",
            "",
        );
        return;
    };
    // Re-push with focus pinned to CredentialSource for the TextInput round-trip.
    editor.modal_parents.push(Modal::AuthForm {
        target,
        state,
        focus: AuthFormFocus::CredentialSource,
        literal_buffer: literal_buffer.clone(),
    });
    editor.modal = Some(Modal::TextInput {
        target: TextInputTarget::AuthCredential,
        state: auth_credential_input_state(literal_buffer),
    });
}

/// Commit branch for the credential `Modal::TextInput`. Lifts the
/// stashed auth form back, applies the typed value via `set_literal`,
/// and re-mounts the form with focus on Save. Also the post-mint
/// re-mount target for the plain-text generate path in the
/// `run_console` loop, hence the wider visibility.
pub(in crate::console) fn apply_plain_text_to_auth_form(editor: &mut EditorState<'_>, value: &str) {
    let Some(Modal::AuthForm {
        target, mut state, ..
    }) = editor.modal_parents.pop()
    else {
        log_missing_return_path(
            AUTH_MISSING_PLAIN_TEXT,
            "apply_plain_text_to_auth_form",
            " — typed credential dropped",
        );
        return;
    };
    state.set_literal(value.to_owned());
    editor.modal = Some(Modal::AuthForm {
        target,
        state,
        focus: AuthFormFocus::Save,
        literal_buffer: value.to_owned(),
    });
}

pub(in crate::console) fn apply_source_folder_to_auth_form(
    editor: &mut EditorState<'_>,
    value: PathBuf,
) {
    let Some(Modal::AuthForm {
        target,
        mut state,
        literal_buffer,
        ..
    }) = editor.modal_parents.pop()
    else {
        log_missing_return_path(
            AUTH_MISSING_FOLDER_COMMIT,
            "apply_source_folder_to_auth_form",
            " — selected folder dropped",
        );
        return;
    };
    state.set_source_folder(value);
    editor.modal = Some(Modal::AuthForm {
        target,
        state,
        focus: AuthFormFocus::Save,
        literal_buffer,
    });
}

/// Commit branch for `Modal::AuthSourcePicker` when the operator picks
/// the 1Password source. Pins the stashed return-path focus to
/// `CredentialSource` (so cancel/error paths land back on the source
/// row) and mounts a fresh `Modal::OpPicker`.
pub(super) fn open_op_picker_from_auth_source(
    editor: &mut EditorState<'_>,
    op_cache: std::rc::Rc<std::cell::RefCell<OpCache>>,
) {
    let Some(Modal::AuthForm { focus, .. }) = editor.modal_parents.last_mut() else {
        log_missing_return_path(
            AUTH_MISSING_OP_SOURCE,
            "open_op_picker_from_auth_source",
            " — closing modal",
        );
        editor.modal = None;
        return;
    };
    *focus = AuthFormFocus::CredentialSource;
    editor.modal = Some(Modal::OpPicker {
        state: Box::new(OpPickerState::new_with_cache(op_cache)),
    });
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
pub(in crate::console) fn apply_op_picker_to_auth_form_committed(
    editor: &mut EditorState<'_>,
    op_ref: crate::operator_env::OpRef,
) {
    let Some(Modal::AuthForm {
        target,
        mut state,
        focus,
        literal_buffer,
    }) = editor.modal_parents.pop()
    else {
        log_missing_return_path(
            AUTH_MISSING_OP_COMMIT,
            "apply_op_picker_to_auth_form_committed",
            " — async OpRef commit dropped",
        );
        return;
    };
    // The read already succeeded; set the ref directly without re-reading.
    state.set_op_ref(op_ref);
    editor.modal = Some(Modal::AuthForm {
        target,
        state,
        focus: AuthFormFocus::Save,
        literal_buffer,
    });
    // `focus` from the destructuring above is not forwarded (we always land on
    // Save after a successful commit), so suppress the unused-variable warning.
    let _ = focus;
}

/// Called when the async 1Password read for an op picker commit fails
/// (Touch ID rejected, network error, vault not found, etc.). Re-mounts the
/// `ErrorPopup` over the stashed auth form so the operator sees the error;
/// dismissing the popup restores the form via
/// `restore_auth_form_after_op_picker_cancel`.
///
/// The auth form remains on `editor.modal_parents` (it was NOT popped by the
/// async path before spawning) so the cancel/restore path can lift it back.
pub(in crate::console) fn apply_op_picker_commit_failed(
    editor: &mut EditorState<'_>,
    error: &anyhow::Error,
) {
    editor.modal = Some(Modal::ErrorPopup {
        state: jackin_console::tui::components::error_popup::op_read_failed_error_popup_state(
            error,
        ),
    });
}

/// Restore the auth-form modal unchanged after the operator cancels
/// the `OpPicker` or the literal `TextInput`. Both side modals share
/// the same recovery shape, so the same helper handles both.
pub(super) fn restore_auth_form_after_op_picker_cancel(editor: &mut EditorState<'_>) {
    let Some(Modal::AuthForm {
        target,
        state,
        focus,
        literal_buffer,
    }) = editor.modal_parents.pop()
    else {
        log_missing_return_path(
            AUTH_MISSING_OP_CANCEL,
            "restore_auth_form_after_op_picker_cancel",
            "",
        );
        return;
    };
    editor.modal = Some(Modal::AuthForm {
        target,
        state,
        focus,
        literal_buffer,
    });
}

/// Inner helper split out so tests can inject a fake `OpRunner`
/// without touching the real `op` binary.
#[cfg(test)]
fn apply_op_picker_to_auth_form_with_runner<R: crate::operator_env::OpRunner + ?Sized>(
    editor: &mut EditorState<'_>,
    op_ref: crate::operator_env::OpRef,
    runner: &R,
) {
    apply_op_picker_to_auth_form_with_validator(editor, op_ref, |op_ref| {
        runner.read(&op_ref.op).map(|_| ())
    });
}

#[cfg(test)]
fn apply_op_picker_to_auth_form_with_validator(
    editor: &mut EditorState<'_>,
    op_ref: crate::operator_env::OpRef,
    validate: impl FnOnce(&crate::operator_env::OpRef) -> anyhow::Result<()>,
) {
    let Some(Modal::AuthForm {
        target,
        mut state,
        focus,
        literal_buffer,
    }) = editor.modal_parents.pop()
    else {
        log_missing_return_path(
            AUTH_MISSING_OP_COMMIT,
            "apply_op_picker_to_auth_form",
            " — OpRef commit dropped",
        );
        return;
    };
    let read_result = validate(&op_ref);
    if let Err(e) = read_result {
        // Re-push the form so the ErrorPopup dismiss handler can
        // restore it via restore_auth_form_after_op_picker_cancel.
        editor.modal_parents.push(Modal::AuthForm {
            target,
            state,
            focus,
            literal_buffer,
        });
        editor.modal = Some(Modal::ErrorPopup {
            state: jackin_console::tui::components::error_popup::op_read_failed_error_popup_state(
                e,
            ),
        });
        return;
    }
    state.set_op_ref(op_ref);
    editor.modal = Some(Modal::AuthForm {
        target,
        state,
        // Drop the cursor onto Save so Enter commits.
        focus: AuthFormFocus::Save,
        literal_buffer,
    });
}

fn commit_auth_form_save(editor: &mut EditorState<'_>) -> bool {
    let Some(Modal::AuthForm { target, state, .. }) = editor.modal.as_mut() else {
        return false;
    };
    let committed_target = target.clone();
    let kind = state.kind;
    let form = std::mem::replace(state.as_mut(), AuthForm::new(kind));
    editor.modal = None;
    persist_form(editor, &committed_target, &form);
    true
}

fn reset_auth_form_layer(editor: &mut EditorState<'_>) -> bool {
    let Some(Modal::AuthForm { target, .. }) = editor.modal.as_mut() else {
        return false;
    };
    let committed_target = target.clone();
    editor.modal = None;
    clear_layer(editor, &committed_target);
    true
}

/// Apply a successful form commit to `editor.pending`. Writes both the
/// kind block (`auth_forward`) AND the credential env var when the
/// form's outcome includes one.
///
/// Claude / Codex credentials land on the workspace / role env block
/// (`[workspaces.<ws>.env]` / `[…roles.<role>.env]`); Github
/// credentials land under the kind-scoped `[github.env]` block at the
/// matching layer (parallel to the global `[github.env]` map and
/// resolved through [`crate::config::build_github_env_layers`]).
fn persist_form(editor: &mut EditorState<'_>, target: &AuthFormTarget, form: &AuthForm) {
    let Some(outcome) = form.commit() else {
        return;
    };
    match target {
        AuthFormTarget::Workspace { kind } => {
            apply_workspace_auth_commit(
                &mut editor.pending,
                *kind,
                outcome.mode,
                outcome.env_var_name,
                outcome.env_value.clone(),
            );
            set_workspace_sync_source_dir(&mut editor.pending, *kind, outcome.source_folder);
        }
        AuthFormTarget::WorkspaceRole { role, kind } => {
            let entry = editor.pending.roles.entry(role.clone()).or_default();
            apply_role_auth_commit(
                entry,
                *kind,
                outcome.mode,
                outcome.env_var_name,
                outcome.env_value.clone(),
            );
            set_role_sync_source_dir(entry, *kind, outcome.source_folder);
        }
    }
}

/// Clear the `auth_forward` at the form's target layer. Does NOT touch
/// the credential env var — operators delete those via the Secrets tab
/// (Claude / Codex) or the Github env block on the workspace × github
/// layer.
fn clear_layer(editor: &mut EditorState<'_>, target: &AuthFormTarget) {
    match target {
        AuthFormTarget::Workspace { kind } => {
            clear_workspace_auth_layer(&mut editor.pending, *kind);
            set_workspace_sync_source_dir(&mut editor.pending, *kind, None);
        }
        AuthFormTarget::WorkspaceRole { role, kind } => {
            if let Some(entry) = editor.pending.roles.get_mut(role) {
                clear_role_auth_layer(entry, *kind);
                set_role_sync_source_dir(entry, *kind, None);
            }
        }
    }
}

#[cfg(test)]
mod tests;
