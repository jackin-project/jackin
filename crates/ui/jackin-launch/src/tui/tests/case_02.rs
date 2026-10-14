// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

#[test]
fn launch_container_info_renders_from_footer_chip_state() {
    jackin_diagnostics::set_debug_mode(true);
    let backend = TestBackend::new(100, 28);
    let mut terminal = ratatui::Terminal::new(backend).unwrap();
    let mut view = initial_view();
    view.identity = Some(LaunchIdentity {
        role: "agent-smith".to_owned(),
        agent: "codex".to_owned(),
        target_kind: LaunchTargetKind::Workspace,
        target_label: "big-monorepo".to_owned(),
        mounts: Vec::new(),
        image: None,
        container: Some("jk-k7p9m2xq-bigmonorepo-agentsmith".to_owned()),
    });
    view.container_info_open = true;
    terminal
        .draw(|frame| {
            render_launch_frame(frame, &view, "jk-run-rendered", true, None);
        })
        .unwrap();
    jackin_diagnostics::set_debug_mode(false);

    let rendered = format!("{:?}", terminal.backend().buffer());
    for needle in [
        "Debug info",
        "jk-k7p9m2xq-bigmonorepo-agentsmith",
        "jackin version",
        "agent-smith",
        "jk-run-rendered",
        "Telemetry",
    ] {
        assert!(
            rendered.contains(needle),
            "container info dialog must contain {needle:?}: {rendered}"
        );
    }
}

#[test]
fn failure_copy_target_at_ignores_non_copyable_rows() {
    let area = Rect::new(0, 0, 80, 24);
    let failure = launch_failure();
    let run_id = "jk-run-x";
    let rows = failure_popup_rows(&failure, run_id);
    let body_area = bottom_chrome_areas(area).body;
    let rect = failure_popup_rect_for_rows(body_area, &rows);
    let run_id_rect = failure_popup_value_rect(rect, &rows, FailureCopyTarget::RunId).unwrap();

    let message_y = run_id_rect.y.saturating_sub(2);
    assert_eq!(
        failure_copy_target_at(area, &failure, run_id, true, run_id_rect.x, message_y, None),
        None,
        "click on the non-copyable message row must not hit any target",
    );
}

#[test]
fn failure_copy_payload_sources_value_from_rows() {
    let failure = launch_failure();
    let run_id = "jk-run-payload";
    assert_eq!(
        failure_copy_payload(&failure, run_id, FailureCopyTarget::RunId).as_deref(),
        Some(run_id),
    );
}

#[test]
fn failure_popup_renders_copyable_rows_and_copied_badge() {
    let backend = TestBackend::new(120, 28);
    let mut terminal = ratatui::Terminal::new(backend).unwrap();
    let mut view = initial_view();
    view.failure = Some(launch_failure());
    view.failure_copied = Some(FailureCopyTarget::RunId);
    let run_id = "jk-run-rendered";
    terminal
        .draw(|frame| render_launch_frame(frame, &view, run_id, true, None))
        .unwrap();
    let rendered = format!("{:?}", terminal.backend().buffer());

    for needle in ["run id", run_id, "✓", "copy value"] {
        assert!(
            rendered.contains(needle),
            "rendered failure popup must contain {needle:?}; got {rendered}",
        );
    }
}
