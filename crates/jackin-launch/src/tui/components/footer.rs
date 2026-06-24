//! Launch cockpit footer helpers.

use jackin_tui::components::{StatusRightGroup, render_status_footer_right_group};
use ratatui::Frame;
use ratatui::layout::Rect;

use crate::LaunchView;

/// The status-bar activity text: the current step with an upper-cased first
/// word and a trailing ellipsis (`wiring private network` -> `Wiring private
/// network...`). The live build/step detail lives only here, never inside the
/// box.
#[must_use]
pub fn format_activity(status: &str) -> String {
    let trimmed = status
        .trim()
        .trim_end_matches('…')
        .trim_end_matches("...")
        .trim_end();
    let mut chars = trimmed.chars();
    let Some(first) = chars.next() else {
        return String::new();
    };
    format!("{}{}…", first.to_uppercase(), chars.as_str())
}

pub fn render_footer(
    frame: &mut Frame<'_>,
    area: Rect,
    view: &LaunchView,
    run_id: &str,
    debug_mode: bool,
) {
    let instance = footer_instance(view);
    // The run id rides the status bar only in --debug, in amber, so the
    // operator is never unsure whether they are in a debug run; the blue
    // instance-id chip always shows once the container is named.
    let debug_chip = debug_mode.then_some(run_id);
    // Fade the bar up from black over the first ~30 frames so it appears
    // gradually with the rain rather than popping in.
    #[allow(clippy::cast_precision_loss)]
    let alpha = (view.frame as f32 / 30.0).min(1.0);
    render_status_footer_right_group(
        frame,
        area,
        &format_activity(&view.status),
        StatusRightGroup {
            usage: None,
            container: &instance,
            run_id: debug_chip,
        },
        alpha,
        view.footer_hover,
    );
}

/// The container's short instance id once the container is named, else empty.
#[must_use]
pub fn footer_instance(view: &LaunchView) -> String {
    view.identity
        .as_ref()
        .and_then(|identity| identity.container.as_deref())
        .and_then(jackin_core::constants::instance_id_from_container_base)
        .map(str::to_owned)
        .unwrap_or_default()
}
