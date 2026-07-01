#[cfg(test)]
use super::*;
use std::path::PathBuf;

use crate::LaunchStage;
    use crate::tui::update::initial_view;
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;
    use ratatui::buffer::Buffer;
    use ratatui::layout::Rect;

    fn row_text(buf: &Buffer, row: u16, width: u16) -> String {
        (0..width)
            .map(|x| buf[(x, row)].symbol().to_owned())
            .collect()
    }

    fn quit_confirm_view() -> LaunchView {
        let mut view = initial_view();
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
        view.quit_confirm = Some(jackin_tui::components::exit_confirm_state());
        view
    }

    fn render_quit_confirm(debug_mode: bool) -> Buffer {
        let area = Rect::new(0, 0, 90, 18);
        let view = quit_confirm_view();
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
                    debug_mode,
                    "0.6.0-test",
                );
            })
            .expect("render should succeed");
        terminal.backend().buffer().clone()
    }

    #[test]
    fn build_log_open_suppresses_failure_hyperlink_overlay() {
        let mut view = initial_view();
        view.build_log_open = true;
        view.failure = Some(LaunchFailure {
            title: "Docker build failed".to_owned(),
            summary: "build failed".to_owned(),
            detail: None,
            next_step: None,
            stage: LaunchStage::DerivedImage,
            diagnostics_path: Some(PathBuf::from(
                "/Users/donbeave/Projects/jackin-project/test/pr-641/state/home/data/diagnostics/runs/18bc0fd1093b23b0.jsonl",
            )),
            command_output_path: None,
        });

        let overlay = launch_hyperlink_overlays(
            Rect::new(0, 0, 120, 40),
            &view,
            "18bc0fd1093b23b0",
            "/Users/donbeave/Projects/jackin-project/test/pr-641/state/home/data/diagnostics/runs/18bc0fd1093b23b0.jsonl",
            true,
            "0.6.0-test",
        );

        assert!(
            overlay.is_empty(),
            "build-log overlay owns the screen; failure hyperlinks must not render over it: {:?}",
            String::from_utf8_lossy(&overlay)
        );
    }

    #[test]
    fn quit_confirm_keeps_status_footer_in_debug_mode() {
        let area = Rect::new(0, 0, 90, 18);
        let buffer = render_quit_confirm(true);
        let footer = row_text(&buffer, area.height - 1, area.width);

        assert!(
            footer.contains("jk-run-c46709") && footer.contains("2y0t4aw6"),
            "debug quit confirm should keep the status footer visible: {footer:?}"
        );
    }

    #[test]
    fn quit_confirm_hides_status_footer_when_debug_disabled() {
        let area = Rect::new(0, 0, 90, 18);
        let buffer = render_quit_confirm(false);
        let footer = row_text(&buffer, area.height - 1, area.width);

        assert!(
            !footer.contains("jk-run-c46709") && !footer.contains("2y0t4aw6"),
            "non-debug quit confirm must not render the status footer: {footer:?}"
        );
    }
}
