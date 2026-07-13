// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

const ENV_FORCE_PANIC: &str = "JACKIN_CAPSULE_FORCE_PANIC";

#[expect(
    clippy::panic,
    reason = "operator-triggered diagnostics path used to verify symbolicated capsule panic logs"
)]
pub(crate) fn panic_if_requested_from_env() {
    let enabled = std::env::var(ENV_FORCE_PANIC)
        .ok()
        .as_deref()
        .is_some_and(force_panic_enabled);
    if !enabled {
        return;
    }

    crate::clog!("{ENV_FORCE_PANIC}=1 requested; forcing capsule diagnostics panic");
    panic!("{ENV_FORCE_PANIC}=1 forced capsule diagnostics panic");
}

fn force_panic_enabled(raw: &str) -> bool {
    matches!(
        raw.trim().to_ascii_lowercase().as_str(),
        "1" | "true" | "yes" | "on"
    )
}

#[cfg(test)]
mod tests;
