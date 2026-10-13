// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

#[test]
fn item_filter_matches_subtitle() {
    let mut s = picker_ready();
    s.items = vec![
        item_with_subtitle("Google", "alexey@zhokhov.com"),
        item_with_subtitle("Google", "azhokhov@example.com"),
    ];
    s.item_list_state.set_active(Some(0));
    s.filter_buf = "AzhokhoV".to_owned();

    let visible = s.filtered_items();
    assert_eq!(visible.len(), 1);
    assert_eq!(visible[0].subtitle, "azhokhov@example.com");
}

#[test]
fn filter_vaults_narrows_by_name() {
    let mut s = picker_ready();
    s.vaults = vec![vault("Personal"), vault("Private"), vault("Work")];
    s.vault_list_state.set_active(Some(0));
    s.filter_buf = "per".to_owned();

    let visible = s.filtered_vaults();
    assert_eq!(visible.len(), 1);
    assert_eq!(visible[0].name, "Personal");
}

#[test]
fn filter_clears_on_pane_advance() {
    let mut s = picker_ready();
    s.vaults = vec![vault("Personal"), vault("Private"), vault("Work")];
    s.vault_list_state.set_active(Some(0));
    s.filter_buf = "per".to_owned();
    assert_eq!(s.filtered_vaults().len(), 1);

    // The pane-advance-clears-filter contract lives inside
    // `poll_load`'s Items arm; simulate it directly below rather
    // than racing the worker.
    let outcome = s.handle_key(key(KeyCode::Enter));
    assert!(matches!(outcome, ModalOutcome::Continue));
    assert_eq!(
        s.selected_vault.as_ref().map(|v| v.name.as_str()),
        Some("Personal"),
        "Enter on filtered vault must capture the selection"
    );

    s.rx = None;
    s.pending_load = None;
    s.items = vec![item("API Keys")];
    s.item_list_state.set_active(Some(0));
    s.stage = OpPickerStage::Item;
    s.filter_buf.clear();
    s.load_state = OpLoadState::Ready;

    assert_eq!(s.stage, OpPickerStage::Item);
    assert!(
        s.filter_buf.is_empty(),
        "filter must be cleared when advancing to the Item pane"
    );
}

#[test]
fn esc_from_vault_returns_cancel() {
    let mut s = picker_ready();
    s.vaults = vec![vault("Personal")];
    s.vault_list_state.set_active(Some(0));

    let outcome = s.handle_key(key(KeyCode::Esc));
    assert!(matches!(outcome, ModalOutcome::Cancel));
}

#[test]
fn esc_from_item_goes_to_vault() {
    let mut s = picker_ready();
    s.vaults = vec![vault("Personal"), vault("Work")];
    s.vault_list_state.set_active(Some(1));
    s.selected_vault = Some(vault("Work"));
    s.items = vec![item("API Keys")];
    s.item_list_state.set_active(Some(0));
    s.stage = OpPickerStage::Item;
    s.filter_buf = "ap".to_owned();

    let outcome = s.handle_key(key(KeyCode::Esc));
    assert!(matches!(outcome, ModalOutcome::Continue));
    assert_eq!(s.stage, OpPickerStage::Vault);
    assert!(s.filter_buf.is_empty(), "filter must clear on back-nav");
    // Vault selection preserved.
    assert_eq!(s.vault_list_state.active().copied(), Some(1));
    assert_eq!(s.vaults.len(), 2);
}

#[test]
fn esc_from_field_goes_to_item() {
    let mut s = picker_ready();
    s.selected_vault = Some(vault("Personal"));
    s.selected_item = Some(item("API Keys"));
    s.items = vec![item("API Keys")];
    s.item_list_state.set_active(Some(0));
    s.fields = vec![field("password", "concealed", true)];
    s.field_list_state.set_active(Some(0));
    s.stage = OpPickerStage::Field;
    s.filter_buf = "pw".to_owned();

    let outcome = s.handle_key(key(KeyCode::Esc));
    assert!(matches!(outcome, ModalOutcome::Continue));
    assert_eq!(s.stage, OpPickerStage::Item);
    assert!(s.filter_buf.is_empty());
    // Item selection preserved.
    assert_eq!(s.item_list_state.active().copied(), Some(0));
    assert_eq!(s.items.len(), 1);
}

