// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use crate::tui::update::FullRedrawReason;

#[test]
fn wipe_policy_erases_only_on_first_attach_and_resize() {
    let dir = tempfile::tempdir().unwrap();
    let mut mux = super::test_mux(dir.path());
    assert!(mux.render.wipe_pending.is_none());
    mux.invalidate(FullRedrawReason::DialogChange);
    assert!(mux.render.wipe_pending.is_none());
    mux.invalidate(FullRedrawReason::StatusChange);
    assert!(mux.render.wipe_pending.is_none());
    mux.invalidate(FullRedrawReason::FirstAttach);
    assert_eq!(mux.render.wipe_pending, Some(FullRedrawReason::FirstAttach));
    let mut mux = super::test_mux(dir.path());
    mux.invalidate(FullRedrawReason::Resize);
    assert_eq!(mux.render.wipe_pending, Some(FullRedrawReason::Resize));
}
