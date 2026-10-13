// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

#[test]
fn vault_list_uses_injected_runner_in_async_worker() {
    let runner = Arc::new(RecorderRunner {
        accounts: Mutex::new(vec![account(
            "acct1",
            "single@example.com",
            "single.1password.com",
        )]),
        ..Default::default()
    });
    let runner_for_assert: Arc<RecorderRunner> = Arc::clone(&runner);
    let mut s = new_picker_with_runner(runner);
    // Single-account fast path also fires a vault_list — drain so
    // the counter only reflects the explicit call below.
    drain_initial_account_load(&mut s);
    *runner_for_assert.vault_list_calls.lock().unwrap() = 0;
    *runner_for_assert.last_vault_list_account.lock().unwrap() = None;

    s.start_vault_load(Some("acct1".into()));
    drain_worker_load(&mut s);

    assert_eq!(
        *runner_for_assert.vault_list_calls.lock().unwrap(),
        1,
        "worker thread must call the injected runner exactly once"
    );
    assert_eq!(
        *runner_for_assert.last_vault_list_account.lock().unwrap(),
        Some(Some("acct1".to_owned())),
        "worker thread must thread the explicit account UUID through"
    );
}

#[test]
fn item_list_uses_injected_runner_in_async_worker() {
    let runner = Arc::new(RecorderRunner {
        accounts: Mutex::new(vec![account(
            "acct1",
            "single@example.com",
            "single.1password.com",
        )]),
        ..Default::default()
    });
    let runner_for_assert: Arc<RecorderRunner> = Arc::clone(&runner);
    let mut s = new_picker_with_runner(runner);
    drain_initial_account_load(&mut s);
    drain_worker_load(&mut s);

    s.start_item_load("v-personal".into(), Some("acct1".into()));
    drain_worker_load(&mut s);

    assert_eq!(
        *runner_for_assert.item_list_calls.lock().unwrap(),
        1,
        "worker thread must call item_list on the injected runner"
    );
    assert_eq!(
        *runner_for_assert.last_item_list_args.lock().unwrap(),
        Some(("v-personal".to_owned(), Some("acct1".to_owned()))),
        "worker thread must forward (vault_id, account_id) verbatim"
    );
}

#[test]
fn item_get_uses_injected_runner_in_async_worker() {
    let runner = Arc::new(RecorderRunner {
        accounts: Mutex::new(vec![account(
            "acct1",
            "single@example.com",
            "single.1password.com",
        )]),
        ..Default::default()
    });
    let runner_for_assert: Arc<RecorderRunner> = Arc::clone(&runner);
    let mut s = new_picker_with_runner(runner);
    drain_initial_account_load(&mut s);
    drain_worker_load(&mut s);

    s.start_field_load("i-aws".into(), "v-personal".into(), Some("acct1".into()));
    drain_worker_load(&mut s);

    assert_eq!(
        *runner_for_assert.item_get_calls.lock().unwrap(),
        1,
        "worker thread must call item_get on the injected runner"
    );
    assert_eq!(
        *runner_for_assert.last_item_get_args.lock().unwrap(),
        Some((
            "i-aws".to_owned(),
            "v-personal".to_owned(),
            Some("acct1".to_owned())
        )),
        "worker thread must forward (item_id, vault_id, account_id) verbatim"
    );
}

#[test]
fn picker_commit_writes_op_ref_with_uuid_form_and_clean_path_when_unique() {
    let field = OpField {
        id: "f_uuid".into(),
        section_id: None,
        label: "api key".into(),
        reference: "op://Private/Stripe/api key".into(),
        field_type: "concealed".into(),
        concealed: true,
    };
    let state = test_state_picked(
        OpVault {
            id: "v_uuid".into(),
            name: "Private".into(),
        },
        vec![OpItem {
            id: "i_uuid".into(),
            name: "Stripe".into(),
            subtitle: String::new(),
        }],
        OpItem {
            id: "i_uuid".into(),
            name: "Stripe".into(),
            subtitle: String::new(),
        },
        field.clone(),
    );
    let r = state
        .build_op_ref_on_commit(&field)
        .expect("fixture IDs form a valid secret reference");
    assert_eq!(r.op, "op://v_uuid/i_uuid/f_uuid");
    assert_eq!(r.path, "Private/Stripe/api key");
}

#[test]
fn picker_commit_embeds_subtitle_when_item_name_collides_in_vault() {
    let claude_a = OpItem {
        id: "i_uuid_a".into(),
        name: "Claude".into(),
        subtitle: "alexey@zhokhov.com".into(),
    };
    let claude_b = OpItem {
        id: "i_uuid_b".into(),
        name: "Claude".into(),
        subtitle: "alexey@chainargos.com".into(),
    };
    let field = OpField {
        id: "f_uuid".into(),
        section_id: Some("s_security_uuid".into()),
        label: "auth token".into(),
        reference: "op://Private/Claude/security/auth token".into(),
        field_type: "concealed".into(),
        concealed: true,
    };
    let state = test_state_picked(
        OpVault {
            id: "v_uuid".into(),
            name: "Private".into(),
        },
        vec![claude_a.clone(), claude_b],
        claude_a,
        field.clone(),
    );
    let r = state
        .build_op_ref_on_commit(&field)
        .expect("fixture IDs form a valid secret reference");
    assert_eq!(r.op, "op://v_uuid/i_uuid_a/s_security_uuid/f_uuid");
    // Section label stays display-only while its opaque ID scopes the URI.
    assert!(
        r.op.starts_with("op://v_uuid/i_uuid_a/"),
        "op had wrong prefix: {}",
        r.op
    );
    assert!(r.op.ends_with("/f_uuid"), "op had wrong suffix: {}", r.op);
    assert_eq!(
        r.path,
        "Private/Claude[alexey@zhokhov.com]/security/auth token"
    );
}