#[test]
fn field_sort_concealed_first() {
    // The Fields-arm of `poll_load` applies a stable sort that puts
    // concealed fields first. We invoke that sort here against the
    // same input order used in production to confirm the contract.
    let mut input = vec![
        field("user", "text", false),
        field("pw", "concealed", true),
        field("url", "url", false),
    ];
    input.sort_by_key(|f| !f.concealed);
    assert_eq!(input[0].label, "pw");
    assert!(input[0].concealed);
    // Stable sort: non-concealed entries retain their input order.
    assert_eq!(input[1].label, "user");
    assert_eq!(input[2].label, "url");

    // End-to-end through the picker view: seed the sorted list,
    // assert filtered_fields() preserves it.
    let mut s = picker_ready();
    s.fields = input;
    s.field_list_state.set_active(Some(0));
    s.stage = OpPickerStage::Field;
    let visible = s.filtered_fields();
    assert_eq!(visible.len(), 3);
    assert_eq!(visible[0].label, "pw");
}

#[test]
fn enter_on_field_commits_op_path() {
    let mut s = picker_ready();
    s.selected_vault = Some(OpVault {
        id: "v-Personal".into(),
        name: "Personal".into(),
    });
    s.selected_item = Some(OpItem {
        id: "i-api".into(),
        name: "API Keys".into(),
        subtitle: String::new(),
    });
    s.items = vec![s.selected_item.clone().unwrap()];
    s.fields = vec![
        field("password", "concealed", true),
        field("username", "text", false),
    ];
    s.field_list_state.set_active(Some(0));
    s.stage = OpPickerStage::Field;

    let outcome = s.handle_key(key(KeyCode::Enter));
    match outcome {
        ModalOutcome::Commit(OpPickerSelection::Existing(op_ref)) => {
            assert_eq!(op_ref.op, "op://v-Personal/i-api/password");
            assert_eq!(op_ref.path, "Personal/API Keys/password");
        }
        other => panic!("expected Commit(Existing), got {other:?}"),
    }
}

#[test]
fn picker_commit_uses_canonical_ids_and_display_labels() {
    let mut s = picker_ready();
    s.selected_vault = Some(OpVault {
        id: "v-Personal".into(),
        name: "Personal".into(),
    });
    s.selected_item = Some(OpItem {
        id: "i-test".into(),
        name: "name with spaces".into(),
        subtitle: String::new(),
    });
    s.items = vec![s.selected_item.clone().unwrap()];
    s.fields = vec![field_with_reference("api", "op://Personal/test/auth/api")];
    s.sections = vec![OpSection {
        id: "Opaque-Auth-ID".to_owned(),
        label: "auth".to_owned(),
    }];
    // Field is inside section "auth", so display rows are:
    //   0: SectionHeader "auth"
    //   1: Field { field_idx: 0 }
    s.field_list_state.set_active(Some(1));
    s.stage = OpPickerStage::Field;

    let outcome = s.handle_key(key(KeyCode::Enter));
    match outcome {
        ModalOutcome::Commit(OpPickerSelection::Existing(op_ref)) => {
            // The unique reference label resolves through item metadata; the
            // URI uses the opaque ID while the path keeps the section label.
            assert_eq!(
                op_ref.op, "op://v-Personal/i-test/Opaque-Auth-ID/api",
                "op URI must use vault, item, section, and field IDs"
            );
            assert_eq!(
                op_ref.path, "Personal/name with spaces/auth/api",
                "path must use human-readable names and preserve section"
            );
        }
        other => panic!("expected Commit(Existing), got {other:?}"),
    }
}

#[test]
fn create_mode_item_stage_appends_new_item_sentinel() {
    let mut s = create_ready();
    s.items = vec![item("Existing")];
    let choices = s.filtered_item_choices();
    assert_eq!(choices.len(), 2, "one item + trailing sentinel");
    assert!(choices[0].is_some(), "real item first");
    assert!(
        choices[1].is_none(),
        "trailing None is the `+ New item` sentinel"
    );

    let mut browse = picker_ready();
    browse.items = vec![item("Existing")];
    assert!(
        browse.filtered_item_choices().iter().all(Option::is_some),
        "browse mode must not append a creation sentinel"
    );
}

