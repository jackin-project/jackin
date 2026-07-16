// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;
use crossterm::event::{KeyEventKind, KeyEventState};
use jackin_core::{OpAccount, OpItem};

fn key(code: KeyCode) -> KeyEvent {
    KeyEvent {
        code,
        modifiers: KeyModifiers::NONE,
        kind: KeyEventKind::Press,
        state: KeyEventState::NONE,
    }
}

fn account(id: &str, email: &str) -> OpAccount {
    OpAccount {
        id: id.to_owned(),
        email: email.to_owned(),
        url: format!("{id}.1password.com"),
    }
}

fn item(id: &str, name: &str) -> OpItem {
    OpItem {
        id: id.to_owned(),
        name: name.to_owned(),
        subtitle: String::new(),
    }
}

#[test]
fn account_filter_shrinking_below_selection_resets_to_first_match() {
    let mut state = OpPickerState::new();
    state.load_state = OpLoadState::Ready;
    state.stage = OpPickerStage::Account;
    state.accounts = vec![
        account("a", "alex@example.com"),
        account("b", "briar@example.com"),
        account("c", "casey@example.com"),
    ];
    state
        .account_list_state
        .select((!state.accounts.is_empty()).then_some(0));
    state.account_list_state.select(Some(2));

    state.handle_key(key(KeyCode::Char('b')));

    assert_eq!(state.filtered_accounts().len(), 1);
    assert_eq!(state.filtered_accounts()[0].email, "briar@example.com");
    assert_eq!(state.account_list_state.selected().copied(), Some(0));
}

#[test]
fn item_filter_without_matches_clears_selection() {
    let mut state = OpPickerState::new();
    state.load_state = OpLoadState::Ready;
    state.stage = OpPickerStage::Item;
    state.items = vec![
        item("a", "Cloudflare"),
        item("b", "GitHub"),
        item("c", "Stripe"),
    ];
    state
        .item_list_state
        .select((!state.items.is_empty()).then_some(0));
    state.item_list_state.select(Some(2));

    state.handle_key(key(KeyCode::Char('z')));

    assert!(state.filtered_item_choices().is_empty());
    assert_eq!(state.item_list_state.selected().copied(), None);
}
