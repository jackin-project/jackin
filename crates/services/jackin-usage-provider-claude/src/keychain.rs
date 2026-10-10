// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! `Claude` macOS Keychain reads with an unattended default and an explicit
//! foreground lease bootstrap.

use zeroize::{Zeroize as _, Zeroizing};

/// Safe failure while establishing a process-wide unattended Keychain scope.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClaudeKeychainPolicyError {
    /// Security.framework could not report the current interaction setting.
    StateUnavailable,
    /// Security.framework could not disable Keychain UI.
    DisableFailed,
    /// A different foreground Claude source already owns the process cache.
    ScopeConflict,
    /// The selected Keychain service is invalid or too long.
    InvalidService,
}

/// Process-wide RAII scope that prevents Keychain operations from displaying
/// UI. Hold this for the unattended broker lifetime, before discovery starts.
/// Drop restores the previous setting when this scope disabled it.
#[expect(
    missing_debug_implementations,
    reason = "RAII guard owns process-wide Security.framework state"
)]
pub struct ClaudeUnattendedKeychainGuard {
    #[cfg(target_os = "macos")]
    active: bool,
}

#[cfg(any(target_os = "macos", test))]
static KEYCHAIN_INTERACTION_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

#[cfg(target_os = "macos")]
type NativeUnattendedKeychainGuardState = UnattendedKeychainGuardState<
    security_framework::os::macos::keychain::KeychainUserInteractionLock,
>;

#[cfg(any(target_os = "macos", test))]
struct UnattendedKeychainGuardState<Guard> {
    active_scopes: usize,
    interaction_lock: Option<Guard>,
}

#[cfg(any(target_os = "macos", test))]
impl<Guard> UnattendedKeychainGuardState<Guard> {
    fn new() -> Self {
        Self {
            active_scopes: 0,
            interaction_lock: None,
        }
    }
}

#[cfg(target_os = "macos")]
fn unattended_keychain_guard_state() -> &'static std::sync::Mutex<NativeUnattendedKeychainGuardState>
{
    static STATE: std::sync::OnceLock<std::sync::Mutex<NativeUnattendedKeychainGuardState>> =
        std::sync::OnceLock::new();
    STATE.get_or_init(|| std::sync::Mutex::new(NativeUnattendedKeychainGuardState::new()))
}

#[cfg(any(target_os = "macos", test))]
fn acquire_unattended_keychain_scope<Guard>(
    state: &mut UnattendedKeychainGuardState<Guard>,
    user_interaction_allowed: impl FnOnce() -> Result<bool, ()>,
    disable_user_interaction: impl FnOnce() -> Result<Guard, ()>,
) -> Result<(), ClaudeKeychainPolicyError> {
    if state.active_scopes == 0 {
        let interaction_allowed =
            user_interaction_allowed().map_err(|()| ClaudeKeychainPolicyError::StateUnavailable)?;
        state.interaction_lock = if interaction_allowed {
            Some(
                disable_user_interaction()
                    .map_err(|()| ClaudeKeychainPolicyError::DisableFailed)?,
            )
        } else {
            None
        };
    }
    state.active_scopes += 1;
    Ok(())
}

#[cfg(any(target_os = "macos", test))]
fn release_unattended_keychain_scope<Guard>(state: &mut UnattendedKeychainGuardState<Guard>) {
    state.active_scopes = state.active_scopes.saturating_sub(1);
    if state.active_scopes == 0 {
        drop(state.interaction_lock.take());
    }
}

/// Disable Keychain UI for an unattended process scope.
///
/// On non-macOS platforms the guard is a no-op because Claude Keychain access
/// is unavailable. If macOS cannot report or disable UI, callers must fail
/// closed and avoid credential discovery.
pub fn unattended_keychain_guard()
-> Result<ClaudeUnattendedKeychainGuard, ClaudeKeychainPolicyError> {
    #[cfg(target_os = "macos")]
    {
        use security_framework::os::macos::keychain::SecKeychain;

        let _serialized = KEYCHAIN_INTERACTION_LOCK
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let mut state = unattended_keychain_guard_state()
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        acquire_unattended_keychain_scope(
            &mut state,
            || SecKeychain::user_interaction_allowed().map_err(|_| ()),
            || SecKeychain::disable_user_interaction().map_err(|_| ()),
        )?;
        Ok(ClaudeUnattendedKeychainGuard { active: true })
    }
    #[cfg(not(target_os = "macos"))]
    {
        Ok(ClaudeUnattendedKeychainGuard {})
    }
}

