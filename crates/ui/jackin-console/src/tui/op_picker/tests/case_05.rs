// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

#[test]
fn parity_ambiguous_item_with_subtitle_cli_matches_picker() {
    let field = OpField {
        id: "f_uuid".into(),
        section_id: None,
        label: "auth token".into(),
        reference: "op://Private/Claude/auth token".into(),
        field_type: "concealed".into(),
        concealed: true,
    };
    let item_a = OpItem {
        id: "i_uuid_a".into(),
        name: "Claude".into(),
        subtitle: "alexey@zhokhov.com".into(),
    };
    let item_b = OpItem {
        id: "i_uuid_b".into(),
        name: "Claude".into(),
        subtitle: "alexey@chainargos.com".into(),
    };
    let state = test_state_picked(
        OpVault {
            id: "v_uuid".into(),
            name: "Private".into(),
        },
        vec![item_a.clone(), item_b],
        item_a,
        field.clone(),
    );
    let picker_ref = state
        .build_op_ref_on_commit(&field)
        .expect("fixture IDs form a valid secret reference");

    let stub = ParityStub::new()
        .with_vault("Private", "v_uuid")
        .with_item("v_uuid", "Claude", "i_uuid_a", "alexey@zhokhov.com")
        .with_item("v_uuid", "Claude", "i_uuid_b", "alexey@chainargos.com")
        .with_field_with_reference(
            "i_uuid_a",
            "auth token",
            "f_uuid",
            true,
            "op://Private/Claude/auth token",
        );
    let cli_ref = resolve_op_uri_to_ref(
        "op://Private/Claude[alexey@zhokhov.com]/auth token",
        &stub,
        None,
    )
    .unwrap();

    assert_eq!(cli_ref.op, picker_ref.op, "op URI must match");
    assert_eq!(cli_ref.path, picker_ref.path, "display path must match");
}

#[test]
fn parity_sectioned_field_cli_matches_picker() {
    let field = OpField {
        id: "f_uuid".into(),
        section_id: Some("s_security_uuid".into()),
        label: "auth token".into(),
        reference: "op://Private/Claude/Security/auth token".into(),
        field_type: "concealed".into(),
        concealed: true,
    };
    let the_item = OpItem {
        id: "i_uuid".into(),
        name: "Claude".into(),
        subtitle: String::new(),
    };
    let mut state = test_state_picked(
        OpVault {
            id: "v_uuid".into(),
            name: "Private".into(),
        },
        vec![the_item.clone()],
        the_item,
        field.clone(),
    );
    let security = OpSection {
        id: "s_security_uuid".to_owned(),
        label: "Security".to_owned(),
    };
    state.sections = vec![security.clone()];
    let picker_ref = state
        .build_op_ref_on_commit(&field)
        .expect("fixture IDs form a valid secret reference");

    let stub = ParityStub::new()
        .with_vault("Private", "v_uuid")
        .with_item("v_uuid", "Claude", "i_uuid", "")
        .with_section("i_uuid", security)
        .with_section_field_with_reference(
            "i_uuid",
            "auth token",
            "f_uuid",
            true,
            "s_security_uuid",
            "op://Private/Claude/Security/auth token",
        );
    let cli_ref =
        resolve_op_uri_to_ref("op://Private/Claude/Security/auth token", &stub, None).unwrap();

    assert_eq!(cli_ref.op, picker_ref.op, "op URI must match");
    assert_eq!(cli_ref.path, picker_ref.path, "display path must match");
}

#[test]
fn parity_3seg_input_with_sectioned_field_cli_matches_picker() {
    let field = OpField {
        id: "f_uuid".into(),
        section_id: Some("s_security_uuid".into()),
        label: "auth token".into(),
        reference: "op://Private/Claude/Security/auth token".into(),
        field_type: "concealed".into(),
        concealed: true,
    };
    let the_item = OpItem {
        id: "i_uuid".into(),
        name: "Claude".into(),
        subtitle: String::new(),
    };
    let mut state = test_state_picked(
        OpVault {
            id: "v_uuid".into(),
            name: "Private".into(),
        },
        vec![the_item.clone()],
        the_item,
        field.clone(),
    );
    let security = OpSection {
        id: "s_security_uuid".to_owned(),
        label: "Security".to_owned(),
    };
    state.sections = vec![security.clone()];
    let picker_ref = state
        .build_op_ref_on_commit(&field)
        .expect("fixture IDs form a valid secret reference");

    // CLI path: 3-segment input, but field.reference has "Security"
    let stub = ParityStub::new()
        .with_vault("Private", "v_uuid")
        .with_item("v_uuid", "Claude", "i_uuid", "")
        .with_section("i_uuid", security)
        .with_section_field_with_reference(
            "i_uuid",
            "auth token",
            "f_uuid",
            true,
            "s_security_uuid",
            "op://Private/Claude/Security/auth token",
        );
    let cli_ref = resolve_op_uri_to_ref("op://Private/Claude/auth token", &stub, None).unwrap();

    assert_eq!(cli_ref.op, picker_ref.op, "op URI must match");
    assert_eq!(cli_ref.path, picker_ref.path, "display path must match");
}

