// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use crate::tui::components::dialog::{Dialog, PaletteCloseLabel};

#[tokio::test]
async fn last_session_exit_defers_when_dialog_already_open_inv_d19() {
    let dir = tempfile::tempdir().unwrap();
    let mut mux = super::test_mux(dir.path());
    mux.dialog_push(Dialog::new_command_palette(
        PaletteCloseLabel::for_pane_count(0),
    ));
    assert!(!super::super::handle_last_session_exit(&mut mux, None).await);
    assert!(mux.dialog_open());
}
