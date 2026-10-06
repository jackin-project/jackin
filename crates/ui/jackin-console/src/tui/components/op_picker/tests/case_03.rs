// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

#[test]
fn build_op_picker_ref_preserves_sections_and_ambiguous_subtitles() {
    let built = build_op_picker_ref(
        OpPickerVaultRef {
            id: "v_uuid",
            name: "Private",
        },
        OpPickerItemRef {
            id: "i_a",
            name: "Claude",
            subtitle: "alice@example.com",
        },
        [
            OpPickerItemRef {
                id: "i_a",
                name: "Claude",
                subtitle: "alice@example.com",
            },
            OpPickerItemRef {
                id: "i_b",
                name: "Claude",
                subtitle: "bob@example.com",
            },
        ],
        OpPickerFieldRef {
            id: "f_uuid",
            label: "token",
            reference: "op://Private/Claude/aUtH/token",
            section_id: None,
            section_label: None,
        },
        [OpPickerFieldRef {
            id: "f_uuid",
            label: "token",
            reference: "op://Private/Claude/aUtH/token",
            section_id: None,
            section_label: None,
        }],
        &[OpSection {
            id: "s_auth_uuid".to_owned(),
            label: "Auth".to_owned(),
        }],
    )
    .expect("fixture IDs form a valid secret reference");
    assert_eq!(built.op, "op://v_uuid/i_a/s_auth_uuid/f_uuid");
    assert_eq!(built.path, "Private/Claude[alice@example.com]/Auth/token");
}

#[test]
fn build_op_picker_ref_flags_empty_reference_with_sibling_refs() {
    let built = build_op_picker_ref(
        OpPickerVaultRef {
            id: "v_uuid",
            name: "Private",
        },
        OpPickerItemRef {
            id: "i_uuid",
            name: "MyItem",
            subtitle: "",
        },
        [OpPickerItemRef {
            id: "i_uuid",
            name: "MyItem",
            subtitle: "",
        }],
        OpPickerFieldRef {
            id: "f_noref",
            label: "notes",
            reference: "",
            section_id: None,
            section_label: None,
        },
        [
            OpPickerFieldRef {
                id: "f_noref",
                label: "notes",
                reference: "",
                section_id: None,
                section_label: None,
            },
            OpPickerFieldRef {
                id: "f_sectioned",
                label: "password",
                reference: "op://Private/MyItem/Auth/password",
                section_id: Some("s_auth_uuid"),
                section_label: None,
            },
        ],
        &[OpSection {
            id: "s_auth_uuid".to_owned(),
            label: "Auth".to_owned(),
        }],
    )
    .expect("fixture IDs form a valid secret reference");
    assert_eq!(built.op, "op://v_uuid/i_uuid/f_noref");
    assert_eq!(built.path, "Private/MyItem/notes");
    assert!(built.empty_reference_with_sibling_refs);
}

#[test]
fn section_lines_append_new_section_sentinel() {
    let lines = section_lines(
        [
            None,
            Some(OpSection {
                id: "aaaa-id".to_owned(),
                label: "Auth".to_owned(),
            }),
            Some(OpSection {
                id: "bbbb-id".to_owned(),
                label: "Auth".to_owned(),
            }),
        ],
        Some(2),
    );
    assert_eq!(lines.len(), 4);
    assert_eq!(
        lines[0].spans[0].content.as_ref(),
        "(root)",
        "root choice renders first"
    );
    assert_eq!(
        lines[1].spans[0].content.as_ref(),
        "Auth [aaaa]",
        "duplicate section labels include an opaque ID prefix"
    );
    assert_eq!(
        lines[2].spans[0].content.as_ref(),
        "Auth [bbbb]",
        "duplicate labels remain distinguishable"
    );
    assert_eq!(
        lines[3].spans[0].content.as_ref(),
        "+ New section",
        "sentinel renders last without embedding selection chrome"
    );
}

#[test]
fn account_vault_and_item_lines_leave_selection_to_shared_renderer() {
    let account = account_lines(
        [OpPickerAccountRef {
            email: "alice@example.com",
            url: "alice.1password.com",
        }],
        Some(0),
    );
    assert_eq!(account[0].spans[0].content.as_ref(), "alice@example.com");
    assert_eq!(
        account[0].spans[2].content.as_ref(),
        "(alice.1password.com)"
    );

    let vault = vault_lines(
        [OpPickerVaultRef {
            id: "v1",
            name: "Private",
        }],
        None,
    );
    assert_eq!(vault[0].spans[0].content.as_ref(), "Private");

    let items = item_choice_lines(
        [
            Some(OpPickerItemRef {
                id: "i1",
                name: "Claude",
                subtitle: "alice@example.com",
            }),
            None,
        ],
        Some(1),
    );
    assert_eq!(items[0].spans[0].content.as_ref(), "Claude");
    assert_eq!(items[0].spans[2].content.as_ref(), "alice@example.com");
    assert_eq!(items[1].spans[0].content.as_ref(), "+ New item");
}

#[test]
fn field_lines_render_headers_fields_and_sentinels() {
    let mut collapsed = HashSet::new();
    collapsed.insert("auth-id".to_owned());
    let lines = field_lines(
        [
            FieldDisplayRow::SectionHeader {
                section_id: "auth-id".to_owned(),
                name: "Auth".to_owned(),
                field_count: 1,
            },
            FieldDisplayRow::Field { field_idx: 0 },
            FieldDisplayRow::NewFieldSentinel,
        ],
        [OpPickerFieldDisplayRef {
            id: "f1",
            label: "token",
            field_type: "CONCEALED",
            concealed: true,
        }],
        &collapsed,
        Some(1),
    );

    assert_eq!(lines[0].spans[0].content.as_ref(), "\u{25b6}");
    assert_eq!(lines[1].spans[0].content.as_ref(), "token");
    assert_eq!(lines[1].spans[2].content.as_ref(), "(concealed)");
    assert_eq!(lines[2].spans[0].content.as_ref(), "+ New field");
}