#[test]
fn invalidate_cache_for_ref_drops_items_and_fields() {
    use jackin_core::OpRef;
    let cache = Rc::new(RefCell::new(OpCache::default()));
    let account = Some("ACCT");
    cache.borrow_mut().put_items(
        account,
        "v1",
        vec![OpItem {
            id: "i1".into(),
            name: "Claude".into(),
            subtitle: String::new(),
        }],
    );
    cache.borrow_mut().put_fields(
        account,
        "v1",
        "i1",
        vec![OpField {
            id: "f1".into(),
            section_id: None,
            label: "token".into(),
            field_type: "CONCEALED".into(),
            concealed: true,
            reference: String::new(),
        }],
    );

    invalidate_cache_for_ref(
        &cache,
        &OpRef {
            op: "op://v1/i1/f1".into(),
            path: "Work/Claude/token".into(),
            account: Some("ACCT".into()),
            on_demand: false,
        },
    );

    assert!(cache.borrow().get_items(account, "v1").is_none());
    assert!(cache.borrow().get_fields(account, "v1", "i1").is_none());
}

#[test]
fn invalidate_cache_for_ref_ignores_unparseable_ref() {
    use jackin_core::OpRef;
    let cache = Rc::new(RefCell::new(OpCache::default()));
    invalidate_cache_for_ref(
        &cache,
        &OpRef {
            op: "not-a-ref".into(),
            path: String::new(),
            account: None,
            on_demand: false,
        },
    );
}

#[test]
fn breadcrumb_title_content_matrix_single_account() {
    let mut s = picker_ready();
    s.selected_account = Some(account(
        "acct1",
        "single@example.com",
        "single.1password.com",
    ));
    s.selected_vault = Some(vault("Personal"));
    s.selected_item = Some(item("Login"));

    s.stage = OpPickerStage::Account;
    assert_modal_title(&s, "1Password");
    s.stage = OpPickerStage::Vault;
    assert_modal_title(&s, "1Password");
    s.stage = OpPickerStage::Item;
    assert_modal_title(&s, "Personal");
    s.stage = OpPickerStage::Section;
    assert_modal_title(&s, "Personal \u{2192} Login");
    s.stage = OpPickerStage::Field;
    assert_modal_title(&s, "Personal \u{2192} Login");
}

#[test]
fn breadcrumb_title_content_matrix_multi_account() {
    let mut s = picker_ready();
    s.accounts = vec![
        account("acct1", "alice@example.com", "alice.1password.com"),
        account("acct2", "bob@example.com", "bob.1password.com"),
    ];
    s.selected_account = Some(account("acct1", "alice@example.com", "alice.1password.com"));
    s.selected_vault = Some(vault("Work"));
    s.selected_item = Some(item("Login"));

    s.stage = OpPickerStage::Account;
    assert_modal_title(&s, "1Password");
    s.stage = OpPickerStage::Vault;
    assert_modal_title(&s, "alice@example.com");
    s.stage = OpPickerStage::Item;
    assert_modal_title(&s, "alice@example.com \u{2192} Work");
    s.stage = OpPickerStage::Section;
    assert_modal_title(&s, "alice@example.com \u{2192} Work \u{2192} Login");
    s.stage = OpPickerStage::Field;
    assert_modal_title(&s, "alice@example.com \u{2192} Work \u{2192} Login");
}

#[test]
fn keyboard_selection_wraps_at_both_ends() {
    let mut s = picker_ready();
    s.vaults = vec![vault("A"), vault("B"), vault("C")];
    s.vault_list_state.set_active(Some(0));

    // Up from the first row wraps to the last.
    let outcome = s.handle_key(key(KeyCode::Up));
    assert!(matches!(outcome, ModalOutcome::Continue));
    assert_eq!(s.vault_list_state.active().copied(), Some(2));
    // Down past the last row wraps to the first.
    let outcome = s.handle_key(key(KeyCode::Down));
    assert!(matches!(outcome, ModalOutcome::Continue));
    assert_eq!(s.vault_list_state.active().copied(), Some(0));
}

