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

fn synthetic_picker_state(stage: OpPickerStage, load_state: OpLoadState) -> OpPickerState {
    let mut state = OpPickerState::new();
    state.pending_load = None;
    state.rx = None;
    state.mode = if matches!(
        stage,
        OpPickerStage::Item
            | OpPickerStage::Section
            | OpPickerStage::NewItemName
            | OpPickerStage::FieldLabel
            | OpPickerStage::NewSectionName
    ) {
        OpPickerMode::Create {
            item_name_default: "synthetic-item-name".to_owned(),
            field_label_default: "synthetic-field-label".to_owned(),
        }
    } else {
        OpPickerMode::Browse
    };
    state.stage = stage;
    state.load_state = load_state;
    state.accounts = (0..12)
        .map(|index| jackin_core::OpAccount {
            id: format!("account-{index:02}"),
            email: format!("account-{index:02}-marker@example.invalid"),
            url: format!("account-{index:02}.invalid"),
        })
        .collect();
    state.account_list_state.set_active(Some(11));
    state.selected_account = state.accounts.first().cloned();
    state.vaults = (0..12)
        .map(|index| jackin_core::OpVault {
            id: format!("vault-{index:02}"),
            name: format!("vault-{index:02}-marker"),
        })
        .collect();
    state.vault_list_state.set_active(Some(11));
    state.selected_vault = state.vaults.first().cloned();
    state.items = (0..12)
        .map(|index| jackin_core::OpItem {
            id: format!("item-{index:02}"),
            name: format!("item-{index:02}-marker"),
            subtitle: String::new(),
        })
        .collect();
    state.item_list_state.set_active(Some(11));
    state.selected_item = state.items.first().cloned();
    state.sections = (0..3)
        .map(|index| OpSection {
            id: format!("section-{index:02}"),
            label: format!("section-{index:02}-marker"),
        })
        .collect();
    state.section_list_state.set_active(Some(1));
    state.fields = (0..12)
        .map(|index| jackin_core::OpField {
            id: format!("field-{index:02}"),
            section_id: Some(format!("section-{:02}", index % 3)),
            label: format!("field-{index:02}-marker"),
            field_type: "STRING".to_owned(),
            concealed: index == 11,
            reference: if index == 11 {
                "synthetic-hidden-reference-marker".to_owned()
            } else {
                String::new()
            },
        })
        .collect();
    state.field_list_state.set_active(Some(11));
    state.selected_section = None;
    state.collapsed_sections.clear();
    state.filter_buf.clear();

    let accounts = state.accounts.clone();
    let vaults = state.vaults.clone();
    let items = state.items.clone();
    let fields = state.fields.clone();
    let sections = state.sections.clone();
    let mut cache = state.op_cache.borrow_mut();
    cache.put_accounts(accounts);
    cache.put_vaults(Some("account-00"), vaults);
    cache.put_items(Some("account-00"), "vault-00", items);
    cache.put_fields(Some("account-00"), "vault-00", "item-00", fields);
    cache.put_sections(Some("account-00"), "vault-00", "item-00", sections);
    drop(cache);
    state
}

fn paint_picker_at(
    state: &OpPickerState,
    area: ratatui::layout::Rect,
    terminal_size: ratatui::layout::Rect,
) -> ratatui::buffer::Buffer {
    use ratatui::{Terminal, backend::TestBackend};

    let mut terminal = Terminal::new(TestBackend::new(terminal_size.width, terminal_size.height))
        .expect("Op picker TestBackend should initialize");
    terminal
        .draw(|frame| render_picker(frame, area, state))
        .expect("Op picker should render to TestBackend");
    terminal.backend().buffer().clone()
}

fn painted_picker_position(
    buffer: &ratatui::buffer::Buffer,
    area: ratatui::layout::Rect,
    marker: &str,
) -> ratatui::layout::Position {
    (area.y..area.bottom())
        .find_map(|row| {
            let text = (area.x..area.right())
                .map(|column| buffer[(column, row)].symbol())
                .collect::<String>();
            let byte = text.find(marker)?;
            let column = termrock::text::display_cols(&text[..byte]);
            Some(ratatui::layout::Position::new(
                area.x.saturating_add(
                    u16::try_from(column).expect("painted Op picker marker column fits u16"),
                ),
                row,
            ))
        })
        .unwrap_or_else(|| panic!("actual Op picker TestBackend buffer lacks {marker:?}"))
}

