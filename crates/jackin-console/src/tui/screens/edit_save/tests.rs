//! Tests for `edit_save`.
use super::*;

#[test]
fn plan_clean_is_noop() {
    assert_eq!(plan_edit_save(false, true), EditSaveDisposition::Noop);
    assert_eq!(plan_edit_save(false, false), EditSaveDisposition::Noop);
}

#[test]
fn plan_dirty_without_confirm_saves_now() {
    assert_eq!(plan_edit_save(true, false), EditSaveDisposition::SaveNow);
    assert!(!save_opens_confirm_modal(EditSaveDisposition::SaveNow));
}

#[test]
fn plan_dirty_with_confirm_opens_modal() {
    assert_eq!(
        plan_edit_save(true, true),
        EditSaveDisposition::ConfirmDiscard
    );
    assert!(save_opens_confirm_modal(
        EditSaveDisposition::ConfirmDiscard
    ));
}
