// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

#[test]
fn metadata_only_rewrites_echo_full_cell_attributes() {
    for glyph in ["x", "界"] {
        let (mut mux, mut client, sid) = attached_single_pane();
        for style in ["4:3;58;5;12;53", "4:5;58;2;12;34;56;55", "4;59"] {
            let bytes = format!("\x1b[H\x1b[{style}m{glyph}");
            feed_and_compose(&mut mux, &mut client, sid, bytes.as_bytes());
            assert_frame_conformance(&mut mux, &client, "metadata-only rewrite");
        }
    }
}

#[test]
fn modal_owns_sgr_metadata_then_restores_pane_on_dismiss() {
    let (mut mux, mut client, sid) = attached_single_pane();
    let pane = mux.visible_panes().into_iter().next().unwrap();
    let mut bytes = String::from("\x1b[4:3;58;5;12;53m");
    for row in 1..=pane.inner.rows {
        bytes.push_str(&format!(
            "\x1b[{row};1H{}",
            "x".repeat(usize::from(pane.inner.cols))
        ));
    }
    feed_and_compose(&mut mux, &mut client, sid, bytes.as_bytes());
    assert_frame_conformance(&mut mux, &client, "styled pane before modal");

    mux.apply_action(Action::OpenGithubContext);
    assert!(mux.dialog_open());
    let frame = mux.compose_pending_frame();
    assert!(!frame.is_empty());
    for sequence in [b"\x1b[53m".as_slice(), b"\x1b[4:3m", b"\x1b[58;"] {
        assert!(
            !frame.windows(sequence.len()).any(|bytes| bytes == sequence),
            "hidden pane metadata leaked into modal bytes: {sequence:?}"
        );
    }
    client.apply(&frame);
    feed_and_compose(&mut mux, &mut client, sid, b"\x1b[H\x1b[4:5;58;5;42;55mx");
    mux.apply_dialog_action(DialogAction::Dismiss);
    client.apply(&mux.compose_pending_frame());
    assert_frame_conformance(&mut mux, &client, "styled pane restored after modal");
}