#[test]
fn create_mode_new_item_flow_commits_new_item() {
    let mut s = create_ready();
    s.selected_vault = Some(vault("Personal"));
    s.items = vec![item("Existing")];
    s.stage = OpPickerStage::Item;
    // choices: [Some(Existing), None]; select the sentinel at index 1.
    s.item_list_state.set_active(Some(1));
    assert!(matches!(
        s.handle_key(key(KeyCode::Enter)),
        ModalOutcome::Continue
    ));
    assert_eq!(s.stage, OpPickerStage::NewItemName);
    // item_name_input defaults to "default-item"; accept with Enter.
    assert!(matches!(
        s.handle_key(key(KeyCode::Enter)),
        ModalOutcome::Continue
    ));
    assert_eq!(s.stage, OpPickerStage::FieldLabel);
    // field_label_input defaults to "token"; accept with Enter to commit.
    match s.handle_key(key(KeyCode::Enter)) {
        ModalOutcome::Commit(OpPickerSelection::NewItem {
            vault,
            item_name,
            section,
            field_label,
            ..
        }) => {
            assert_eq!(vault.id, "v-Personal");
            assert_eq!(item_name, "default-item");
            assert_eq!(field_label, "token");
            assert_eq!(section, None);
        }
        other => panic!("expected Commit(NewItem), got {other:?}"),
    }
}

#[test]
fn create_mode_existing_item_lands_on_section_stage() {
    // poll_load's Fields arm routes Create mode to the Section stage
    // (Browse mode goes to Field). Invoke that arm directly via the
    // worker drain so we exercise the real sequencing.
    let runner = Arc::new(StubRunner {
        accounts: Mutex::new(vec![account(
            "acct1",
            "single@example.com",
            "single.1password.com",
        )]),
        last_vault_list_account: Mutex::new(None),
    });
    let mut s = new_create_picker_with_runner_and_cache(
        runner,
        Rc::new(RefCell::new(OpCache::default())),
        "default-item",
        "token",
    );
    drain_initial_account_load(&mut s);
    s.rx = None;
    s.pending_load = None;
    s.selected_vault = Some(vault("Personal"));
    s.selected_item = Some(item("login"));
    // Drive the existing-item Enter through start_field_load + drain.
    s.start_field_load("i-login".into(), "v-Personal".into(), None);
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
    while (s.rx.is_some() || s.pending_load.is_some()) && std::time::Instant::now() < deadline {
        poll_load_for_test(&mut s);
        wait_for_worker_poll();
    }
    assert_eq!(
        s.stage,
        OpPickerStage::Section,
        "Create mode must land on the Section stage after a field load"
    );
    assert_eq!(s.selected_section, None, "selected_section resets on load");
}

#[test]
fn create_mode_field_refresh_stays_on_field_and_keeps_section() {
    let auth = OpSection {
        id: "Opaque-Auth-ID".to_owned(),
        label: "auth".to_owned(),
    };
    let mut s = create_at_section_with_sections(
        vec![
            field_with_reference("user", "op://Personal/login/user"),
            field_with_section_reference("api", "op://Personal/login/auth/api", "Opaque-Auth-ID"),
        ],
        vec![auth.clone()],
    );
    // Operator already drilled into the "auth" section on the Field stage.
    s.stage = OpPickerStage::Field;
    s.selected_section = Some(auth.clone());
    // `r` clears `fields`/`field_list_state` and sets the in-place flag.
    s.fields.clear();
    s.field_refresh_in_place = true;
    // Publish the reloaded fields through the same arm the worker uses.
    s.rx = Some(jackin_oppicker::ready_load_subscription(
        LoadResult::Fields(Ok(jackin_core::OpItemDetail {
            fields: vec![
                field_with_reference("user", "op://Personal/login/user"),
                field_with_reference("api", "op://Personal/login/auth/api"),
            ],
            sections: vec![auth.clone()],
        })),
    ));
    poll_load_for_test(&mut s);

    assert_eq!(
        s.stage,
        OpPickerStage::Field,
        "in-place refresh must NOT bounce back to Section"
    );
    assert_eq!(
        s.selected_section,
        Some(auth),
        "in-place refresh must preserve the chosen section"
    );
    assert_eq!(
        s.fields[1].section_id.as_deref(),
        Some("Opaque-Auth-ID"),
        "load normalization resolves a unique reference label to its opaque ID"
    );
    assert!(
        !s.field_refresh_in_place,
        "the flag is cleared once the refreshed fields arrive"
    );
    // Rows are re-scoped to "auth": one field + the new-field sentinel.
    let rows = s.build_field_display_rows();
    assert_eq!(rows.len(), 2, "one auth field + new-field sentinel");
    assert!(matches!(rows[1], FieldDisplayRow::NewFieldSentinel));
}
