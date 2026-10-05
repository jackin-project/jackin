// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Modal input coordinates come from the emitted frame, not hit-test geometry.

use super::*;

pub(super) fn painted_value_position(mux: &mut Multiplexer, text: &str) -> (u16, u16) {
    let (rows, cols) = mux.render.terminal_size();
    let mut client = VirtualClient::new(rows, cols);
    client.apply(&compose_after(mux, FullRedrawReason::ExplicitRedraw));
    for row in 0..rows {
        let line = (0..cols)
            .map(|col| VirtualClient::cell_text(client.grid.cell(row, col)))
            .collect::<String>();
        if let Some(byte_col) = line.find(text) {
            let col = line[..byte_col].chars().count();
            return (row, u16::try_from(col).unwrap());
        }
    }
    panic!("expected painted modal value {text:?}");
}

#[test]
fn modal_coordinates_painted_borders_consume_and_neighbors_dismiss() {
    let (rows, cols) = (32, 100);
    let mut source = test_mux(rows, cols);
    source.dialog_push(Dialog::new_rename_tab(0, "coordinate oracle"));
    let mut client = VirtualClient::new(rows, cols);
    client.apply(&compose_after(&mut source, FullRedrawReason::DialogChange));
    let find = |symbols: &[&str]| {
        (0..rows)
            .flat_map(|row| (0..cols).map(move |col| (row, col)))
            .find(|&(row, col)| {
                client
                    .grid
                    .cell(row, col)
                    .is_some_and(|cell| symbols.contains(&cell.contents()))
            })
            .expect("modal corner must appear in emitted frame")
    };
    let (top, left) = find(&["╭", "┌", "╔"]);
    let (bottom, right) = find(&["╯", "┘", "╝"]);
    for (row, col) in [(top, left), (top, right), (bottom, left), (bottom, right)] {
        let mut mux = test_mux(rows, cols);
        mux.dialog_push(Dialog::new_rename_tab(0, "coordinate oracle"));
        mux.handle_input(InputEvent::MousePress {
            row,
            col,
            button: 0,
        });
        assert!(
            mux.dialog_open(),
            "painted corner ({row}, {col}) must consume"
        );
    }
    for (row, col) in [
        (top - 1, left),
        (top, left - 1),
        (bottom + 1, right),
        (bottom, right + 1),
    ] {
        let mut mux = test_mux(rows, cols);
        mux.dialog_push(Dialog::new_rename_tab(0, "coordinate oracle"));
        mux.handle_input(InputEvent::MousePress {
            row,
            col,
            button: 0,
        });
        assert!(
            !mux.dialog_open(),
            "outside neighbor ({row}, {col}) must dismiss"
        );
    }
}

#[test]
fn modal_coordinates_painted_copy_value_matches_hover_pointer_and_clipboard() {
    let mut mux = single_pane_tab_mux_with_size(32, 100);
    mux.status.status_bar.identity_label = "jk-coordinate-oracle".to_owned();
    mux.open_container_info_dialog();
    let (row, col) = painted_value_position(&mut mux, "jk-coordinate-oracle");
    mux.client_registry.pointer_shapes_supported = true;
    let (tx, mut rx) = mpsc::unbounded_channel();
    mux.client_registry.client.attach(tx);

    // Motion goes through the same parsed-event dispatch as a real terminal.
    mux.handle_input(InputEvent::MousePress {
        row,
        col,
        button: SGR_NO_BUTTON_MOTION,
    });
    assert_eq!(
        mux.render.hover_target,
        Some(crate::tui::model::HoverTarget::DialogCopyTarget)
    );
    assert!(matches!(
        mux.dialog_top(),
        Some(Dialog::ContainerInfo {
            hovered_row: Some(0),
            ..
        })
    ));
    mux.client_registry.client.flush_out_of_band();
    assert!(rx.try_recv().unwrap().ends_with(b"\x1b]22;pointer\x1b\\"));

    mux.handle_input(InputEvent::MousePress {
        row,
        col,
        button: 0,
    });
    assert!(matches!(
        mux.dialog_top(),
        Some(Dialog::ContainerInfo {
            copied_row: Some(0),
            ..
        })
    ));
    mux.client_registry.client.flush_out_of_band();
    assert_eq!(
        rx.try_recv().unwrap(),
        crate::tui::view::encode_osc52_clipboard_write("jk-coordinate-oracle")
    );

    // The row above the painted value is outside the copy row.
    mux.handle_input(InputEvent::MousePress {
        row: row - 1,
        col,
        button: SGR_NO_BUTTON_MOTION,
    });
    assert_eq!(mux.render.hover_target, None);
    assert!(matches!(
        mux.dialog_top(),
        Some(Dialog::ContainerInfo {
            hovered_row: None,
            ..
        })
    ));
    mux.client_registry.client.flush_out_of_band();
    assert!(rx.try_recv().unwrap().ends_with(b"\x1b]22;default\x1b\\"));
}

