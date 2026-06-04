//! Console-local container/session information state construction.

/// Build the debug-info dialog rows for the console surface.
///
/// Base content (always shown): Run ID (copyable/emphasised) + run log path
/// (hyperlinked). When `log_tail` is provided, appends the last N log lines
/// so the operator can skim recent events without leaving the dialog.
pub fn debug_run_info_state(
    run_id: impl Into<String>,
    log_path: impl Into<String>,
) -> jackin_tui::components::ContainerInfoState {
    let log_path = log_path.into();
    let mut rows = vec![
        jackin_tui::components::ContainerInfoRow::new("Run ID", run_id)
            .copyable()
            .emphasised(),
        jackin_tui::components::ContainerInfoRow::new("Run log", &log_path)
            .hyperlink(format!("file://{log_path}")),
    ];
    // Read the last few lines of the run log so the operator can see recent
    // activity without opening a separate viewer. Soft-limit to 20 lines so
    // the dialog stays manageable; scroll is available (ContainerInfoState
    // has scroll_y).
    if let Ok(contents) = std::fs::read_to_string(&log_path) {
        let lines: Vec<&str> = contents.lines().collect();
        let tail_start = lines.len().saturating_sub(20);
        for line in &lines[tail_start..] {
            // Truncate long lines so they fit in the dialog width.
            let label = "";
            let value: String = line.chars().take(100).collect();
            rows.push(jackin_tui::components::ContainerInfoRow::new(label, value));
        }
    }
    jackin_tui::components::ContainerInfoState::new("Container info", rows)
}

#[cfg(test)]
mod tests;
