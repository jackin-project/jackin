// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! `Claude` macOS Keychain reads with explicit interaction policy.

/// Whether a Keychain read may ask macOS to display an operator consent UI.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClaudeKeychainInteractionPolicy {
    /// Background discovery and monitoring must never display UI.
    Unattended,
    /// An explicit operator action may display Keychain consent UI.
    OperatorInitiated,
}

/// Safe failure while establishing a process-wide unattended Keychain scope.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClaudeKeychainPolicyError {
    /// Security.framework could not report the current interaction setting.
    StateUnavailable,
    /// Security.framework could not disable Keychain UI.
    DisableFailed,
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
#[derive(Default)]
struct UnattendedKeychainGuardState {
    active_scopes: usize,
    interaction_lock: Option<security_framework::os::macos::keychain::KeychainUserInteractionLock>,
}

#[cfg(target_os = "macos")]
fn unattended_keychain_guard_state() -> &'static std::sync::Mutex<UnattendedKeychainGuardState> {
    static STATE: std::sync::OnceLock<std::sync::Mutex<UnattendedKeychainGuardState>> =
        std::sync::OnceLock::new();
    STATE.get_or_init(|| std::sync::Mutex::new(UnattendedKeychainGuardState::default()))
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
        if state.active_scopes == 0 {
            let interaction_allowed = SecKeychain::user_interaction_allowed()
                .map_err(|_| ClaudeKeychainPolicyError::StateUnavailable)?;
            state.interaction_lock = if interaction_allowed {
                Some(
                    SecKeychain::disable_user_interaction()
                        .map_err(|_| ClaudeKeychainPolicyError::DisableFailed)?,
                )
            } else {
                None
            };
        }
        state.active_scopes += 1;
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
        state.active_scopes = state.active_scopes.saturating_sub(1);
        if state.active_scopes == 0 {
            // Drop under the read/transition lock so an in-flight search never
            // sees interaction re-enabled midway through its lookup.
            drop(state.interaction_lock.take());
        }
    }
}

/// Raw Keychain lookup outcome for one service. Secret-free in its own labels
/// (`json` carries the payload but the type is never formatted/logged).
#[expect(
    missing_debug_implementations,
    reason = "credential type: the keychain payload must never be formatted into a log or error"
)]
pub enum ClaudeKeychainRead {
    #[cfg(any(target_os = "macos", test))]
    Payload {
        json: String,
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

/// Read a Claude Keychain item under an explicit interaction policy.
///
/// All reads are serialized because Security.framework's user-interaction
/// setting is process-wide. An unattended read disables UI for the lookup and
/// restores the previous setting through an RAII guard. The operator-initiated
/// policy is intended only for an explicit credential-preparation action.
#[cfg(target_os = "macos")]
pub fn read_claude_keychain_item(
    service: &str,
    policy: ClaudeKeychainInteractionPolicy,
) -> ClaudeKeychainRead {
    use security_framework::item::{ItemClass, ItemSearchOptions};
    use security_framework::os::macos::keychain::SecKeychain;

    with_keychain_interaction_policy(
        policy,
        || SecKeychain::user_interaction_allowed().map_err(|_| ()),
        || SecKeychain::disable_user_interaction().map_err(|_| ()),
        || {
            let mut options = ItemSearchOptions::new();
            options
                .class(ItemClass::generic_password())
                .service(service)
                .load_data(true)
                .limit(1);
            match options.search() {
                Ok(results) => parse_claude_keychain_search_results(results),
                Err(error) => classify_claude_keychain_status(error.code()),
            }
        },
    )
    .unwrap_or(ClaudeKeychainRead::ConsentRequired)
}

#[cfg(target_os = "macos")]
fn parse_claude_keychain_search_results(
    results: impl IntoIterator<Item = security_framework::item::SearchResult>,
) -> ClaudeKeychainRead {
    use security_framework::item::SearchResult;

    for result in results {
        if let SearchResult::Data(bytes) = result {
            return match String::from_utf8(bytes) {
                Ok(text) if !text.trim().is_empty() => ClaudeKeychainRead::Payload {
                    json: text.trim().to_owned(),
                },
                _ => ClaudeKeychainRead::Missing,
            };
        }
    }
    ClaudeKeychainRead::Missing
}

#[cfg(not(target_os = "macos"))]
pub fn read_claude_keychain_item(
    _service: &str,
    _policy: ClaudeKeychainInteractionPolicy,
) -> ClaudeKeychainRead {
    ClaudeKeychainRead::Missing
}

#[cfg(any(target_os = "macos", test))]
fn with_keychain_interaction_policy<T, Guard>(
    policy: ClaudeKeychainInteractionPolicy,
    user_interaction_allowed: impl FnOnce() -> Result<bool, ()>,
    disable_user_interaction: impl FnOnce() -> Result<Guard, ()>,
    search: impl FnOnce() -> T,
) -> Result<T, ()> {
    let _serialized = KEYCHAIN_INTERACTION_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);

    if policy == ClaudeKeychainInteractionPolicy::OperatorInitiated {
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;

    struct RestoreOnDrop<'a>(&'a RefCell<Vec<&'static str>>);

    impl Drop for RestoreOnDrop<'_> {
        fn drop(&mut self) {
            self.0.borrow_mut().push("restore");
        }
    }

    #[test]
    fn unattended_policy_disables_ui_for_search_and_restores_it() {
        let events = RefCell::new(Vec::new());
        let outcome = with_keychain_interaction_policy(
            ClaudeKeychainInteractionPolicy::Unattended,
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
        let outcome = with_keychain_interaction_policy(
            ClaudeKeychainInteractionPolicy::Unattended,
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
    fn operator_policy_does_not_disable_ui() {
        let events = RefCell::new(Vec::new());
        let outcome = with_keychain_interaction_policy(
            ClaudeKeychainInteractionPolicy::OperatorInitiated,
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
        assert_eq!(*events.borrow(), ["search"]);
    }

    #[test]
    fn unattended_policy_fails_closed_if_ui_cannot_be_disabled() {
        let events = RefCell::new(Vec::new());
        let outcome = with_keychain_interaction_policy(
            ClaudeKeychainInteractionPolicy::Unattended,
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
}
