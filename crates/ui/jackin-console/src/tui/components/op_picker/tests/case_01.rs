// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

#[test]
fn background_worker_disconnected_message_is_component_owned() {
    assert_eq!(
        background_worker_disconnected_error_message(),
        "background worker disconnected",
    );
}

#[test]
fn probe_load_error_state_classifies_operator_states() {
    assert!(matches!(
        probe_load_error_state("failed to spawn op"),
        OpLoadState::Error(OpPickerError::Fatal(OpPickerFatalState::NotInstalled))
    ));
    assert!(matches!(
        probe_load_error_state("not currently signed in"),
        OpLoadState::Error(OpPickerError::Fatal(OpPickerFatalState::NotSignedIn))
    ));
}

#[test]
fn recoverable_load_error_state_preserves_message() {
    assert!(matches!(
        recoverable_load_error_state("field read failed"),
        OpLoadState::Error(OpPickerError::Recoverable { message }) if message == "field read failed"
    ));
}

#[test]
fn disconnected_worker_error_state_uses_standard_message() {
    assert!(matches!(
        disconnected_worker_error_state(),
        OpLoadState::Error(OpPickerError::Recoverable { message })
            if message == background_worker_disconnected_error_message()
    ));
}

#[test]
fn blocked_load_key_plan_cancels_loading_or_fatal_on_escape() {
    assert_eq!(
        blocked_load_key_plan(&OpLoadState::Loading { spinner_tick: 0 }, true),
        Some(OpPickerBlockedLoadKeyPlan::Cancel)
    );
    assert_eq!(
        blocked_load_key_plan(
            &OpLoadState::Error(OpPickerError::Fatal(OpPickerFatalState::NoVaults)),
            true,
        ),
        Some(OpPickerBlockedLoadKeyPlan::Cancel)
    );
}

#[test]
fn blocked_load_key_plan_continues_loading_or_fatal_on_other_keys() {
    assert_eq!(
        blocked_load_key_plan(&OpLoadState::Loading { spinner_tick: 0 }, false),
        Some(OpPickerBlockedLoadKeyPlan::Continue)
    );
    assert_eq!(
        blocked_load_key_plan(
            &OpLoadState::Error(OpPickerError::Fatal(OpPickerFatalState::NoVaults)),
            false,
        ),
        Some(OpPickerBlockedLoadKeyPlan::Continue)
    );
}

#[test]
fn blocked_load_key_plan_ignores_ready_and_recoverable_states() {
    assert_eq!(blocked_load_key_plan(&OpLoadState::Ready, true), None);
    assert_eq!(
        blocked_load_key_plan(&recoverable_load_error_state("temporary failure"), true),
        None
    );
}

#[test]
fn breadcrumb_omits_pane_type_suffix_multi_account() {
    let title = breadcrumb_title(
        OpPickerStage::Vault,
        true,
        "alice@example.com",
        "ignored",
        "ignored",
    );
    assert_eq!(title, "alice@example.com");
    assert!(!title.contains("Vaults"), "no `Vaults` suffix: {title}");

    let title = breadcrumb_title(
        OpPickerStage::Item,
        true,
        "alice@example.com",
        "Personal",
        "",
    );
    assert_eq!(title, "alice@example.com \u{2192} Personal");
    assert!(!title.contains("Items"));

    let title = breadcrumb_title(
        OpPickerStage::Field,
        true,
        "alice@example.com",
        "Personal",
        "API Keys",
    );
    assert_eq!(
        title,
        "alice@example.com \u{2192} Personal \u{2192} API Keys"
    );
    assert!(!title.contains("Fields"));
}

#[test]
fn breadcrumb_single_account_uses_brand_or_bare_context() {
    let v = breadcrumb_title(OpPickerStage::Vault, false, "", "Personal", "");
    assert_eq!(v, "1Password");

    let i = breadcrumb_title(OpPickerStage::Item, false, "", "Personal", "API Keys");
    assert_eq!(i, "Personal");

    let f = breadcrumb_title(OpPickerStage::Field, false, "", "Personal", "API Keys");
    assert_eq!(f, "Personal \u{2192} API Keys");
}

