// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

#[test]
fn console_modal_reports_debug_kind() {
    type TestModal = ConsoleModal<
        &'static str,
        (),
        (),
        (),
        (),
        (),
        (),
        (),
        (),
        (),
        (),
        (),
        (),
        (),
        (),
        (),
        (),
        (),
        (),
        (),
        (),
        (),
    >;

    let modal = TestModal::TextInput {
        target: "name",
        state: (),
    };

    assert_eq!(modal.debug_kind(), ModalDebugKind::TextInput);
}

#[test]
fn console_modal_opens_auth_source_picker_from_form() {
    type TestModal = ConsoleModal<
        (),
        (),
        (),
        (),
        (),
        (),
        (),
        (),
        (),
        (),
        (),
        (),
        (),
        (),
        (),
        (),
        &'static str,
        (),
        crate::tui::screens::settings::model::AuthFormTarget<crate::tui::auth::AuthKind>,
        crate::tui::components::auth_panel::AuthForm<jackin_core::EnvValue>,
        crate::tui::screens::settings::model::AuthFormFocus,
        (),
    >;

    let mut form =
        crate::tui::components::auth_panel::AuthForm::new(crate::tui::auth::AuthKind::Claude);
    form.set_mode(crate::tui::auth::AuthMode::ApiKey);
    let mut modal = Some(TestModal::AuthForm {
        target: crate::tui::screens::settings::model::AuthFormTarget::Workspace {
            kind: crate::tui::auth::AuthKind::Claude,
        },
        state: Box::new(form),
        focus: crate::tui::screens::settings::model::AuthFormFocus::CredentialSource,
        literal_buffer: "existing".into(),
    });
    let mut parents = Vec::new();

    let opened = crate::tui::auth_config::ModalAuthSourcePickerOpen::open_auth_source_picker(
        &mut modal,
        &mut parents,
        |env_var| env_var,
    );

    assert!(opened);
    assert_eq!(parents.len(), 1);
    let expected_env_var = crate::tui::auth::AuthKind::Claude
        .required_env_var(crate::tui::auth::AuthMode::ApiKey)
        .expect("Claude API key mode requires env var");
    assert!(matches!(
        modal,
        Some(TestModal::AuthSourcePicker { state }) if state == expected_env_var
    ));
}

#[test]
fn console_modal_opens_auth_source_folder_browser() {
    type TestModal = ConsoleModal<
        (),
        (),
        &'static str,
        &'static str,
        (),
        (),
        (),
        (),
        (),
        (),
        (),
        (),
        (),
        (),
        (),
        (),
        (),
        (),
        crate::tui::screens::settings::model::AuthFormTarget<crate::tui::auth::AuthKind>,
        crate::tui::components::auth_panel::AuthForm<jackin_core::EnvValue>,
        crate::tui::screens::settings::model::AuthFormFocus,
        (),
    >;

    let mut form =
        crate::tui::components::auth_panel::AuthForm::new(crate::tui::auth::AuthKind::Claude)
            .with_source_folder(
                None,
                Some(
                    crate::tui::components::editor_rows::AuthSourceFolderDisplay {
                        kind: crate::tui::components::editor_rows::AuthSourceFolderKind::Default,
                        path: "~/.claude".into(),
                    },
                ),
            );
    form.set_mode(crate::tui::auth::AuthMode::Sync);
    let mut modal = Some(TestModal::AuthForm {
        target: crate::tui::screens::settings::model::AuthFormTarget::Workspace {
            kind: crate::tui::auth::AuthKind::Claude,
        },
        state: Box::new(form),
        focus: crate::tui::screens::settings::model::AuthFormFocus::SourceFolder,
        literal_buffer: String::new(),
    });
    let mut parents = Vec::new();

    let opened =
        crate::tui::auth_config::ModalAuthSourceFolderBrowserOpen::open_auth_source_folder_browser(
            &mut modal,
            &mut parents,
            crate::tui::screens::settings::model::AuthFormFocus::SourceFolder,
            "auth-source-folder",
            || Ok::<_, ()>("browser"),
        );

    assert_eq!(
        opened,
        crate::tui::auth_config::AuthSourceFolderBrowserOpenResult::Opened
    );
    assert_eq!(parents.len(), 1);
    assert!(matches!(
        modal,
        Some(TestModal::FileBrowser {
            target: "auth-source-folder",
            state: "browser"
        })
    ));
}

#[test]
fn console_modal_opens_plain_source_text_input() {
    type TestModal = ConsoleModal<
        &'static str,
        String,
        (),
        (),
        (),
        (),
        (),
        (),
        (),
        (),
        (),
        (),
        (),
        (),
        (),
        (),
        (),
        (),
        crate::tui::screens::settings::model::AuthFormTarget<crate::tui::auth::AuthKind>,
        crate::tui::components::auth_panel::AuthForm<jackin_core::EnvValue>,
        crate::tui::screens::settings::model::AuthFormFocus,
        (),
    >;

    let mut modal = None;
    let mut parents = vec![TestModal::AuthForm {
        target: crate::tui::screens::settings::model::AuthFormTarget::Workspace {
            kind: crate::tui::auth::AuthKind::Claude,
        },
        state: Box::new(crate::tui::components::auth_panel::AuthForm::new(
            crate::tui::auth::AuthKind::Claude,
        )),
        focus: crate::tui::screens::settings::model::AuthFormFocus::Mode,
        literal_buffer: "existing".into(),
    }];

    let opened =
        crate::tui::auth_config::ModalAuthPlainSourceOpen::open_auth_plain_source_text_input(
            &mut modal,
            &mut parents,
            crate::tui::screens::settings::model::AuthFormFocus::CredentialSource,
            "auth",
            |literal| literal,
        );

    assert!(opened);
    assert_eq!(parents.len(), 1);
    assert!(
        matches!(modal, Some(TestModal::TextInput { target: "auth", state }) if state == "existing")
    );
}

