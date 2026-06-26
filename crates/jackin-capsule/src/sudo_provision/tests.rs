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
    // The read-only-root fix: no sudo grant + no existing entry must be a no-op,
    // never an unlink — otherwise EROFS on hardened/locked fails the launch.
    assert_eq!(sudo_action(false, false), SudoAction::Noop);
}