#[test]
fn modal_coordinates_painted_usage_tab_matches_hover_and_click() {
    let mut mux = single_pane_tab_mux_with_size(32, 100);
    let projection =
        usage_projection_fixture("openai", "OpenAI", &[("canonical-openai", "seed")], 1);
    mux.dialog_push(Dialog::new_usage(Some(projection)));
    let (row, label_col) = painted_value_position(&mut mux, "OpenAI");
    // Tabs paint one padding cell on each side of their visible label.
    // The first cell of a nonfirst tab exposes a stale col-1 shim: it
    // would hit the gap immediately to the left instead of selecting OpenAI.
    let left = label_col - 1;
    let right = label_col + u16::try_from("OpenAI".len()).unwrap();
    assert_eq!(
        mux.dialog_top().unwrap().usage_selected_tab(),
        Some(crate::tui::components::dialog::UsageDialogTab::Overview)
    );
    mux.handle_input(InputEvent::MousePress {
        row,
        col: left,
        button: SGR_NO_BUTTON_MOTION,
    });
    assert_eq!(
        mux.render.hover_target,
        Some(crate::tui::model::HoverTarget::DialogCopyTarget)
    );
    assert!(matches!(
        mux.dialog_top(),
        Some(Dialog::Usage {
            hovered_tab: Some(1),
            ..
        })
    ));
    mux.handle_input(InputEvent::MousePress {
        row,
        col: left,
        button: 0,
    });
    assert_eq!(
        mux.dialog_top().unwrap().usage_selected_tab(),
        Some(crate::tui::components::dialog::UsageDialogTab::Provider)
    );
    assert_eq!(
        mux.dialog_top()
            .unwrap()
            .usage_destination()
            .unwrap()
            .canonical_account_id,
        "canonical-openai"
    );
    mux.handle_input(InputEvent::MousePress {
        row,
        col: right,
        button: SGR_NO_BUTTON_MOTION,
    });
    assert!(matches!(
        mux.dialog_top(),
        Some(Dialog::Usage {
            hovered_tab: Some(1),
            ..
        })
    ));
    for (neighbor_row, neighbor_col) in [
        (row, left - 1),
        (row, right + 1),
        (row + 1, label_col),
        (row - 1, label_col),
    ] {
        mux.handle_input(InputEvent::MousePress {
            row: neighbor_row,
            col: neighbor_col,
            button: SGR_NO_BUTTON_MOTION,
        });
        assert_eq!(
            mux.render.hover_target, None,
            "tab neighbor ({neighbor_row}, {neighbor_col}) must not advertise action"
        );
        assert!(matches!(
            mux.dialog_top(),
            Some(Dialog::Usage {
                hovered_tab: None,
                ..
            })
        ));
        mux.handle_input(InputEvent::MousePress {
            row: neighbor_row,
            col: neighbor_col,
            button: 0,
        });
        assert_eq!(
            mux.dialog_top()
                .unwrap()
                .usage_destination()
                .unwrap()
                .canonical_account_id,
            "canonical-openai"
        );
    }
}
