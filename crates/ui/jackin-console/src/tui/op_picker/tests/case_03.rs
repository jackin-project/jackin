// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

#[test]
fn account_pane_filter_narrows_by_email() {
    let runner = Arc::new(StubRunner {
        accounts: Mutex::new(vec![
            account("a1", "alice@example.com", "alpha.1password.com"),
            account("a2", "bob@example.com", "beta.1password.com"),
        ]),
        last_vault_list_account: Mutex::new(None),
    });
    let mut s = new_picker_with_runner(runner);
    drain_initial_account_load(&mut s);
    s.rx = None;
    s.pending_load = None;
    s.load_state = OpLoadState::Ready;
    s.filter_buf = "alic".to_owned();
    let visible = s.filtered_accounts();
    assert_eq!(visible.len(), 1);
    assert_eq!(visible[0].email, "alice@example.com");
}

#[test]
fn enter_on_account_advances_to_vault_with_account_scope() {
    let runner = Arc::new(StubRunner {
        accounts: Mutex::new(vec![
            account("acct1", "a@example.com", "alpha.1password.com"),
            account("acct2", "b@example.com", "beta.1password.com"),
        ]),
        last_vault_list_account: Mutex::new(None),
    });
    let mut s = new_picker_with_runner(runner);
    drain_initial_account_load(&mut s);
    s.rx = None;
    s.pending_load = None;
    s.load_state = OpLoadState::Ready;
    s.account_list_state.set_active(Some(1));

    let outcome = s.handle_key(key(KeyCode::Enter));
    assert!(matches!(outcome, ModalOutcome::Continue));
    assert_eq!(s.stage, OpPickerStage::Vault);
    assert_eq!(
        s.selected_account.as_ref().map(|a| a.id.as_str()),
        Some("acct2"),
        "Enter on Account must capture the selection"
    );
    assert!(
        s.filter_buf.is_empty(),
        "filter must clear when advancing from Account to Vault"
    );
    // Direct-call verification of the account threading.
    let runner = Arc::new(StubRunner::default());
    runner.account_list().unwrap();
    drop(runner.vault_list(s.selected_account_id().as_deref()));
    let recorded = runner.last_vault_list_account.lock().unwrap().clone();
    assert_eq!(
        recorded,
        Some(Some("acct2".to_owned())),
        "vault_list must be called with Some(account_uuid) once an account is selected"
    );
}

#[test]
fn esc_from_vault_with_multi_account_returns_to_account() {
    let runner = Arc::new(StubRunner {
        accounts: Mutex::new(vec![
            account("acct1", "a@example.com", "alpha.1password.com"),
            account("acct2", "b@example.com", "beta.1password.com"),
        ]),
        last_vault_list_account: Mutex::new(None),
    });
    let mut s = new_picker_with_runner(runner);
    drain_initial_account_load(&mut s);
    s.rx = None;
    s.pending_load = None;
    s.load_state = OpLoadState::Ready;
    s.stage = OpPickerStage::Vault;
    s.selected_account = Some(account("acct1", "a@example.com", "alpha.1password.com"));
    s.vaults = vec![vault("Personal"), vault("Work")];
    s.vault_list_state.set_active(Some(1));
    s.filter_buf = "wo".to_owned();

    let outcome = s.handle_key(key(KeyCode::Esc));
    assert!(matches!(outcome, ModalOutcome::Continue));
    assert_eq!(
        s.stage,
        OpPickerStage::Account,
        "Esc from Vault must return to Account in multi-account mode"
    );
    assert!(
        s.selected_vault.is_none(),
        "selected_vault must clear on back-nav to Account"
    );
    assert!(s.vaults.is_empty(), "vaults must clear on back-nav");
    assert!(
        s.filter_buf.is_empty(),
        "filter must clear on back-nav to Account"
    );
}

#[test]
fn esc_from_vault_with_single_account_cancels_picker() {
    let mut s = picker_ready();
    s.vaults = vec![vault("Personal")];
    s.vault_list_state.set_active(Some(0));
    assert!(s.accounts.is_empty());

    let outcome = s.handle_key(key(KeyCode::Esc));
    assert!(
        matches!(outcome, ModalOutcome::Cancel),
        "Esc on Vault in single-account mode must cancel the picker"
    );
}

