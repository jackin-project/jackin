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

use super::super::super::widgets::auth_panel::{AuthForm, CredentialInput};
use super::super::super::widgets::op_picker::OpPickerState;
use super::super::super::widgets::role_picker::RolePickerState;
use super::super::auth_kind::{AuthKind, AuthMode};
use super::super::render::editor::resolve_auth_row_target;
use super::super::state::{
    AuthFormFocus, AuthFormReturnPath, AuthFormTarget, EditorState, FieldFocus, Modal,
    TextInputTarget,
};
use crate::config::AppConfig;
use crate::config::{
    AgentAuthConfig, AmpAuthConfig, CodexAuthConfig, GithubAuthConfig, KimiAuthConfig,
};
use crate::console::op_cache::OpCache;
use crate::console::widgets::text_input::TextInputState;
use crate::operator_env::EnvValue;
use crate::workspace::WorkspaceRoleOverride;

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
    let kind = target_kind(&target);
    let (existing_mode, existing_cred) = current_mode_and_credential(editor, &target);
    let form = existing_mode.map_or_else(
        || AuthForm::new(kind),
        |mode| AuthForm::from_existing(kind, mode, existing_cred),
    );
    let literal_buffer = if let CredentialInput::Literal(s) = &form.credential {
        s.clone()
    } else {
        String::new()
    };
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
pub(super) fn open_auth_role_picker(editor: &mut EditorState<'_>, config: &AppConfig) {
    let Some(kind) = editor.auth_selected_kind else {
        crate::debug_log!(
            "auth_role_picker",
            "open_auth_role_picker: no auth kind selected (root view or stale state)"
        );
        return;
    };
    let eligible = super::super::render::editor::eligible_agents_for_override(editor, config);
    let already_overridden: std::collections::BTreeSet<String> = editor
        .pending
        .roles
        .iter()
        .filter(|(_, ro)| kind.role_override_present(ro))
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
/// - `RoleMode` / `RoleSource` → silently clear the selected auth kind's
///   role-level override.
/// - `WorkspaceMode` / `WorkspaceSource` → clear the workspace-level
///   override for the selected auth kind.
/// - Anything else (`AuthKindRow`, `AddSentinel`, `Spacer`) → no-op.
pub(super) fn handle_d_on_auth_row(editor: &mut EditorState<'_>, config: &AppConfig) {
    let FieldFocus::Row(n) = editor.active_field;
    let rows = super::super::render::editor::auth_flat_rows(editor, config);
    match rows.get(n).cloned() {
        Some(super::super::render::editor::AuthRow::RoleHeader { role, .. }) => {
            if let Some(kind) = editor.auth_selected_kind {
                clear_role_kind(editor, &role, kind);
            }
        }
        Some(
            super::super::render::editor::AuthRow::RoleMode { role, kind }
            | super::super::render::editor::AuthRow::RoleSource { role, kind },
        ) => {
            clear_role_kind(editor, &role, kind);
        }
        Some(
            super::super::render::editor::AuthRow::WorkspaceMode { kind }
            | super::super::render::editor::AuthRow::WorkspaceSource { kind },
        ) => {
            clear_workspace_kind(&mut editor.pending, kind);
        }
        _ => {}
    }
}

/// Lift the [`AuthKind`] out of an [`AuthFormTarget`] regardless of
/// whether it points at the workspace or the workspace × role layer.
const fn target_kind(target: &AuthFormTarget) -> AuthKind {
    match target {
        AuthFormTarget::Workspace { kind } | AuthFormTarget::WorkspaceRole { kind, .. } => *kind,
    }
}

fn clear_role_kind(editor: &mut EditorState<'_>, role: &str, kind: AuthKind) {
    if let Some(ro) = editor.pending.roles.get_mut(role) {
        match kind {
            AuthKind::Claude => ro.claude = None,
            AuthKind::Codex => ro.codex = None,
            AuthKind::Amp => ro.amp = None,
            AuthKind::Kimi => ro.kimi = None,
            AuthKind::Github => ro.github = None,
        }
    }
}

fn clear_workspace_kind(ws: &mut crate::workspace::WorkspaceConfig, kind: AuthKind) {
    match kind {
        AuthKind::Claude => ws.claude = None,
        AuthKind::Codex => ws.codex = None,
        AuthKind::Amp => ws.amp = None,
        AuthKind::Kimi => ws.kimi = None,
        AuthKind::Github => ws.github = None,
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
        AuthFormTarget::Workspace { kind } => workspace_mode_and_cred(editor, *kind),
        AuthFormTarget::WorkspaceRole { role, kind } => {
            workspace_role_mode_and_cred(editor, role, *kind)
        }
    }
}

fn workspace_mode_and_cred(
    editor: &EditorState<'_>,
    kind: AuthKind,
) -> (Option<AuthMode>, Option<EnvValue>) {
    match kind {
        AuthKind::Claude => {
            let mode = editor
                .pending
                .claude
                .as_ref()
                .map(|c| AuthMode::from_auth_forward(c.auth_forward));
            let env_var = mode.and_then(|m| kind.required_env_var(m));
            let cred = env_var.and_then(|v| editor.pending.env.get(v).cloned());
            (mode, cred)
        }
        AuthKind::Codex => {
            let mode = editor
                .pending
                .codex
                .as_ref()
                .map(|c| AuthMode::from_auth_forward(c.auth_forward));
            let env_var = mode.and_then(|m| kind.required_env_var(m));
            let cred = env_var.and_then(|v| editor.pending.env.get(v).cloned());
            (mode, cred)
        }
        AuthKind::Amp => {
            let mode = editor
                .pending
                .amp
                .as_ref()
                .map(|c| AuthMode::from_auth_forward(c.auth_forward));
            let env_var = mode.and_then(|m| kind.required_env_var(m));
            let cred = env_var.and_then(|v| editor.pending.env.get(v).cloned());
            (mode, cred)
        }
        AuthKind::Kimi => {
            let mode = editor
                .pending
                .kimi
                .as_ref()
                .map(|c| AuthMode::from_auth_forward(c.auth_forward));
            let env_var = mode.and_then(|m| kind.required_env_var(m));
            let cred = env_var.and_then(|v| editor.pending.env.get(v).cloned());
            (mode, cred)
        }
        AuthKind::Github => {
            let mode = editor
                .pending
                .github
                .as_ref()
                .map(|g| AuthMode::from_github(g.auth_forward));
            let env_var = mode.and_then(|m| kind.required_env_var(m));
            // GH_TOKEN lives on `[workspaces.<ws>.github.env]`,
            // parallel to how the global `[github.env]` is laid out
            // — see [`crate::config::build_github_env_layers`].
            let cred = env_var.and_then(|v| {
                editor
                    .pending
                    .github
                    .as_ref()
                    .and_then(|g| g.env.get(v).cloned())
            });
            (mode, cred)
        }
    }
}

fn workspace_role_mode_and_cred(
    editor: &EditorState<'_>,
    role: &str,
    kind: AuthKind,
) -> (Option<AuthMode>, Option<EnvValue>) {
    let override_ref = editor.pending.roles.get(role);
    match kind {
        AuthKind::Claude => {
            let mode = override_ref
                .and_then(|ro| ro.claude.as_ref())
                .map(|c| AuthMode::from_auth_forward(c.auth_forward));
            let env_var = mode.and_then(|m| kind.required_env_var(m));
            let cred = env_var.and_then(|v| override_ref.and_then(|ro| ro.env.get(v).cloned()));
            (mode, cred)
        }
        AuthKind::Codex => {
            let mode = override_ref
                .and_then(|ro| ro.codex.as_ref())
                .map(|c| AuthMode::from_auth_forward(c.auth_forward));
            let env_var = mode.and_then(|m| kind.required_env_var(m));
            let cred = env_var.and_then(|v| override_ref.and_then(|ro| ro.env.get(v).cloned()));
            (mode, cred)
        }
        AuthKind::Amp => {
            let mode = override_ref
                .and_then(|ro| ro.amp.as_ref())
                .map(|c| AuthMode::from_auth_forward(c.auth_forward));
            let env_var = mode.and_then(|m| kind.required_env_var(m));
            let cred = env_var.and_then(|v| override_ref.and_then(|ro| ro.env.get(v).cloned()));
            (mode, cred)
        }
        AuthKind::Kimi => {
            let mode = override_ref
                .and_then(|ro| ro.kimi.as_ref())
                .map(|c| AuthMode::from_auth_forward(c.auth_forward));
            let env_var = mode.and_then(|m| kind.required_env_var(m));
            let cred = env_var.and_then(|v| override_ref.and_then(|ro| ro.env.get(v).cloned()));
            (mode, cred)
        }
        AuthKind::Github => {
            let mode = override_ref
                .and_then(|ro| ro.github.as_ref())
                .map(|g| AuthMode::from_github(g.auth_forward));
            let env_var = mode.and_then(|m| kind.required_env_var(m));
            let cred = env_var.and_then(|v| {
                override_ref
                    .and_then(|ro| ro.github.as_ref())
                    .and_then(|g| g.env.get(v).cloned())
            });
            (mode, cred)
        }
    }
}

/// Drive a single keystroke into an open `Modal::AuthForm`. Returns
/// `true` when the modal was closed (committed, cancelled, or
/// transitioned away from the form via `Modal::AuthSourcePicker`).
///
/// `op_available` gates the 1Password choice rendered inside
/// `Modal::AuthSourcePicker` — passed through from `EditorState` so
/// the form doesn't reprobe the `op` binary on every keypress. The
/// `OpPicker` itself is opened by `open_op_picker_from_auth_source`
/// after the operator selects 1Password from the source picker; that
/// helper takes its own `op_cache` so this entry point doesn't have
/// to.
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
        editor.pending_auth_form_return = None;
        return true;
    }

    if current_focus == AuthFormFocus::CredentialSource {
        return handle_credential_source_key(editor, key, op_available);
    }

    let Some(Modal::AuthForm { state, focus, .. }) = editor.modal.as_mut() else {
        return false;
    };

    match current_focus {
        AuthFormFocus::Mode => handle_mode_key(focus, state.as_mut(), key),
        AuthFormFocus::CredentialSource => unreachable!("handled above"),
        AuthFormFocus::Save => {
            return handle_save_key(editor, key);
        }
        AuthFormFocus::Cancel => {
            return handle_cancel_key(editor, key);
        }
        AuthFormFocus::Reset => {
            return handle_reset_key(editor, key);
        }
    }
    false
}

