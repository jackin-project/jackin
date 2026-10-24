// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::ChildRegistry;

#[test]
fn older_reservation_does_not_release_newer_reservation_for_reused_pid() {
    let mut registry = ChildRegistry::default();
    let older = registry.register(u32::MAX).expect("older reservation");
    let newer = registry.register(u32::MAX).expect("newer reservation");
    drop(older);
    assert!(registry.contains(u32::MAX));
    drop(newer);
    assert!(!registry.contains(u32::MAX));
}