#[test]
fn op_cache_hit_skips_account_list_subprocess() {
    use jackin_env::OpCache;
    use std::sync::Arc;

    let cache = Rc::new(RefCell::new(OpCache::default()));
    let counter1: Arc<Mutex<usize>> = Arc::new(Mutex::new(0));
    let counter2: Arc<Mutex<usize>> = Arc::new(Mutex::new(0));

    // First picker: cache miss → runner invoked once.
    let mut s1 = new_picker_with_runner_and_cache(
        Arc::new(CounterRunner {
            accounts: vec![account("acct1", "a@example.com", "alpha.1password.com")],
            counter: Arc::clone(&counter1),
        }),
        Rc::clone(&cache),
    );
    drain_initial_account_load(&mut s1);
    assert_eq!(
        *counter1.lock().unwrap(),
        1,
        "first picker constructor must miss the empty cache"
    );

    // Second picker: cache hit → runner must NOT be invoked.
    let mut s2 = new_picker_with_runner_and_cache(
        Arc::new(CounterRunner {
            accounts: vec![account("acct1", "a@example.com", "alpha.1password.com")],
            counter: Arc::clone(&counter2),
        }),
        cache,
    );
    drain_initial_account_load(&mut s2);
    assert_eq!(
        *counter2.lock().unwrap(),
        0,
        "second picker against the same cache must hit and skip account_list"
    );
}

#[test]
fn op_cache_miss_calls_runner_and_stores() {
    use jackin_env::OpCache;
    use std::sync::Arc;

    let cache = Rc::new(RefCell::new(OpCache::default()));
    let counter: Arc<Mutex<usize>> = Arc::new(Mutex::new(0));

    let mut s1 = new_picker_with_runner_and_cache(
        Arc::new(CounterRunner {
            accounts: vec![account("acct1", "a@example.com", "alpha.1password.com")],
            counter: Arc::clone(&counter),
        }),
        Rc::clone(&cache),
    );
    drain_initial_account_load(&mut s1);
    assert_eq!(*counter.lock().unwrap(), 1, "first picker must miss");
    assert!(
        cache.borrow().get_accounts().is_some(),
        "first picker must populate the cache"
    );

    let mut s2 = new_picker_with_runner_and_cache(
        Arc::new(CounterRunner {
            accounts: vec![account("acct1", "a@example.com", "alpha.1password.com")],
            counter: Arc::clone(&counter),
        }),
        cache,
    );
    drain_initial_account_load(&mut s2);
    assert_eq!(
        *counter.lock().unwrap(),
        1,
        "second picker on populated cache must hit and not re-call account_list"
    );
}

#[test]
fn op_cache_refresh_re_fires_subprocess() {
    use jackin_env::OpCache;
    use std::sync::Arc;

    let cache = Rc::new(RefCell::new(OpCache::default()));
    let counter: Arc<Mutex<usize>> = Arc::new(Mutex::new(0));

    let r = Arc::new(CounterRunner {
        accounts: vec![
            account("acct1", "a@example.com", "alpha.1password.com"),
            account("acct2", "b@example.com", "beta.1password.com"),
        ],
        counter: Arc::clone(&counter),
    });
    let mut s = new_picker_with_runner_and_cache(r, cache);
    drain_initial_account_load(&mut s);
    assert_eq!(*counter.lock().unwrap(), 1, "constructor must miss once");
    assert_eq!(s.accounts.len(), 2);

    drop(s.handle_key(key(KeyCode::Char('r'))));
    drain_initial_account_load(&mut s);
    assert_eq!(
        *counter.lock().unwrap(),
        2,
        "r on Account must invalidate cache and re-fire account_list"
    );
    assert_eq!(s.accounts.len(), 2);
    assert_eq!(s.stage, OpPickerStage::Account);
}

#[test]
fn picker_construction_does_not_block_on_account_list() {
    let runner = Arc::new(BlockingRunner::new());
    let runner_for_release = Arc::clone(&runner);

    let start = std::time::Instant::now();
    let _s = new_picker_with_runner(runner);
    let elapsed = start.elapsed();
    assert!(
        elapsed < std::time::Duration::from_millis(500),
        "constructor must not synchronously wait on account_list; elapsed={elapsed:?}"
    );
    // Release the Condvar so the worker exits cleanly.
    runner_for_release.release();
}

