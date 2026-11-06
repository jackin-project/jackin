// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use tui_test::TerminalSnapshot;

use crate::app::{App, Point, find, find_all, row_text, text};

pub(crate) const PAUSED: &[&str] = &["--motion", "paused", "--frame", "40"];

pub(crate) fn button(app: &App, label: &str) -> Point {
    let screen = app.wait_text(label);
    let candidates: Vec<_> = find_all(&screen, label)
        .into_iter()
        .filter(|point| {
            u32::from(point.y) + 2 < screen.rows()
                && row_text(&screen, u32::from(point.y)).contains("Cancel")
                && row_text(&screen, u32::from(point.y)).contains('│')
        })
        .collect();
    assert_eq!(
        candidates.len(),
        1,
        "one dialog action {label:?}:\n{}",
        text(&screen)
    );
    candidates[0]
}

pub(crate) fn jackin(name: &str, motion: &str) -> App {
    App::new(
        name,
        &[
            "--scenario",
            "returning",
            "--motion",
            motion,
            "--frame",
            "40",
        ],
        120,
        40,
        "truecolor",
    )
}

pub(crate) fn scenario(name: &str, scenario: &str, cols: u16, rows: u16) -> App {
    let mut args = vec!["--scenario", scenario];
    args.extend_from_slice(PAUSED);
    App::new(name, &args, cols, rows, "truecolor")
}

pub(crate) fn fixed_app(name: &str) -> App {
    scenario(name, "openshell-fixed", 120, 40)
}

pub(crate) fn launch_checks(name: &str) -> App {
    scenario(name, "launch-checks", 120, 40)
}

pub(crate) fn tab_to(app: &App, label: &str) {
    for _ in 0..16 {
        let screen = app.settle();
        let focused = find_all(&screen, label).into_iter().any(|p| {
            [p.y, p.y + 1]
                .into_iter()
                .any(|y| row_text(&screen, u32::from(y)).contains('▎'))
        });
        if focused {
            return;
        }
        app.key("tab");
    }
    panic!("{label:?} never took focus:\n{}", text(&app.screen()));
}

pub(crate) fn turn_docker_on(app: &App) {
    app.key("down");
    app.key("e");
    app.wait_text("payments-platform › edit");
    tab_to(app, "Docker for the agent");
    app.key("space");
    app.wait_text("Turn on Docker for payments-platform?");
    app.key("escape");
    app.wait_absent("Turn on Docker for payments-platform?");
    app.settle();
    app.key("space");
    app.wait_text("Turn on Docker for payments-platform?");
    app.key("right");
    app.key("enter");
    app.wait_absent("Turn on Docker for payments-platform?");
}

pub(crate) fn footer(screen: &TerminalSnapshot) -> String {
    row_text(screen, screen.rows() - 1)
}

pub(crate) fn row_with(screen: &TerminalSnapshot, needle: &str) -> String {
    row_text(screen, u32::from(find(screen, needle).y))
}

pub(crate) fn row_has(screen: &TerminalSnapshot, needle: &str, part: &str) -> bool {
    (0..screen.rows())
        .map(|row| row_text(screen, row))
        .any(|row| row.contains(needle) && row.contains(part))
}

pub(crate) fn inside(point: Point) -> Point {
    Point {
        x: point.x + 2,
        ..point
    }
}

pub(crate) fn refused_with(app: &App, reason: &str) -> TerminalSnapshot {
    app.wait_screen(&format!("status reads {reason}"), |s| {
        footer(s).contains(reason)
    })
}

pub(crate) fn status_expired(app: &App, reason: &str) -> TerminalSnapshot {
    app.advance_clock(6_000);
    app.wait_screen("status expired", |s| !footer(s).contains(reason))
}

pub(crate) fn quiet_after(app: &App, reason: &str, unrelated_key: &str) {
    status_expired(app, reason);
    app.key(unrelated_key);
    app.settle();
}

pub(crate) fn open_fixed_editor(app: &App, downs: usize) -> TerminalSnapshot {
    app.wait_text("Current directory");
    for _ in 0..downs {
        app.key("down");
    }
    app.key("e");
    app.wait_text("Directory basename by default")
}

pub(crate) fn open_fixed_settings(app: &App, tab: &str, presses: usize) -> TerminalSnapshot {
    app.wait_text("Current directory");
    app.key("s");
    app.wait_text("Settings › global › General");
    for _ in 0..presses {
        app.key("]");
    }
    app.wait_text(&format!("Settings › global › {tab}"));
    app.key("enter");
    app.wait_screen("the tab body has focus", |s| {
        !footer(s).contains("Enter Edit fields")
    })
}

pub(crate) fn open_fixed_accounts(app: &App, downs: usize, crumb: &str) -> TerminalSnapshot {
    app.wait_text("Current directory");
    app.key("c");
    app.wait_text("Accounts › Overview");
    for _ in 0..downs {
        app.key("down");
    }
    app.wait_text(crumb)
}

pub(crate) fn open_fixed_wizard(app: &App) -> TerminalSnapshot {
    app.wait_text("Current directory");
    app.key("n");
    app.wait_text("New workspace · step 1 of 5 · Source")
}

pub(crate) fn agent_at(screen: &TerminalSnapshot, label: &str) -> Point {
    find_all(screen, label)
        .into_iter()
        .find(|point| point.x < 12)
        .unwrap_or_else(|| panic!("agent row {label}:\n{}", text(screen)))
}

pub(crate) fn agent_row(screen: &TerminalSnapshot, label: &str) -> String {
    row_text(screen, u32::from(agent_at(screen, label).y))
}

pub(crate) fn attach_capsule(app: &App) {
    app.wait_text("Current directory");
    app.key("down");
    app.key("right");
    app.key("down");
    app.key("enter");
    app.wait_text("Ctrl+B Pane commands");
}

pub(crate) fn launch_first_agent(app: &App, downs: usize, scope: &str) {
    app.wait_text("Current directory");
    for _ in 0..downs {
        app.key("down");
    }
    app.key("enter");
    app.wait_text(scope);
    app.key("space");
    app.key("enter");
    app.wait_text("Cancel launch…");
}

pub(crate) fn pick_one_password_reference(app: &App) {
    app.click_text("Choose…");
    app.wait_text("loading accounts…");
    app.advance_clock(300);
    app.wait_text("chainargos.1password.com");
    app.key("enter");
    app.wait_text("loading vaults…");
    app.advance_clock(300);
    app.wait_text("Engineering");
    app.key("enter");
    app.wait_text("loading items from Engineering…");
    app.advance_clock(300);
    app.wait_text("Anthropic · Work");
    app.type_text("Work");
    app.wait_screen("the item list is narrowed", |s| {
        !text(s).contains("OpenAI · Codex Primary")
    });
    app.key("enter");
    app.wait_text("loading Anthropic · Work…");
    app.advance_clock(300);
    app.wait_text("Choose field");
    app.key("enter");
    app.wait_text("Reference chosen");
}
