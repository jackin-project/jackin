//! Launch container-info dialog helpers.

use jackin_tui::centered_rect;
use jackin_tui::components::{
    ContainerInfoRow, ContainerInfoState, container_info_required_height, render_container_info,
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
    let mut rows = vec![
        ContainerInfoRow::new(
            "Container ID",
            identity
                .and_then(|identity| identity.container.as_deref())
                .unwrap_or("loading..."),
        )
        .copyable()
        .emphasised(),
        ContainerInfoRow::new("jackin version", jackin_version),
    ];
    if let Some(identity) = identity {
        rows.push(ContainerInfoRow::new("Role", &identity.role));
        rows.push(ContainerInfoRow::new("Agent", &identity.agent));
        rows.push(ContainerInfoRow::new("Target", &identity.target_label));
    }
    if debug_mode {
        rows.push(
            ContainerInfoRow::new("Run ID", run_id)
                .copyable()
                .emphasised(),
        );
        rows.push(
            ContainerInfoRow::new("Diagnostics log", run_log_path)
                .hyperlink(format!("file://{run_log_path}")),
        );
    }
    let mut state = ContainerInfoState::new("Container info", rows);
    if let Some(row) = view.container_info_copied {
        state.mark_copied(row);
    }
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
}

#[must_use]
pub fn launch_container_info_rect(area: Rect, state: &ContainerInfoState) -> Rect {
    let width = (area.width.saturating_mul(3) / 5).clamp(40, area.width.max(40));
    let height = container_info_required_height(state);
    centered_rect(width, height.min(area.height), area)
}
