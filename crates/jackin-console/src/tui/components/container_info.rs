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
mod tests {
    use super::debug_run_info_state;

    #[test]
    fn debug_run_info_state_marks_run_id_copyable_and_log_hyperlinked() {
        let state = debug_run_info_state("run-1", "/tmp/jackin/run.log");
        let rows = state.rows();

        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].value(), "run-1");
        assert!(rows[0].is_copyable());
        assert_eq!(rows[1].value(), "/tmp/jackin/run.log");
        assert_eq!(rows[1].href(), Some("file:///tmp/jackin/run.log"));
    }
}