/// Keystroke router for the `CredentialSource` row.
///
/// - `Enter` → open the shared source picker (literal vs. 1Password).
/// - `Down/j`/`Tab` → focus `Save` (forward through the cycle).
/// - `Up/k`/`BackTab` → focus `Mode` (backward through the cycle).
fn handle_credential_source_key(
    editor: &mut EditorState<'_>,
    key: KeyEvent,
    op_available: bool,
) -> bool {
    let Some(Modal::AuthForm { focus, .. }) = editor.modal.as_mut() else {
        return false;
    };

    match key.code {
        KeyCode::Enter => open_auth_source_picker_from_form(editor, op_available),
        KeyCode::Down | KeyCode::Char('j') | KeyCode::Tab => {
            *focus = AuthFormFocus::Save;
            false
        }
        KeyCode::Up | KeyCode::Char('k') | KeyCode::BackTab => {
            *focus = AuthFormFocus::Mode;
            false
        }
        _ => false,
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

    editor.pending_auth_form_return = Some(AuthFormReturnPath {
        target,
        state,
        focus,
        literal_buffer,
    });
    editor.modal = Some(Modal::AuthSourcePicker {
        state: crate::console::widgets::source_picker::SourcePickerState::new(
            env_var.to_string(),
            op_available,
        ),
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
    let Some(AuthFormReturnPath {
        target,
        state,
        literal_buffer,
        ..
    }) = editor.pending_auth_form_return.take()
    else {
        log_missing_return_path(
            AUTH_MISSING_PLAIN_SOURCE,
            "apply_plain_source_picker_to_auth_form",
            "",
        );
        return;
    };
    editor.pending_auth_form_return = Some(AuthFormReturnPath {
        target,
        state,
        focus: AuthFormFocus::CredentialSource,
        literal_buffer: literal_buffer.clone(),
    });
    editor.modal = Some(Modal::TextInput {
        target: TextInputTarget::AuthCredential,
        state: TextInputState::new("Credential", literal_buffer),
    });
}

/// Commit branch for the credential `Modal::TextInput`. Lifts the
/// stashed auth form back, applies the typed value via `set_literal`,
/// and re-mounts the form with focus on Save.
pub(super) fn apply_plain_text_to_auth_form(editor: &mut EditorState<'_>, value: &str) {
    let Some(AuthFormReturnPath {
        target, mut state, ..
    }) = editor.pending_auth_form_return.take()
    else {
        log_missing_return_path(
            AUTH_MISSING_PLAIN_TEXT,
            "apply_plain_text_to_auth_form",
            " — typed credential dropped",
        );
        return;
    };
    state.set_literal(value.to_string());
    editor.modal = Some(Modal::AuthForm {
        target,
        state,
        focus: AuthFormFocus::Save,
        literal_buffer: value.to_string(),
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
    let Some(return_path) = editor.pending_auth_form_return.as_mut() else {
        log_missing_return_path(
            AUTH_MISSING_OP_SOURCE,
            "open_op_picker_from_auth_source",
            " — closing modal",
        );
        editor.modal = None;
        return;
    };
    return_path.focus = AuthFormFocus::CredentialSource;
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
/// On `try_commit_op_ref` failure (vault read error), the form is
/// re-stashed into `pending_auth_form_return` and `Modal::ErrorPopup`
/// is mounted; dismissing the popup invokes
/// `restore_auth_form_after_op_picker_cancel` so the operator lands
/// back on the form with the prior credential unchanged. The
/// `read-then-commit` invariant on `try_commit_op_ref` guarantees a
/// broken reference never lands in `editor.pending`.
pub(super) fn apply_op_picker_to_auth_form(
    editor: &mut EditorState<'_>,
    op_ref: crate::operator_env::OpRef,
) {
    apply_op_picker_to_auth_form_with_runner(editor, op_ref, &crate::operator_env::OpCli::new());
}

/// Restore the auth-form modal unchanged after the operator cancels
/// the `OpPicker` or the literal `TextInput`. Both side modals share
/// the same recovery shape, so the same helper handles both.
pub(super) fn restore_auth_form_after_op_picker_cancel(editor: &mut EditorState<'_>) {
    let Some(AuthFormReturnPath {
        target,
        state,
        focus,
        literal_buffer,
    }) = editor.pending_auth_form_return.take()
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
/// without touching the real `op` binary. Mirrors the test pattern
/// for `try_commit_op_ref` in `form.rs`.
fn apply_op_picker_to_auth_form_with_runner<R: crate::operator_env::OpRunner + ?Sized>(
    editor: &mut EditorState<'_>,
    op_ref: crate::operator_env::OpRef,
    runner: &R,
) {
    use crate::console::widgets::error_popup::ErrorPopupState;

    let Some(AuthFormReturnPath {
        target,
        mut state,
        focus,
        literal_buffer,
    }) = editor.pending_auth_form_return.take()
    else {
        log_missing_return_path(
            AUTH_MISSING_OP_COMMIT,
            "apply_op_picker_to_auth_form",
            " — OpRef commit dropped",
        );
        return;
    };
    let read_result = state.try_commit_op_ref(runner, op_ref);
    if let Err(e) = read_result {
        // Mount the error popup directly and re-stash the form into
        // `pending_auth_form_return` so the popup's dismiss handler
        // (in `editor.rs`'s `Modal::ErrorPopup` branch) can re-mount
        // the auth form via `restore_auth_form_after_op_picker_cancel`.
        // The credential is left unchanged because `try_commit_op_ref`
        // mutates `state` only on Ok (read-then-commit invariant).
        editor.pending_auth_form_return = Some(AuthFormReturnPath {
            target,
            state,
            focus,
            literal_buffer,
        });
        editor.modal = Some(Modal::ErrorPopup {
            state: ErrorPopupState::new("1Password read failed", e.to_string()),
        });
        return;
    }
    editor.modal = Some(Modal::AuthForm {
        target,
        state,
        // Drop the cursor onto Save so Enter commits.
        focus: AuthFormFocus::Save,
        literal_buffer,
    });
}

fn handle_mode_key(focus: &mut AuthFormFocus, form: &mut AuthForm, key: KeyEvent) {
    match key.code {
        KeyCode::Char(' ') => cycle_mode(form),
        KeyCode::Down | KeyCode::Char('j') | KeyCode::Tab => *focus = next_focus_after_mode(form),
        // BackTab wraps backward through the cycle to Reset (the last
        // focusable control). Forward Tab from Reset wraps to Mode in
        // `handle_reset_key`.
        KeyCode::BackTab => *focus = AuthFormFocus::Reset,
        _ => {}
    }
}

fn handle_save_key(editor: &mut EditorState<'_>, key: KeyEvent) -> bool {
    let Some(Modal::AuthForm {
        target,
        state,
        focus,
        ..
    }) = editor.modal.as_mut()
    else {
        return false;
    };
    match key.code {
        KeyCode::Right | KeyCode::Tab => {
            *focus = AuthFormFocus::Cancel;
            false
        }
        // BackTab walks backward through the cycle to the credential
        // row (when shown) or Mode (otherwise); Up mirrors that.
        KeyCode::Up | KeyCode::BackTab => {
            *focus = if state.shows_credential_block() {
                AuthFormFocus::CredentialSource
            } else {
                AuthFormFocus::Mode
            };
            false
        }
        KeyCode::Enter => {
            if !state.can_save() {
                return false;
            }
            let committed_target = target.clone();
            let kind = state.kind;
            let form = std::mem::replace(state.as_mut(), AuthForm::new(kind));
            editor.modal = None;
            persist_form(editor, &committed_target, &form);
            true
        }
        _ => false,
    }
}

fn handle_cancel_key(editor: &mut EditorState<'_>, key: KeyEvent) -> bool {
    let Some(Modal::AuthForm { focus, .. }) = editor.modal.as_mut() else {
        return false;
    };
    match key.code {
        KeyCode::Left | KeyCode::BackTab => {
            *focus = AuthFormFocus::Save;
            false
        }
        KeyCode::Right | KeyCode::Tab => {
            *focus = AuthFormFocus::Reset;
            false
        }
        KeyCode::Enter => {
            editor.modal = None;
            true
        }
        _ => false,
    }
}

fn handle_reset_key(editor: &mut EditorState<'_>, key: KeyEvent) -> bool {
    let Some(Modal::AuthForm { target, focus, .. }) = editor.modal.as_mut() else {
        return false;
    };
    match key.code {
        KeyCode::Left | KeyCode::BackTab => {
            *focus = AuthFormFocus::Cancel;
            false
        }
        // Tab from the last focusable control wraps to Mode (first).
        KeyCode::Right | KeyCode::Tab => {
            *focus = AuthFormFocus::Mode;
            false
        }
        KeyCode::Enter => {
            let committed_target = target.clone();
            editor.modal = None;
            clear_layer(editor, &committed_target);
            true
        }
        _ => false,
    }
}

fn cycle_mode(form: &mut AuthForm) {
    let modes = form.available_modes();
    if modes.is_empty() {
        return;
    }
    let next = form.mode.map_or(modes[0], |current| {
        let idx = modes.iter().position(|m| *m == current).unwrap_or(0);
        modes[(idx + 1) % modes.len()]
    });
    form.set_mode(next);
}

const fn next_focus_after_mode(form: &AuthForm) -> AuthFormFocus {
    if form.shows_credential_block() {
        AuthFormFocus::CredentialSource
    } else {
        AuthFormFocus::Save
    }
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
            set_workspace_mode(&mut editor.pending, *kind, Some(outcome.mode));
            if let (Some(name), Some(value)) = (outcome.env_var_name, outcome.env_value.clone()) {
                match kind {
                    AuthKind::Claude | AuthKind::Codex | AuthKind::Amp | AuthKind::Kimi => {
                        editor.pending.env.insert(name.to_string(), value);
                    }
                    AuthKind::Github => {
                        let github = editor.pending.github.get_or_insert_with(Default::default);
                        github.env.insert(name.to_string(), value);
                    }
                }
            }
        }
        AuthFormTarget::WorkspaceRole { role, kind } => {
            let entry = editor.pending.roles.entry(role.clone()).or_default();
            set_role_mode(entry, *kind, Some(outcome.mode));
            if let (Some(name), Some(value)) = (outcome.env_var_name, outcome.env_value.clone()) {
                match kind {
                    AuthKind::Claude | AuthKind::Codex | AuthKind::Amp | AuthKind::Kimi => {
                        entry.env.insert(name.to_string(), value);
                    }
                    AuthKind::Github => {
                        let github = entry.github.get_or_insert_with(Default::default);
                        github.env.insert(name.to_string(), value);
                    }
                }
            }
        }
    }
}

/// Clear the `auth_forward` at the form's target layer. Does NOT touch
/// the credential env var — operators delete those via the Secrets tab
/// (Claude / Codex) or the Github env block on the workspace × github
/// layer. Mirrors the existing Claude / Codex behaviour.
fn clear_layer(editor: &mut EditorState<'_>, target: &AuthFormTarget) {
    match target {
        AuthFormTarget::Workspace { kind } => {
            set_workspace_mode(&mut editor.pending, *kind, None);
        }
        AuthFormTarget::WorkspaceRole { role, kind } => {
            if let Some(entry) = editor.pending.roles.get_mut(role) {
                set_role_mode(entry, *kind, None);
            }
        }
    }
}

fn set_workspace_mode(
    ws: &mut crate::workspace::WorkspaceConfig,
    kind: AuthKind,
    mode: Option<AuthMode>,
) {
    match kind {
        AuthKind::Claude => {
            ws.claude = mode
                .and_then(AuthMode::to_auth_forward)
                .map(|auth_forward| AgentAuthConfig { auth_forward });
        }
        AuthKind::Codex => {
            ws.codex = mode
                .and_then(AuthMode::to_auth_forward)
                .map(|auth_forward| CodexAuthConfig(AgentAuthConfig { auth_forward }));
        }
        AuthKind::Amp => {
            ws.amp = mode
                .and_then(AuthMode::to_auth_forward)
                .map(|auth_forward| AmpAuthConfig(AgentAuthConfig { auth_forward }));
        }
        AuthKind::Kimi => {
            ws.kimi = mode
                .and_then(AuthMode::to_auth_forward)
                .map(|auth_forward| KimiAuthConfig(AgentAuthConfig { auth_forward }));
        }
        AuthKind::Github => {
            ws.github = mode.and_then(AuthMode::to_github).map(|auth_forward| {
                // Preserve any existing env block on the workspace's
                // [github] entry — the operator may have already set
                // `GH_TOKEN` and we're only flipping the mode.
                let env = ws
                    .github
                    .as_ref()
                    .map(|g| g.env.clone())
                    .unwrap_or_default();
                GithubAuthConfig { auth_forward, env }
            });
        }
    }
}

fn set_role_mode(entry: &mut WorkspaceRoleOverride, kind: AuthKind, mode: Option<AuthMode>) {
    match kind {
        AuthKind::Claude => {
            entry.claude = mode
                .and_then(AuthMode::to_auth_forward)
                .map(|auth_forward| AgentAuthConfig { auth_forward });
        }
        AuthKind::Codex => {
            entry.codex = mode
                .and_then(AuthMode::to_auth_forward)
                .map(|auth_forward| CodexAuthConfig(AgentAuthConfig { auth_forward }));
        }
        AuthKind::Amp => {
            entry.amp = mode
                .and_then(AuthMode::to_auth_forward)
                .map(|auth_forward| AmpAuthConfig(AgentAuthConfig { auth_forward }));
        }
        AuthKind::Kimi => {
            entry.kimi = mode
                .and_then(AuthMode::to_auth_forward)
                .map(|auth_forward| KimiAuthConfig(AgentAuthConfig { auth_forward }));
        }
        AuthKind::Github => {
            entry.github = mode.and_then(AuthMode::to_github).map(|auth_forward| {
                // Same env-preservation invariant as the workspace
                // setter above: flipping the mode must not clobber a
                // role-scoped GH_TOKEN the operator already provided.
                let env = entry
                    .github
                    .as_ref()
                    .map(|g| g.env.clone())
                    .unwrap_or_default();
                GithubAuthConfig { auth_forward, env }
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{AppConfig, AuthForwardMode, GithubAuthMode};
    use crate::console::manager::auth_kind::AuthKind;
    use crate::console::manager::render::editor::{AuthRow, auth_flat_rows};
    use crate::console::manager::state::{
        AuthFormTarget, EditorState, FieldFocus, ManagerStage, ManagerState,
    };
    use crate::operator_env::{OpRef, OpRunner};
    use crate::workspace::{MountConfig, WorkspaceConfig};
    use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyEventState, KeyModifiers};

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent {
            code,
            modifiers: KeyModifiers::NONE,
            kind: KeyEventKind::Press,
            state: KeyEventState::NONE,
        }
    }

    /// Per-test op-cache (no shared state between test cases).
    fn fresh_op_cache() -> std::rc::Rc<std::cell::RefCell<OpCache>> {
        std::rc::Rc::new(std::cell::RefCell::new(OpCache::default()))
    }

    /// Test wrapper around `handle_auth_form_key` with
    /// `op_available = true`.
    fn drive_key(editor: &mut EditorState<'_>, k: KeyEvent) -> bool {
        handle_auth_form_key(editor, k, true)
    }

    /// Return the flat-row index for `WorkspaceMode { Claude }`.
    /// Panics if the row doesn't exist (which would indicate a broken
    /// fixture, not a test-under-test failure).
    fn workspace_claude_row_idx(editor: &EditorState<'_>, config: &AppConfig) -> usize {
        auth_flat_rows(editor, config)
            .iter()
            .position(|r| {
                matches!(
                    r,
                    AuthRow::WorkspaceMode {
                        kind: AuthKind::Claude,
                    }
                )
            })
            .expect("WorkspaceMode × Claude row must exist in auth_flat_rows")
    }

    /// Return the flat-row index for `WorkspaceMode { Github }`.
    fn workspace_github_row_idx(editor: &EditorState<'_>, config: &AppConfig) -> usize {
        auth_flat_rows(editor, config)
            .iter()
            .position(|r| {
                matches!(
                    r,
                    AuthRow::WorkspaceMode {
                        kind: AuthKind::Github,
                    }
                )
            })
            .expect("WorkspaceMode × Github row must exist in auth_flat_rows")
    }

    fn build_state() -> (AppConfig, ManagerState<'static>) {
        let mut cfg = AppConfig::default();
        let mut ws = WorkspaceConfig {
            workdir: "/code/proj".into(),
            mounts: vec![MountConfig {
                src: "/code/proj".into(),
                dst: "/code/proj".into(),
                readonly: false,
                isolation: crate::isolation::MountIsolation::Shared,
            }],
            allowed_roles: vec!["smith".into()],
            ..Default::default()
        };
        ws.allowed_roles.sort();
        cfg.workspaces.insert("proj".into(), ws);
        cfg.roles.insert(
            "smith".into(),
            crate::config::RoleSource {
                git: "https://example.com/jackin-smith.git".into(),
                trusted: true,
                env: std::collections::BTreeMap::default(),
            },
        );

        let cwd = std::path::PathBuf::from("/tmp");
        let mut state = ManagerState::from_config(&cfg, &cwd);
        let ws = cfg.workspaces.get("proj").unwrap().clone();
        let mut editor = EditorState::new_edit("proj".into(), ws);
        editor.active_tab = crate::console::manager::state::EditorTab::Auth;
        editor.auth_selected_kind = Some(AuthKind::Claude);
        let ws_claude_idx = workspace_claude_row_idx(&editor, &cfg);
        editor.active_field = FieldFocus::Row(ws_claude_idx);
        state.stage = ManagerStage::Editor(editor);
        (cfg, state)
    }

    /// Build state focused on the GitHub kind for the github-tab tests.
    fn build_github_state() -> (AppConfig, ManagerState<'static>) {
        let (cfg, mut state) = build_state();
        let ManagerStage::Editor(editor) = &mut state.stage else {
            panic!()
        };
        editor.auth_selected_kind = Some(AuthKind::Github);
        let ws_github_idx = workspace_github_row_idx(editor, &cfg);
        editor.active_field = FieldFocus::Row(ws_github_idx);
        (cfg, state)
    }

    /// Walking from the workspace × Claude row through the form:
    /// Enter opens form, Space cycles mode to `api_key`, Tab moves to
    /// credential, Enter picks source, type literal, Enter confirms,
    /// Enter saves. The
    /// in-memory `pending.claude` and `pending.env` reflect the change.
    #[test]
    fn auth_form_save_persists_workspace_layer_into_pending() {
        let (cfg, mut state) = build_state();
        // Open form (Enter) on row 0 → workspace × Claude.
        let ManagerStage::Editor(editor) = &mut state.stage else {
            panic!()
        };
        open_auth_form_modal(editor, &cfg);
        assert!(matches!(editor.modal, Some(Modal::AuthForm { .. })));

        // Cycle mode: None → first available (sync) → ApiKey is two cycles.
        drive_key(editor, key(KeyCode::Char(' ')));
        drive_key(editor, key(KeyCode::Char(' ')));
        // Tab advances to credential row, then Enter opens the source picker.
        drive_key(editor, key(KeyCode::Tab));
        drive_key(editor, key(KeyCode::Enter));
        assert!(matches!(editor.modal, Some(Modal::AuthSourcePicker { .. })));
        apply_plain_source_picker_to_auth_form(editor);
        assert!(matches!(editor.modal, Some(Modal::TextInput { .. })));
        apply_plain_text_to_auth_form(editor, "secret");
        // Enter → save.
        let closed = drive_key(editor, key(KeyCode::Enter));
        assert!(closed, "save must close the modal");
        assert!(editor.modal.is_none(), "modal should be gone");

        // pending.claude reflects ApiKey.
        let claude_cfg = editor
            .pending
            .claude
            .as_ref()
            .expect("workspace claude block must be set");
        assert_eq!(claude_cfg.auth_forward, AuthForwardMode::ApiKey);
        // pending.env carries the credential.
        let value = editor
            .pending
            .env
            .get("ANTHROPIC_API_KEY")
            .expect("credential env var must be set");
        match value {
            EnvValue::Plain(s) => assert_eq!(s, "secret"),
            EnvValue::OpRef(_) => panic!("expected plain literal credential"),
        }
    }

    /// Reset action clears the layer's mode without touching any
    /// credential env var. Confirms that the Reset button on the form
    /// produces the "drop down to inherited" behavior.
    #[test]
    fn auth_form_reset_clears_workspace_layer_mode() {
        let (cfg, mut state) = build_state();
        let ManagerStage::Editor(editor) = &mut state.stage else {
            panic!()
        };
        editor.pending.claude = Some(AgentAuthConfig {
            auth_forward: AuthForwardMode::ApiKey,
        });
        open_auth_form_modal(editor, &cfg);
        // Tab through to Reset and Enter.
        // From Mode → Down → Cred → Down → Save → Tab → Cancel → Tab → Reset.
        drive_key(editor, key(KeyCode::Down)); // Mode → CredentialSource
        drive_key(editor, key(KeyCode::Down)); // → Save
        drive_key(editor, key(KeyCode::Tab)); // → Cancel
        drive_key(editor, key(KeyCode::Tab)); // → Reset
        let closed = drive_key(editor, key(KeyCode::Enter));
        assert!(closed, "reset must close the modal");
        assert!(
            editor.pending.claude.is_none(),
            "reset must clear workspace claude block"
        );
    }

    /// Cancel doesn't persist anything to pending: the workspace layer
    /// stays untouched.
    #[test]
    fn auth_form_cancel_does_not_mutate_pending() {
        let (cfg, mut state) = build_state();
        let ManagerStage::Editor(editor) = &mut state.stage else {
            panic!()
        };
        open_auth_form_modal(editor, &cfg);
        drive_key(editor, key(KeyCode::Char(' '))); // cycle to sync
        // Esc cancels at any focus.
        let closed = drive_key(editor, key(KeyCode::Esc));
        assert!(closed);
        assert!(
            editor.pending.claude.is_none(),
            "cancel must not write to pending"
        );
    }

    #[test]
    fn auth_form_enter_on_mode_does_not_navigate_tab_does() {
        let (cfg, mut state) = build_state();
        let ManagerStage::Editor(editor) = &mut state.stage else {
            panic!()
        };
        open_auth_form_modal(editor, &cfg);
        drive_key(editor, key(KeyCode::Char(' ')));
        drive_key(editor, key(KeyCode::Char(' ')));

        drive_key(editor, key(KeyCode::Enter));
        let Some(Modal::AuthForm { focus, .. }) = &editor.modal else {
            panic!("auth form must still be open")
        };
        assert_eq!(
            *focus,
            AuthFormFocus::Mode,
            "Enter on mode must not move to the next actionable row"
        );

        drive_key(editor, key(KeyCode::Tab));
        let Some(Modal::AuthForm { focus, .. }) = &editor.modal else {
            panic!("auth form must still be open")
        };
        assert_eq!(
            *focus,
            AuthFormFocus::CredentialSource,
            "Tab on mode must move to the credential row"
        );
    }

    /// Tab from the last focusable control wraps back to the first.
    /// Mirrors the convention used by every other modal in the TUI.
    #[test]
    fn auth_form_tab_wraps_around_at_reset() {
        let (cfg, mut state) = build_state();
        let ManagerStage::Editor(editor) = &mut state.stage else {
            panic!()
        };
        open_auth_form_modal(editor, &cfg);
        // Walk to Reset (last focusable):
        // Mode → Tab → Save → Tab → Cancel → Tab → Reset.
        drive_key(editor, key(KeyCode::Tab));
        drive_key(editor, key(KeyCode::Tab));
        drive_key(editor, key(KeyCode::Tab));
        let Some(Modal::AuthForm { focus, .. }) = &editor.modal else {
            panic!("auth form must still be open")
        };
        assert_eq!(*focus, AuthFormFocus::Reset);

        // Tab from Reset wraps to Mode.
        drive_key(editor, key(KeyCode::Tab));
        let Some(Modal::AuthForm { focus, .. }) = &editor.modal else {
            panic!("auth form must still be open")
        };
        assert_eq!(
            *focus,
            AuthFormFocus::Mode,
            "Tab on Reset must wrap to Mode"
        );

        // BackTab from Mode wraps to Reset (last).
        drive_key(editor, key(KeyCode::BackTab));
        let Some(Modal::AuthForm { focus, .. }) = &editor.modal else {
            panic!("auth form must still be open")
        };
        assert_eq!(
            *focus,
            AuthFormFocus::Reset,
            "BackTab on Mode must wrap to Reset"
        );
    }

    #[test]
    fn auth_form_typing_on_credential_row_does_not_set_plain_text() {
        let (cfg, mut state) = build_state();
        let ManagerStage::Editor(editor) = &mut state.stage else {
            panic!()
        };
        open_auth_form_modal(editor, &cfg);
        drive_key(editor, key(KeyCode::Char(' ')));
        drive_key(editor, key(KeyCode::Char(' ')));
        drive_key(editor, key(KeyCode::Tab));
        drive_key(editor, key(KeyCode::Char('x')));

        let Some(Modal::AuthForm { state, focus, .. }) = &editor.modal else {
            panic!("auth form must still be open")
        };
        assert_eq!(*focus, AuthFormFocus::CredentialSource);
        assert_eq!(
            state.credential,
            CredentialInput::None,
            "typing on credential row must not bypass the source picker"
        );
    }

    /// Picking the role × kind row mounts the form against the
    /// override layer. Save persists the mode under
    /// `pending.roles[role].claude` and the env var under
    /// `pending.roles[role].env`.
    #[test]
    fn auth_form_save_persists_role_layer_into_pending() {
        let (cfg, mut state) = build_state();
        let ManagerStage::Editor(editor) = &mut state.stage else {
            panic!()
        };
        // Insert a Claude override entry for "smith" so it materialises
        // in the focused Claude auth view.
        editor.pending.roles.insert(
            "smith".into(),
            WorkspaceRoleOverride {
                claude: Some(crate::config::AgentAuthConfig {
                    auth_forward: AuthForwardMode::Sync,
                }),
                ..Default::default()
            },
        );
        // Expand the role so the RoleMode child is emitted.
        editor.auth_expanded.insert("smith".into());
        // Dynamically locate the smith × Claude kind row.
        let smith_claude_idx = auth_flat_rows(editor, &cfg)
            .iter()
            .position(|r| {
                matches!(
                    r,
                    AuthRow::RoleMode {
                        role,
                        kind: AuthKind::Claude,
                    } if role == "smith"
                )
            })
            .expect("RoleMode smith × Claude must exist after override insertion");
        editor.active_field = FieldFocus::Row(smith_claude_idx);
        open_auth_form_modal(editor, &cfg);
        let Some(Modal::AuthForm { target, .. }) = &editor.modal else {
            panic!("form must be open");
        };
        assert_eq!(
            target,
            &AuthFormTarget::WorkspaceRole {
                role: "smith".into(),
                kind: AuthKind::Claude,
            }
        );

        // Cycle sync to api_key, choose plain credential, type, tab to save, enter.
        drive_key(editor, key(KeyCode::Char(' ')));
        drive_key(editor, key(KeyCode::Tab));
        drive_key(editor, key(KeyCode::Enter));
        assert!(matches!(editor.modal, Some(Modal::AuthSourcePicker { .. })));
        apply_plain_source_picker_to_auth_form(editor);
        assert!(matches!(editor.modal, Some(Modal::TextInput { .. })));
        apply_plain_text_to_auth_form(editor, "abc");
        let closed = drive_key(editor, key(KeyCode::Enter));
        assert!(closed);

        let role_entry = editor
            .pending
            .roles
            .get("smith")
            .expect("role override must exist");
        let cfg = role_entry
            .claude
            .as_ref()
            .expect("role override claude must be set");
        assert_eq!(cfg.auth_forward, AuthForwardMode::ApiKey);
        let env_val = role_entry
            .env
            .get("ANTHROPIC_API_KEY")
            .expect("role env credential must be set");
        match env_val {
            EnvValue::Plain(s) => assert_eq!(s, "abc"),
            EnvValue::OpRef(_) => panic!("expected plain literal"),
        }
    }

    /// Choosing 1Password from the credential source picker swaps the
    /// auth-form modal for an `OpPicker` and stashes the form context in
    /// `pending_auth_form_return`. Confirms the open path of the picker
    /// round-trip wiring.
    #[test]
    fn auth_form_op_ref_picker_invocation_opens_op_picker_modal() {
        let (cfg, mut state) = build_state();
        let ManagerStage::Editor(editor) = &mut state.stage else {
            panic!()
        };
        open_auth_form_modal(editor, &cfg);
        // Mode → ApiKey (two cycles past `None → sync`).
        drive_key(editor, key(KeyCode::Char(' ')));
        drive_key(editor, key(KeyCode::Char(' ')));
        // Tab advances to the credential row, then Enter opens the source picker.
        drive_key(editor, key(KeyCode::Tab));
        let closed = drive_key(editor, key(KeyCode::Enter));
        assert!(closed, "opening source picker must close auth form");
        assert!(matches!(editor.modal, Some(Modal::AuthSourcePicker { .. })));
        open_op_picker_from_auth_source(editor, fresh_op_cache());
        assert!(
            matches!(editor.modal, Some(Modal::OpPicker { .. })),
            "auth form must hand off to OpPicker from the source picker"
        );
        assert!(
            editor.pending_auth_form_return.is_some(),
            "auth-form context must be stashed for the picker to return to"
        );
    }

    /// Simulating a successful `OpPicker` commit re-mounts the auth
    /// form with the picked `OpRef` applied. `can_save` flips to true
    /// because the form now carries a valid `OpRef` and a committed
    /// mode. Uses an injected fake `OpRunner` so the test never
    /// shells out to the real `op` binary.
    #[test]
    fn auth_form_op_ref_picker_commit_applies_to_form() {
        struct StubRunner;
        impl OpRunner for StubRunner {
            fn read(&self, _r: &str) -> anyhow::Result<String> {
                Ok("sk-ant-from-vault".into())
            }
        }

        let (cfg, mut state) = build_state();
        let ManagerStage::Editor(editor) = &mut state.stage else {
            panic!()
        };
        // Open the auth form on workspace × Claude and choose
        // 1Password from the source picker.
        open_auth_form_modal(editor, &cfg);
        drive_key(editor, key(KeyCode::Char(' ')));
        drive_key(editor, key(KeyCode::Char(' ')));
        drive_key(editor, key(KeyCode::Tab));
        drive_key(editor, key(KeyCode::Enter));
        open_op_picker_from_auth_source(editor, fresh_op_cache());
        assert!(matches!(editor.modal, Some(Modal::OpPicker { .. })));

        // Simulate the picker committing a valid OpRef. Bypass the
        // production `OpCli` by calling the runner-injecting helper
        // directly — same code path the editor.rs handler invokes,
        // just with a stub runner.
        let picked = OpRef {
            op: "op://uuid/anthropic-vault".into(),
            path: "Work/Anthropic/api-key".into(),
        };
        super::apply_op_picker_to_auth_form_with_runner(editor, picked.clone(), &StubRunner);

        // Form is back; the credential carries the picked OpRef and
        // can_save must be true (mode + non-empty OpRef both set).
        let Some(Modal::AuthForm { state, focus, .. }) = &editor.modal else {
            panic!("auth form must be re-mounted after picker commit");
        };
        assert_eq!(
            *focus,
            AuthFormFocus::Save,
            "successful picker commit drops cursor onto Save"
        );
        match &state.credential {
            CredentialInput::OpRef(r) => assert_eq!(r, &picked),
            other => panic!("expected OpRef credential after picker commit; got {other:?}"),
        }
        assert!(
            state.can_save(),
            "form must be commitable after picker supplies a non-empty OpRef"
        );
        assert!(
            editor.pending_auth_form_return.is_none(),
            "stash must be drained on commit"
        );
    }

    /// A failed vault read (e.g. biometric timeout) must NOT corrupt
    /// the form's credential — `try_commit_op_ref` only mutates state
    /// on Ok. The form is re-stashed into `pending_auth_form_return`
    /// and `Modal::ErrorPopup` is mounted; dismissing the popup must
    /// restore the form with the prior credential intact.
    #[test]
    fn auth_form_op_ref_picker_failed_read_does_not_apply_op_ref() {
        struct FailRunner;
        impl OpRunner for FailRunner {
            fn read(&self, _r: &str) -> anyhow::Result<String> {
                Err(anyhow::anyhow!("biometric prompt timed out"))
            }
        }

        let (cfg, mut state) = build_state();
        let ManagerStage::Editor(editor) = &mut state.stage else {
            panic!()
        };
        open_auth_form_modal(editor, &cfg);
        drive_key(editor, key(KeyCode::Char(' ')));
        drive_key(editor, key(KeyCode::Char(' ')));
        drive_key(editor, key(KeyCode::Tab));
        drive_key(editor, key(KeyCode::Enter));
        open_op_picker_from_auth_source(editor, fresh_op_cache());

        let picked = OpRef {
            op: "op://uuid/missing".into(),
            path: "Vault/Missing/field".into(),
        };
        super::apply_op_picker_to_auth_form_with_runner(editor, picked, &FailRunner);

        // ErrorPopup mounted; form re-stashed for the popup dismissal
        // path to re-open via restore_auth_form_after_op_picker_cancel.
        assert!(
            matches!(editor.modal, Some(Modal::ErrorPopup { .. })),
            "failed vault read must surface an error popup"
        );
        assert!(
            editor.pending_auth_form_return.is_some(),
            "form must be re-stashed so popup dismiss can restore it"
        );

        // Simulate ErrorPopup dismiss → form restored.
        restore_auth_form_after_op_picker_cancel(editor);
        let Some(Modal::AuthForm { state, .. }) = &editor.modal else {
            panic!("popup dismiss must restore the auth form");
        };
        assert!(
            !matches!(state.credential, CredentialInput::OpRef(ref r) if r.path == "Vault/Missing/field"),
            "failed OpRef must not land in form credential"
        );
    }

    /// Esc on an open auth form must drain `pending_auth_form_return`
    /// alongside dismissing the modal — leaving it set would let a
    /// later `OpPicker` open from the Secrets tab silently inherit a
    /// stale auth-form context. Defensive cleanup against future
    /// picker flows.
    #[test]
    fn auth_form_esc_clears_pending_auth_form_return() {
        use crate::console::manager::state::AuthFormReturnPath;

        let (cfg, mut state) = build_state();
        let ManagerStage::Editor(editor) = &mut state.stage else {
            panic!()
        };
        // Stash a return path manually as if a picker handoff was in
        // flight. (Reaching this state via the public API is hard
        // because the picker swap takes the modal — the defensive
        // cleanup is for reentrancy / partial-flow bugs we don't want
        // to leak through Esc.)
        editor.pending_auth_form_return = Some(AuthFormReturnPath {
            target: AuthFormTarget::Workspace {
                kind: AuthKind::Claude,
            },
            state: Box::new(AuthForm::new(AuthKind::Claude)),
            focus: AuthFormFocus::Mode,
            literal_buffer: String::new(),
        });
        // Open the auth form modal so handle_auth_form_key can be
        // entered.
        open_auth_form_modal(editor, &cfg);
        assert!(matches!(editor.modal, Some(Modal::AuthForm { .. })));

        let closed = drive_key(editor, key(KeyCode::Esc));
        assert!(closed, "Esc must close the auth form");
        assert!(editor.modal.is_none(), "modal must be dropped");
        assert!(
            editor.pending_auth_form_return.is_none(),
            "Esc must drain pending_auth_form_return so future picker flows \
             don't inherit stale stash state"
        );
    }

    /// `Enter` on the Save focus when `can_save` is false (e.g. an
    /// `OpRef` credential with empty `op` and `path`) must NOT dismiss
    /// the modal NOR mutate `editor.pending`. `can_save` rejects empty
    /// `OpRef`s; this test pins that the input layer honours the guard
    /// rather than ignoring it.
    #[test]
    fn auth_form_save_disabled_blocks_enter() {
        let (cfg, mut state) = build_state();
        let ManagerStage::Editor(editor) = &mut state.stage else {
            panic!()
        };
        let pending_before = editor.pending.clone();
        open_auth_form_modal(editor, &cfg);
        // Build a form state with mode = ApiKey and credential = empty
        // OpRef so `can_save` returns false.
        drive_key(editor, key(KeyCode::Char(' ')));
        drive_key(editor, key(KeyCode::Char(' ')));
        if let Some(Modal::AuthForm { state, .. }) = editor.modal.as_mut() {
            state.credential = CredentialInput::OpRef(OpRef {
                op: String::new(),
                path: String::new(),
            });
        } else {
            panic!("auth form must still be open");
        }

        // Confirm the form's credential is the empty OpRef and can_save
        // is false.
        let Some(Modal::AuthForm { state, .. }) = &editor.modal else {
            panic!("auth form must still be open");
        };
        match &state.credential {
            CredentialInput::OpRef(r) => {
                assert!(
                    r.op.is_empty() && r.path.is_empty(),
                    "expected empty OpRef as setup; got {r:?}"
                );
            }
            other => panic!("expected OpRef credential after toggle; got {other:?}"),
        }
        assert!(
            !state.can_save(),
            "form must NOT be save-able with mode set + empty OpRef"
        );

        // Move focus directly to Save and press Enter. The handler
        // must short-circuit on `!can_save()` and leave the modal
        // open + pending untouched.
        if let Some(Modal::AuthForm { focus, .. }) = editor.modal.as_mut() {
            *focus = AuthFormFocus::Save;
        } else {
            panic!("auth form must still be open");
        }
        let closed = drive_key(editor, key(KeyCode::Enter));
        assert!(
            !closed,
            "Enter on Save with !can_save must NOT close the modal"
        );
        assert!(
            matches!(editor.modal, Some(Modal::AuthForm { .. })),
            "modal must remain on AuthForm; got {:?}",
            editor.modal
        );
        assert_eq!(
            editor.pending, pending_before,
            "Enter on Save with !can_save must NOT mutate editor.pending"
        );
    }

    /// Saving the GitHub form on the workspace layer with `token` mode
    /// plus a literal `GH_TOKEN` writes the workspace `[github]` block
    /// AND lands the credential under `[workspaces.<ws>.github.env]`
    /// (NOT the regular `[workspaces.<ws>.env]` block — that would
    /// leak `GH_TOKEN` into the operator-env layer launch resolves
    /// through, while the github-specific layer is what
    /// `build_github_env_layers` reads).
    #[test]
    fn github_form_save_persists_workspace_layer_into_pending() {
        let (cfg, mut state) = build_github_state();
        let ManagerStage::Editor(editor) = &mut state.stage else {
            panic!()
        };
        open_auth_form_modal(editor, &cfg);
        assert!(matches!(editor.modal, Some(Modal::AuthForm { .. })));
        // available_modes for Github is [sync, token, ignore].
        // Cycle: None → sync → token (two presses).
        drive_key(editor, key(KeyCode::Char(' ')));
        drive_key(editor, key(KeyCode::Char(' ')));
        // Token requires GH_TOKEN — Tab to credential, Enter, then
        // pick literal source and type a token.
        drive_key(editor, key(KeyCode::Tab));
        drive_key(editor, key(KeyCode::Enter));
        assert!(matches!(editor.modal, Some(Modal::AuthSourcePicker { .. })));
        apply_plain_source_picker_to_auth_form(editor);
        apply_plain_text_to_auth_form(editor, "ghp_xxx");
        let closed = drive_key(editor, key(KeyCode::Enter));
        assert!(closed, "save must close the modal");
        let github_block = editor
            .pending
            .github
            .as_ref()
            .expect("workspace github block must be set");
        assert_eq!(github_block.auth_forward, GithubAuthMode::Token);
        let value = github_block
            .env
            .get("GH_TOKEN")
            .expect("GH_TOKEN must land on the github env block, not the regular env block");
        match value {
            EnvValue::Plain(s) => assert_eq!(s, "ghp_xxx"),
            EnvValue::OpRef(_) => panic!("expected plain literal credential"),
        }
        // GH_TOKEN must NOT have leaked into the regular workspace env
        // map — that would shadow the kind-scoped value at launch
        // resolution and bypass `build_github_env_layers`.
        assert!(
            !editor.pending.env.contains_key("GH_TOKEN"),
            "GH_TOKEN must not land in the regular workspace env map"
        );
    }

    /// `D` on a Github `RoleHeader` clears the role's
    /// `[workspaces.<ws>.roles.<role>.github]` override.
    #[test]
    fn d_on_github_role_header_clears_role_override() {
        let (cfg, mut state) = build_github_state();
        let ManagerStage::Editor(editor) = &mut state.stage else {
            panic!()
        };
        // Seed a role override on smith × Github.
        editor.pending.roles.insert(
            "smith".into(),
            WorkspaceRoleOverride {
                github: Some(GithubAuthConfig {
                    auth_forward: GithubAuthMode::Ignore,
                    ..Default::default()
                }),
                ..Default::default()
            },
        );
        // Locate the RoleHeader and put the cursor on it.
        let header_idx = auth_flat_rows(editor, &cfg)
            .iter()
            .position(|r| matches!(r, AuthRow::RoleHeader { role, .. } if role == "smith"))
            .expect("smith RoleHeader must exist after override insertion");
        editor.active_field = FieldFocus::Row(header_idx);
        handle_d_on_auth_row(editor, &cfg);
        let smith = editor
            .pending
            .roles
            .get("smith")
            .expect("override entry must remain");
        assert!(
            smith.github.is_none(),
            "D on github RoleHeader must clear the role's github override"
        );
    }

    /// `D` on a Github workspace mode row clears
    /// `[workspaces.<ws>.github]` so resolution falls back to the
    /// global `[github]` default.
    #[test]
    fn d_on_github_workspace_mode_row_clears_workspace_block() {
        let (cfg, mut state) = build_github_state();
        let ManagerStage::Editor(editor) = &mut state.stage else {
            panic!()
        };
        editor.pending.github = Some(GithubAuthConfig {
            auth_forward: GithubAuthMode::Token,
            ..Default::default()
        });
        let ws_github_idx = workspace_github_row_idx(editor, &cfg);
        editor.active_field = FieldFocus::Row(ws_github_idx);
        handle_d_on_auth_row(editor, &cfg);
        assert!(
            editor.pending.github.is_none(),
            "D on github WorkspaceMode must clear [workspaces.<ws>.github]"
        );
    }

    /// Round-trip: save a workspace `[github]` block with `token`
    /// plus `GH_TOKEN`, build a fresh editor over the resulting
    /// `WorkspaceConfig`, and confirm `auth_flat_rows` re-renders the
    /// saved values (mode → token, `GH_TOKEN` visible) without any
    /// extra operator interaction.
    #[test]
    fn github_form_save_round_trip_renders_persisted_values() {
        let (cfg, mut state) = build_github_state();
        let ManagerStage::Editor(editor) = &mut state.stage else {
            panic!()
        };
        open_auth_form_modal(editor, &cfg);
        drive_key(editor, key(KeyCode::Char(' '))); // None → sync
        drive_key(editor, key(KeyCode::Char(' '))); // sync → token
        drive_key(editor, key(KeyCode::Tab));
        drive_key(editor, key(KeyCode::Enter));
        apply_plain_source_picker_to_auth_form(editor);
        apply_plain_text_to_auth_form(editor, "ghp_round_trip");
        drive_key(editor, key(KeyCode::Enter));

        // Pull the persisted workspace and remount the editor from it
        // — this is the same shape `from_config` materialises after a
        // disk reload.
        let saved_ws = editor.pending.clone();
        let mut reloaded = EditorState::new_edit("proj".into(), saved_ws);
        reloaded.active_tab = crate::console::manager::state::EditorTab::Auth;
        reloaded.auth_selected_kind = Some(AuthKind::Github);
        let rows = auth_flat_rows(&reloaded, &cfg);
        // WorkspaceMode + WorkspaceSource (token requires GH_TOKEN).
        assert!(
            rows.iter().any(|r| matches!(
                r,
                AuthRow::WorkspaceMode {
                    kind: AuthKind::Github
                }
            )),
            "reload must surface WorkspaceMode for Github; got {rows:?}"
        );
        assert!(
            rows.iter().any(|r| matches!(
                r,
                AuthRow::WorkspaceSource {
                    kind: AuthKind::Github
                }
            )),
            "reload must surface WorkspaceSource for Github (token mode requires GH_TOKEN); got {rows:?}"
        );
        let github_block = reloaded.pending.github.expect("github block must persist");
        assert_eq!(github_block.auth_forward, GithubAuthMode::Token);
        match github_block
            .env
            .get("GH_TOKEN")
            .expect("GH_TOKEN must persist on the github env block")
        {
            EnvValue::Plain(s) => assert_eq!(s, "ghp_round_trip"),
            EnvValue::OpRef(_) => panic!("expected plain literal"),
        }
    }

    /// The role-override picker filters out any role that already has
    /// a `[workspaces.<ws>.roles.<role>.github]` override — same "no
    /// duplicate override" rule the Claude / Codex picker applies for
    /// their respective kinds.
    #[test]
    fn github_role_override_picker_filters_already_overridden_roles() {
        let mut cfg = AppConfig::default();
        let mut ws = WorkspaceConfig {
            workdir: "/code/proj".into(),
            mounts: vec![MountConfig {
                src: "/code/proj".into(),
                dst: "/code/proj".into(),
                readonly: false,
                isolation: crate::isolation::MountIsolation::Shared,
            }],
            allowed_roles: vec!["smith".into(), "brown".into()],
            ..Default::default()
        };
        ws.allowed_roles.sort();
        // Pre-seed an override on "brown" × Github so the picker
        // should filter it out and only offer "smith".
        ws.roles.insert(
            "brown".into(),
            WorkspaceRoleOverride {
                github: Some(GithubAuthConfig {
                    auth_forward: GithubAuthMode::Ignore,
                    ..Default::default()
                }),
                ..Default::default()
            },
        );
        cfg.workspaces.insert("proj".into(), ws.clone());
        for r in ["smith", "brown"] {
            cfg.roles.insert(
                r.into(),
                crate::config::RoleSource {
                    git: format!("https://example.com/{r}.git"),
                    trusted: true,
                    env: std::collections::BTreeMap::default(),
                },
            );
        }
        let cwd = std::path::PathBuf::from("/tmp");
        let mut state = ManagerState::from_config(&cfg, &cwd);
        let mut editor = EditorState::new_edit("proj".into(), ws);
        editor.active_tab = crate::console::manager::state::EditorTab::Auth;
        editor.auth_selected_kind = Some(AuthKind::Github);
        state.stage = ManagerStage::Editor(editor);
        let ManagerStage::Editor(editor) = &mut state.stage else {
            panic!()
        };
        open_auth_role_picker(editor, &cfg);
        let Some(Modal::AuthRolePicker { state: picker }) = editor.modal.as_ref() else {
            panic!("AuthRolePicker must be open; got {:?}", editor.modal);
        };
        // The picker exposes its candidate list as the `roles` field —
        // pull the keys and assert "brown" was filtered out before the
        // picker was even seeded.
        let labels: Vec<String> = picker
            .roles
            .iter()
            .map(crate::selector::RoleSelector::key)
            .collect();
        assert!(
            labels.iter().any(|s| s == "smith"),
            "smith must remain a candidate; got {labels:?}"
        );
        assert!(
            !labels.iter().any(|s| s == "brown"),
            "brown already has a github override and must be filtered out; got {labels:?}"
        );
    }
}
