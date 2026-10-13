// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

#[test]
fn console_modal_applies_auth_op_ref() {
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

    let mut form =
        crate::tui::components::auth_panel::AuthForm::new(crate::tui::auth::AuthKind::Claude);
    form.set_mode(crate::tui::auth::AuthMode::ApiKey);
    let op_ref = jackin_core::OpRef {
        op: "op://vault/item/field".into(),
        path: "Vault/Item/Field".into(),
        account: None,
        on_demand: false,
    };
    let mut modal = None;
    let mut parents = vec![TestModal::AuthForm {
        target: crate::tui::screens::settings::model::AuthFormTarget::Workspace {
            kind: crate::tui::auth::AuthKind::Claude,
        },
        state: Box::new(form),
        focus: crate::tui::screens::settings::model::AuthFormFocus::CredentialSource,
        literal_buffer: String::new(),
    }];

    let applied = crate::tui::auth_config::ModalAuthFormOpRefApply::apply_auth_op_ref(
        &mut modal,
        &mut parents,
        crate::tui::screens::settings::model::AuthFormFocus::Save,
        op_ref.clone(),
    );

    assert!(applied);
    assert!(parents.is_empty());
    assert!(matches!(
        modal,
        Some(TestModal::AuthForm {
            state,
            focus: crate::tui::screens::settings::model::AuthFormFocus::Save,
            ..
        }) if matches!(
            &state.credential,
            crate::tui::components::auth_panel::CredentialInput::OpRef(value)
                if *value == op_ref
        )
    ));
}

#[test]
fn console_modal_role_picker_overlay_size_uses_filtered_rows() {
    let modal = RectTestModal::RolePicker {
        state: TestRolePicker(5),
    };

    let size = modal.overlay_size(Rect::new(0, 0, 100, 40));
    // 50% of the 160-column reference, 5 filtered rows + 6 chrome rows.
    assert_eq!(size.width, 80);
    assert_eq!(size.height, 11);
}

#[test]
fn console_modal_error_overlay_size_uses_required_height() {
    let modal = RectTestModal::ErrorPopup { state: TestError };

    let size = modal.overlay_size(Rect::new(0, 0, 100, 40));
    assert_eq!(size.width, 96);
    assert_eq!(size.height, 14);
}

#[test]
fn console_modal_dismiss_policy_matches_pre_cutover_behavior() {
    use termrock::interaction::DismissAction;
    let cases: [(RectTestModal, DismissAction); 19] = [
        (
            RectTestModal::TextInput {
                target: (),
                state: (),
            },
            DismissAction::Dismiss,
        ),
        (
            RectTestModal::FileBrowser {
                target: (),
                state: TestFileBrowser,
            },
            DismissAction::Bubble,
        ),
        (
            RectTestModal::MountDstChoice {
                target: (),
                state: (),
            },
            DismissAction::Dismiss,
        ),
        (
            RectTestModal::WorkdirPick { state: () },
            DismissAction::Dismiss,
        ),
        (
            RectTestModal::Confirm {
                target: (),
                state: TestConfirm,
            },
            DismissAction::Dismiss,
        ),
        (
            RectTestModal::SaveDiscardCancel { state: () },
            DismissAction::Dismiss,
        ),
        (
            RectTestModal::GithubPicker {
                state: TestGithubPicker(3),
            },
            DismissAction::Dismiss,
        ),
        (
            RectTestModal::ConfirmSave {
                state: TestConfirmSave,
            },
            DismissAction::Dismiss,
        ),
        (
            RectTestModal::ErrorPopup { state: TestError },
            DismissAction::Dismiss,
        ),
        (
            RectTestModal::ContainerInfo {
                state: TestContainerInfo,
            },
            DismissAction::Dismiss,
        ),
        (
            RectTestModal::StatusPopup { state: () },
            DismissAction::Dismiss,
        ),
        (
            RectTestModal::OpPicker {
                secrets_target: None,
                state: Box::new(TestOpPicker(false)),
            },
            DismissAction::Bubble,
        ),
        (
            RectTestModal::RolePicker {
                state: TestRolePicker(2),
            },
            DismissAction::Dismiss,
        ),
        (
            RectTestModal::RoleOverridePicker {
                state: TestRolePicker(2),
            },
            DismissAction::Dismiss,
        ),
        (
            RectTestModal::RoleOverridePicker {
                state: TestRolePicker(2),
            },
            DismissAction::Dismiss,
        ),
        (
            RectTestModal::SourcePicker {
                state: (),
                env_key: None,
            },
            DismissAction::Dismiss,
        ),
        (
            RectTestModal::AuthSourcePicker { state: () },
            DismissAction::Dismiss,
        ),
        (
            RectTestModal::ScopePicker { state: () },
            DismissAction::Dismiss,
        ),
        (
            RectTestModal::AuthForm {
                target: (),
                state: Box::new(TestAuthForm),
                focus: (),
                literal_buffer: String::new(),
            },
            DismissAction::Dismiss,
        ),
    ];
    assert_eq!(cases.len(), 19);
    for (modal, escape) in &cases {
        let policy = modal.dismiss_policy();
        assert_eq!(policy.escape, *escape);
        assert_eq!(policy.outside, DismissAction::Trap);
        assert_eq!(policy.parent_closed, DismissAction::Dismiss);
        assert_eq!(policy.explicit, DismissAction::Dismiss);
    }
}

#[test]
fn console_modal_container_info_rect_reports_only_container_info_area() {
    let outer = Rect::new(0, 0, 100, 40);
    let modal = RectTestModal::ContainerInfo {
        state: TestContainerInfo,
    };

    assert_eq!(modal.container_info_rect(outer), Some(modal.rect(outer)));
    assert_eq!(
        RectTestModal::ErrorPopup { state: TestError }.container_info_rect(outer),
        None
    );
}

#[test]
fn console_modal_reports_footer_items() {
    let modal = RectTestModal::RolePicker {
        state: TestRolePicker(5),
    };

    assert!(
        modal
            .footer_items(false)
            .contains(&termrock::widgets::HintSpan::Text("filter"))
    );
}

#[test]
fn console_modal_footer_items_for_area_reflects_container_info_overflow() {
    let modal = RectTestModal::ContainerInfo {
        state: TestContainerInfo,
    };

    assert!(
        modal
            .footer_items_for_area(false, Rect::new(0, 0, 100, 20))
            .contains(&termrock::widgets::HintSpan::Text("scroll"))
    );
}
