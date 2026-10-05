// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Tests for the editor-stage keymap dispatch resolver.

use super::dispatch_editor_top_level;
use crate::tui::screens::editor::model::{EditorNavigationKeyPlan, EditorTopLevelKeyPlan};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

fn key(code: KeyCode) -> KeyEvent {
    KeyEvent::new(code, KeyModifiers::NONE)
}

/// The editor top-level resolver composes three keymaps (global → tab-bar →
/// content) in precedence order. This asserts that composition end-to-end, so
/// the ordering logic in `dispatch_editor_top_level` cannot regress. Per-keymap
/// chord coverage lives in `tui::keymap::tests`.
#[test]
fn dispatch_editor_top_level_preserves_precedence() {
    // Global keymap wins regardless of focus.
    assert_eq!(
        dispatch_editor_top_level(key(KeyCode::Char('s')), false),
        EditorTopLevelKeyPlan::Save
    );
    assert_eq!(
        dispatch_editor_top_level(key(KeyCode::Esc), false),
        EditorTopLevelKeyPlan::Escape
    );

    // Tab-bar keymap applies only when the tab bar has focus.
    assert_eq!(
        dispatch_editor_top_level(key(KeyCode::Left), true),
        EditorTopLevelKeyPlan::Navigation(EditorNavigationKeyPlan::MoveTab {
            delta: -1,
            focus_tab_bar: true,
        })
    );
    assert_eq!(
        dispatch_editor_top_level(key(KeyCode::Right), true),
        EditorTopLevelKeyPlan::Navigation(EditorNavigationKeyPlan::MoveTab {
            delta: 1,
            focus_tab_bar: true,
        })
    );
    assert_eq!(
        dispatch_editor_top_level(key(KeyCode::Down), true),
        EditorTopLevelKeyPlan::Navigation(EditorNavigationKeyPlan::FocusContent)
    );

    // Content keymap (tab bar not focused, or keys that fall through).
    assert_eq!(
        dispatch_editor_top_level(key(KeyCode::BackTab), false),
        EditorTopLevelKeyPlan::Navigation(EditorNavigationKeyPlan::FocusTabBar)
    );
    assert_eq!(
        dispatch_editor_top_level(key(KeyCode::Char('h')), false),
        EditorTopLevelKeyPlan::ScrollHorizontal { delta: -8 }
    );
    assert_eq!(
        dispatch_editor_top_level(key(KeyCode::Char('L')), false),
        EditorTopLevelKeyPlan::ScrollHorizontal { delta: 8 }
    );
    assert_eq!(
        dispatch_editor_top_level(key(KeyCode::Char('k')), false),
        EditorTopLevelKeyPlan::MoveField { delta: -1 }
    );
    assert_eq!(
        dispatch_editor_top_level(key(KeyCode::Down), false),
        EditorTopLevelKeyPlan::MoveField { delta: 1 }
    );
    assert_eq!(
        dispatch_editor_top_level(key(KeyCode::Right), false),
        EditorTopLevelKeyPlan::SetRoleHeaderExpanded { expanded: true }
    );
    assert_eq!(
        dispatch_editor_top_level(key(KeyCode::Left), false),
        EditorTopLevelKeyPlan::SetRoleHeaderExpanded { expanded: false }
    );
    assert_eq!(
        dispatch_editor_top_level(key(KeyCode::Enter), false),
        EditorTopLevelKeyPlan::CheckImmediateAction
    );
    // A printable char that is no shortcut → immediate-action check.
    assert_eq!(
        dispatch_editor_top_level(key(KeyCode::Char('z')), false),
        EditorTopLevelKeyPlan::CheckImmediateAction
    );
    // A non-char, non-shortcut key → fall through to tab actions.
    assert_eq!(
        dispatch_editor_top_level(key(KeyCode::PageDown), false),
        EditorTopLevelKeyPlan::ContinueToTabActions
    );
}

#[test]
fn terminal_reverse_tab_dispatches_editor_focus_navigation() {
    let reverse_tab = KeyEvent::new(KeyCode::BackTab, KeyModifiers::SHIFT);
    assert_eq!(
        dispatch_editor_top_level(reverse_tab, false),
        EditorTopLevelKeyPlan::Navigation(EditorNavigationKeyPlan::FocusTabBar)
    );
    assert_eq!(
        dispatch_editor_top_level(reverse_tab, true),
        EditorTopLevelKeyPlan::Navigation(EditorNavigationKeyPlan::MoveTab {
            delta: -1,
            focus_tab_bar: true,
        })
    );
}

fn dispatch_modal(
    editor: &mut crate::tui::state::EditorState<'_>,
    code: KeyCode,
) -> super::EditorModalOutcome {
    let temp = tempfile::tempdir().unwrap();
    let paths = jackin_core::JackinPaths::for_tests(temp.path());
    super::handle_editor_modal(
        editor,
        key(code),
        false,
        std::rc::Rc::new(std::cell::RefCell::new(jackin_env::OpCache::default())),
        &mut jackin_config::AppConfig::default(),
        &paths,
        ratatui::layout::Rect::new(0, 0, 100, 30),
    )
}

