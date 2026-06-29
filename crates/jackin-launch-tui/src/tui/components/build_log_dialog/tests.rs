use super::*;
use crate::tui::model::{LaunchIdentity, LaunchTargetKind};
use ratatui::{Terminal, backend::TestBackend};

fn row_text(buf: &ratatui::buffer::Buffer, row: u16, width: u16) -> String {
    (0..width)
        .map(|col| buf[(col, row)].symbol().to_owned())
        .collect::<String>()
}

#[test]
fn scrollbar_hit_maps_track_to_top_offset() {
    let area = Rect {
        x: 0,
        y: 0,
        width: 40,
        height: 12,
    };
    let raw: Vec<String> = (0..20).map(|idx| format!("line {idx}")).collect();
    let scrollbar = vertical_scrollbar_area(build_log_box_area(area));

    let top = build_log_scrollbar_top_offset_at(area, &raw, scrollbar.x, scrollbar.y)
        .expect("top of scrollable track should hit");
    let bottom = build_log_scrollbar_top_offset_at(
        area,
        &raw,
        scrollbar.x,
        scrollbar.y + scrollbar.height.saturating_sub(1),
    )
    .expect("bottom of scrollable track should hit");

    assert_eq!(top, 0);
    assert!(bottom > top);
}

#[test]
fn scrollbar_hit_ignores_non_track_columns() {
    let area = Rect {
        x: 0,
        y: 0,
        width: 40,
        height: 12,
    };
    let raw: Vec<String> = (0..20).map(|idx| format!("line {idx}")).collect();
    let scrollbar = vertical_scrollbar_area(build_log_box_area(area));

    assert_eq!(
        build_log_scrollbar_top_offset_at(area, &raw, scrollbar.x.saturating_sub(1), scrollbar.y),
        None
    );
}

#[test]
fn build_log_overlay_keeps_status_footer_in_debug_mode() {
    let area = Rect::new(0, 0, 80, 12);
    let mut view = crate::tui::update::initial_view();
    view.build_log_open = true;
    view.build_log_active = true;
    view.frame = 30;
    view.status = "building docker image".to_owned();
    view.identity = Some(LaunchIdentity {
        role: "the-architect".to_owned(),
        agent: "claude".to_owned(),
        target_kind: LaunchTargetKind::Directory,
        target_label: ".".to_owned(),
        mounts: Vec::new(),
        image: None,
        container: Some("jk-2y0t4aw6-the-architect".to_owned()),
    });
    view.build_log_lines = (0..30).map(|idx| format!("line {idx}")).collect();
    refresh_build_log_layout(&mut view, area, true);

    let backend = TestBackend::new(area.width, area.height);
    let mut terminal = Terminal::new(backend).expect("test backend should initialize");
    terminal
        .draw(|frame| render_build_log_dialog(frame, area, &view, "jk-run-c46709", true))
        .expect("render should succeed");

    let hint = row_text(terminal.backend().buffer(), area.height - 3, area.width);
    let separator = row_text(terminal.backend().buffer(), area.height - 2, area.width);
    let footer = row_text(terminal.backend().buffer(), area.height - 1, area.width);
    assert!(
        hint.contains("Esc"),
        "hint row should stay above separator and footer: {hint:?}"
    );
    assert!(
        !separator.contains("Esc")
            && !separator.contains("jk-run-c46709")
            && !separator.contains("2y0t4aw6"),
        "separator row should stay visually empty between hint and footer: {separator:?}"
    );
    assert!(
        footer.contains("jk-run-c46709"),
        "debug footer should stay visible while build log is open: {footer:?}"
    );
    assert!(
        footer.contains("2y0t4aw6"),
        "instance footer should stay visible while build log is open: {footer:?}"
    );
}

#[test]
fn build_log_overlay_hides_status_footer_when_debug_disabled() {
    let area = Rect::new(0, 0, 80, 12);
    let mut view = crate::tui::update::initial_view();
    view.build_log_open = true;
    view.build_log_active = true;
    view.frame = 30;
    view.status = "building docker image".to_owned();
    view.identity = Some(LaunchIdentity {
        role: "the-architect".to_owned(),
        agent: "claude".to_owned(),
        target_kind: LaunchTargetKind::Directory,
        target_label: ".".to_owned(),
        mounts: Vec::new(),
        image: None,
        container: Some("jk-2y0t4aw6-the-architect".to_owned()),
    });
    view.build_log_lines = (0..30).map(|idx| format!("line {idx}")).collect();
    refresh_build_log_layout(&mut view, area, true);

    let backend = TestBackend::new(area.width, area.height);
    let mut terminal = Terminal::new(backend).expect("test backend should initialize");
    terminal
        .draw(|frame| render_build_log_dialog(frame, area, &view, "jk-run-c46709", false))
        .expect("render should succeed");

    let bottom = row_text(terminal.backend().buffer(), area.height - 1, area.width);
    assert!(
        bottom.contains("Esc"),
        "build log hint should use the bottom row in non-debug: {bottom:?}"
    );
    assert!(
        !bottom.contains("jk-run-c46709") && !bottom.contains("2y0t4aw6"),
        "non-debug build-log overlay must not render the status footer: {bottom:?}"
    );
}
