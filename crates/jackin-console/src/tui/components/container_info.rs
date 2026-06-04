//! Console-local container/session information state construction.

/// Build the debug-info dialog rows for the console surface.
///
/// Always-available rows: jackin version, Run ID (bare id, copyable/emphasised),
/// run log path (copyable + hyperlinked). No log contents: the path is the
/// artifact to share.
pub fn debug_run_info_state(
    run_id: impl Into<String>,
    log_path: impl Into<String>,
) -> jackin_tui::components::ContainerInfoState {
    let log_path = log_path.into();
    let rows = vec![
        jackin_tui::components::ContainerInfoRow::new("jackin", env!("CARGO_PKG_VERSION")),
        jackin_tui::components::ContainerInfoRow::new("Run ID", run_id)
            .copyable()
            .emphasised(),
        jackin_tui::components::ContainerInfoRow::new("Run log", &log_path)
            .copyable()
            .hyperlink(format!("file://{log_path}")),
    ];
    jackin_tui::components::ContainerInfoState::new("Debug info", rows)
}

#[cfg(test)]
mod tests;
