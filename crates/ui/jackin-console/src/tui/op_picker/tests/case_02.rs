// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

#[test]
fn section_choices_returns_root_plus_distinct_sections() {
    let s = create_at_section_with_sections(
        vec![
            field_with_reference("user", "op://Personal/login/user"),
            field_with_section_reference("api", "op://Personal/login/auth/api", "Opaque-Auth-ID"),
            field_with_section_reference("key", "op://Personal/login/auth/key", "Opaque-Auth-ID"),
            field_with_section_reference(
                "recovery",
                "op://Personal/login/auth/recovery",
                "Opaque-Auth-ID-2",
            ),
            field_with_section_reference(
                "note",
                "op://Personal/login/extra/note",
                "Opaque-Extra-ID",
            ),
        ],
        vec![
            OpSection {
                id: "Opaque-Auth-ID".to_owned(),
                label: "auth".to_owned(),
            },
            OpSection {
                id: "Opaque-Auth-ID-2".to_owned(),
                label: "auth".to_owned(),
            },
            OpSection {
                id: "Opaque-Extra-ID".to_owned(),
                label: "extra".to_owned(),
            },
        ],
    );
    let choices = s.section_choices();
    assert_eq!(
        choices,
        vec![
            None,
            Some(OpSection {
                id: "Opaque-Auth-ID".to_owned(),
                label: "auth".to_owned(),
            }),
            Some(OpSection {
                id: "Opaque-Auth-ID-2".to_owned(),
                label: "auth".to_owned(),
            }),
            Some(OpSection {
                id: "Opaque-Extra-ID".to_owned(),
                label: "extra".to_owned(),
            }),
        ],
        "root first, then distinct sections in first-appearance order"
    );
}

#[test]
fn create_mode_existing_field_commits_edit_item_field() {
    let mut s = create_at_section(vec![field("token", "CONCEALED", true)]);
    // Select `(root)` → Field stage scoped to root.
    assert!(matches!(
        s.handle_key(key(KeyCode::Enter)),
        ModalOutcome::Continue
    ));
    assert_eq!(s.stage, OpPickerStage::Field);
    assert_eq!(s.selected_section, None);
    // Root field "token" → display rows: [Field{0}, NewFieldSentinel].
    s.field_list_state.set_active(Some(0));
    match s.handle_key(key(KeyCode::Enter)) {
        ModalOutcome::Commit(OpPickerSelection::EditItemField {
            item,
            field,
            section,
            ..
        }) => {
            assert_eq!(item.id, "i-login");
            // The real field id is forwarded so the write targets this
            // exact field (not the first label match) and preserves it.
            assert_eq!(
                field,
                FieldTarget::Existing {
                    id: "token".into(),
                    label: "token".into(),
                }
            );
            assert_eq!(section, None);
        }
        other => panic!("expected Commit(EditItemField), got {other:?}"),
    }
}

#[test]
fn create_mode_selecting_section_scopes_field_stage() {
    let auth = OpSection {
        id: "Opaque-Auth-ID".to_owned(),
        label: "auth".to_owned(),
    };
    let mut s = create_at_section_with_sections(
        vec![
            field_with_reference("user", "op://Personal/login/user"),
            field_with_section_reference("api", "op://Personal/login/auth/api", "Opaque-Auth-ID"),
            field_with_section_reference("key", "op://Personal/login/auth/key", "Opaque-Auth-ID"),
        ],
        vec![auth.clone()],
    );
    // section_choices: [None, Some("auth")]; select "auth" (index 1).
    s.section_list_state.set_active(Some(1));
    assert!(matches!(
        s.handle_key(key(KeyCode::Enter)),
        ModalOutcome::Continue
    ));
    assert_eq!(s.stage, OpPickerStage::Field);
    assert_eq!(s.selected_section, Some(auth.clone()));
    // Field stage shows only the two "auth" fields + NewFieldSentinel.
    let rows = s.build_field_display_rows();
    assert_eq!(rows.len(), 3, "two auth fields + new-field sentinel");
    assert!(matches!(rows[2], FieldDisplayRow::NewFieldSentinel));
    // Selecting the first scoped field commits with section Some("auth").
    s.field_list_state.set_active(Some(0));
    match s.handle_key(key(KeyCode::Enter)) {
        ModalOutcome::Commit(OpPickerSelection::EditItemField { section, field, .. }) => {
            assert_eq!(section, Some(OpSectionTarget::Existing(auth)));
            assert_eq!(field.label(), "api");
        }
        other => panic!("expected Commit(EditItemField), got {other:?}"),
    }
}

