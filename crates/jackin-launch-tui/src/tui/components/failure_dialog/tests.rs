use super::{
    failure_copy_target_at, failure_popup_hyperlink_overlay, failure_popup_value_rect_scrolled,
};
use crate::tui::components::chrome::bottom_chrome_areas;
use crate::tui::components::failure_dialog::failure_popup_rows;
use crate::tui::model::{LaunchIdentity, LaunchTargetKind};
use crate::tui::update::initial_view;
use crate::tui::view::render_launch_frame;
use crate::{FailureCopyTarget, LaunchStage, tui::model::LaunchFailure};
use ratatui::{Terminal, backend::TestBackend, buffer::Buffer, layout::Rect};

fn row_text(buf: &Buffer, row: u16, width: u16) -> String {
    (0..width)
        .map(|x| buf[(x, row)].symbol().to_owned())
        .collect()
}

/// Concatenate every rendered cell so a test can assert what owns the screen
/// without hard-coding row/col offsets.
fn screen_text(buf: &Buffer, area: Rect) -> String {
    (0..area.height)
        .flat_map(|y| (0..area.width).map(move |x| buf[(x, y)].symbol().to_owned()))
        .collect()
}

fn failure_with_summary(summary: &str) -> LaunchFailure {
    LaunchFailure {
        title: "Build failed".to_owned(),
        summary: summary.to_owned(),
        detail: None,
        next_step: None,
        stage: LaunchStage::DerivedImage,
        diagnostics_path: None,
        command_output_path: None,
    }
}

#[test]
fn failure_popup_keeps_status_footer_visible() {
    let area = Rect::new(0, 0, 90, 18);
    let mut view = initial_view();
    view.frame = 30;
    view.status = "docker build failed".to_owned();
    view.identity = Some(LaunchIdentity {
        role: "the-architect".to_owned(),
        agent: "claude".to_owned(),
        target_kind: LaunchTargetKind::Directory,
        target_label: ".".to_owned(),
        mounts: Vec::new(),
        image: None,
        container: Some("jk-2y0t4aw6-the-architect".to_owned()),
    });
    view.failure = Some(LaunchFailure {
        title: "Build failed".to_owned(),
        summary: "docker build failed".to_owned(),
        detail: None,
        next_step: None,
        stage: LaunchStage::DerivedImage,
        diagnostics_path: None,
        command_output_path: None,
    });

    let backend = TestBackend::new(area.width, area.height);
    let mut terminal = Terminal::new(backend).expect("test backend should initialize");
    terminal
        .draw(|frame| {
            render_launch_frame(
                frame,
                &view,
                "jk-run-c46709",
                Some("/tmp/jk-run-c46709.jsonl"),
                true,
                None,
                true,
                "0.6.0-test",
            );
        })
        .expect("render should succeed");

    let hint = row_text(terminal.backend().buffer(), area.height - 3, area.width);
    let spacer = row_text(terminal.backend().buffer(), area.height - 2, area.width);
    let footer = row_text(terminal.backend().buffer(), area.height - 1, area.width);
    assert!(
        hint.contains("copy value") && hint.contains("dismiss"),
        "failure popup hints should render in the shared hint row: {hint:?}"
    );
    assert!(
        !spacer.contains("copy value") && !spacer.contains("jk-run-c46709"),
        "spacer row should stay between hints and footer: {spacer:?}"
    );
    assert!(
        footer.contains("jk-run-c46709") && footer.contains("2y0t4aw6"),
        "status footer should remain visible while failure popup is open: {footer:?}"
    );
}

#[test]
fn failure_popup_hides_status_footer_when_debug_disabled() {
    let area = Rect::new(0, 0, 90, 18);
    let mut view = initial_view();
    view.frame = 30;
    view.status = "docker build failed".to_owned();
    view.identity = Some(LaunchIdentity {
        role: "the-architect".to_owned(),
        agent: "claude".to_owned(),
        target_kind: LaunchTargetKind::Directory,
        target_label: ".".to_owned(),
        mounts: Vec::new(),
        image: None,
        container: Some("jk-2y0t4aw6-the-architect".to_owned()),
    });
    view.failure = Some(LaunchFailure {
        title: "Build failed".to_owned(),
        summary: "docker build failed".to_owned(),
        detail: None,
        next_step: None,
        stage: LaunchStage::DerivedImage,
        diagnostics_path: None,
        command_output_path: None,
    });

    let backend = TestBackend::new(area.width, area.height);
    let mut terminal = Terminal::new(backend).expect("test backend should initialize");
    terminal
        .draw(|frame| {
            render_launch_frame(
                frame,
                &view,
                "jk-run-c46709",
                Some("/tmp/jk-run-c46709.jsonl"),
                true,
                None,
                false,
                "0.6.0-test",
            );
        })
        .expect("render should succeed");

    let bottom = row_text(terminal.backend().buffer(), area.height - 1, area.width);
    assert!(
        bottom.contains("copy value") && bottom.contains("dismiss"),
        "failure popup hint should use the bottom row in non-debug: {bottom:?}"
    );
    assert!(
        !bottom.contains("jk-run-c46709") && !bottom.contains("2y0t4aw6"),
        "non-debug dialog must not keep the status footer visible: {bottom:?}"
    );
}