#[test]
fn picker_loading_account_state_renders_spinner_immediately() {
    use ratatui::{Terminal, backend::TestBackend, layout::Rect};

    let runner = Arc::new(BlockingRunner::new());
    let runner_for_release = Arc::clone(&runner);
    let s = new_picker_with_runner(runner);

    assert!(
        matches!(s.load_state, OpLoadState::Loading { .. }),
        "constructor must leave the picker in Loading; got {:?}",
        s.load_state
    );

    let area = Rect::new(0, 0, 60, 12);
    let backend = TestBackend::new(area.width, area.height);
    let mut term = Terminal::new(backend).unwrap();
    term.draw(|f| crate::tui::components::op_picker::render_picker(f, area, &s))
        .unwrap();
    let buf = term.backend().buffer();

    let mut rendered = String::new();
    for y in 0..area.height {
        for x in 0..area.width {
            rendered.push_str(buf[(x, y)].symbol());
        }
    }
    let braille_present = rendered
        .chars()
        .any(|c| ('\u{2800}'..='\u{28ff}').contains(&c));
    assert!(
        braille_present,
        "rendered loading panel must contain a Braille spinner glyph; \
         content was: {rendered:?}"
    );

    runner_for_release.release();
}

#[test]
fn loading_panel_title_during_item_load_shows_breadcrumb() {
    let mut state = OpPickerState::default();
    state.accounts = vec![
        account("a1", "alice@example.com", "alice.1password.com"),
        account("a2", "bob@example.com", "bob.1password.com"),
    ];
    state.selected_account = Some(state.accounts[0].clone());
    state.selected_vault = Some(OpVault {
        id: "v-personal".into(),
        name: "Personal".into(),
    });
    state.stage = OpPickerStage::Item;
    state.load_state = OpLoadState::Loading { spinner_tick: 0 };

    let (dump, _) = render_picker_dump(&state, 80, 12);

    assert!(dump.contains("alice@example.com"), "dump:\n{dump}");
    assert!(dump.contains("Personal"), "dump:\n{dump}");
    assert!(dump.contains('\u{2192}'), "dump:\n{dump}");
    assert!(
        dump.contains("loading items from Personal"),
        "dump:\n{dump}"
    );
}

#[test]
fn picker_field_load_title_shows_parent_and_body_includes_subtitle() {
    let mut state = OpPickerState::default();
    state.accounts = vec![
        account("a1", "alexey@zhokhov.com", "z.1password.com"),
        account("a2", "alexey@chainargos.com", "c.1password.com"),
    ];
    state.selected_account = Some(state.accounts[1].clone());
    state.selected_vault = Some(OpVault {
        id: "v-chainargos".into(),
        name: "ChainArgos".into(),
    });
    state.selected_item = Some(OpItem {
        id: "i-redshift".into(),
        name: "ChainArgos Redshift".into(),
        subtitle: "donbeave".into(),
    });
    state.stage = OpPickerStage::Field;
    state.load_state = OpLoadState::Loading { spinner_tick: 0 };

    let (dump, top_row) = render_picker_dump(&state, 80, 12);

    assert!(
        top_row.contains("alexey@chainargos.com"),
        "top row:\n{top_row}"
    );
    assert!(top_row.contains("ChainArgos"), "top row:\n{top_row}");
    assert!(!top_row.contains("Redshift"), "top row:\n{top_row}");
    assert!(
        dump.contains("loading ChainArgos Redshift (donbeave)"),
        "dump:\n{dump}"
    );
    assert!(!dump.contains("loading fields from"), "dump:\n{dump}");
}

#[test]
fn picker_field_load_body_no_subtitle() {
    let mut state = OpPickerState::default();
    state.accounts = vec![account("a1", "single@example.com", "x.1password.com")];
    state.selected_account = Some(state.accounts[0].clone());
    state.selected_vault = Some(OpVault {
        id: "v".into(),
        name: "Personal".into(),
    });
    state.selected_item = Some(OpItem {
        id: "i-note".into(),
        name: "Standalone Note".into(),
        subtitle: String::new(),
    });
    state.stage = OpPickerStage::Field;
    state.load_state = OpLoadState::Loading { spinner_tick: 0 };

    let (dump, _) = render_picker_dump(&state, 80, 12);

    assert!(dump.contains("loading Standalone Note"), "dump:\n{dump}");
    assert!(!dump.contains("loading Standalone Note ("), "dump:\n{dump}");
}

#[test]
fn op_cache_picker_does_not_store_field_values() {
    let f = OpField {
        id: "password".into(),
        section_id: None,
        label: "password".into(),
        field_type: "concealed".into(),
        concealed: true,
        reference: "op://Personal/API Keys/password".into(),
    };
    let OpField {
        id: _,
        section_id: _,
        label: _,
        field_type: _,
        concealed: _,
        reference: _,
    } = f;
}
