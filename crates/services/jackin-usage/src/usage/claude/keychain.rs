// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! `Claude` macOS keychain reads and state.

/// Raw Keychain lookup outcome for one service. Secret-free in its own labels
/// (`json` carries the payload but the type is never formatted/logged).
pub(crate) enum ClaudeKeychainRead {
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
pub(crate) fn classify_claude_keychain_status(code: i32) -> ClaudeKeychainRead {
    match code {
        -128 | -25293 => ClaudeKeychainRead::Denied,
        -25308 => ClaudeKeychainRead::ConsentRequired,
        _ => ClaudeKeychainRead::Missing,
    }
}

#[cfg(target_os = "macos")]
pub(crate) fn read_claude_keychain_item(service: &str) -> ClaudeKeychainRead {
    use security_framework::item::{ItemClass, ItemSearchOptions, SearchResult};

    let mut options = ItemSearchOptions::new();
    options
        .class(ItemClass::generic_password())
        .service(service)
        .load_data(true)
        .limit(1);
    // Keep authentication UI enabled: `errSecInteractionNotAllowed` tells us
    // that the matching item exists but needs consent, while
    // `errSecItemNotFound` means it is absent. A skip-auth query would erase
    // that distinction by hiding consent-gated items.
    match options.search() {
        Ok(results) => {
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
        Err(error) => classify_claude_keychain_status(error.code()),
    }
}

#[cfg(not(target_os = "macos"))]
pub(crate) fn read_claude_keychain_item(_service: &str) -> ClaudeKeychainRead {
    ClaudeKeychainRead::Missing
}

/// Process-lifetime Keychain coordination: serializes reader I/O so a consent
/// sheet is prompted at most once per wave, and remembers services the operator
/// explicitly denied so a denial is terminal for that service for the process
/// (no retry-prompt storm). A *missing* item is never cached, so a later
/// `claude /login` is picked up without an app restart (flow W5).
#[derive(Default)]
pub(crate) struct ClaudeKeychainState {
    inner: std::sync::Mutex<ClaudeKeychainInner>,
}

#[derive(Default)]
pub(crate) struct ClaudeKeychainInner {
    denied_services: std::collections::HashSet<String>,
    /// Count of reader invocations — a test seam proving reads are shared and
    /// each service is queried at most once per wave.
    reads: u64,
}

impl ClaudeKeychainState {
    /// Resolve one Keychain read for `service` through `reader`, honoring the
    /// process-terminal denial cache and serializing reader I/O.
    pub(crate) fn read_with<F>(&self, service: &str, reader: F) -> ClaudeKeychainRead
    where
        F: FnOnce(&str) -> ClaudeKeychainRead,
    {
        {
            let inner = self
                .inner
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            if inner.denied_services.contains(service) {
                return ClaudeKeychainRead::Denied;
            }
        }
        // Reader runs while holding the serialization lock so concurrent waves
        // cannot open two consent sheets for the same service at once.
        let mut inner = self
            .inner
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if inner.denied_services.contains(service) {
            return ClaudeKeychainRead::Denied;
        }
        inner.reads += 1;
        let read = reader(service);
        if matches!(read, ClaudeKeychainRead::Denied) {
            inner.denied_services.insert(service.to_owned());
        }
        read
    }

    #[cfg(test)]
    pub(crate) fn read_count(&self) -> u64 {
        self.inner
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .reads
    }
}

/// Production global Keychain state (one per process).
pub(crate) fn claude_keychain_state() -> &'static ClaudeKeychainState {
    static STATE: std::sync::OnceLock<ClaudeKeychainState> = std::sync::OnceLock::new();
    STATE.get_or_init(ClaudeKeychainState::default)
}