#[test]
fn failure_popup_renders_over_stale_build_log_overlay() {
    // Defense-in-depth render guard: even if `build_log_open` is stale-true when
    // a failure arrives, the failure popup must own the screen and the opaque
    // build-log backdrop must not paint over it.
    let area = Rect::new(0, 0, 90, 18);
    let mut view = initial_view();
    view.status = "docker build failed".to_owned();
    view.build_log_open = true;
    view.build_log_lines = vec!["ZZZ-BUILD-LOG-ONLY-MARKER-ZZZ".to_owned()];
    view.failure = Some(failure_with_summary("docker build failed"));

    let backend = TestBackend::new(area.width, area.height);
    let mut terminal = Terminal::new(backend).expect("test backend should initialize");
    terminal
        .draw(|frame| {
            render_launch_frame(
                frame,
                &view,
                "jk-run-c46709",
                Some("/tmp/jk-run-c46709.jsonl"),
                true,
                None,
                false,
                "0.6.0-test",
            );
        })
        .expect("render should succeed");

    let screen = screen_text(terminal.backend().buffer(), area);
    assert!(
        screen.contains("Build failed"),
        "failure title must render over stale build-log overlay: {screen:?}"
    );
    assert!(
        screen.contains("docker build failed"),
        "failure summary must render over stale build-log overlay: {screen:?}"
    );
    assert!(
        !screen.contains("ZZZ-BUILD-LOG-ONLY-MARKER-ZZZ"),
        "build-log-only content must not own the screen when failure is open: {screen:?}"
    );
}

#[test]
fn long_failure_body_is_reachable_by_scrolling() {
    // Long diagnostics/next-step rows must not be silently clipped: the body
    // scrolls, so the tail is reachable by advancing `failure_scroll`.
    let area = Rect::new(0, 0, 90, 18);
    // ~150 "filler " words wrap well past the viewport-safe popup body height,
    // with distinct first/last markers so the scrolled viewport is observable.
    let long_body = format!("FIRST {}", "filler ".repeat(150)) + "LAST";
    let mut view = initial_view();
    view.failure = Some(LaunchFailure {
        title: "Build failed".to_owned(),
        summary: "docker build failed".to_owned(),
        detail: None,
        next_step: Some(long_body),
        stage: LaunchStage::DerivedImage,
        diagnostics_path: None,
        command_output_path: None,
    });

    // Top of the body: FIRST marker visible, tail marker clipped out.
    let mut terminal = Terminal::new(TestBackend::new(area.width, area.height))
        .expect("test backend should initialize");
    terminal
        .draw(|frame| {
            render_launch_frame(
                frame,
                &view,
                "jk-run-c46709",
                Some("/tmp/jk-run-c46709.jsonl"),
                true,
                None,
                false,
                "0.6.0-test",
            );
        })
        .expect("render should succeed");
    let top = screen_text(terminal.backend().buffer(), area);
    assert!(
        top.contains("FIRST"),
        "head of long body reachable at scroll 0"
    );
    assert!(
        !top.contains("LAST"),
        "tail of long body must not render at scroll 0: {top:?}"
    );

    // Scroll to the tail: render clamps the offset, exposing LAST.
    view.failure_scroll.scroll_y = u16::MAX;
    terminal
        .draw(|frame| {
            render_launch_frame(
                frame,
                &view,
                "jk-run-c46709",
                Some("/tmp/jk-run-c46709.jsonl"),
                true,
                None,
                false,
                "0.6.0-test",
            );
        })
        .expect("render should succeed");
    let tail = screen_text(terminal.backend().buffer(), area);
    assert!(
        tail.contains("LAST"),
        "tail of long body must be reachable by scrolling: {tail:?}"
    );
    assert!(
        !tail.contains("FIRST"),
        "head of long body must scroll off once at the tail: {tail:?}"
    );
}

