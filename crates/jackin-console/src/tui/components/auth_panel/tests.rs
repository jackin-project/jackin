// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Tests for `auth_panel`.
use super::*;
use jackin_core::env_model;
use ratatui::{Terminal, backend::TestBackend};

#[derive(Debug, Clone, PartialEq, Eq)]
struct TestOpRef {
    path: String,
}

impl AuthCredentialRef for TestOpRef {
    fn path(&self) -> &str {
        &self.path
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum TestCredential {
    Plain(String),
    OpRef(TestOpRef),
}

impl AuthCredential for TestCredential {
    type Ref = TestOpRef;

    fn into_credential_input(self) -> CredentialInput<Self::Ref> {
        match self {
            Self::Plain(value) => CredentialInput::Literal(value),
            Self::OpRef(value) => CredentialInput::OpRef(value),
        }
    }

    fn from_plain(value: String) -> Self {
        Self::Plain(value)
    }

    fn from_op_ref(value: Self::Ref) -> Self {
        Self::OpRef(value)
    }
}

type TestForm = AuthForm<TestCredential>;

fn dump_form(form: &TestForm) -> String {
    let backend = TestBackend::new(100, 20);
    let mut term = Terminal::new(backend).unwrap();
    term.draw(|frame| {
        let area = frame.area();
        render_form(frame, area, form, AuthFormFocus::Mode);
    })
    .unwrap();
    let buf = term.backend().buffer();
    let mut output = String::new();
    for y in 0..buf.area.height {
        for x in 0..buf.area.width {
            output.push_str(buf[(x, y)].symbol());
        }
        output.push('\n');
    }
    output
}

#[test]
fn auth_credential_input_state_names_credential() {
    let state = auth_credential_input_state("secret");

    assert_eq!(state.label, "Credential");
    assert_eq!(state.value(), "secret");
}

#[test]
fn auth_source_picker_state_keeps_env_label() {
    let state = auth_source_picker_state("CLAUDE_API_KEY", true);

    assert_eq!(state.key, "CLAUDE_API_KEY");
}

#[test]
fn generated_token_source_picker_state_uses_component_label() {
    let state = generated_token_source_picker_state(true);

    assert_eq!(state.key, "generated token");
}

#[test]
fn generated_token_op_item_name_applies_scope_label() {
    assert_eq!(
        generated_token_op_item_name("Claude ({ws})", "global"),
        "Claude (global)"
    );
}

#[test]
fn auth_panel_title_pads_kind_label_for_panel() {
    assert_eq!(auth_panel_title("Claude"), " Claude ");
}

#[test]
fn save_disabled_when_mode_unset() {
    let form = TestForm::new(AuthKind::Claude);
    assert!(!form.can_save());
}

#[test]
fn save_enabled_for_sync() {
    let mut form = TestForm::new(AuthKind::Claude);
    form.set_mode(AuthMode::Sync);
    assert!(form.can_save());
}

#[test]
fn save_disabled_for_api_key_without_credential() {
    let mut form = TestForm::new(AuthKind::Claude);
    form.set_mode(AuthMode::ApiKey);
    assert!(!form.can_save());
}

#[test]
fn save_enabled_for_api_key_with_literal() {
    let mut form = TestForm::new(AuthKind::Claude);
    form.set_mode(AuthMode::ApiKey);
    form.set_literal("sk-ant-test".into());
    assert!(form.can_save());
}

#[test]
fn literal_buffer_reads_only_plain_literal() {
    let mut form = TestForm::new(AuthKind::Claude);
    assert_eq!(form.literal_buffer(), "");

    form.set_literal("sk-ant-test".into());
    assert_eq!(form.literal_buffer(), "sk-ant-test");

    form.set_op_ref(TestOpRef {
        path: "vault/item/field".into(),
    });
    assert_eq!(form.literal_buffer(), "");
}

#[test]
fn save_disabled_for_api_key_with_empty_op_ref() {
    let mut form = TestForm::new(AuthKind::Claude);
    form.set_mode(AuthMode::ApiKey);
    form.set_op_ref(TestOpRef {
        path: String::new(),
    });
    assert!(!form.can_save());
}

#[test]
fn commit_emits_required_env_var() {
    let mut form = TestForm::new(AuthKind::Github);
    form.set_mode(AuthMode::Token);
    form.set_literal("ghp_xxxx".into());
    let outcome = form.commit().expect("can save");
    assert_eq!(outcome.mode, AuthMode::Token);
    assert_eq!(outcome.env_var_name, Some("GH_TOKEN"));
    assert!(matches!(
        outcome.env_value,
        Some(TestCredential::Plain(ref value)) if value == "ghp_xxxx"
    ));
}

#[test]
fn cycle_mode_wraps_supported_modes_and_updates_focus_target() {
    let mut form = TestForm::new(AuthKind::Github);

    assert_eq!(form.next_focus_after_mode(), AuthFormFocus::Save);
    form.cycle_mode();
    assert_eq!(form.mode, Some(AuthMode::Sync));
    assert_eq!(form.next_focus_after_mode(), AuthFormFocus::Save);
    form.cycle_mode();
    assert_eq!(form.mode, Some(AuthMode::Token));
    assert_eq!(
        form.next_focus_after_mode(),
        AuthFormFocus::CredentialSource
    );
    form.cycle_mode();
    assert_eq!(form.mode, Some(AuthMode::Ignore));
    form.cycle_mode();
    assert_eq!(form.mode, Some(AuthMode::Sync));
}

#[test]
fn source_folder_row_requires_form_source_state_and_supported_mode() {
    let mut form = TestForm::new(AuthKind::Claude);
    form.set_mode(AuthMode::Sync);
    assert!(
        !form.shows_source_folder(),
        "settings forms opt in during their own phase"
    );

    let form = form.with_source_folder(
        None,
        Some(AuthSourceFolderDisplay {
            kind: AuthSourceFolderKind::Default,
            path: "~/.claude".to_owned(),
        }),
    );
    assert!(form.shows_source_folder());
    assert_eq!(form.next_focus_after_mode(), AuthFormFocus::SourceFolder);

    let mut api_key = form;
    api_key.set_mode(AuthMode::ApiKey);
    assert!(!api_key.shows_source_folder());
}

#[test]
fn source_folder_row_renders_fallback_and_staged_path() {
    let mut form = TestForm::new(AuthKind::Claude).with_source_folder(
        None,
        Some(AuthSourceFolderDisplay {
            kind: AuthSourceFolderKind::Default,
            path: "~/.claude".to_owned(),
        }),
    );
    form.set_mode(AuthMode::Sync);
    let output = dump_form(&form);
    assert!(output.contains("Source folder"));
    assert!(output.contains("default: ~/.claude"));
    assert!(!output.contains("CLAUDE_CONFIG_DIR"));

    form.set_source_folder(PathBuf::from("/host/claude"));
    let output = dump_form(&form);
    assert!(output.contains("/host/claude"));
    assert!(!output.contains("default: ~/.claude"));
}

#[test]
fn source_folder_row_renders_display_kinds_without_env_suffix() {
    for (kind, path, expected) in [
        (
            AuthSourceFolderKind::Default,
            "~/.claude",
            "default: ~/.claude",
        ),
        (
            AuthSourceFolderKind::Inherited,
            "/global/claude",
            "inherited: /global/claude",
        ),
        (
            AuthSourceFolderKind::Explicit,
            "/workspace/claude",
            "/workspace/claude",
        ),
    ] {
        let mut form = TestForm::new(AuthKind::Claude).with_source_folder(
            None,
            Some(AuthSourceFolderDisplay {
                kind,
                path: path.to_owned(),
            }),
        );
        form.set_mode(AuthMode::Sync);
        let output = dump_form(&form);
        assert!(output.contains(expected), "{output}");
        assert!(!output.contains("explicit:"), "{output}");
        assert!(!output.contains('('), "{output}");
    }
}

#[test]
fn auth_form_key_plan_routes_shared_focus_model() {
    assert_eq!(
        auth_form_key_plan(AuthFormFocus::Mode, KeyCode::Char(' '), false, false),
        AuthFormKeyPlan::CycleMode
    );
    assert_eq!(
        auth_form_key_plan(AuthFormFocus::Mode, KeyCode::Tab, true, false),
        AuthFormKeyPlan::Focus(AuthFormFocus::CredentialSource)
    );
    assert_eq!(
        auth_form_key_plan(AuthFormFocus::CredentialSource, KeyCode::Enter, true, false),
        AuthFormKeyPlan::OpenCredentialSource
    );
    assert_eq!(
        auth_form_key_plan(AuthFormFocus::Save, KeyCode::BackTab, true, false),
        AuthFormKeyPlan::Focus(AuthFormFocus::CredentialSource)
    );
    assert_eq!(
        auth_form_key_plan(AuthFormFocus::Save, KeyCode::Enter, true, false),
        AuthFormKeyPlan::Stay
    );
    assert_eq!(
        auth_form_key_plan(AuthFormFocus::Save, KeyCode::Enter, true, true),
        AuthFormKeyPlan::Save
    );
    assert_eq!(
        auth_form_key_plan(AuthFormFocus::Cancel, KeyCode::Enter, false, false),
        AuthFormKeyPlan::Cancel
    );
    assert_eq!(
        auth_form_key_plan(AuthFormFocus::Reset, KeyCode::Enter, false, false),
        AuthFormKeyPlan::Reset
    );
}

#[test]
fn auth_form_key_plan_routes_source_folder_row() {
    assert_eq!(
        auth_form_key_plan_with_source_folder(AuthFormFocus::Mode, KeyCode::Tab, true, false, true),
        AuthFormKeyPlan::Focus(AuthFormFocus::SourceFolder)
    );
    assert_eq!(
        auth_form_key_plan_with_source_folder(
            AuthFormFocus::SourceFolder,
            KeyCode::Enter,
            true,
            false,
            true
        ),
        AuthFormKeyPlan::OpenSourceFolderBrowser
    );
    assert_eq!(
        auth_form_key_plan_with_source_folder(
            AuthFormFocus::SourceFolder,
            KeyCode::Down,
            true,
            true,
            true
        ),
        AuthFormKeyPlan::Focus(AuthFormFocus::CredentialSource)
    );
    assert_eq!(
        auth_form_key_plan_with_source_folder(
            AuthFormFocus::CredentialSource,
            KeyCode::Up,
            true,
            true,
            true
        ),
        AuthFormKeyPlan::Focus(AuthFormFocus::SourceFolder)
    );
    assert_eq!(
        auth_form_key_plan_with_source_folder(
            AuthFormFocus::Save,
            KeyCode::BackTab,
            true,
            false,
            true
        ),
        AuthFormKeyPlan::Focus(AuthFormFocus::SourceFolder)
    );
}

#[test]
fn form_with_unset_mode_hides_credential_block() {
    let form = TestForm::new(AuthKind::Claude);
    let output = dump_form(&form);
    assert!(output.contains("Edit auth"));
    assert!(output.contains("Mode"));
    assert!(output.contains("(unset)"));
    assert!(!output.contains(env_model::ANTHROPIC_API_KEY_ENV_NAME));
}

#[test]
fn form_with_api_key_literal_masks_value() {
    let mut form = TestForm::new(AuthKind::Claude);
    form.set_mode(AuthMode::ApiKey);
    form.set_literal("sk-ant-test".into());
    let output = dump_form(&form);
    assert!(output.contains("api_key"));
    assert!(output.contains(env_model::ANTHROPIC_API_KEY_ENV_NAME));
    assert!(output.contains("●●●●●●●●●●●"));
}

#[test]
fn form_with_op_ref_credential_shows_path() {
    let mut form = TestForm::new(AuthKind::Claude);
    form.set_mode(AuthMode::ApiKey);
    form.set_op_ref(TestOpRef {
        path: "Work/Anthropic/api-key".into(),
    });
    let output = dump_form(&form);
    assert!(output.contains("Work / Anthropic → api-key"));
}
