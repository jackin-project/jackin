//! Auth-tab input handling: open the form modal, route keystrokes to
//! the form, and persist commits back to `editor.pending`.
//!
//! Mirrors the Secrets tab's pattern of "form mutates `editor.pending`
//! in memory; the editor's existing save flow (`edit_workspace`)
//! serializes the whole `WorkspaceConfig` block back to disk on save".

use crossterm::event::{KeyCode, KeyEvent};

use super::super::super::widgets::auth_panel::{AuthForm, CredentialInput};
use super::super::render::editor::resolve_auth_row_target;
use super::super::state::{AuthFormFocus, AuthFormTarget, EditorState, FieldFocus, Modal};
use crate::agent::Agent;
use crate::config::{AgentAuthConfig, AuthForwardMode, CodexAuthConfig};
use crate::operator_env::EnvValue;
use crate::workspace::WorkspaceRoleOverride;

/// Open the auth-edit form modal for the row currently under the
/// cursor on the Auth tab. Pre-populates the form from the row's
/// effective mode + credential so editing an existing entry shows
/// what's there.
pub(super) fn open_auth_form_modal(editor: &mut EditorState<'_>) {
    let FieldFocus::Row(n) = editor.active_field;
    let Some(target) = resolve_auth_row_target(editor, n) else {
        return;
    };
    let agent = match &target {
        AuthFormTarget::Workspace { agent } | AuthFormTarget::WorkspaceRole { agent, .. } => *agent,
    };
    let (existing_mode, existing_cred) = current_mode_and_credential(editor, &target);
    let form = existing_mode.map_or_else(
        || AuthForm::new(agent),
        |mode| AuthForm::from_existing(agent, mode, existing_cred),
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

/// Read the current mode + credential for the form's target out of
/// `editor.pending`. Returns `(None, _)` when the layer has no explicit
/// mode set yet — the form opens with the mode picker unset.
fn current_mode_and_credential(
    editor: &EditorState<'_>,
    target: &AuthFormTarget,
) -> (Option<AuthForwardMode>, Option<EnvValue>) {
    match target {
        AuthFormTarget::Workspace { agent } => {
            let mode = match agent {
                Agent::Claude => editor.pending.claude.as_ref().map(|c| c.auth_forward),
                Agent::Codex => editor.pending.codex.as_ref().map(|c| c.0.auth_forward),
            };
            let env_var = mode.and_then(|m| agent.required_env_var(m));
            let cred = env_var.and_then(|v| editor.pending.env.get(v).cloned());
            (mode, cred)
        }
        AuthFormTarget::WorkspaceRole { role, agent } => {
            let override_ref = editor.pending.roles.get(role);
            let mode = override_ref.and_then(|ro| match agent {
                Agent::Claude => ro.claude.as_ref().map(|c| c.auth_forward),
                Agent::Codex => ro.codex.as_ref().map(|c| c.0.auth_forward),
            });
            let env_var = mode.and_then(|m| agent.required_env_var(m));
            let cred = env_var.and_then(|v| override_ref.and_then(|ro| ro.env.get(v).cloned()));
            (mode, cred)
        }
    }
}

/// Drive a single keystroke into an open `Modal::AuthForm`. Returns
/// `true` when the modal was closed (committed or cancelled).
pub(super) fn handle_auth_form_key(editor: &mut EditorState<'_>, key: KeyEvent) -> bool {
    let Some(Modal::AuthForm {
        state,
        focus,
        literal_buffer,
        ..
    }) = editor.modal.as_mut()
    else {
        return false;
    };

    // Esc cancels at every focus.
    if key.code == KeyCode::Esc {
        editor.modal = None;
        return true;
    }

    match *focus {
        AuthFormFocus::Mode => {
            handle_mode_key(focus, state.as_mut(), key);
        }
        AuthFormFocus::CredentialSource => {
            handle_credential_source_key(focus, state.as_mut(), literal_buffer.as_str(), key);
        }
        AuthFormFocus::LiteralValue => {
            handle_literal_value_key(focus, state.as_mut(), literal_buffer, key);
        }
        AuthFormFocus::OpRefValue => match key.code {
            KeyCode::Down | KeyCode::Tab => *focus = AuthFormFocus::Save,
            KeyCode::Up => *focus = AuthFormFocus::CredentialSource,
            _ => {}
        },
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

fn handle_mode_key(focus: &mut AuthFormFocus, form: &mut AuthForm, key: KeyEvent) {
    match key.code {
        KeyCode::Tab | KeyCode::Char(' ') => cycle_mode(form),
        KeyCode::Down | KeyCode::Char('j') => *focus = next_focus_after_mode(form),
        KeyCode::Enter => {
            *focus = if form.shows_credential_block() {
                AuthFormFocus::CredentialSource
            } else {
                AuthFormFocus::Save
            };
        }
        _ => {}
    }
}

fn handle_credential_source_key(
    focus: &mut AuthFormFocus,
    form: &mut AuthForm,
    literal_buffer: &str,
    key: KeyEvent,
) {
    match key.code {
        KeyCode::Char(' ') => toggle_credential_source(form, literal_buffer),
        KeyCode::Down | KeyCode::Char('j') | KeyCode::Enter => {
            *focus = focus_for_credential_value(form);
        }
        KeyCode::Up | KeyCode::Char('k') => *focus = AuthFormFocus::Mode,
        _ => {}
    }
}

fn handle_literal_value_key(
    focus: &mut AuthFormFocus,
    form: &mut AuthForm,
    literal_buffer: &mut String,
    key: KeyEvent,
) {
    match key.code {
        KeyCode::Char(c) => {
            literal_buffer.push(c);
            form.set_literal(literal_buffer.clone());
        }
        KeyCode::Backspace => {
            literal_buffer.pop();
            form.set_literal(literal_buffer.clone());
        }
        KeyCode::Down | KeyCode::Tab | KeyCode::Enter => *focus = AuthFormFocus::Save,
        KeyCode::Up => *focus = AuthFormFocus::CredentialSource,
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
        KeyCode::Up => {
            *focus = if state.shows_credential_block() {
                focus_for_credential_value(state.as_ref())
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
            let agent = state.agent;
            let form = std::mem::replace(state.as_mut(), AuthForm::new(agent));
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
        KeyCode::Left => {
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
        KeyCode::Left => {
            *focus = AuthFormFocus::Cancel;
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

const fn focus_for_credential_value(form: &AuthForm) -> AuthFormFocus {
    if matches!(form.credential, CredentialInput::OpRef(_)) {
        AuthFormFocus::OpRefValue
    } else {
        AuthFormFocus::LiteralValue
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

fn toggle_credential_source(form: &mut AuthForm, literal_buffer: &str) {
    use crate::operator_env::OpRef;
    let next = match &form.credential {
        CredentialInput::OpRef(_) => CredentialInput::Literal(literal_buffer.to_string()),
        CredentialInput::None | CredentialInput::Literal(_) => CredentialInput::OpRef(OpRef {
            op: String::new(),
            path: String::new(),
        }),
    };
    form.credential = next;
}

/// Apply a successful form commit to `editor.pending`. Writes both the
/// agent block (`auth_forward`) AND the credential env var when the
/// form's outcome includes one.
fn persist_form(editor: &mut EditorState<'_>, target: &AuthFormTarget, form: &AuthForm) {
    let Some(outcome) = form.commit() else {
        return;
    };
    match target {
        AuthFormTarget::Workspace { agent } => {
            set_workspace_mode(&mut editor.pending, *agent, Some(outcome.mode));
            if let (Some(name), Some(value)) = (outcome.env_var_name, outcome.env_value.clone()) {
                editor.pending.env.insert(name.to_string(), value);
            }
        }
        AuthFormTarget::WorkspaceRole { role, agent } => {
            let entry = editor.pending.roles.entry(role.clone()).or_default();
            set_role_mode(entry, *agent, Some(outcome.mode));
            if let (Some(name), Some(value)) = (outcome.env_var_name, outcome.env_value.clone()) {
                entry.env.insert(name.to_string(), value);
            }
        }
    }
}

/// Clear the `auth_forward` at the form's target layer. Does NOT touch
/// the credential env var — operators delete those via the Secrets tab
/// so the deletion is explicit.
fn clear_layer(editor: &mut EditorState<'_>, target: &AuthFormTarget) {
    match target {
        AuthFormTarget::Workspace { agent } => {
            set_workspace_mode(&mut editor.pending, *agent, None);
        }
        AuthFormTarget::WorkspaceRole { role, agent } => {
            if let Some(entry) = editor.pending.roles.get_mut(role) {
                set_role_mode(entry, *agent, None);
            }
        }
    }
}

fn set_workspace_mode(
    ws: &mut crate::workspace::WorkspaceConfig,
    agent: Agent,
    mode: Option<AuthForwardMode>,
) {
    match agent {
        Agent::Claude => {
            ws.claude = mode.map(|auth_forward| AgentAuthConfig { auth_forward });
        }
        Agent::Codex => {
            ws.codex = mode.map(|auth_forward| CodexAuthConfig(AgentAuthConfig { auth_forward }));
        }
    }
}

fn set_role_mode(entry: &mut WorkspaceRoleOverride, agent: Agent, mode: Option<AuthForwardMode>) {
    match agent {
        Agent::Claude => {
            entry.claude = mode.map(|auth_forward| AgentAuthConfig { auth_forward });
        }
        Agent::Codex => {
            entry.codex =
                mode.map(|auth_forward| CodexAuthConfig(AgentAuthConfig { auth_forward }));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::Agent;
    use crate::config::{AppConfig, AuthForwardMode};
    use crate::console::manager::state::{
        AuthFormTarget, EditorState, FieldFocus, ManagerStage, ManagerState,
    };
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
        editor.active_field = FieldFocus::Row(0);
        state.stage = ManagerStage::Editor(editor);
        (cfg, state)
    }

    /// Walking from the workspace × Claude row through the form:
    /// Enter opens form, Space cycles mode to `api_key`, Enter into
    /// credential, type literal, navigate to Save, Enter saves. The
    /// in-memory `pending.claude` and `pending.env` reflect the change.
    #[test]
    fn auth_form_save_persists_workspace_layer_into_pending() {
        let (_cfg, mut state) = build_state();
        // Open form (Enter) on row 0 → workspace × Claude.
        let ManagerStage::Editor(editor) = &mut state.stage else {
            panic!()
        };
        open_auth_form_modal(editor);
        assert!(matches!(editor.modal, Some(Modal::AuthForm { .. })));

        // Cycle mode: None → first available (sync) → ApiKey is two cycles.
        handle_auth_form_key(editor, key(KeyCode::Char(' ')));
        handle_auth_form_key(editor, key(KeyCode::Char(' ')));
        // Enter to advance to credential block.
        handle_auth_form_key(editor, key(KeyCode::Enter));
        // Enter on cred radio → into LiteralValue.
        handle_auth_form_key(editor, key(KeyCode::Enter));
        // Type "secret".
        for c in "secret".chars() {
            handle_auth_form_key(editor, key(KeyCode::Char(c)));
        }
        // Tab to Save.
        handle_auth_form_key(editor, key(KeyCode::Tab));
        // Enter → save.
        let closed = handle_auth_form_key(editor, key(KeyCode::Enter));
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
        let (_cfg, mut state) = build_state();
        let ManagerStage::Editor(editor) = &mut state.stage else {
            panic!()
        };
        editor.pending.claude = Some(AgentAuthConfig {
            auth_forward: AuthForwardMode::ApiKey,
        });
        open_auth_form_modal(editor);
        // Tab through to Reset and Enter.
        // From Mode → Down → Cred → Down → Literal → Tab → Save → Tab → Cancel → Tab → Reset.
        handle_auth_form_key(editor, key(KeyCode::Down)); // Mode → CredentialSource
        handle_auth_form_key(editor, key(KeyCode::Down)); // → LiteralValue
        handle_auth_form_key(editor, key(KeyCode::Tab)); // → Save
        handle_auth_form_key(editor, key(KeyCode::Tab)); // → Cancel
        handle_auth_form_key(editor, key(KeyCode::Tab)); // → Reset
        let closed = handle_auth_form_key(editor, key(KeyCode::Enter));
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
        let (_cfg, mut state) = build_state();
        let ManagerStage::Editor(editor) = &mut state.stage else {
            panic!()
        };
        open_auth_form_modal(editor);
        handle_auth_form_key(editor, key(KeyCode::Char(' '))); // cycle to sync
        // Esc cancels at any focus.
        let closed = handle_auth_form_key(editor, key(KeyCode::Esc));
        assert!(closed);
        assert!(
            editor.pending.claude.is_none(),
            "cancel must not write to pending"
        );
    }

    /// Picking the role × agent row mounts the form against the
    /// override layer. Save persists the mode under
    /// `pending.roles[role].claude` and the env var under
    /// `pending.roles[role].env`.
    #[test]
    fn auth_form_save_persists_role_layer_into_pending() {
        let (_cfg, mut state) = build_state();
        let ManagerStage::Editor(editor) = &mut state.stage else {
            panic!()
        };
        // Row 2 is workspace×role[0]×Claude (rows 0..2 are the workspace
        // layer × {Claude, Codex}; row 2 is the first role-agent row).
        editor.active_field = FieldFocus::Row(2);
        open_auth_form_modal(editor);
        let Some(Modal::AuthForm { target, .. }) = &editor.modal else {
            panic!("form must be open");
        };
        assert_eq!(
            target,
            &AuthFormTarget::WorkspaceRole {
                role: "smith".into(),
                agent: Agent::Claude,
            }
        );

        // Cycle to api_key, enter cred, type, tab to save, enter.
        handle_auth_form_key(editor, key(KeyCode::Char(' ')));
        handle_auth_form_key(editor, key(KeyCode::Char(' ')));
        handle_auth_form_key(editor, key(KeyCode::Enter));
        handle_auth_form_key(editor, key(KeyCode::Enter));
        for c in "abc".chars() {
            handle_auth_form_key(editor, key(KeyCode::Char(c)));
        }
        handle_auth_form_key(editor, key(KeyCode::Tab));
        let closed = handle_auth_form_key(editor, key(KeyCode::Enter));
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
}
