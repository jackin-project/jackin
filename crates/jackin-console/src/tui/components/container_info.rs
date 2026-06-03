//! Console-local container/session information state construction.

pub fn debug_run_info_state(
    run_id: impl Into<String>,
    log_path: impl Into<String>,
) -> jackin_tui::components::ContainerInfoState {
    let log_path = log_path.into();
    jackin_tui::components::ContainerInfoState::new(
        "Container info",
        vec![
            jackin_tui::components::ContainerInfoRow::new("Run ID", run_id)
                .copyable()
                .emphasised(),
            jackin_tui::components::ContainerInfoRow::new("Run log", &log_path)
                .hyperlink(format!("file://{log_path}")),
        ],
    )
}

#[cfg(test)]
mod tests;
