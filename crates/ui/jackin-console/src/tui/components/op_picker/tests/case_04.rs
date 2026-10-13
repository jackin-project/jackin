// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

#[test]
fn existing_field_commit_selection_builds_component_owned_selection_shape() {
    let section = OpSection {
        id: "opaque-api-id".to_owned(),
        label: "api".to_owned(),
    };
    let selection = existing_field_commit_selection(
        ExistingFieldCommitPlan::EditItemField {
            section: Some(OpSectionTarget::Existing(section.clone())),
            field_id: "field-id".to_owned(),
            field_label: "token".to_owned(),
        },
        ExistingFieldCommitSelectionInput {
            account: Some("account"),
            vault: "vault",
            item: "item",
        },
        || "op://unused",
        |id, label| (id, label),
    );
    assert_eq!(
        selection,
        OpPickerSelection::EditItemField {
            account: Some("account"),
            vault: "vault",
            item: "item",
            section: Some(OpSectionTarget::Existing(section)),
            field: ("field-id".to_owned(), "token".to_owned()),
        }
    );

    let selection = existing_field_commit_selection::<&str, &str, &str, &str, (String, String)>(
        ExistingFieldCommitPlan::ExistingReference,
        ExistingFieldCommitSelectionInput {
            account: None,
            vault: "vault",
            item: "item",
        },
        || "op://vault/item/field",
        |id, label| (id, label),
    );
    assert_eq!(
        selection,
        OpPickerSelection::Existing("op://vault/item/field")
    );
}

#[test]
fn stage_classification_separates_naming_and_filterable_lists() {
    assert!(OpPickerStage::FieldLabel.is_naming());
    assert!(OpPickerStage::NewSectionName.is_naming());
    assert!(!OpPickerStage::Field.is_naming());

    assert!(OpPickerStage::Account.is_filterable());
    assert!(OpPickerStage::Field.is_filterable());
    assert!(!OpPickerStage::Section.is_filterable());
    assert!(!OpPickerStage::FieldLabel.is_filterable());
}

#[test]
fn account_load_completion_plan_keeps_root_adapter_out_of_transition_policy() {
    assert_eq!(accounts_loaded_plan(0), AccountsLoadedPlan::NotSignedIn);
    assert_eq!(
        accounts_loaded_plan(1),
        AccountsLoadedPlan::SelectSingleAccount
    );
    assert_eq!(accounts_loaded_plan(2), AccountsLoadedPlan::ShowAccountPane);
}

#[test]
fn vault_item_and_field_load_completion_plans_keep_root_adapter_out_of_transition_policy() {
    assert_eq!(vaults_loaded_plan(0), VaultsLoadedPlan::NoVaults);
    assert_eq!(
        vaults_loaded_plan(2),
        VaultsLoadedPlan::ShowVaultPane { selected: Some(0) }
    );

    assert_eq!(items_loaded_plan(0).selected, None);
    assert_eq!(items_loaded_plan(3).selected, Some(0));

    assert_eq!(
        fields_loaded_plan(&OpPickerMode::Browse, false, 2, 4),
        FieldsLoadedPlan::ShowFieldPane {
            field_selected: Some(0),
            clear_selected_section: true,
        }
    );
    assert_eq!(
        fields_loaded_plan(
            &OpPickerMode::Create {
                item_name_default: String::new(),
                field_label_default: String::new(),
            },
            false,
            2,
            4,
        ),
        FieldsLoadedPlan::ShowSectionPane {
            stage: OpPickerStage::Section,
            section_selected: Some(0),
            clear_selected_section: true,
        }
    );
    assert_eq!(
        fields_loaded_plan(
            &OpPickerMode::Create {
                item_name_default: String::new(),
                field_label_default: String::new(),
            },
            true,
            2,
            4,
        ),
        FieldsLoadedPlan::RefreshFieldPane {
            field_selected: Some(0),
            clear_refresh_in_place: true,
        }
    );
}

#[test]
fn refresh_completion_resets_selection_when_rows_change() {
    assert_eq!(items_loaded_plan(1).selected, Some(0));
    assert_eq!(items_loaded_plan(0).selected, None);

    assert_eq!(
        fields_loaded_plan(&OpPickerMode::Browse, true, 1, 1),
        FieldsLoadedPlan::RefreshFieldPane {
            field_selected: Some(0),
            clear_refresh_in_place: true,
        }
    );
    assert_eq!(
        fields_loaded_plan(&OpPickerMode::Browse, true, 1, 0),
        FieldsLoadedPlan::RefreshFieldPane {
            field_selected: None,
            clear_refresh_in_place: true,
        }
    );
}

#[test]
fn recoverable_banner_preserves_selected_list_geometry() {
    let mut state = RenderStateFixture::new(OpPickerStage::Account, Some(1));
    state.load_state = recoverable_load_error_state("temporary op failure");
    let buffer = render_picker_buffer(&state, 60, 9);
    let selected_y = (0..9)
        .find(|y| {
            (0..60)
                .map(|x| buffer[(x, *y)].symbol())
                .collect::<String>()
                .contains("bob@example.com")
        })
        .expect("selected account should remain visible below the banner");

    assert!(
        selected_y > 4,
        "selected row should render in the list area below banner/filter rows"
    );
}

#[test]
fn field_load_sort_policy_puts_concealed_fields_first() {
    #[derive(Debug, PartialEq, Eq)]
    struct Field {
        label: &'static str,
        concealed: bool,
    }

    let mut fields = vec![
        Field {
            label: "plain",
            concealed: false,
        },
        Field {
            label: "secret",
            concealed: true,
        },
    ];

    sort_fields_by_concealed_first(&mut fields, |field| field.concealed);

    assert_eq!(fields[0].label, "secret");
    assert_eq!(fields[1].label, "plain");
}

#[test]
fn selected_account_helpers_derive_cache_key_from_selected_account() {
    struct Account {
        id: &'static str,
    }

    let account = Account { id: "acct_1" };

    assert_eq!(
        selected_account_id(Some(&account), |account| account.id),
        Some("acct_1".to_owned())
    );
    assert_eq!(
        selected_account_id_ref(Some(&account), |account| account.id),
        Some("acct_1")
    );
    assert_eq!(
        selected_account_id::<Account>(None, |account| account.id),
        None
    );
    assert_eq!(
        selected_account_id_ref::<Account>(None, |account| account.id),
        None
    );
}

#[test]
fn selected_entity_id_or_default_derives_or_falls_back_empty() {
    struct Vault {
        id: &'static str,
    }

    let vault = Vault { id: "vault_1" };

    assert_eq!(
        selected_entity_id_or_default(Some(&vault), |vault| vault.id),
        "vault_1"
    );
    assert_eq!(
        selected_entity_id_or_default::<Vault>(None, |vault| vault.id),
        ""
    );
}

#[test]
fn selected_entity_label_or_empty_derives_or_falls_back_empty() {
    struct Item {
        name: &'static str,
    }

    let item = Item { name: "login" };

    assert_eq!(
        selected_entity_label_or_empty(Some(&item), |item| item.name),
        "login"
    );
    assert_eq!(
        selected_entity_label_or_empty::<Item>(None, |item| item.name),
        ""
    );
}
