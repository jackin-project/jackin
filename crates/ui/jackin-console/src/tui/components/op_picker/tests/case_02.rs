// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

#[test]
fn filter_reset_selection_routes_only_filterable_stages() {
    assert_eq!(
        filter_reset_selection_for_stage(OpPickerStage::Account, 2, 3, 4, 5),
        Some(Some(0))
    );
    assert_eq!(
        filter_reset_selection_for_stage(OpPickerStage::Item, 2, 3, 0, 5),
        Some(None)
    );
    assert_eq!(
        filter_reset_selection_for_stage(OpPickerStage::Section, 2, 3, 4, 5),
        None
    );
    assert_eq!(
        filter_reset_selection_for_stage(OpPickerStage::FieldLabel, 2, 3, 4, 5),
        None
    );
}

#[test]
fn field_stage_back_plan_preserves_create_mode_sections() {
    assert_eq!(
        field_stage_back_plan(&OpPickerMode::Create {
            item_name_default: String::new(),
            field_label_default: String::new(),
        }),
        FieldStageBackPlan {
            stage: OpPickerStage::Section,
            reset_selected_section: true,
            clear_fields: false,
            clear_collapsed_sections: false,
            clear_selected_item: false,
            reset_section_list: true,
        }
    );
    assert_eq!(
        field_stage_back_plan(&OpPickerMode::Browse),
        FieldStageBackPlan {
            stage: OpPickerStage::Item,
            reset_selected_section: false,
            clear_fields: true,
            clear_collapsed_sections: true,
            clear_selected_item: true,
            reset_section_list: false,
        }
    );
}

#[test]
fn field_stage_refresh_plan_tracks_create_mode_in_place_reload() {
    assert_eq!(
        field_stage_refresh_plan(&OpPickerMode::Browse),
        FieldStageRefreshPlan {
            clear_fields: true,
            reset_field_list: true,
            clear_collapsed_sections: true,
            refresh_in_place: false,
        }
    );
    assert_eq!(
        field_stage_refresh_plan(&OpPickerMode::Create {
            item_name_default: String::new(),
            field_label_default: String::new(),
        }),
        FieldStageRefreshPlan {
            clear_fields: true,
            reset_field_list: true,
            clear_collapsed_sections: true,
            refresh_in_place: true,
        }
    );
}

#[test]
fn section_stage_back_plan_returns_to_item() {
    assert_eq!(
        section_stage_back_plan(),
        SectionStageBackPlan {
            stage: OpPickerStage::Item,
            clear_fields: true,
            clear_collapsed_sections: true,
            clear_selected_section: true,
            clear_selected_item: true,
        }
    );
}

#[test]
fn section_stage_commit_plan_resolves_sentinel_and_choices() {
    let api = OpSection {
        id: "opaque-api-id".to_owned(),
        label: "api".to_owned(),
    };
    let choices = vec![None, Some(api.clone())];

    assert_eq!(
        section_stage_commit_plan(Some(0), &choices),
        SectionStageCommitPlan::ExistingSection {
            selected_section: None
        }
    );
    assert_eq!(
        section_stage_commit_plan(Some(1), &choices),
        SectionStageCommitPlan::ExistingSection {
            selected_section: Some(api)
        }
    );
    assert_eq!(
        section_stage_commit_plan(Some(2), &choices),
        SectionStageCommitPlan::NewSectionName
    );
    assert_eq!(
        section_stage_commit_plan(Some(3), &choices),
        SectionStageCommitPlan::NoSelection
    );
}

#[test]
fn item_stage_back_plan_returns_to_vault() {
    assert_eq!(
        item_stage_back_plan(),
        ItemStageBackPlan {
            stage: OpPickerStage::Vault,
            clear_items: true,
            clear_selected_item: true,
        }
    );
}

#[test]
fn item_stage_commit_plan_routes_existing_new_and_empty() {
    assert_eq!(
        item_stage_commit_plan(Some(Some("item"))),
        ItemStageCommitPlan::ExistingItem("item")
    );
    assert_eq!(
        item_stage_commit_plan::<&str>(Some(None)),
        ItemStageCommitPlan::NewItemName
    );
    assert_eq!(
        item_stage_commit_plan::<&str>(None),
        ItemStageCommitPlan::NoSelection
    );
}

#[test]
fn item_stage_refresh_plan_clears_loaded_state() {
    assert_eq!(
        item_stage_refresh_plan(),
        ItemStageRefreshPlan {
            clear_items: true,
            reset_item_list: true,
        }
    );
}

#[test]
fn vault_stage_back_plan_handles_single_and_multi_account() {
    assert_eq!(vault_stage_back_plan(1), VaultStageBackPlan::Cancel);
    assert_eq!(
        vault_stage_back_plan(2),
        VaultStageBackPlan::BackToAccount {
            stage: OpPickerStage::Account,
            clear_selected_vault: true,
            clear_vaults: true,
            reset_vault_list: true,
            ready_load_state: true,
        }
    );
}

#[test]
fn vault_stage_commit_plan_routes_existing_and_empty() {
    assert_eq!(
        vault_stage_commit_plan(Some("vault")),
        VaultStageCommitPlan::ExistingVault("vault")
    );
    assert_eq!(
        vault_stage_commit_plan::<&str>(None),
        VaultStageCommitPlan::NoSelection
    );
}

#[test]
fn vault_stage_refresh_plan_clears_loaded_state() {
    assert_eq!(
        vault_stage_refresh_plan(),
        VaultStageRefreshPlan {
            clear_vaults: true,
            reset_vault_list: true,
            clear_selected_vault: true,
        }
    );
}

