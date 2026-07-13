// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use ratatui::layout::Rect;
use ratatui::{Terminal, backend::TestBackend, buffer::Buffer};

use crate::tui::model::{LaunchIdentity, LaunchTargetKind, LaunchView};
use crate::tui::update::initial_view;
use crate::tui::view::render_launch_frame;

use super::{launch_container_info_rect, launch_container_info_state};

fn row_text(buf: &Buffer, row: u16, width: u16) -> String {
    (0..width)
        .map(|x| buf[(x, row)].symbol().to_owned())
        .collect()
}

fn view_with_identity() -> LaunchView {
    let mut view = initial_view();
    view.frame = 30;
    view.status = "building docker image".to_owned();
    view.identity = Some(LaunchIdentity {
        role: "the-architect".to_owned(),
        agent: "codex".to_owned(),
        target_kind: LaunchTargetKind::Directory,
        target_label: "/workspace/jackin".to_owned(),
        mounts: Vec::new(),
        image: None,
        container: Some("jk-2y0t4aw6-thearchitect".to_owned()),
    });
    view
}

#[test]
fn launch_container_info_keeps_run_id_bare_and_log_path_separate() {
    let view = view_with_identity();

    let state = launch_container_info_state(
        &view,
        "jk-run-b93735",
        Some("/Users/donbeave/.jackin-pr-495/data/diagnostics/runs/jk-run-b93735.jsonl"),
        true,
        "0.6.0-test",
    );
    let rows = state.rows();
    assert_eq!(
        rows.first()
            .map(jackin_tui::components::ContainerInfoRow::value),
        Some("jk-run-b93735"),
        "Run ID must stay the first Debug info row even when launch knows the container"
    );
    let run_row = rows
        .iter()
        .find(|row| row.value() == "jk-run-b93735")
        .expect("bare run id row present");
    assert!(run_row.is_copyable());
    assert!(
        !run_row.value().contains(".jsonl"),
        "Run ID row must not contain diagnostics path"
    );
    let log_row = rows
        .iter()
        .find(|row| row.value().ends_with("jk-run-b93735.jsonl"))
        .expect("diagnostics log path row present");
    assert!(log_row.is_copyable());
    assert_eq!(
        log_row.href(),
        Some("file:///Users/donbeave/.jackin-pr-495/data/diagnostics/runs/jk-run-b93735.jsonl")
    );
    let reveal_row = rows
        .iter()
        .find(|row| row.value().ends_with("jk-run-b93735.jsonl") && !row.is_copyable())
        .expect("diagnostics reveal row present");
    assert_eq!(
        reveal_row.href(),
        Some("file:///Users/donbeave/.jackin-pr-495/data/diagnostics/runs/jk-run-b93735.jsonl")
    );
}

#[test]
fn launch_container_info_omits_run_rows_when_debug_disabled() {
    let view = initial_view();
    let state = launch_container_info_state(
        &view,
        "jk-run-b93735",
        Some("/Users/donbeave/.jackin-pr-495/data/diagnostics/runs/jk-run-b93735.jsonl"),
        false,
        "0.6.0-test",
    );
    assert!(
        state
            .rows()
            .iter()
            .all(|row| !row.value().contains("jk-run")),
        "launch run diagnostics rows are debug-only"
    );
}

#[test]
fn launch_container_info_backend_only_shows_telemetry_without_reveal() {
    let view = initial_view();
    let state = launch_container_info_state(&view, "jk-run-b93735", None, true, "0.6.0-test");
    let rows = state.rows();

    assert!(
        rows.iter()
            .any(|row| row.label() == "Telemetry" && row.value().contains("jk-run-b93735")),
        "backend-only runs should show a telemetry query hint"
    );
    assert!(
        rows.iter().all(|row| row.label() != "Diagnostics log"
            && row.label() != "Reveal diagnostics"
            && row.href().is_none()),
        "backend-only runs must not expose a fabricated diagnostics path"
    );
}

#[test]
fn launch_debug_info_keeps_status_footer_visible() {
    let area = Rect::new(0, 0, 90, 18);
    let mut view = view_with_identity();
    view.container_info_open = true;
    let backend = TestBackend::new(area.width, area.height);
    let mut terminal = Terminal::new(backend).expect("test backend should initialize");

    terminal
        .draw(|frame| {
            render_launch_frame(
                frame,
                &view,
                "jk-run-c46709",
                Some("/Users/donbeave/.jackin-pr-495/data/diagnostics/runs/jk-run-c46709.jsonl"),
                true,
                None,
                true,
                "0.6.0-test",
            );
        })
        .expect("render should succeed");

    let footer = row_text(terminal.backend().buffer(), area.height - 1, area.width);
    let hint = row_text(terminal.backend().buffer(), area.height - 3, area.width);
    let separator = row_text(terminal.backend().buffer(), area.height - 2, area.width);
    assert!(
        footer.contains("jk-run-c46709"),
        "debug footer should stay visible while Debug info is open: {footer:?}"
    );
    assert!(
        footer.contains("2y0t4aw6"),
        "instance footer should stay visible while Debug info is open: {footer:?}"
    );
    assert!(
        hint.contains("copy value") && hint.contains("Esc"),
        "Debug info hint should render in the reserved hint row: {hint:?}"
    );
    assert!(
        separator.trim().is_empty(),
        "separator row should stay blank between hint and footer: {separator:?}"
    );

    let state = launch_container_info_state(
        &view,
        "jk-run-c46709",
        Some("/Users/donbeave/.jackin-pr-495/data/diagnostics/runs/jk-run-c46709.jsonl"),
        true,
        "0.6.0-test",
    );
    let rect = launch_container_info_rect(area, &state, true);
    let below_dialog_y = rect.y.saturating_add(rect.height);
    if below_dialog_y < area.height.saturating_sub(3) {
        let below_dialog = row_text(terminal.backend().buffer(), below_dialog_y, area.width);
        assert!(
            !below_dialog.contains("copy value") && !below_dialog.contains("dismiss"),
            "Debug info hint must not float below the dialog: {below_dialog:?}"
        );
    }
}

#[test]
fn launch_debug_info_hides_status_footer_when_debug_disabled() {
    let area = Rect::new(0, 0, 90, 18);
    let mut view = view_with_identity();
    view.container_info_open = true;
    let backend = TestBackend::new(area.width, area.height);
    let mut terminal = Terminal::new(backend).expect("test backend should initialize");

    terminal
        .draw(|frame| {
            render_launch_frame(
                frame,
                &view,
                "jk-run-c46709",
                Some("/Users/donbeave/.jackin-pr-495/data/diagnostics/runs/jk-run-c46709.jsonl"),
                true,
                None,
                false,
                "0.6.0-test",
            );
        })
        .expect("render should succeed");

    let bottom = row_text(terminal.backend().buffer(), area.height - 1, area.width);
    assert!(
        bottom.contains("copy value") && bottom.contains("Esc"),
        "Debug info hint should use the bottom row in non-debug: {bottom:?}"
    );
    assert!(
        !bottom.contains("jk-run-c46709") && !bottom.contains("2y0t4aw6"),
        "non-debug Debug info must not keep the status footer visible: {bottom:?}"
    );
}