#[test]
fn wheel_selection_clamps_at_both_ends() {
    let mut s = picker_ready();
    s.vaults = vec![vault("A"), vault("B"), vault("C")];
    s.vault_list_state.set_active(Some(2));

    // Wheel past the last row stays on the last.
    assert!(!s.scroll_selection(1));
    assert_eq!(s.vault_list_state.active().copied(), Some(2));
    s.vault_list_state.set_active(Some(0));
    // Wheel above the first row stays on the first.
    assert!(!s.scroll_selection(-1));
    assert_eq!(s.vault_list_state.active().copied(), Some(0));
    assert!(s.scroll_selection(1));
    assert_eq!(s.vault_list_state.active().copied(), Some(1));
}

#[test]
fn empty_stage_leaves_no_selection_for_keyboard_and_wheel() {
    let mut s = picker_ready();
    s.vaults = Vec::new();
    s.vault_list_state.set_active(None);

    let outcome = s.handle_key(key(KeyCode::Down));
    assert!(matches!(outcome, ModalOutcome::Continue));
    assert_eq!(s.vault_list_state.active().copied(), None);
    assert!(!s.scroll_selection(1));
    assert_eq!(s.vault_list_state.active().copied(), None);
}

#[test]
fn filter_edit_resets_selection_to_first_filtered_row() {
    let mut s = picker_ready();
    s.vaults = vec![vault("Work"), vault("Personal"), vault("Travel")];
    s.vault_list_state.set_active(Some(1));

    // Narrowing to a one-row projection selects that row.
    let outcome = s.handle_key(key(KeyCode::Char('W')));
    assert!(matches!(outcome, ModalOutcome::Continue));
    assert_eq!(s.filter_buf, "W");
    assert_eq!(s.vault_list_state.active().copied(), Some(0));

    // Clearing the filter restores the full list with first-row selection.
    let outcome = s.handle_key(key(KeyCode::Backspace));
    assert!(matches!(outcome, ModalOutcome::Continue));
    assert!(s.filter_buf.is_empty());
    assert_eq!(s.vault_list_state.active().copied(), Some(0));
}

#[test]
fn cached_load_resolves_on_first_poll_and_clears_slot() {
    let mut s = picker_ready();
    s.attach_load_receiver(jackin_oppicker::ready_load_subscription(
        LoadResult::Vaults(Ok(vec![vault("Personal")])),
    ));

    assert!(s.poll_load(), "cached load must be Ready on the first poll");
    assert!(s.rx.is_none(), "rx slot clears after Ready");
    assert_eq!(s.vaults.len(), 1);
    assert!(matches!(s.load_state, OpLoadState::Ready));
}

#[test]
fn worker_load_reports_pending_until_delivery_then_clears_slot() {
    let mut s = picker_ready();
    // Gate the worker on a channel so the Pending poll is deterministic
    // (no sleep): the worker blocks until the test releases it.
    let (gate_tx, gate_rx) = std::sync::mpsc::channel::<()>();
    s.attach_load_receiver(jackin_oppicker::spawn_named_worker_subscription(
        "jackin-op-picker-load-test",
        move || {
            gate_rx.recv().unwrap();
            LoadResult::Vaults(Ok(vec![vault("Work")]))
        },
    ));

    assert!(!s.poll_load(), "worker still in flight reports Pending");
    assert!(s.rx.is_some(), "rx slot survives Pending");

    gate_tx.send(()).unwrap();
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
    while s.rx.is_some() && std::time::Instant::now() < deadline {
        wait_for_worker_poll();
        let _ = s.poll_load();
    }
    assert!(s.rx.is_none(), "rx slot clears after Ready");
    assert_eq!(s.vaults.len(), 1);
    assert_eq!(s.vaults[0].name, "Work");
}

#[test]
fn dropped_worker_reports_closed_and_clears_slot() {
    let mut s = picker_ready();
    s.attach_load_receiver(jackin_oppicker::spawn_named_worker_subscription(
        "jackin-op-picker-load-test",
        || -> LoadResult { panic!("simulated worker disconnect") },
    ));

    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
    while s.rx.is_some() && std::time::Instant::now() < deadline {
        wait_for_worker_poll();
        let _ = s.poll_load();
    }
    assert!(s.rx.is_none(), "rx slot clears after Closed");
    assert!(
        matches!(s.load_state, OpLoadState::Error(_)),
        "dropped worker surfaces the disconnected-worker error"
    );
}
