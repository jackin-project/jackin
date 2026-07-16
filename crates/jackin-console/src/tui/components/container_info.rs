// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Console-local container/session information state construction.

/// Build the debug-info dialog state for the console surface from the shared
/// [`DebugInfo`](crate::tui::components::container_info_surface::DebugInfo) model.
///
/// The console knows only the run: its bare id and the diagnostics log path.
/// `jackin_version` must be the exact `jackin --version` string (the binary
/// crate passes `env!("JACKIN_VERSION")`) so the dialog never disagrees with
/// the CLI. Container/role/agent rows appear later, on the launch surface,
/// from the same model.
pub fn debug_run_info_state(
    jackin_version: impl Into<String>,
    run_id: impl Into<String>,
    log_path: impl Into<String>,
) -> crate::tui::components::container_info_surface::ContainerInfoState {
    crate::tui::components::container_info_surface::DebugInfo {
        jackin_version: Some(jackin_version.into()),
        run_id: Some(run_id.into()),
        diagnostics_log_path: Some(log_path.into()),
        ..Default::default()
    }
    .into_state()
}

#[cfg(test)]
mod tests;
