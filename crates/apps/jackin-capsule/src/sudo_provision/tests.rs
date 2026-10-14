// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

#[test]
fn grant_on_missing_writes() {
    assert_eq!(sudo_action(true, false), SudoAction::Write);
}

#[test]
fn grant_off_present_removes() {
    assert_eq!(sudo_action(false, true), SudoAction::Remove);
}

#[test]
fn grant_on_present_is_noop() {
    assert_eq!(sudo_action(true, true), SudoAction::Noop);
}

#[test]
fn grant_off_missing_is_noop_not_remove() {
    assert_eq!(sudo_action(false, false), SudoAction::Noop);
}
