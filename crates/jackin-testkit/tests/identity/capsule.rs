// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use crate::app::{App, row_text, text};

fn capsule(name: &str) -> App {
    App::new(
        name,
        &[
            "--scenario",
            "capsule-terminal",
            "--motion",
            "paused",
            "--frame",
            "40",
        ],
        120,
        40,
        "truecolor",
    )
}

fn drawn(app: &App) {
    app.wait_text("Waiting for room");
}

fn chord(app: &App, key: &str) {
    app.key("ctrl+b");
    app.wait_text("prefix…");
    app.key(key);
}

#[test]
fn jackin_terminal_live_tab_dialogs_and_info() {
    let app = capsule("jackin-terminal-live");
    drawn(&app);
    app.advance_clock(1_000);
    app.wait_text("gpt-5-codex");

    chord(&app, "x");
    app.wait_text("Codex keeps running in the sandbox.");
    app.visual("functional/jackin_close_pane_codex");
    app.key("esc");
    app.wait_absent("Close pane?");

    chord(&app, "l");
    chord(&app, "x");
    app.wait_text("The shell ends.");
    app.visual("functional/jackin_close_pane_shell");
    app.key("esc");
    app.wait_absent("Close pane?");

    chord(&app, "&");
    app.wait_text("Closes every pane in this tab.");
    app.visual("functional/jackin_close_tab");
    app.key("esc");
    app.wait_absent("Close tab?");

    chord(&app, "h");
    app.paste("git status\ngit diff\ncargo test");
    app.wait_text("Paste 3 lines?");
    app.visual("functional/jackin_paste_confirm");
    app.key("esc");
    app.wait_absent("Paste 3 lines?");

    chord(&app, "i");
    app.wait_text("payments-platform · gpt-5-codex");
    app.visual("functional/jackin_pane_info_title");
}

#[test]
fn jackin_terminal_pending_attaching_and_viewing_panes() {
    let app = capsule("jackin-terminal-states-a");
    drawn(&app);
    app.advance_clock(1_000);
    app.wait_text("gpt-5-codex");

    chord(&app, "n");
    app.advance_clock(1_500);
    app.wait_text("Pane too small · zoom with Ctrl+B z");
    app.visual("functional/jackin_pane_pending");
    chord(&app, "z");
    app.advance_clock(300);
    app.wait_text("Attaching to Codex…");
    chord(&app, "z");

    chord(&app, "n");
    app.advance_clock(6_000);
    app.wait_text("Attaching to Codex…");
    app.visual("functional/jackin_pane_attaching");

    chord(&app, "n");
    app.advance_clock(1_500);
    app.wait_text("Read-only · another client controls this pane");
    app.visual("functional/jackin_pane_viewing");
}

#[test]
fn jackin_terminal_detached_exited_and_failed_panes() {
    let app = capsule("jackin-terminal-states-b");
    drawn(&app);
    app.advance_clock(1_000);
    app.wait_text("gpt-5-codex");

    chord(&app, "5");
    app.advance_clock(2_500);
    app.wait_text("Detached from Codex · press Enter to reattach");
    app.visual("functional/jackin_pane_detached");
    app.key("enter");
    app.advance_clock(6_000);
    app.wait_absent("Detached from Codex");

    chord(&app, "6");
    app.advance_clock(2_500);
    app.wait_text("Codex exited (code 0) · press Enter to restart · the sandbox keeps running");
    app.visual("functional/jackin_pane_exited");

    app.advance_clock(300);
    chord(&app, "7");
    app.wait_screen("the failure is in the status line", |screen| {
        row_text(screen, screen.rows() - 1).contains("could not attach to Codex")
    });
    app.visual("functional/jackin_pane_failed");
}

#[test]
fn jackin_terminal_task_panel_shows_task_and_statuses() {
    let app = capsule("jackin-terminal-task");
    drawn(&app);
    app.advance_clock(1_000);
    app.wait_text("gpt-5-codex");
    chord(&app, "i");
    let info = app.wait_text("Ship the multi-agent launch");
    assert!(text(&info).contains("working"), "{}", text(&info));
    assert!(text(&info).contains("OMP"), "{}", text(&info));
    app.visual("functional/jackin_terminal_task_panel");
    app.key("esc");
    chord(&app, "d");
    app.wait_text("Detached · payments-platform keeps running");
}
