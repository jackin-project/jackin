// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;
pub(super) fn render_launch_frame(
    frame: &mut Frame<'_>,
    view: &LaunchView,
    run_id: &str,
    no_motion: bool,
    rain: Option<&crate::tui::components::rain::RainState>,
) {
    render_launch_frame_view(
        frame,
        view,
        run_id,
        no_motion,
        rain,
        jackin_diagnostics::is_debug_mode(),
        env!("JACKIN_VERSION"),
    );
}

pub(super) fn launch_failure() -> LaunchFailure {
    LaunchFailure {
        title: "Docker build failed".to_owned(),
        summary: "Building the Docker container failed.".to_owned(),
        detail: None,
        next_step: None,
        stage: LaunchStage::DerivedImage,
    }
}
