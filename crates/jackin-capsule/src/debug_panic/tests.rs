// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::force_panic_enabled;

#[test]
fn force_panic_enabled_accepts_truthy_values() {
    for raw in ["1", "true", "TRUE", " yes ", "on"] {
        assert!(force_panic_enabled(raw), "{raw:?}");
    }
}

#[test]
fn force_panic_enabled_rejects_falsey_and_unknown_values() {
    for raw in ["", "0", "false", "no", "off", "panic"] {
        assert!(!force_panic_enabled(raw), "{raw:?}");
    }
}
