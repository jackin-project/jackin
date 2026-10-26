// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use crate::tui::components::dialog::{Dialog, PaletteCloseLabel};

use super::super::{ExitDisposition, PORTS, PersistencePort};

fn test_mux() -> crate::daemon::Multiplexer {
    let dir = tempfile::tempdir().unwrap();
    let config = jackin_protocol::CapsuleConfig {
        role: "test".to_owned(),
        workdir: dir.path().display().to_string(),
        ..jackin_protocol::CapsuleConfig::default()
    };
    crate::daemon::Multiplexer::new(24, 80, config).unwrap()
}

#[test]
fn persistence_port_reads_live_dialog_stack() {
    let mut mux = test_mux();
    assert!(matches!(
        PORTS.last_session_exit(&mux.control),
        ExitDisposition::Evaluate
    ));
    mux.dialog_push(Dialog::new_command_palette(
        PaletteCloseLabel::for_pane_count(0),
    ));
    assert!(matches!(
        PORTS.last_session_exit(&mux.control),
        ExitDisposition::Defer
    ));
}