fn dismiss_error_popup(editor: &mut crate::tui::state::EditorState<'_>, code: KeyCode) {
    assert!(matches!(
        dispatch_modal(editor, code),
        super::EditorModalOutcome::Continue
    ));
}

fn auth_form_parent() -> crate::tui::state::Modal<'static> {
    use crate::tui::state::{AuthForm, AuthFormFocus, AuthFormTarget, Modal};
    let mut form = AuthForm::new(crate::tui::auth::AuthKind::Claude);
    form.set_literal("prior-credential".into());
    Modal::AuthForm {
        target: AuthFormTarget::Workspace {
            kind: crate::tui::auth::AuthKind::Claude,
        },
        state: Box::new(form),
        focus: AuthFormFocus::CredentialSource,
        literal_buffer: "prior-buffer".into(),
    }
}

#[test]
fn failed_auth_credential_popup_returns_to_form_with_prior_credential() {
    use crate::tui::state::{AuthFormFocus, EditorState, Modal};
    for code in [KeyCode::Esc, KeyCode::Enter] {
        let mut editor = EditorState::new_create();
        editor.modal_parents.push(auth_form_parent());
        editor.open_error_popup(crate::tui::components::error_popup::error_popup_state(
            "1Password read failed",
            "read rejected",
        ));
        dismiss_error_popup(&mut editor, code);
        assert!(editor.modal_parents.is_empty());
        assert!(matches!(
            &editor.modal,
            Some(Modal::AuthForm { state, focus: AuthFormFocus::CredentialSource, literal_buffer, .. })
                if state.literal_buffer() == "prior-credential" && literal_buffer == "prior-buffer"
        ));
    }
}

#[test]
fn source_folder_error_popup_restores_browser_and_keeps_form_parent() {
    use crate::tui::state::{EditorState, FileBrowserTarget, Modal};
    let temp = tempfile::tempdir().unwrap();
    let browser = crate::tui::components::file_browser::FileBrowserState::from_listing(
        crate::services::file_browser::listing_at(temp.path().into(), temp.path().into()),
    );
    let mut editor = EditorState::new_create();
    editor.modal_parents.push(auth_form_parent());
    editor.modal = Some(Modal::FileBrowser {
        target: FileBrowserTarget::AuthFormSourceFolder,
        state: browser,
    });
    editor.open_sub_modal(Modal::ErrorPopup {
        state: crate::tui::components::error_popup::error_popup_state(
            "Invalid source folder",
            "pick another folder",
        ),
    });
    dismiss_error_popup(&mut editor, KeyCode::Esc);
    assert_eq!(editor.modal_parents.len(), 1);
    assert!(matches!(
        editor.modal_parents.last(),
        Some(Modal::AuthForm { .. })
    ));
    assert!(matches!(
        editor.modal,
        Some(Modal::FileBrowser {
            target: FileBrowserTarget::AuthFormSourceFolder,
            ..
        })
    ));
}

#[test]
fn root_error_popup_dismissal_resets_save_flow() {
    use crate::tui::state::{EditorSaveFlow, EditorState};
    let mut editor = EditorState::new_create();
    editor.save_flow = EditorSaveFlow::Confirming {
        exit_on_success: true,
    };
    editor.open_error_popup(crate::tui::components::error_popup::error_popup_state(
        "Save failed",
        "write rejected",
    ));
    dismiss_error_popup(&mut editor, KeyCode::Esc);
    assert!(editor.modal.is_none());
    assert!(editor.modal_parents.is_empty());
    assert!(matches!(editor.save_flow, EditorSaveFlow::Idle));
}

fn secret_source_picker() -> crate::tui::state::Modal<'static> {
    crate::tui::state::Modal::SourcePicker {
        state: crate::tui::components::source_picker::SourcePickerState::new(
            "API_KEY".into(),
            true,
        ),
        env_key: Some((
            crate::tui::state::SecretsScopeTag::Workspace,
            "API_KEY".into(),
        )),
    }
}

fn prepare_picker_commit(picker: &mut crate::tui::op_picker::OpPickerState) {
    picker.stage = crate::tui::op_picker::model::OpPickerStage::Field;
    picker.load_state = crate::tui::op_picker::model::OpLoadState::Ready;
    picker.pending_load = None;
    picker.selected_vault = Some(jackin_core::OpVault {
        id: "vault".into(),
        name: "Personal".into(),
    });
    picker.selected_item = Some(jackin_core::OpItem {
        id: "item".into(),
        name: "API".into(),
        subtitle: String::new(),
    });
    picker.fields = vec![jackin_core::OpField {
        id: "password".into(),
        section_id: None,
        label: "password".into(),
        field_type: "CONCEALED".into(),
        concealed: true,
        reference: "op://vault/item/password".into(),
    }];
    picker.field_list_state = termrock::interaction::CollectionState::new();
    picker.field_list_state.set_active(Some(0));
}

