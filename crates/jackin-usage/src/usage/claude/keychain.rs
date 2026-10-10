// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! `Claude` macOS Keychain reads with an unattended default and a guarded
//! operator preparation entry point.

use zeroize::{Zeroize as _, Zeroizing};

/// Safe failure while establishing a process-wide unattended Keychain scope.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClaudeKeychainPolicyError {
    /// Security.framework could not report the current interaction setting.
    StateUnavailable,
    /// Security.framework could not disable Keychain UI.
    DisableFailed,
    /// This process already owns a different selected Claude service.
    ScopeConflict,
    /// A caller supplied an invalid or oversized Keychain service name.
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

/// Raw Keychain lookup outcome for one service. Secret-free in its own labels
/// (`json` carries the payload but the type is never formatted/logged).
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
pub fn read_claude_keychain_item(service: &str) -> ClaudeKeychainRead {
    if !super::lease::valid_claude_keychain_service(service) {
        return ClaudeKeychainRead::Missing;
    }
    if let Some(json) = super::lease::cached_claude_keychain_payload(service) {
        return ClaudeKeychainRead::Payload { json };
    }
    if super::lease::bootstrapped_claude_service().is_some() {
        // A foreground bootstrap scopes this process to one exact source.
        // Never silently probe another profile's Keychain item.
        return ClaudeKeychainRead::ConsentRequired;
    }
    read_claude_keychain_item_uncached(service)
}

/// Read one service with Keychain UI disabled, bypassing the selected cache.
/// Only the explicit 401 reread path may call this after bootstrap.
pub(crate) fn read_claude_keychain_item_uncached(service: &str) -> ClaudeKeychainRead {
    if !super::lease::valid_claude_keychain_service(service) {
        return ClaudeKeychainRead::Missing;
    }
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

/// Explicitly prepare Claude credentials from an attached operator terminal.
///
/// The provider boundary checks all three standard streams before it queries
/// or searches Keychain. Headless callers receive `ConsentRequired`, which
/// maps to the stable `interaction_required` outcome at the broker boundary.
pub fn prepare_claude_keychain_auth(service: &str) -> ClaudeKeychainRead {
    let terminal = all_stdio_are_terminal();
    if !terminal {
        return ClaudeKeychainRead::ConsentRequired;
    }
    if !super::lease::valid_claude_keychain_service(service) {
        return ClaudeKeychainRead::Missing;
    }
    prepare_claude_keychain_auth_with(terminal, || {
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
// terminal check performed by `prepare_claude_keychain_auth`.
fn prepare_claude_keychain_auth_with(
    all_stdio_are_terminal: bool,
    read_item: impl FnOnce() -> ClaudeKeychainRead,
) -> ClaudeKeychainRead {
    if !all_stdio_are_terminal {
        return ClaudeKeychainRead::ConsentRequired;
    }
    read_item()
}

pub(super) fn all_stdio_are_terminal() -> bool {
    use std::io::IsTerminal as _;

    let stdin_is_terminal = std::io::stdin().is_terminal();
    let stdout_is_terminal = std::io::stdout().is_terminal();
    let stderr_is_terminal = std::io::stderr().is_terminal();
    stdin_is_terminal && stdout_is_terminal && stderr_is_terminal
}

fn trim_keychain_payload_and_zeroize(json: &mut String) -> ClaudeKeychainRead {
    let payload = Zeroizing::new(json.trim().to_owned());
    json.zeroize();
    if payload.is_empty() {
        ClaudeKeychainRead::Missing
    } else {
        ClaudeKeychainRead::Payload { json: payload }
    }
}

#[cfg(test)]
mod tests;