#[test]
fn console_modal_opens_auth_op_picker() {
    type TestModal = ConsoleModal<
        (),
        (),
        (),
        (),
        (),
        (),
        (),
        (),
        (),
        (),
        (),
        (),
        (),
        (),
        &'static str,
        (),
        (),
        (),
        crate::tui::screens::settings::model::AuthFormTarget<crate::tui::auth::AuthKind>,
        crate::tui::components::auth_panel::AuthForm<jackin_core::EnvValue>,
        crate::tui::screens::settings::model::AuthFormFocus,
        (),
    >;

    let mut modal = None;
    let mut parents = vec![TestModal::AuthForm {
        target: crate::tui::screens::settings::model::AuthFormTarget::Workspace {
            kind: crate::tui::auth::AuthKind::Claude,
        },
        state: Box::new(crate::tui::components::auth_panel::AuthForm::new(
            crate::tui::auth::AuthKind::Claude,
        )),
        focus: crate::tui::screens::settings::model::AuthFormFocus::Mode,
        literal_buffer: String::new(),
    }];

    let opened = crate::tui::auth_config::ModalAuthOpPickerOpen::open_auth_op_picker(
        &mut modal,
        &mut parents,
        crate::tui::screens::settings::model::AuthFormFocus::CredentialSource,
        || "op-picker",
    );

    assert!(opened);
    assert!(matches!(
        parents.last(),
        Some(TestModal::AuthForm {
            focus: crate::tui::screens::settings::model::AuthFormFocus::CredentialSource,
            ..
        })
    ));
    assert!(matches!(modal, Some(TestModal::OpPicker { state, .. }) if *state == "op-picker"));
}

#[test]
fn console_modal_applies_auth_plain_text() {
    type TestModal = ConsoleModal<
        (),
        (),
        (),
        (),
        (),
        (),
        (),
        (),
        (),
        (),
        (),
        (),
        (),
        (),
        (),
        (),
        (),
        (),
        crate::tui::screens::settings::model::AuthFormTarget<crate::tui::auth::AuthKind>,
        crate::tui::components::auth_panel::AuthForm<jackin_core::EnvValue>,
        crate::tui::screens::settings::model::AuthFormFocus,
        (),
    >;

    let mut modal = None;
    let mut parents = vec![TestModal::AuthForm {
        target: crate::tui::screens::settings::model::AuthFormTarget::Workspace {
            kind: crate::tui::auth::AuthKind::Claude,
        },
        state: Box::new(crate::tui::components::auth_panel::AuthForm::new(
            crate::tui::auth::AuthKind::Claude,
        )),
        focus: crate::tui::screens::settings::model::AuthFormFocus::CredentialSource,
        literal_buffer: String::new(),
    }];

    let applied = crate::tui::auth_config::ModalAuthFormCredentialApply::apply_auth_plain_text(
        &mut modal,
        &mut parents,
        crate::tui::screens::settings::model::AuthFormFocus::Save,
        "token",
    );

    assert!(applied);
    assert!(parents.is_empty());
    assert!(matches!(
        modal,
        Some(TestModal::AuthForm {
            state,
            focus: crate::tui::screens::settings::model::AuthFormFocus::Save,
            literal_buffer,
            ..
        }) if state.literal_buffer() == "token" && literal_buffer == "token"
    ));
}

#[test]
fn console_modal_restores_auth_form_modal() {
    type TestModal = ConsoleModal<
        (),
        (),
        (),
        (),
        (),
        (),
        (),
        (),
        (),
        (),
        (),
        (),
        (),
        (),
        (),
        (),
        (),
        (),
        crate::tui::screens::settings::model::AuthFormTarget<crate::tui::auth::AuthKind>,
        crate::tui::components::auth_panel::AuthForm<jackin_core::EnvValue>,
        crate::tui::screens::settings::model::AuthFormFocus,
        (),
    >;

    let mut modal = None;
    let mut parents = vec![TestModal::AuthForm {
        target: crate::tui::screens::settings::model::AuthFormTarget::Workspace {
            kind: crate::tui::auth::AuthKind::Claude,
        },
        state: Box::new(crate::tui::components::auth_panel::AuthForm::new(
            crate::tui::auth::AuthKind::Claude,
        )),
        focus: crate::tui::screens::settings::model::AuthFormFocus::CredentialSource,
        literal_buffer: "existing".into(),
    }];

    let restored = crate::tui::auth_config::ModalAuthFormCredentialApply::restore_auth_form_modal(
        &mut modal,
        &mut parents,
    );

    assert!(restored);
    assert!(parents.is_empty());
    assert!(matches!(
        modal,
        Some(TestModal::AuthForm {
            focus: crate::tui::screens::settings::model::AuthFormFocus::CredentialSource,
            literal_buffer,
            ..
        }) if literal_buffer == "existing"
    ));
}