#[test]
fn secret_source_picker_op_commit_writes_secret_with_source_parent() {
    use crate::tui::state::{EditorState, Modal};
    let mut editor = EditorState::new_create();
    editor.modal = Some(secret_source_picker());
    assert!(matches!(
        dispatch_modal(&mut editor, KeyCode::Char('o')),
        super::EditorModalOutcome::Continue
    ));
    let Some(Modal::OpPicker { state, .. }) = editor.modal.as_mut() else {
        panic!("op picker expected")
    };
    prepare_picker_commit(state);
    assert!(matches!(
        dispatch_modal(&mut editor, KeyCode::Enter),
        super::EditorModalOutcome::Continue
    ));
    assert!(editor.modal.is_none());
    assert!(editor.modal_parents.is_empty());
    let Some(jackin_core::EnvValue::OpRef(reference)) = editor.pending.env.get("API_KEY") else {
        panic!("committed secret reference expected")
    };
    assert_eq!(reference.op, "op://vault/item/password");
}

#[test]
fn secret_source_picker_cancel_keeps_context_for_retry() {
    use crate::tui::state::{EditorState, Modal, SecretsScopeTag};
    for choice in ['o', 'p'] {
        let mut editor = EditorState::new_create();
        editor.modal = Some(secret_source_picker());
        assert!(matches!(
            dispatch_modal(&mut editor, KeyCode::Char(choice)),
            super::EditorModalOutcome::Continue
        ));
        assert!(matches!(
            dispatch_modal(&mut editor, KeyCode::Esc),
            super::EditorModalOutcome::Continue
        ));
        assert!(matches!(
            &editor.modal,
            Some(Modal::SourcePicker { env_key: Some((SecretsScopeTag::Workspace, key)), .. }) if key == "API_KEY"
        ));
        assert!(editor.modal_parents.is_empty());
        assert!(editor.pending.env.is_empty());
        assert!(matches!(
            dispatch_modal(&mut editor, KeyCode::Char(choice)),
            super::EditorModalOutcome::Continue
        ));
        assert!(matches!(
            editor.modal,
            Some(Modal::OpPicker { .. } | Modal::TextInput { .. })
        ));
    }
}

#[test]
fn auth_picker_commit_uses_validation_with_typed_form_parent() {
    use crate::tui::state::{EditorState, Modal};
    let mut editor = EditorState::new_create();
    editor.modal_parents.push(auth_form_parent());
    let mut picker = crate::tui::op_picker::OpPickerState::new();
    prepare_picker_commit(&mut picker);
    editor.modal = Some(Modal::OpPicker {
        secrets_target: None,
        state: Box::new(picker),
    });
    assert!(matches!(
        dispatch_modal(&mut editor, KeyCode::Enter),
        super::EditorModalOutcome::ValidateOpRef(_)
    ));
    assert!(editor.modal.is_none());
    assert!(matches!(
        editor.modal_parents.last(),
        Some(Modal::AuthForm { .. })
    ));
    assert!(editor.pending.env.is_empty());
}

#[test]
fn auth_resume_rejects_wrong_parent_without_consuming_it() {
    use crate::tui::auth_config::{
        ModalAuthFormCredentialApply, ModalAuthFormOpRefApply, ModalAuthPlainSourceOpen,
    };
    use crate::tui::state::{AuthFormFocus, Modal, TextInputTarget};
    let mut modal = Some(secret_source_picker());
    let mut parents = vec![secret_source_picker()];
    assert!(!Modal::apply_auth_op_ref(
        &mut modal,
        &mut parents,
        AuthFormFocus::Save,
        jackin_core::OpRef {
            op: "op://vault/item/password".into(),
            path: "Personal/API/password".into(),
            account: None,
            on_demand: false,
        }
    ));
    assert!(!Modal::apply_auth_plain_text(
        &mut modal,
        &mut parents,
        AuthFormFocus::Save,
        "token"
    ));
    assert!(!Modal::apply_auth_source_folder(
        &mut modal,
        &mut parents,
        AuthFormFocus::Save,
        "/tmp/source".into()
    ));
    assert!(!Modal::restore_auth_form_modal(&mut modal, &mut parents));
    assert!(!Modal::open_auth_plain_source_text_input(
        &mut modal,
        &mut parents,
        AuthFormFocus::CredentialSource,
        TextInputTarget::AuthCredential,
        crate::tui::components::auth_panel::auth_credential_input_state
    ));
    assert_eq!(parents.len(), 1);
    assert!(matches!(parents.last(), Some(Modal::SourcePicker { .. })));
    assert!(matches!(modal, Some(Modal::SourcePicker { .. })));
}