#[test]
fn loading_descriptor_names_current_load_target() {
    assert_eq!(
        loading_descriptor(OpPickerStage::Account, false, "", "", "", ""),
        "loading accounts\u{2026}"
    );
    assert_eq!(
        loading_descriptor(OpPickerStage::Vault, true, "alice@example.com", "", "", ""),
        "loading vaults from alice@example.com\u{2026}"
    );
    assert_eq!(
        loading_descriptor(
            OpPickerStage::Field,
            false,
            "",
            "",
            "Claude",
            "alice@example.com"
        ),
        "loading Claude (alice@example.com)\u{2026}"
    );
    assert_eq!(
        loading_title_stage(OpPickerStage::Field),
        OpPickerStage::Item
    );
}

#[test]
fn fatal_body_lines_truncate_generic_errors() {
    let long = "x".repeat(140);
    let lines = fatal_body_lines(&OpPickerFatalState::GenericFatal { message: long });
    assert_eq!(lines[0].spans[0].content.as_ref(), "1Password CLI error.");
    assert_eq!(lines[2].spans[0].content.chars().count(), 120);

    let missing = fatal_body_lines(&OpPickerFatalState::NotInstalled);
    assert!(missing.iter().any(|line| {
        line.spans
            .iter()
            .any(|span| span.content.contains("brew install"))
    }));
}

#[test]
fn field_label_origin_maps_to_cancel_stage() {
    assert_eq!(
        FieldLabelOrigin::NewItem.cancel_stage(),
        OpPickerStage::NewItemName
    );
    assert_eq!(
        FieldLabelOrigin::NewField.cancel_stage(),
        OpPickerStage::Field
    );
    assert_eq!(
        FieldLabelOrigin::NewSection.cancel_stage(),
        OpPickerStage::NewSectionName
    );
}

#[test]
fn field_label_commit_plan_trims_and_routes_item_presence() {
    let section = OpSection {
        id: "section-id".to_owned(),
        label: "section".to_owned(),
    };
    assert_eq!(
        field_label_commit_plan(
            Some("account"),
            "vault",
            Some("item"),
            Some(OpSectionTarget::Existing(section.clone())),
            "ignored".to_owned(),
            "  token  ",
        ),
        FieldLabelCommitPlan::EditItemField {
            account: Some("account"),
            vault: "vault",
            item: "item",
            section: Some(OpSectionTarget::Existing(section)),
            field_label: "token".to_owned(),
        }
    );
    assert_eq!(
        field_label_commit_plan::<&str, &str, &str>(
            None,
            "vault",
            None,
            None,
            "login".to_owned(),
            "  password  ",
        ),
        FieldLabelCommitPlan::NewItem {
            account: None,
            vault: "vault",
            item_name: "login".to_owned(),
            section: None,
            field_label: "password".to_owned(),
        }
    );
}

#[test]
fn field_label_commit_selection_builds_component_owned_selection_shape() {
    let section = OpSection {
        id: "opaque-api-id".to_owned(),
        label: "api".to_owned(),
    };
    let selection = field_label_commit_selection::<&str, &str, &str, &str, (&str, String)>(
        FieldLabelCommitPlan::EditItemField {
            account: Some("account"),
            vault: "vault",
            item: "item",
            section: Some(OpSectionTarget::Existing(section.clone())),
            field_label: "token".to_owned(),
        },
        |label| ("new", label),
    );
    assert_eq!(
        selection,
        OpPickerSelection::EditItemField {
            account: Some("account"),
            vault: "vault",
            item: "item",
            section: Some(OpSectionTarget::Existing(section)),
            field: ("new", "token".to_owned()),
        }
    );

    let selection = field_label_commit_selection::<&str, &str, &str, &str, (&str, String)>(
        FieldLabelCommitPlan::NewItem {
            account: None,
            vault: "vault",
            item_name: "Login".to_owned(),
            section: None,
            field_label: "password".to_owned(),
        },
        |label| ("new", label),
    );
    assert_eq!(
        selection,
        OpPickerSelection::NewItem {
            account: None,
            vault: "vault",
            item_name: "Login".to_owned(),
            section: None,
            field_label: "password".to_owned(),
        }
    );
}

#[test]
fn existing_field_commit_plan_routes_create_mode_to_field_target_data() {
    let api = OpSection {
        id: "opaque-api-id".to_owned(),
        label: "api".to_owned(),
    };
    assert_eq!(
        existing_field_commit_plan(
            &OpPickerMode::Create {
                item_name_default: String::new(),
                field_label_default: String::new(),
            },
            "field-id",
            "token",
            Some(api.clone()),
        ),
        ExistingFieldCommitPlan::EditItemField {
            section: Some(OpSectionTarget::Existing(api)),
            field_id: "field-id".to_owned(),
            field_label: "token".to_owned(),
        }
    );
    assert_eq!(
        existing_field_commit_plan(&OpPickerMode::Browse, "field-id", "token", None),
        ExistingFieldCommitPlan::ExistingReference,
    );
}
