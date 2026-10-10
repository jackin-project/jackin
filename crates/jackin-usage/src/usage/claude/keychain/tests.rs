// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;
use std::cell::{Cell, RefCell};

struct RestoreOnDrop<'a>(&'a RefCell<Vec<&'static str>>);

impl Drop for RestoreOnDrop<'_> {
    fn drop(&mut self) {
        self.0.borrow_mut().push("restore");
    }
}

struct FakeInteractionLock<'a> {
    user_interaction_allowed: &'a Cell<bool>,
    restore_count: &'a Cell<usize>,
    previous_value: bool,
}

impl Drop for FakeInteractionLock<'_> {
    fn drop(&mut self) {
        self.user_interaction_allowed.set(self.previous_value);
        self.restore_count.set(self.restore_count.get() + 1);
    }
}

struct FakeKeychain {
    user_interaction_allowed: Cell<bool>,
    query_error: Cell<bool>,
    disable_error: Cell<bool>,
    search_status: Cell<Option<i32>>,
    payload: RefCell<Option<String>>,
    query_count: Cell<usize>,
    disable_count: Cell<usize>,
    search_count: Cell<usize>,
    restore_count: Cell<usize>,
}

impl FakeKeychain {
    fn new() -> Self {
        Self {
            user_interaction_allowed: Cell::new(true),
            query_error: Cell::new(false),
            disable_error: Cell::new(false),
            search_status: Cell::new(None),
            payload: RefCell::new(None),
            query_count: Cell::new(0),
            disable_count: Cell::new(0),
            search_count: Cell::new(0),
            restore_count: Cell::new(0),
        }
    }

    fn read_unattended(&self) -> ClaudeKeychainRead {
        self.read_with_ui(false)
    }

    fn prepare(&self, all_stdio_are_terminal: bool) -> ClaudeKeychainRead {
        prepare_claude_keychain_auth_with(all_stdio_are_terminal, || self.read_with_ui(true))
    }

    fn read_with_ui(&self, allow_ui: bool) -> ClaudeKeychainRead {
        read_claude_keychain_item_with(
            allow_ui,
            || {
                self.query_count.set(self.query_count.get() + 1);
                if self.query_error.get() {
                    Err(())
                } else {
                    Ok(self.user_interaction_allowed.get())
                }
            },
            || {
                self.disable_count.set(self.disable_count.get() + 1);
                if self.disable_error.get() {
                    return Err(());
                }
                let previous_value = self.user_interaction_allowed.replace(false);
                Ok(FakeInteractionLock {
                    user_interaction_allowed: &self.user_interaction_allowed,
                    restore_count: &self.restore_count,
                    previous_value,
                })
            },
            || {
                self.search_count.set(self.search_count.get() + 1);
                if !allow_ui {
                    assert!(
                        !self.user_interaction_allowed.get(),
                        "unattended fake Keychain search ran with UI enabled"
                    );
                }
                match self.search_status.get() {
                    Some(status) => Err(status),
                    None => Ok(self.payload.borrow().clone().map(Zeroizing::new)),
                }
            },
        )
    }
}

#[test]
fn unattended_policy_disables_ui_for_search_and_restores_it() {
    let events = RefCell::new(Vec::new());
    let outcome = with_keychain_interaction_permission(
        false,
        || {
            events.borrow_mut().push("check");
            Ok(true)
        },
        || {
            events.borrow_mut().push("disable");
            Ok(RestoreOnDrop(&events))
        },
        || {
            events.borrow_mut().push("search");
            7
        },
    )
    .expect("policy setup succeeds");

    assert_eq!(outcome, 7);
    assert_eq!(*events.borrow(), ["check", "disable", "search", "restore"]);
}

#[test]
fn unattended_policy_keeps_preexisting_disabled_state() {
    let events = RefCell::new(Vec::new());
    let outcome = with_keychain_interaction_permission(
        false,
        || Ok(false),
        || {
            events.borrow_mut().push("disable");
            Ok(RestoreOnDrop(&events))
        },
        || {
            events.borrow_mut().push("search");
            7
        },
    )
    .expect("policy setup succeeds");

    assert_eq!(outcome, 7);
    assert_eq!(*events.borrow(), ["search"]);
}

#[test]
fn operator_prepare_searches_without_changing_ui_state_after_tty_gate() {
    let keychain = FakeKeychain::new();
    assert!(matches!(
        keychain.prepare(true),
        ClaudeKeychainRead::Missing
    ));
    assert_eq!(keychain.query_count.get(), 0);
    assert_eq!(keychain.disable_count.get(), 0);
    assert_eq!(keychain.search_count.get(), 1);
    assert!(keychain.user_interaction_allowed.get());
}

#[test]
fn headless_operator_prepare_returns_consent_required_without_keychain_calls() {
    let keychain = FakeKeychain::new();
    assert!(matches!(
        keychain.prepare(false),
        ClaudeKeychainRead::ConsentRequired
    ));
    assert_eq!(keychain.query_count.get(), 0);
    assert_eq!(keychain.disable_count.get(), 0);
    assert_eq!(keychain.search_count.get(), 0);
    assert_eq!(keychain.restore_count.get(), 0);
    assert!(keychain.user_interaction_allowed.get());
}

#[test]
fn unattended_policy_fails_closed_if_ui_cannot_be_disabled() {
    let events = RefCell::new(Vec::new());
    let outcome = with_keychain_interaction_permission(
        false,
        || Ok(true),
        || {
            events.borrow_mut().push("disable");
            Err::<(), ()>(())
        },
        || {
            events.borrow_mut().push("search");
            7
        },
    );

    assert_eq!(outcome.unwrap_err(), ());
    assert_eq!(*events.borrow(), ["disable"]);
}

