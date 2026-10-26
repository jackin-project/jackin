// SPDX-FileCopyrightText: 2026 The jackin❯ Authors
// SPDX-License-Identifier: Apache-2.0

mod fixture;
mod linked_worktrees;
mod owner_publication;
mod replacement;

#[cfg(any(target_os = "linux", target_os = "macos"))]
use super::device_matches;

#[cfg(target_os = "linux")]
#[test]
fn linux_named_and_opened_device_values_match_directly() {
    let named = 4_294_967_297_u64;
    assert!(device_matches(named, named));
    assert!(!device_matches(named, named + 1));
}

#[cfg(target_os = "macos")]
#[test]
fn macos_device_match_preserves_high_bit_patterns_on_both_sides() {
    let named: nix::sys::stat::dev_t = i32::MIN;
    let sign_extended_metadata = i64::from(i32::MIN).cast_unsigned();
    let zero_extended_metadata = u64::from(i32::MIN.cast_unsigned());

    assert!(device_matches(named, sign_extended_metadata));
    assert!(device_matches(named, zero_extended_metadata));
    assert!(!device_matches(named, zero_extended_metadata + 1));
}