#[cfg(target_os = "macos")]
impl Drop for ClaudeUnattendedKeychainGuard {
    fn drop(&mut self) {
        if !self.active {
            return;
        }
        let _serialized = KEYCHAIN_INTERACTION_LOCK
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let mut state = unattended_keychain_guard_state()
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        // Drop under the read/transition lock so an in-flight search never
        // sees interaction re-enabled midway through its lookup.
        release_unattended_keychain_scope(&mut state);
    }
}

/// Keychain lookup outcome. Payloads are zeroizing and the type is never
/// formatted or logged.
#[expect(
    missing_debug_implementations,
    reason = "credential type: the keychain payload must never be formatted into a log or error"
)]
pub enum ClaudeKeychainRead {
    #[cfg(any(target_os = "macos", test))]
    Payload {
        json: Zeroizing<String>,
    },
    Denied,
    Missing,
    /// A matching item requires operator consent before its payload can be read.
    ConsentRequired,
}

/// Classify a macOS `OSStatus` from a Keychain lookup. Only an explicit user
/// cancel (`errSecUserCanceled` = -128) or auth failure (`errSecAuthFailed` =
/// -25293) is a terminal `Denied`; headless interaction-not-allowed (-25308)
/// is `ConsentRequired`; item-not-found (-25300) and any other failure are
/// `Missing` (absence). Pure and cross-platform so tests never touch the real
/// Keychain.
#[cfg(any(target_os = "macos", test))]
pub fn classify_claude_keychain_status(code: i32) -> ClaudeKeychainRead {
    match code {
        -128 | -25293 => ClaudeKeychainRead::Denied,
        -25308 => ClaudeKeychainRead::ConsentRequired,
        _ => ClaudeKeychainRead::Missing,
    }
}

/// Read a Claude Keychain item without allowing Keychain UI.
///
/// The process-wide interaction setting is serialized and restored through an
/// RAII guard. If Security.framework cannot establish that no-UI scope, the
/// result is `ConsentRequired` and the item is not searched.
#[cfg(target_os = "macos")]
pub fn read_claude_keychain_item(service: &str) -> ClaudeKeychainRead {
    if let Some(selected_service) = crate::lease::bootstrapped_claude_service() {
        return if selected_service == service {
            crate::lease::cached_claude_keychain_payload(service)
                .map_or(ClaudeKeychainRead::Missing, |json| {
                    ClaudeKeychainRead::Payload { json }
                })
        } else {
            ClaudeKeychainRead::Missing
        };
    }
    read_claude_keychain_item_uncached(service)
}

/// Exact-service no-UI reread used only after a typed HTTP 401 while the
/// selected foreground lease is active.
pub(crate) fn read_claude_keychain_item_uncached(service: &str) -> ClaudeKeychainRead {
    #[cfg(target_os = "macos")]
    {
        use security_framework::os::macos::keychain::SecKeychain;

        read_claude_keychain_item_with(
            false,
            || SecKeychain::user_interaction_allowed().map_err(|_| ()),
            || SecKeychain::disable_user_interaction().map_err(|_| ()),
            || search_claude_keychain_item(service),
        )
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = service;
        ClaudeKeychainRead::Missing
    }
}

/// Read credentials from an attached operator terminal. The public API only
/// exposes the lease bootstrap, which retains this payload in process memory.
pub(crate) fn read_claude_keychain_item_for_foreground(service: &str) -> ClaudeKeychainRead {
    read_claude_keychain_item_for_foreground_with(all_stdio_are_terminal(), || {
        #[cfg(target_os = "macos")]
        {
            use security_framework::os::macos::keychain::SecKeychain;

            read_claude_keychain_item_with(
                true,
                || SecKeychain::user_interaction_allowed().map_err(|_| ()),
                || SecKeychain::disable_user_interaction().map_err(|_| ()),
                || search_claude_keychain_item(service),
            )
        }
        #[cfg(not(target_os = "macos"))]
        {
            let _ = service;
            ClaudeKeychainRead::Missing
        }
    })
}

