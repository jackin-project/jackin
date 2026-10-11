// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;
pub(super) fn settings_github_row(state: &ManagerState<'_>) -> usize {
    let ManagerStage::Settings(settings) = &state.stage else {
        panic!("expected settings stage");
    };
    // GitHub sits second-to-last; the scan row stays last.
    settings.auth.row_count() - 2
}

pub(super) fn state_with_saved_count(count: usize) -> ManagerState<'static> {
    let tmp = tempfile::tempdir().unwrap();
    let cwd = tmp.path();
    let mut config = jackin_config::AppConfig::default();
    for idx in 0..count {
        config.workspaces.insert(
            format!("workspace-{idx}"),
            jackin_config::WorkspaceConfig {
                workdir: format!("/tmp/workspace-{idx}"),
                ..jackin_config::WorkspaceConfig::default()
            },
        );
    }
    ManagerState::from_config(&config, cwd)
}
