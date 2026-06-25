use crate::tui::app::{LaunchIdentity, LaunchTargetKind};
use crate::tui::update::initial_view;
use crate::tui::view::render_launch_frame;
use crate::{LaunchStage, tui::app::LaunchFailure};
use ratatui::{Terminal, backend::TestBackend, buffer::Buffer, layout::Rect};

fn row_text(buf: &Buffer, row: u16, width: u16) -> String {
    (0..width)
        .map(|x| buf[(x, row)].symbol().to_owned())
        .collect()
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
                "/tmp/jk-run-c46709.jsonl",
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
                "/tmp/jk-run-c46709.jsonl",
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
