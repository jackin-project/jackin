// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::{palette_binding, prefix_binding};

#[test]
fn invalid_prefix_disables_prefix_mode() {
    assert_eq!(prefix_binding(Some("operator-secret-invalid")), None);
}

#[test]
fn invalid_palette_uses_default() {
    assert_eq!(palette_binding(Some("operator-secret-invalid")), Some(0x1C));
}
