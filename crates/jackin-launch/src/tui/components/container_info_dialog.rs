//! Launch container-info dialog helpers.

use jackin_tui::centered_rect;
use jackin_tui::components::{
    ContainerInfoState, DebugInfo, container_info_required_height, render_container_info_on_blank,
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
    render_container_info_on_blank(frame, area, rect, &state);
    // Always show the keys beneath the dialog — shared with the console manager
    // so the dialog is never shown without its hints. The scroll keys reflect
    // the dialog's actual overflow (no axis advertised that cannot move).
    render_debug_info_hint(frame, rect, area, &state);
}

#[must_use]
pub fn launch_container_info_rect(area: Rect, state: &ContainerInfoState) -> Rect {
    let width = (area.width.saturating_mul(3) / 5).clamp(40, area.width.max(40));
    let height = container_info_required_height(state);
    centered_rect(width, height.min(area.height), area)
}