#[test]
fn breadcrumb_account_pane_is_bare_brand() {
    let title = breadcrumb_title(OpPickerStage::Account, true, "ignored", "", "");
    assert_eq!(title, "1Password");
}

#[test]
fn probe_error_message_classifies_operator_states() {
    assert!(matches!(
        classify_probe_error_message("failed to spawn op"),
        OpPickerError::Fatal(OpPickerFatalState::NotInstalled)
    ));
    assert!(matches!(
        classify_probe_error_message("not currently signed in"),
        OpPickerError::Fatal(OpPickerFatalState::NotSignedIn)
    ));
    assert!(matches!(
        classify_probe_error_message("boom"),
        OpPickerError::Fatal(OpPickerFatalState::GenericFatal { .. })
    ));
}

#[test]
fn probe_error_downcast_classifies_without_substring() {
    // Decoy messages prove the typed source wins over the fallback classifier.
    let not_installed = anyhow::Error::new(jackin_core::OpProbeError::NotInstalled {
        detail: "xyzzy".into(),
    })
    .context("xyzzy decoy — not a spawn phrase");
    assert!(matches!(
        classify_probe_error(&not_installed),
        OpPickerError::Fatal(OpPickerFatalState::NotInstalled)
    ));

    let not_signed = anyhow::Error::new(jackin_core::OpProbeError::NotSignedIn {
        detail: "xyzzy".into(),
    })
    .context("xyzzy decoy — not a signin phrase");
    assert!(matches!(
        classify_probe_error(&not_signed),
        OpPickerError::Fatal(OpPickerFatalState::NotSignedIn)
    ));

    let timeout = anyhow::Error::new(jackin_core::OpProbeError::Timeout { seconds: 9 })
        .context("xyzzy decoy timeout");
    assert!(matches!(
        classify_probe_error(&timeout),
        OpPickerError::Fatal(OpPickerFatalState::GenericFatal { .. })
    ));

    let other = anyhow::Error::new(jackin_core::OpProbeError::Other {
        message: "xyzzy".into(),
    });
    assert!(matches!(
        classify_probe_error(&other),
        OpPickerError::Fatal(OpPickerFatalState::GenericFatal { message }) if message.contains("xyzzy")
    ));
}

#[test]
fn field_rows_group_by_explicit_section_id_in_first_seen_order() {
    let sections = vec![
        OpSection {
            id: "opaque-auth-id".to_owned(),
            label: "Auth".to_owned(),
        },
        OpSection {
            id: "opaque-deploy-id".to_owned(),
            label: "Deploy".to_owned(),
        },
    ];
    let rows = field_display_rows_for_picker(
        &OpPickerMode::Browse,
        "",
        &[
            field("root", None, "op://Vault/Item/token"),
            field(
                "auth-password",
                Some("opaque-auth-id"),
                "op://Vault/Item/Auth/password",
            ),
            field(
                "deploy-key",
                Some("opaque-deploy-id"),
                "op://Vault/Item/Deploy/key",
            ),
            // Some CLI responses omit `section.id` even when the reference
            // section segment uniquely matches item section metadata.
            field("auth-otp", None, "op://Vault/Item/Auth/otp"),
        ],
        &sections,
        None,
        &HashSet::new(),
    );
    assert!(matches!(rows[0], FieldDisplayRow::Field { field_idx: 0 }));
    assert!(matches!(
        rows[1],
        FieldDisplayRow::SectionHeader {
            ref section_id,
            ref name,
            field_count: 2,
        } if section_id == "opaque-auth-id" && name == "Auth"
    ));
    assert!(matches!(
        rows[4],
        FieldDisplayRow::SectionHeader {
            ref section_id,
            ref name,
            field_count: 1,
        } if section_id == "opaque-deploy-id" && name == "Deploy"
    ));
}