#[test]
fn scrolled_failure_copy_hit_and_overlay_follow_failure_scroll() {
    // After the operator scrolls a long failure body, hit-testing and OSC8
    // overlays must use the same scroll as render — not scroll 0.
    use std::path::PathBuf;

    let area = Rect::new(0, 0, 90, 18);
    let long_next = format!("FIRST {}", "filler ".repeat(150)) + "LAST";
    let failure = LaunchFailure {
        title: "Build failed".to_owned(),
        summary: "docker build failed".to_owned(),
        detail: None,
        next_step: Some(long_next),
        stage: LaunchStage::DerivedImage,
        diagnostics_path: Some(PathBuf::from("/jk/run/scrolled.jsonl")),
        command_output_path: None,
    };
    let run_id = "jk-run-scroll";
    let rows = failure_popup_rows(&failure, run_id);
    let body_area = bottom_chrome_areas(area).body;

    // At scroll 0 the diagnostics path is typically visible; capture its y.
    let rect0 = super::failure_popup_rect_for_rows(body_area, &rows);
    let vr0 =
        failure_popup_value_rect_scrolled(rect0, &rows, FailureCopyTarget::DiagnosticsPath, None)
            .expect("diagnostics path value rect at scroll 0");
    assert_eq!(
        failure_copy_target_at(area, &failure, run_id, true, vr0.x, vr0.y, None),
        Some(FailureCopyTarget::DiagnosticsPath),
        "scroll 0 hit must land on diagnostics path"
    );

    // Large scroll: the same screen y must no longer hit that absolute row
    // unless we pass the matching scroll into hit-test.
    let mut scrolled = termrock::scroll::DialogScroll::new();
    scrolled.scroll_y = 8;
    let vr_scrolled = failure_popup_value_rect_scrolled(
        rect0,
        &rows,
        FailureCopyTarget::DiagnosticsPath,
        Some(scrolled.clone()),
    );
    // When the path scrolls out of view, value rects are empty; when still
    // partially visible they move up. Either way scroll-0 geometry must not
    // be reused as if it were scrolled.
    let hit_with_scroll = failure_copy_target_at(
        area,
        &failure,
        run_id,
        true,
        vr0.x,
        vr0.y,
        Some(scrolled.clone()),
    );
    let hit_without_scroll =
        failure_copy_target_at(area, &failure, run_id, true, vr0.x, vr0.y, None);
    assert_eq!(
        hit_without_scroll,
        Some(FailureCopyTarget::DiagnosticsPath),
        "control: scroll-0 hit still finds path at original y"
    );
    if let Some(vr) = vr_scrolled {
        assert_eq!(
            failure_copy_target_at(
                area,
                &failure,
                run_id,
                true,
                vr.x,
                vr.y,
                Some(scrolled.clone())
            ),
            Some(FailureCopyTarget::DiagnosticsPath),
            "scrolled value rect must hit diagnostics path"
        );
        // Original y with scroll applied must not falsely claim the same hit
        // when the row has moved (scroll-aware geometry).
        if vr.y != vr0.y {
            assert_ne!(
                hit_with_scroll,
                Some(FailureCopyTarget::DiagnosticsPath),
                "stale scroll-0 y must not hit path after scroll moves the row"
            );
        }
    } else {
        assert_ne!(
            hit_with_scroll,
            Some(FailureCopyTarget::DiagnosticsPath),
            "scrolled-out path must not hit at the old screen y"
        );
    }

    let overlay0 =
        failure_popup_hyperlink_overlay(area, &failure, run_id, true, None, None, None, None, None);
    let overlay_scrolled = failure_popup_hyperlink_overlay(
        area,
        &failure,
        run_id,
        true,
        Some(scrolled),
        None,
        None,
        None,
        None,
    );
    assert!(
        !overlay0.is_empty() || !overlay_scrolled.is_empty(),
        "at least one scroll position should emit OSC8 for the diagnostics path"
    );
    // Scrolled and un-scrolled overlays must not be byte-identical when the
    // body actually scrolls (row CSI positions differ).
    if vr_scrolled.is_some() {
        // When the path is still partially visible after scroll, CSI positions change.
        // When fully scrolled out, scrolled overlay is shorter/empty.
        assert_ne!(
            overlay0, overlay_scrolled,
            "OSC8 overlay must follow failure_scroll, not stay pinned at scroll 0"
        );
    }
}