#[test]
fn create_mode_new_field_in_root_commits_section_none() {
    let mut s = create_at_section(vec![field_with_reference(
        "user",
        "op://Personal/login/user",
    )]);
    // Select `(root)`.
    assert!(matches!(
        s.handle_key(key(KeyCode::Enter)),
        ModalOutcome::Continue
    ));
    assert_eq!(s.stage, OpPickerStage::Field);
    // Rows: [Field{0}, NewFieldSentinel] → select the sentinel.
    s.field_list_state.set_active(Some(1));
    assert!(matches!(
        s.handle_key(key(KeyCode::Enter)),
        ModalOutcome::Continue
    ));
    assert_eq!(s.stage, OpPickerStage::FieldLabel);
    match s.handle_key(key(KeyCode::Enter)) {
        ModalOutcome::Commit(OpPickerSelection::EditItemField { section, field, .. }) => {
            assert_eq!(section, None, "new field in root → section None");
            assert_eq!(field.label(), "token");
        }
        other => panic!("expected Commit(EditItemField), got {other:?}"),
    }
}

#[test]
fn create_mode_new_section_flow_threads_section_into_commit() {
    let mut s = create_at_section(vec![]);
    // section_choices: [None]; sentinel `+ New section` at index 1.
    s.section_list_state.set_active(Some(1));
    assert!(matches!(
        s.handle_key(key(KeyCode::Enter)),
        ModalOutcome::Continue
    ));
    assert_eq!(s.stage, OpPickerStage::NewSectionName);
    // section_name_input starts empty; type a name (empty won't commit).
    for c in "creds".chars() {
        drop(s.handle_key(key(KeyCode::Char(c))));
    }
    assert!(matches!(
        s.handle_key(key(KeyCode::Enter)),
        ModalOutcome::Continue
    ));
    assert_eq!(s.stage, OpPickerStage::FieldLabel);
    match s.handle_key(key(KeyCode::Enter)) {
        ModalOutcome::Commit(OpPickerSelection::EditItemField { section, field, .. }) => {
            assert_eq!(section, Some(OpSectionTarget::NewLabel("creds".to_owned())));
            assert_eq!(field.label(), "token");
        }
        other => panic!("expected Commit(EditItemField) with section, got {other:?}"),
    }
}

#[test]
fn field_label_cancel_clears_pending_section() {
    // New-section flow stages pending_section, then backing out of the
    // field-label stage must discard it so it cannot leak into a later
    // commit on a different path.
    let mut s = create_at_section(vec![]);
    s.section_list_state.set_active(Some(1)); // `+ New section` sentinel
    drop(s.handle_key(key(KeyCode::Enter)));
    assert_eq!(s.stage, OpPickerStage::NewSectionName);
    for c in "foo".chars() {
        drop(s.handle_key(key(KeyCode::Char(c))));
    }
    drop(s.handle_key(key(KeyCode::Enter)));
    assert_eq!(s.stage, OpPickerStage::FieldLabel);
    assert_eq!(
        s.pending_section,
        Some(OpSectionTarget::NewLabel("foo".into()))
    );
    // Esc cancels the field-label stage.
    drop(s.handle_key(key(KeyCode::Esc)));
    assert_eq!(s.stage, OpPickerStage::NewSectionName);
    assert!(
        s.pending_section.is_none(),
        "abandoned section must not survive the field-label cancel"
    );
}

#[test]
fn field_label_commit_trims_whitespace() {
    let mut s = create_at_section(vec![]);
    // Drill `(root)` → Field stage, then `+ New field`.
    drop(s.handle_key(key(KeyCode::Enter)));
    assert_eq!(s.stage, OpPickerStage::Field);
    s.field_label_input = field_label_input_state("  oauth-token  ");
    s.field_label_origin = FieldLabelOrigin::NewField;
    s.stage = OpPickerStage::FieldLabel;
    match s.handle_key(key(KeyCode::Enter)) {
        ModalOutcome::Commit(OpPickerSelection::EditItemField { field, .. }) => {
            assert_eq!(field.label(), "oauth-token", "field label must be trimmed");
        }
        other => panic!("expected Commit(EditItemField), got {other:?}"),
    }
}

#[test]
fn new_section_name_commit_trims_whitespace() {
    let mut s = create_at_section(vec![]);
    s.section_list_state.set_active(Some(1));
    drop(s.handle_key(key(KeyCode::Enter)));
    s.section_name_input = section_name_input_state("  creds  ");
    drop(s.handle_key(key(KeyCode::Enter)));
    assert_eq!(
        s.pending_section,
        Some(OpSectionTarget::NewLabel("creds".into()))
    );
}