#[derive(Debug, PartialEq, Eq)]
struct OpPickerObservation {
    stage: OpPickerStage,
    mode: String,
    load_state: String,
    filter: String,
    accounts: Vec<jackin_core::OpAccount>,
    vaults: Vec<jackin_core::OpVault>,
    items: Vec<jackin_core::OpItem>,
    fields: Vec<jackin_core::OpField>,
    sections: Vec<OpSection>,
    selected_account: Option<jackin_core::OpAccount>,
    selected_vault: Option<jackin_core::OpVault>,
    selected_item: Option<jackin_core::OpItem>,
    selected_section: Option<OpSection>,
    pending_section: String,
    account_active: Option<usize>,
    vault_active: Option<usize>,
    item_active: Option<usize>,
    section_active: Option<usize>,
    field_active: Option<usize>,
    collapsed_sections: HashSet<String>,
    item_name_input: String,
    field_label_input: String,
    section_name_input: String,
    field_label_origin: String,
    field_refresh_in_place: bool,
    pending_load: bool,
    receiver: bool,
}

fn op_picker_observation(state: &OpPickerState) -> OpPickerObservation {
    OpPickerObservation {
        stage: state.stage,
        mode: format!("{:?}", state.mode),
        load_state: format!("{:?}", state.load_state),
        filter: state.filter_buf.clone(),
        accounts: state.accounts.clone(),
        vaults: state.vaults.clone(),
        items: state.items.clone(),
        fields: state.fields.clone(),
        sections: state.sections.clone(),
        selected_account: state.selected_account.clone(),
        selected_vault: state.selected_vault.clone(),
        selected_item: state.selected_item.clone(),
        selected_section: state.selected_section.clone(),
        pending_section: format!("{:?}", state.pending_section),
        account_active: state.account_list_state.active().copied(),
        vault_active: state.vault_list_state.active().copied(),
        item_active: state.item_list_state.active().copied(),
        section_active: state.section_list_state.active().copied(),
        field_active: state.field_list_state.active().copied(),
        collapsed_sections: state.collapsed_sections.clone(),
        item_name_input: format!("{:?}", state.item_name_input),
        field_label_input: format!("{:?}", state.field_label_input),
        section_name_input: format!("{:?}", state.section_name_input),
        field_label_origin: format!("{:?}", state.field_label_origin),
        field_refresh_in_place: state.field_refresh_in_place,
        pending_load: state.pending_load.is_some(),
        receiver: state.rx.is_some(),
    }
}

fn assert_hit_probe_is_read_only(
    state: &OpPickerState,
    area: ratatui::layout::Rect,
    positions: &[ratatui::layout::Position],
    expected: bool,
) {
    let before = op_picker_observation(state);
    let cache = std::rc::Rc::clone(&state.op_cache);
    let cache_before = {
        let cache = cache.borrow();
        (
            cache.get_accounts(),
            cache.get_vaults(Some("account-00")),
            cache.get_items(Some("account-00"), "vault-00"),
            cache.get_fields(Some("account-00"), "vault-00", "item-00"),
            cache.get_sections(Some("account-00"), "vault-00", "item-00"),
        )
    };
    for position in positions {
        assert_eq!(
            selection_hit_at(area, state, *position),
            expected,
            "painted hit result at {position:?} for {:?}",
            state.stage
        );
    }
    assert_eq!(op_picker_observation(state), before);
    assert!(std::rc::Rc::ptr_eq(&cache, &state.op_cache));
    let cache_after = {
        let cache = state.op_cache.borrow();
        (
            cache.get_accounts(),
            cache.get_vaults(Some("account-00")),
            cache.get_items(Some("account-00"), "vault-00"),
            cache.get_fields(Some("account-00"), "vault-00", "item-00"),
            cache.get_sections(Some("account-00"), "vault-00", "item-00"),
        )
    };
    assert_eq!(cache_after, cache_before);
}