#[test]
fn fake_keychain_read_maps_search_outcomes_only_while_ui_is_disabled() {
    enum ExpectedRead {
        Missing,
        Denied,
        ConsentRequired,
    }

    for (status, expected) in [
        (-25300, ExpectedRead::Missing),
        (-25293, ExpectedRead::Denied),
        (-128, ExpectedRead::Denied),
        (-25308, ExpectedRead::ConsentRequired),
        (-1, ExpectedRead::Missing),
    ] {
        let keychain = FakeKeychain::new();
        keychain.search_status.set(Some(status));

        let outcome = keychain.read_unattended();

        match expected {
            ExpectedRead::Missing => assert!(matches!(outcome, ClaudeKeychainRead::Missing)),
            ExpectedRead::Denied => assert!(matches!(outcome, ClaudeKeychainRead::Denied)),
            ExpectedRead::ConsentRequired => {
                assert!(matches!(outcome, ClaudeKeychainRead::ConsentRequired));
            }
        }
        assert_eq!(keychain.query_count.get(), 1);
        assert_eq!(keychain.disable_count.get(), 1);
        assert_eq!(keychain.search_count.get(), 1);
        assert_eq!(keychain.restore_count.get(), 1);
        assert!(keychain.user_interaction_allowed.get());
    }

    let keychain = FakeKeychain::new();
    *keychain.payload.borrow_mut() = Some(" {\"fixture\":true} ".to_owned());
    let outcome = keychain.read_unattended();
    assert!(matches!(
        outcome,
        ClaudeKeychainRead::Payload { json } if json.as_str() == "{\"fixture\":true}"
    ));
    assert_eq!(keychain.search_count.get(), 1);
    assert_eq!(keychain.restore_count.get(), 1);
    assert!(keychain.user_interaction_allowed.get());
}

#[test]
fn keychain_payload_trimming_zeroizes_the_original_including_empty_payloads() {
    let mut secret = " {\"access_token\":\"fixture-secret\"} ".to_owned();
    let outcome = trim_keychain_payload_and_zeroize(&mut secret);
    assert!(matches!(
        outcome,
        ClaudeKeychainRead::Payload { json }
            if json.as_str() == "{\"access_token\":\"fixture-secret\"}"
    ));
    assert!(
        secret.is_empty(),
        "source JSON must be zeroized and cleared"
    );

    let mut empty = " \t\n ".to_owned();
    assert!(matches!(
        trim_keychain_payload_and_zeroize(&mut empty),
        ClaudeKeychainRead::Missing
    ));
    assert!(empty.is_empty(), "empty source JSON must also be cleared");
}

#[test]
fn fake_keychain_query_and_disable_errors_never_run_search() {
    let query_failure = FakeKeychain::new();
    query_failure.query_error.set(true);
    assert!(matches!(
        query_failure.read_unattended(),
        ClaudeKeychainRead::ConsentRequired
    ));
    assert_eq!(query_failure.query_count.get(), 1);
    assert_eq!(query_failure.disable_count.get(), 0);
    assert_eq!(query_failure.search_count.get(), 0);
    assert!(query_failure.user_interaction_allowed.get());

    let disable_failure = FakeKeychain::new();
    disable_failure.disable_error.set(true);
    assert!(matches!(
        disable_failure.read_unattended(),
        ClaudeKeychainRead::ConsentRequired
    ));
    assert_eq!(disable_failure.query_count.get(), 1);
    assert_eq!(disable_failure.disable_count.get(), 1);
    assert_eq!(disable_failure.search_count.get(), 0);
    assert!(disable_failure.user_interaction_allowed.get());
}

#[test]
fn nested_fake_process_guards_restore_ui_only_after_the_last_scope() {
    let user_interaction_allowed = Cell::new(true);
    let restore_count = Cell::new(0);
    let mut state = UnattendedKeychainGuardState::<FakeInteractionLock<'_>>::new();

    acquire_unattended_keychain_scope(
        &mut state,
        || Ok(user_interaction_allowed.get()),
        || {
            let previous_value = user_interaction_allowed.replace(false);
            Ok(FakeInteractionLock {
                user_interaction_allowed: &user_interaction_allowed,
                restore_count: &restore_count,
                previous_value,
            })
        },
    )
    .expect("first guard acquires UI lock");
    acquire_unattended_keychain_scope(
        &mut state,
        || unreachable!("nested guard must reuse the established scope"),
        || unreachable!("nested guard must not toggle UI again"),
    )
    .expect("nested guard reuses UI lock");

    assert_eq!(state.active_scopes, 2);
    assert!(!user_interaction_allowed.get());
    release_unattended_keychain_scope(&mut state);
    assert_eq!(state.active_scopes, 1);
    assert!(!user_interaction_allowed.get());
    assert_eq!(restore_count.get(), 0);

    release_unattended_keychain_scope(&mut state);
    assert_eq!(state.active_scopes, 0);
    assert!(user_interaction_allowed.get());
    assert_eq!(restore_count.get(), 1);
}

#[test]
fn fake_process_guard_setup_errors_do_not_create_a_scope() {
    let mut state = UnattendedKeychainGuardState::<FakeInteractionLock<'_>>::new();
    assert_eq!(
        acquire_unattended_keychain_scope(
            &mut state,
            || Err(()),
            || { unreachable!("a failed state query must stop setup") }
        ),
        Err(ClaudeKeychainPolicyError::StateUnavailable)
    );
    assert_eq!(state.active_scopes, 0);

    assert_eq!(
        acquire_unattended_keychain_scope(&mut state, || Ok(true), || Err(())),
        Err(ClaudeKeychainPolicyError::DisableFailed)
    );
    assert_eq!(state.active_scopes, 0);
    assert!(state.interaction_lock.is_none());
}
