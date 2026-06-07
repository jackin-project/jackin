//! Launch container-info dialog helpers.

use jackin_tui::centered_rect;
use jackin_tui::components::{
    ContainerInfoState, DebugInfo, container_info_required_height, render_container_info,
    render_debug_info_hint,
};
use ratatui::Frame;
use ratatui::layout::Rect;

use crate::LaunchView;

#[must_use]
pub fn launch_container_info_state(
    view: &LaunchView,
    run_id: &str,
    run_log_path: &str,
    debug_mode: bool,
    jackin_version: &'static str,
) -> ContainerInfoState {
    let identity = view.identity.as_ref();
    // The launch surface knows the container/role/agent/target on top of what
    // the console already showed. Build from the shared accumulating model so
    // row order, labels, and copy affordances match every other surface.
    let info = DebugInfo {
        jackin_version: Some(jackin_version.to_owned()),
        container_id: Some(
            identity
                .and_then(|identity| identity.container.as_deref())
                .unwrap_or("loading...")
                .to_owned(),
        ),
        role: identity.map(|identity| identity.role.clone()),
        agent: identity.map(|identity| identity.agent.clone()),
        target: identity.map(|identity| identity.target_label.clone()),
        run_id: debug_mode.then(|| run_id.to_owned()),
        diagnostics_log_path: debug_mode.then(|| run_log_path.to_owned()),
        capsule_version: None,
    };
    let mut state = info.into_state();
    if let Some(row) = view.container_info_copied {
        state.mark_copied(row);
    }
    state.set_hovered_row(view.container_info_hover);
    state.scroll = view.container_info_scroll.clone();
    state
}

pub fn render_launch_container_info(
    frame: &mut Frame<'_>,
    area: Rect,
    view: &LaunchView,
    run_id: &str,
    run_log_path: &str,
    debug_mode: bool,
    jackin_version: &'static str,
) {
    let state = launch_container_info_state(view, run_id, run_log_path, debug_mode, jackin_version);
    let rect = launch_container_info_rect(area, &state);
    render_container_info(frame, rect, &state);
    // Always show the keys beneath the dialog — shared with the console manager
    // so the dialog is never shown without its hints. The scroll keys reflect
    // the dialog's actual overflow (no axis advertised that cannot move).
    render_debug_info_hint(frame, rect, area, &state);
}

#[must_use]
pub fn launch_container_info_rect(area: Rect, state: &ContainerInfoState) -> Rect {
    // Structural exception: launch supplies surface width while shared Debug info owns row height and rendering.
    let width = (area.width.saturating_mul(3) / 5).clamp(40, area.width.max(40));
    let height = container_info_required_height(state);
    centered_rect(width, height.min(area.height), area)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tui::app::LaunchView;
    use crate::tui::app::{LaunchIdentity, LaunchTargetKind};
    use crate::tui::update::initial_view;
    use crate::tui::view::render_launch_frame;
    use ratatui::{Terminal, backend::TestBackend, buffer::Buffer};

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
            "/Users/donbeave/.jackin-pr-495/data/diagnostics/runs/jk-run-b93735.jsonl",
            true,
            "0.6.0-test",
        );
        let rows = state.rows();
        assert_eq!(
            rows.first().map(|row| row.value()),
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
    }

    #[test]
    fn launch_container_info_omits_run_rows_when_debug_disabled() {
        let view = initial_view();
        let state = launch_container_info_state(
            &view,
            "jk-run-b93735",
            "/Users/donbeave/.jackin-pr-495/data/diagnostics/runs/jk-run-b93735.jsonl",
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
                    "/Users/donbeave/.jackin-pr-495/data/diagnostics/runs/jk-run-c46709.jsonl",
                    true,
                    None,
                    true,
                    "0.6.0-test",
                );
            })
            .expect("render should succeed");

        let footer = row_text(terminal.backend().buffer(), area.height - 1, area.width);
        assert!(
            footer.contains("jk-run-c46709"),
            "debug footer should stay visible while Debug info is open: {footer:?}"
        );
        assert!(
            footer.contains("2y0t4aw6"),
            "instance footer should stay visible while Debug info is open: {footer:?}"
        );
    }
}