#[test]
fn op_picker_selection_hits_match_painted_lists() {
    use ratatui::layout::{Position, Rect};

    let terminal_size = Rect::new(0, 0, 110, 40);
    let area = Rect::new(9, 5, 88, 30);
    for stage in [
        OpPickerStage::Account,
        OpPickerStage::Vault,
        OpPickerStage::Item,
        OpPickerStage::Section,
        OpPickerStage::Field,
    ] {
        let state = synthetic_picker_state(stage, OpLoadState::Ready);
        let marker = match stage {
            OpPickerStage::Account => "account-11-marker@example.invalid",
            OpPickerStage::Vault => "vault-11-marker",
            OpPickerStage::Item => "item-11-marker",
            OpPickerStage::Section => "section-02-marker",
            OpPickerStage::Field => "field-11-marker",
            _ => unreachable!(),
        };
        let buffer = paint_picker_at(&state, area, terminal_size);
        let marker_position = painted_picker_position(&buffer, area, marker);
        assert_hit_probe_is_read_only(&state, area, &[marker_position], true);
    }

    let browse_field_state = synthetic_picker_state(OpPickerStage::Field, OpLoadState::Ready);
    let browse_field_buffer = paint_picker_at(&browse_field_state, area, terminal_size);
    let field_header = painted_picker_position(&browse_field_buffer, area, "section-02-marker");
    assert_hit_probe_is_read_only(&browse_field_state, area, &[field_header], true);

    let browse_concealed_field =
        painted_picker_position(&browse_field_buffer, area, "field-11-marker");
    assert_hit_probe_is_read_only(&browse_field_state, area, &[browse_concealed_field], true);

    let create_item_state = synthetic_picker_state(OpPickerStage::Item, OpLoadState::Ready);
    let create_item_buffer = paint_picker_at(&create_item_state, area, terminal_size);
    let new_item = painted_picker_position(&create_item_buffer, area, "+ New item");
    assert_hit_probe_is_read_only(&create_item_state, area, &[new_item], true);

    let create_section_state = synthetic_picker_state(OpPickerStage::Section, OpLoadState::Ready);
    let create_section_buffer = paint_picker_at(&create_section_state, area, terminal_size);
    let new_section = painted_picker_position(&create_section_buffer, area, "+ New section");
    assert_hit_probe_is_read_only(&create_section_state, area, &[new_section], true);

    let mut create_field_state = synthetic_picker_state(OpPickerStage::Field, OpLoadState::Ready);
    create_field_state.mode = OpPickerMode::Create {
        item_name_default: "synthetic-item-name".to_owned(),
        field_label_default: "synthetic-field-label".to_owned(),
    };
    create_field_state.selected_section = Some(create_field_state.sections[2].clone());
    create_field_state.field_list_state.set_active(Some(3));
    let create_field_buffer = paint_picker_at(&create_field_state, area, terminal_size);
    let create_field = painted_picker_position(&create_field_buffer, area, "field-11-marker");
    let new_field = painted_picker_position(&create_field_buffer, area, "+ New field");
    assert_hit_probe_is_read_only(&create_field_state, area, &[create_field, new_field], true);

    let vault_state = synthetic_picker_state(OpPickerStage::Vault, OpLoadState::Ready);
    let vault_buffer = paint_picker_at(&vault_state, area, terminal_size);
    let vault_row = painted_picker_position(&vault_buffer, area, "vault-11-marker");
    let blank_in_same_row = Position::new(
        vault_row.x + u16::try_from("vault-11-marker".len()).expect("marker length fits u16"),
        vault_row.y,
    );
    assert_eq!(
        vault_buffer[(blank_in_same_row.x, blank_in_same_row.y)].symbol(),
        " "
    );
    assert_hit_probe_is_read_only(&vault_state, area, &[blank_in_same_row], true);

    for load_state in [
        OpLoadState::Idle,
        OpLoadState::Ready,
        recoverable_load_error_state("synthetic recoverable banner"),
    ] {
        let state = synthetic_picker_state(OpPickerStage::Account, load_state);
        let buffer = paint_picker_at(&state, area, terminal_size);
        let row = painted_picker_position(&buffer, area, "account-11-marker@example.invalid");
        assert_hit_probe_is_read_only(&state, area, &[row], true);
    }

    let mut long_list = synthetic_picker_state(OpPickerStage::Account, OpLoadState::Ready);
    long_list.account_list_state.set_active(Some(11));
    let short_area = Rect::new(9, 5, 88, 14);
    let short_buffer = paint_picker_at(&long_list, short_area, terminal_size);
    let last_visible = painted_picker_position(
        &short_buffer,
        short_area,
        "account-11-marker@example.invalid",
    );
    assert_hit_probe_is_read_only(&long_list, short_area, &[last_visible], true);
    let scroll_gutter = Position::new(short_area.right() - 2, last_visible.y);
    assert_hit_probe_is_read_only(&long_list, short_area, &[scroll_gutter], false);

    let mut filtered = synthetic_picker_state(OpPickerStage::Account, OpLoadState::Ready);
    filtered.filter_buf = "11-marker".to_owned();
    filtered.account_list_state.set_active(Some(0));
    let filtered_buffer = paint_picker_at(&filtered, short_area, terminal_size);
    let filtered_row = painted_picker_position(
        &filtered_buffer,
        short_area,
        "account-11-marker@example.invalid",
    );
    assert_hit_probe_is_read_only(&filtered, short_area, &[filtered_row], true);

    let hidden_reference = "synthetic-hidden-reference-marker";
    let mut rendered = String::new();
    for row in area.y..area.bottom() {
        for column in area.x..area.right() {
            rendered.push_str(browse_field_buffer[(column, row)].symbol());
        }
    }
    assert!(!rendered.contains(hidden_reference));
    assert!(!rendered.contains("op://"));
}

