// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

pub fn debug_run_info_state(
    jackin_version: impl Into<String>,
    run_id: impl Into<String>,
) -> crate::tui::components::container_info_surface::ContainerInfoState {
    crate::tui::components::container_info_surface::DebugInfo {
        jackin_version: Some(jackin_version.into()),
        run_id: Some(run_id.into()),
        ..Default::default()
    }
    .into_state()
}

#[cfg(test)]
mod tests;