#[test]
fn account_stage_refresh_plan_clears_loaded_state() {
    assert_eq!(
        account_stage_refresh_plan(),
        AccountStageRefreshPlan {
            clear_accounts: true,
            reset_account_list: true,
            clear_selected_account: true,
        }
    );
}

#[test]
fn account_stage_commit_plan_routes_existing_and_empty() {
    assert_eq!(
        account_stage_commit_plan(Some("account")),
        AccountStageCommitPlan::ExistingAccount("account")
    );
    assert_eq!(
        account_stage_commit_plan::<&str>(None),
        AccountStageCommitPlan::NoSelection
    );
}

#[test]
fn section_header_collapse_target_routes_only_headers() {
    let row = FieldDisplayRow::SectionHeader {
        section_id: "opaque-auth-id".to_owned(),
        name: "Auth".to_owned(),
        field_count: 2,
    };
    let mut collapsed = HashSet::new();

    assert_eq!(
        section_header_collapse_target(Some(&row), &collapsed, SectionCollapseIntent::Collapse),
        Some(("opaque-auth-id".to_owned(), true))
    );
    assert_eq!(
        section_header_collapse_target(Some(&row), &collapsed, SectionCollapseIntent::Expand),
        Some(("opaque-auth-id".to_owned(), false))
    );
    assert_eq!(
        section_header_collapse_target(Some(&row), &collapsed, SectionCollapseIntent::Toggle),
        Some(("opaque-auth-id".to_owned(), true))
    );

    collapsed.insert("opaque-auth-id".to_owned());
    assert_eq!(
        section_header_collapse_target(Some(&row), &collapsed, SectionCollapseIntent::Toggle),
        Some(("opaque-auth-id".to_owned(), false))
    );
    assert_eq!(
        section_header_collapse_target(
            Some(&FieldDisplayRow::NewFieldSentinel),
            &collapsed,
            SectionCollapseIntent::Toggle,
        ),
        None
    );
}

#[test]
fn field_stage_commit_plan_routes_row_kinds() {
    let row = FieldDisplayRow::SectionHeader {
        section_id: "opaque-auth-id".to_owned(),
        name: "Auth".to_owned(),
        field_count: 2,
    };
    let collapsed = HashSet::new();
    let auth = OpSection {
        id: "opaque-auth-id".to_owned(),
        label: "Auth".to_owned(),
    };
    assert_eq!(
        field_stage_commit_plan(Some(&row), &collapsed, Some(&auth)),
        FieldStageCommitPlan::ToggleSection {
            section_id: "opaque-auth-id".to_owned(),
            collapsed: true,
        }
    );

    assert_eq!(
        field_stage_commit_plan(
            Some(&FieldDisplayRow::Field { field_idx: 3 }),
            &collapsed,
            Some(&auth),
        ),
        FieldStageCommitPlan::ExistingField { field_idx: 3 }
    );
    assert_eq!(
        field_stage_commit_plan(Some(&FieldDisplayRow::NewFieldSentinel), &collapsed, None),
        FieldStageCommitPlan::NewField {
            pending_section: None,
            field_label_origin: FieldLabelOrigin::NewField,
            stage: OpPickerStage::FieldLabel,
        }
    );
    assert_eq!(
        field_stage_commit_plan(Some(&FieldDisplayRow::NewSectionSentinel), &collapsed, None),
        FieldStageCommitPlan::NoSelection
    );
}

#[test]
fn naming_stage_plans_name_next_stage_and_pending_section() {
    assert_eq!(
        new_item_name_commit_plan(),
        NamingStagePlan {
            stage: OpPickerStage::FieldLabel,
            field_label_origin: Some(FieldLabelOrigin::NewItem),
            pending_section: None,
            clear_pending_section: false,
        }
    );
    assert_eq!(
        new_section_name_commit_plan("  Deploy  "),
        NamingStagePlan {
            stage: OpPickerStage::FieldLabel,
            field_label_origin: Some(FieldLabelOrigin::NewSection),
            pending_section: Some(OpSectionTarget::NewLabel("Deploy".to_owned())),
            clear_pending_section: false,
        }
    );
    assert_eq!(
        field_label_cancel_plan(FieldLabelOrigin::NewField),
        NamingStagePlan {
            stage: OpPickerStage::Field,
            field_label_origin: None,
            pending_section: None,
            clear_pending_section: true,
        }
    );
}

#[test]
fn matches_filter_accepts_empty_or_any_matching_value() {
    assert!(matches_filter("", ["anything"]));
    assert!(matches_filter("api", ["Stripe", "API token"]));
    assert!(matches_filter(
        "example",
        ["alice@example.com", "https://example.test"]
    ));
    assert!(!matches_filter("missing", ["one", "two"]));
}

#[test]
fn build_op_picker_ref_uses_uuid_op_and_clean_path_for_unique_item() {
    let built = build_op_picker_ref(
        OpPickerVaultRef {
            id: "v_uuid",
            name: "Private",
        },
        OpPickerItemRef {
            id: "i_uuid",
            name: "Stripe",
            subtitle: "",
        },
        [OpPickerItemRef {
            id: "i_uuid",
            name: "Stripe",
            subtitle: "",
        }],
        OpPickerFieldRef {
            id: "f_uuid",
            label: "api key",
            reference: "op://Private/Stripe/api key",
            section_id: None,
            section_label: None,
        },
        [OpPickerFieldRef {
            id: "f_uuid",
            label: "api key",
            reference: "op://Private/Stripe/api key",
            section_id: None,
            section_label: None,
        }],
        &[],
    )
    .expect("fixture IDs form a valid secret reference");
    assert_eq!(built.op, "op://v_uuid/i_uuid/f_uuid");
    assert_eq!(built.path, "Private/Stripe/api key");
    assert!(!built.empty_reference_with_sibling_refs);
}