#[cfg(target_os = "macos")]
fn search_claude_keychain_item(service: &str) -> Result<Option<Zeroizing<String>>, i32> {
    use security_framework::item::{ItemClass, ItemSearchOptions};

    let mut options = ItemSearchOptions::new();
    options
        .class(ItemClass::generic_password())
        .service(service)
        .load_data(true)
        .limit(1);
    match options.search() {
        Ok(results) => Ok(parse_claude_keychain_search_results(results)),
        Err(error) => Err(error.code()),
    }
}

#[cfg(target_os = "macos")]
fn parse_claude_keychain_search_results(
    results: impl IntoIterator<Item = security_framework::item::SearchResult>,
) -> Option<Zeroizing<String>> {
    use security_framework::item::SearchResult;

    for result in results {
        if let SearchResult::Data(bytes) = result {
            return match String::from_utf8(bytes) {
                Ok(json) => Some(Zeroizing::new(json)),
                Err(error) => {
                    let mut bytes = error.into_bytes();
                    bytes.zeroize();
                    None
                }
            };
        }
    }
    None
}

#[cfg(not(target_os = "macos"))]
pub fn read_claude_keychain_item(_service: &str) -> ClaudeKeychainRead {
    ClaudeKeychainRead::Missing
}

#[cfg(any(target_os = "macos", test))]
fn with_keychain_interaction_permission<T, Guard>(
    allow_ui: bool,
    user_interaction_allowed: impl FnOnce() -> Result<bool, ()>,
    disable_user_interaction: impl FnOnce() -> Result<Guard, ()>,
    search: impl FnOnce() -> T,
) -> Result<T, ()> {
    let _serialized = KEYCHAIN_INTERACTION_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);

    if allow_ui {
        return Ok(search());
    }

    // Preserve a pre-existing process setting: the RAII guard always
    // re-enables interaction, so do not acquire it if another owner already
    // disabled UI.
    if user_interaction_allowed()? {
        let _interaction_guard = disable_user_interaction()?;
        Ok(search())
    } else {
        Ok(search())
    }
}

#[cfg(any(target_os = "macos", test))]
fn read_claude_keychain_item_with<Guard>(
    allow_ui: bool,
    user_interaction_allowed: impl FnOnce() -> Result<bool, ()>,
    disable_user_interaction: impl FnOnce() -> Result<Guard, ()>,
    search: impl FnOnce() -> Result<Option<Zeroizing<String>>, i32>,
) -> ClaudeKeychainRead {
    let read = || match search() {
        Ok(Some(mut json)) => trim_keychain_payload_and_zeroize(&mut json),
        Ok(None) => ClaudeKeychainRead::Missing,
        Err(status) => classify_claude_keychain_status(status),
    };
    with_keychain_interaction_permission(
        allow_ui,
        user_interaction_allowed,
        disable_user_interaction,
        read,
    )
    .unwrap_or(ClaudeKeychainRead::ConsentRequired)
}

// Private injection seam: production callers cannot supply or bypass the
// terminal check performed by foreground lease bootstrap.
fn read_claude_keychain_item_for_foreground_with(
    all_stdio_are_terminal: bool,
    read_item: impl FnOnce() -> ClaudeKeychainRead,
) -> ClaudeKeychainRead {
    if !all_stdio_are_terminal {
        return ClaudeKeychainRead::ConsentRequired;
    }
    read_item()
}

pub(crate) fn all_stdio_are_terminal() -> bool {
    use std::io::IsTerminal as _;

    let stdin_is_terminal = std::io::stdin().is_terminal();
    let stdout_is_terminal = std::io::stdout().is_terminal();
    let stderr_is_terminal = std::io::stderr().is_terminal();
    stdin_is_terminal && stdout_is_terminal && stderr_is_terminal
}

fn trim_keychain_payload_and_zeroize(json: &mut String) -> ClaudeKeychainRead {
    let payload = json.trim().to_owned();
    json.zeroize();
    if payload.is_empty() {
        ClaudeKeychainRead::Missing
    } else {
        ClaudeKeychainRead::Payload {
            json: Zeroizing::new(payload),
        }
    }
}

#[cfg(test)]
mod tests {
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
            read_claude_keychain_item_for_foreground_with(all_stdio_are_terminal, || {
                self.read_with_ui(true)
            })
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
}