#[test]
fn picker_commit_embeds_subtitle_when_bracketed_item_name_collides() {
    // Breadcrumb escaping keeps literal brackets distinct from the subtitle
    // suffix, which is still required to disambiguate colliding item names.
    let weird_a = OpItem {
        id: "i_uuid_a".into(),
        name: "Item [tag]".into(),
        subtitle: "user@x".into(),
    };
    let weird_b = OpItem {
        id: "i_uuid_b".into(),
        name: "Item [tag]".into(),
        subtitle: "user@y".into(),
    };
    let field = OpField {
        id: "f_uuid".into(),
        section_id: None,
        label: "auth".into(),
        reference: "op://Private/Item [tag]/auth".into(),
        field_type: "concealed".into(),
        concealed: false,
    };
    let state = test_state_picked(
        OpVault {
            id: "v_uuid".into(),
            name: "Private".into(),
        },
        vec![weird_a.clone(), weird_b],
        weird_a,
        field.clone(),
    );
    let r = state
        .build_op_ref_on_commit(&field)
        .expect("fixture IDs form a valid secret reference");
    assert_eq!(
        r.path, "Private/Item %5Btag%5D[user@x]/auth",
        "colliding bracket-bearing item names retain the exact subtitle suffix"
    );
}

#[test]
fn picker_commit_skips_subtitle_when_subtitle_empty() {
    let note_a = OpItem {
        id: "i_a".into(),
        name: "Notes".into(),
        subtitle: String::new(),
    };
    let note_b = OpItem {
        id: "i_b".into(),
        name: "Notes".into(),
        subtitle: String::new(),
    };
    let field = OpField {
        id: "f_uuid".into(),
        section_id: None,
        label: "notesPlain".into(),
        reference: "op://Private/Notes/notesPlain".into(),
        field_type: "string".into(),
        concealed: false,
    };
    let state = test_state_picked(
        OpVault {
            id: "v_uuid".into(),
            name: "Private".into(),
        },
        vec![note_a.clone(), note_b],
        note_a,
        field.clone(),
    );
    let r = state
        .build_op_ref_on_commit(&field)
        .expect("fixture IDs form a valid secret reference");
    assert_eq!(
        r.path, "Private/Notes/notesPlain",
        "empty subtitle => no embed even on collision"
    );
}

#[test]
fn picker_commit_3seg_fallback_preserved_when_sibling_has_reference() {
    let sectioned_field = OpField {
        id: "f_sectioned".into(),
        section_id: Some("s_auth_uuid".into()),
        label: "password".into(),
        reference: "op://Private/MyItem/Auth/password".into(),
        field_type: "CONCEALED".into(),
        concealed: true,
    };
    let no_ref_field = OpField {
        id: "f_noref".into(),
        section_id: None,
        label: "notes".into(),
        reference: String::new(),
        field_type: "STRING".into(),
        concealed: false,
    };
    let the_item = OpItem {
        id: "i_uuid".into(),
        name: "MyItem".into(),
        subtitle: String::new(),
    };
    let mut state = test_state_picked(
        OpVault {
            id: "v_uuid".into(),
            name: "Private".into(),
        },
        vec![the_item.clone()],
        the_item,
        no_ref_field.clone(),
    );
    // Add the sectioned sibling so the anomaly log path is exercised.
    state.fields.push(sectioned_field);

    // Must not panic; must produce a 3-segment OpRef.
    let r = state
        .build_op_ref_on_commit(&no_ref_field)
        .expect("fixture IDs form a valid secret reference");
    assert_eq!(r.op, "op://v_uuid/i_uuid/f_noref");
    assert_eq!(r.path, "Private/MyItem/notes");
}

#[test]
fn parity_unique_item_3seg_field_cli_matches_picker() {
    let field = OpField {
        id: "f_uuid".into(),
        section_id: None,
        label: "api key".into(),
        reference: "op://Private/Stripe/api key".into(),
        field_type: "concealed".into(),
        concealed: true,
    };
    let the_item = OpItem {
        id: "i_uuid".into(),
        name: "Stripe".into(),
        subtitle: String::new(),
    };
    let state = test_state_picked(
        OpVault {
            id: "v_uuid".into(),
            name: "Private".into(),
        },
        vec![the_item.clone()],
        the_item,
        field.clone(),
    );
    let picker_ref = state
        .build_op_ref_on_commit(&field)
        .expect("fixture IDs form a valid secret reference");

    let stub = ParityStub::new()
        .with_vault("Private", "v_uuid")
        .with_item("v_uuid", "Stripe", "i_uuid", "")
        .with_field_with_reference(
            "i_uuid",
            "api key",
            "f_uuid",
            true,
            "op://Private/Stripe/api key",
        );
    let cli_ref = resolve_op_uri_to_ref("op://Private/Stripe/api key", &stub, None).unwrap();

    assert_eq!(cli_ref.op, picker_ref.op, "op URI must match");
    assert_eq!(cli_ref.path, picker_ref.path, "display path must match");
}