#[test]
fn op_picker_nonlist_states_reject_selection_hits() {
    use ratatui::layout::{Position, Rect};

    let terminal_size = Rect::new(0, 0, 110, 40);
    let area = Rect::new(9, 5, 88, 24);
    let ready = synthetic_picker_state(OpPickerStage::Account, OpLoadState::Ready);
    let ready_buffer = paint_picker_at(&ready, area, terminal_size);
    let row = painted_picker_position(&ready_buffer, area, "account-00-marker@example.invalid");
    let last_row =
        painted_picker_position(&ready_buffer, area, "account-11-marker@example.invalid");
    let filter = painted_picker_position(&ready_buffer, area, "Filter");
    let spacer = Position::new(filter.x, filter.y + 1);
    let title_border = Position::new(area.x, area.y);
    let unused_body = Position::new(row.x, last_row.y + 1);
    assert!(area.contains(unused_body));
    assert_eq!(ready_buffer[(unused_body.x, unused_body.y)].symbol(), " ");
    let outside = Position::new(0, 0);
    assert_hit_probe_is_read_only(
        &ready,
        area,
        &[filter, spacer, title_border, unused_body, outside],
        false,
    );
    assert_hit_probe_is_read_only(
        &ready,
        Rect::new(area.x, area.y, 0, area.height),
        &[row],
        false,
    );
    assert_hit_probe_is_read_only(
        &ready,
        Rect::new(area.x, area.y, 1, 1),
        &[Position::new(area.x, area.y)],
        false,
    );

    let no_match = {
        let mut state = synthetic_picker_state(OpPickerStage::Account, OpLoadState::Ready);
        state.filter_buf = "no-synthetic-match".to_owned();
        state
    };
    let no_match_buffer = paint_picker_at(&no_match, area, terminal_size);
    let no_match_position = painted_picker_position(&no_match_buffer, area, "(no matches)");
    assert_hit_probe_is_read_only(&no_match, area, &[no_match_position], false);

    for load_state in [
        OpLoadState::Loading { spinner_tick: 2 },
        OpLoadState::Error(OpPickerError::Fatal(OpPickerFatalState::NotInstalled)),
        OpLoadState::Error(OpPickerError::Fatal(OpPickerFatalState::NotSignedIn)),
        OpLoadState::Error(OpPickerError::Fatal(OpPickerFatalState::NoVaults)),
        OpLoadState::Error(OpPickerError::Fatal(OpPickerFatalState::GenericFatal {
            message: "synthetic fatal".to_owned(),
        })),
    ] {
        let state = synthetic_picker_state(OpPickerStage::Account, load_state);
        let position = Position::new(area.x + area.width / 2, area.y + area.height / 2);
        assert_hit_probe_is_read_only(&state, area, &[position], false);
    }

    for stage in [
        OpPickerStage::NewItemName,
        OpPickerStage::FieldLabel,
        OpPickerStage::NewSectionName,
    ] {
        let state = synthetic_picker_state(stage, OpLoadState::Ready);
        let buffer = paint_picker_at(&state, area, terminal_size);
        let label = match stage {
            OpPickerStage::NewItemName => "Item name",
            OpPickerStage::FieldLabel => "Field label",
            OpPickerStage::NewSectionName => "Section name",
            _ => unreachable!(),
        };
        let input_position = painted_picker_position(&buffer, area, label);
        assert_hit_probe_is_read_only(&state, area, &[input_position], false);
    }
}