#[test]
fn browse_field_rows_group_sections_and_respect_collapse() {
    let mut collapsed = HashSet::new();
    collapsed.insert("opaque-auth-id".to_owned());
    let rows = field_display_rows_for_picker(
        &OpPickerMode::Browse,
        "",
        &[
            field("root", None, "op://Vault/Item/root"),
            field(
                "password",
                Some("opaque-auth-id"),
                "op://Vault/Item/Auth/password",
            ),
            field("otp", Some("opaque-auth-id"), "op://Vault/Item/Auth/otp"),
            field(
                "key",
                Some("opaque-deploy-id"),
                "op://Vault/Item/Deploy/key",
            ),
        ],
        &[
            OpSection {
                id: "opaque-auth-id".to_owned(),
                label: "Auth".to_owned(),
            },
            OpSection {
                id: "opaque-deploy-id".to_owned(),
                label: "Deploy".to_owned(),
            },
        ],
        None,
        &collapsed,
    );
    assert!(matches!(rows[0], FieldDisplayRow::Field { field_idx: 0 }));
    assert!(matches!(
        rows[1],
        FieldDisplayRow::SectionHeader {
            ref section_id,
            ref name,
            field_count: 2
        } if section_id == "opaque-auth-id" && name == "Auth"
    ));
    assert!(matches!(
        rows[2],
        FieldDisplayRow::SectionHeader {
            ref section_id,
            ref name,
            field_count: 1
        } if section_id == "opaque-deploy-id" && name == "Deploy"
    ));
    assert!(matches!(rows[3], FieldDisplayRow::Field { field_idx: 3 }));
}

#[test]
fn create_field_rows_scope_to_section_and_add_sentinel() {
    let rows = field_display_rows_for_picker(
        &OpPickerMode::Create {
            item_name_default: String::new(),
            field_label_default: String::new(),
        },
        "",
        &[
            field("root", None, "op://Vault/Item/root"),
            field(
                "password",
                Some("opaque-auth-id"),
                "op://Vault/Item/Auth/password",
            ),
            field("otp", Some("opaque-auth-id"), "op://Vault/Item/Auth/otp"),
        ],
        &[OpSection {
            id: "opaque-auth-id".to_owned(),
            label: "Auth".to_owned(),
        }],
        Some("opaque-auth-id"),
        &HashSet::new(),
    );
    assert!(matches!(rows[0], FieldDisplayRow::Field { field_idx: 1 }));
    assert!(matches!(rows[1], FieldDisplayRow::Field { field_idx: 2 }));
    assert!(matches!(rows[2], FieldDisplayRow::NewFieldSentinel));
}

#[test]
fn selected_index_routes_by_visible_stage() {
    assert_eq!(
        selected_index_for_stage(
            OpPickerStage::Account,
            Some(1),
            Some(2),
            Some(3),
            Some(4),
            Some(5),
        ),
        Some(1)
    );
    assert_eq!(
        selected_index_for_stage(
            OpPickerStage::Field,
            Some(1),
            Some(2),
            Some(3),
            Some(4),
            Some(5),
        ),
        Some(5)
    );
    assert_eq!(
        selected_index_for_stage(
            OpPickerStage::FieldLabel,
            Some(1),
            Some(2),
            Some(3),
            Some(4),
            Some(5),
        ),
        None
    );
}

#[test]
fn naming_stage_input_routes_by_naming_stage() {
    let item = item_name_input_state("");
    let field = field_label_input_state("");
    let section = section_name_input_state("");

    assert_eq!(
        naming_stage_input_for_stage(OpPickerStage::NewItemName, &item, &field, &section)
            .map(TextInputState::label),
        Some("Item name")
    );
    assert_eq!(
        naming_stage_input_for_stage(OpPickerStage::FieldLabel, &item, &field, &section)
            .map(TextInputState::label),
        Some("Field label")
    );
    assert_eq!(
        naming_stage_input_for_stage(OpPickerStage::NewSectionName, &item, &field, &section)
            .map(TextInputState::label),
        Some("Section name")
    );
    assert!(naming_stage_input_for_stage(OpPickerStage::Field, &item, &field, &section).is_none());
}