#[test]
fn left_collapse_via_header_keeps_selection_in_range() {
    // Browse-mode flat field list with a collapsible header. Left on the
    // header collapses it and (like the Enter toggle) clamps the field
    // selection so it never points past the shrunken row list.
    let mut s = picker_ready();
    s.selected_vault = Some(OpVault {
        id: "v-Personal".into(),
        name: "Personal".into(),
    });
    s.selected_item = Some(item("login"));
    s.fields = vec![
        field_with_section_reference("api", "op://Personal/login/auth/api", "Opaque-Auth-ID"),
        field_with_section_reference("key", "op://Personal/login/auth/key", "Opaque-Auth-ID"),
    ];
    s.sections = vec![OpSection {
        id: "Opaque-Auth-ID".to_owned(),
        label: "auth".to_owned(),
    }];
    s.stage = OpPickerStage::Field;
    // Rows: [SectionHeader(auth), Field, Field]. Park on the last field.
    let last = s.build_field_display_rows().len() - 1;
    s.field_list_state.set_active(Some(last));
    // Move up onto the header row, then collapse with Left.
    let header_idx = s
        .build_field_display_rows()
        .iter()
        .position(|r| matches!(r, FieldDisplayRow::SectionHeader { .. }))
        .expect("a section header row");
    s.field_list_state.set_active(Some(header_idx));
    drop(s.handle_key(key(KeyCode::Left)));
    assert!(
        s.collapsed_sections.contains("Opaque-Auth-ID"),
        "Left must collapse the section"
    );
    let new_len = s.build_field_display_rows().len();
    let sel = s
        .field_list_state
        .active()
        .copied()
        .expect("selection retained");
    assert!(
        sel < new_len,
        "selection {sel} must stay within {new_len} rows"
    );
}

#[test]
fn create_mode_esc_chain_field_to_section_to_item() {
    let mut s = create_at_section_with_sections(
        vec![field_with_section_reference(
            "api",
            "op://Personal/login/auth/api",
            "Opaque-Auth-ID",
        )],
        vec![OpSection {
            id: "Opaque-Auth-ID".to_owned(),
            label: "auth".to_owned(),
        }],
    );
    // Drill into "auth", then Esc back to Section, then Esc back to Item.
    s.section_list_state.set_active(Some(1));
    drop(s.handle_key(key(KeyCode::Enter)));
    assert_eq!(s.stage, OpPickerStage::Field);

    drop(s.handle_key(key(KeyCode::Esc)));
    assert_eq!(s.stage, OpPickerStage::Section, "Field Esc → Section");
    assert_eq!(s.selected_section, None, "section cleared on back-nav");
    assert!(s.selected_item.is_some(), "item kept on Field→Section Esc");

    drop(s.handle_key(key(KeyCode::Esc)));
    assert_eq!(s.stage, OpPickerStage::Item, "Section Esc → Item");
    assert!(
        s.selected_item.is_none(),
        "item cleared on Section→Item Esc"
    );
}

#[test]
fn stub_runner_constructor_is_not_fatal() {
    let runner = Arc::new(StubRunner {
        accounts: Mutex::new(vec![account("a", "a@example.com", "a.1password.com")]),
        last_vault_list_account: Mutex::new(None),
    });
    let mut s = new_picker_with_runner(runner);
    drain_initial_account_load(&mut s);
    let bad = matches!(
        s.load_state,
        OpLoadState::Error(OpPickerError::Fatal(
            OpPickerFatalState::NotInstalled | OpPickerFatalState::NotSignedIn
        ))
    );
    assert!(
        !bad,
        "stub runner returning Ok must not produce NotInstalled / NotSignedIn; got {:?}",
        s.load_state
    );
}

#[test]
fn picker_starts_at_account_when_multiple_accounts() {
    let runner = Arc::new(StubRunner {
        accounts: Mutex::new(vec![
            account("acct1", "a@example.com", "alpha.1password.com"),
            account("acct2", "b@example.com", "beta.1password.com"),
        ]),
        last_vault_list_account: Mutex::new(None),
    });
    let mut s = new_picker_with_runner(runner);
    drain_initial_account_load(&mut s);
    assert_eq!(
        s.stage,
        OpPickerStage::Account,
        "two accounts must route to the Account pane"
    );
    assert_eq!(s.accounts.len(), 2);
    assert_eq!(s.account_list_state.active().copied(), Some(0));
    assert!(
        s.selected_account.is_none(),
        "selected_account must remain None until the operator picks one"
    );
}

#[test]
fn picker_starts_at_vault_when_single_account() {
    let runner = Arc::new(StubRunner {
        accounts: Mutex::new(vec![account(
            "solo",
            "solo@example.com",
            "solo.1password.com",
        )]),
        last_vault_list_account: Mutex::new(None),
    });
    let mut s = new_picker_with_runner(runner);
    drain_initial_account_load(&mut s);
    assert_eq!(
        s.stage,
        OpPickerStage::Vault,
        "single account must skip the Account pane"
    );
    assert_eq!(
        s.selected_account.as_ref().map(|a| a.id.as_str()),
        Some("solo"),
        "single account must be auto-selected"
    );
    assert!(
        s.accounts.is_empty(),
        "single-account setup leaves the accounts vec empty so render/Esc paths skip multi-account branches"
    );
}
