// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! Create-prelude modal baselines.
#![cfg(test)]

use super::*;
use std::path::PathBuf;

use crate::tui::state::{ManagerStage, ManagerState, Modal};
use jackin_config::AppConfig;

pub(crate) fn prelude_with_modal(
    modal: Modal<'static>,
) -> (ManagerState<'static>, AppConfig, PathBuf) {
    let (mut state, config, cwd) = plain();
    let prelude = crate::tui::state::CreatePreludeState {
        modal: Some(modal),
        ..Default::default()
    };
    state.stage = ManagerStage::CreatePrelude(prelude);
    (state, config, cwd)
}

pub(crate) fn create_prelude_workdir_pick() -> (ManagerState<'static>, AppConfig, PathBuf) {
    prelude_with_modal(Modal::WorkdirPick {
        state: crate::tui::components::workdir_pick::WorkdirPickState::from_mounts::<
            jackin_config::MountConfig,
        >(&[]),
    })
}

pub(crate) fn create_prelude_file_browser() -> (ManagerState<'static>, AppConfig, PathBuf) {
    let cwd = test_cwd();
    prelude_with_modal(Modal::FileBrowser {
        target: crate::tui::state::FileBrowserTarget::CreateFirstMountSrc,
        state: crate::tui::components::file_browser::FileBrowserState::from_listing(
            crate::services::file_browser::listing_at(cwd.clone(), cwd),
        ),
    })
}

pub(crate) fn create_prelude_mount_dst_choice() -> (ManagerState<'static>, AppConfig, PathBuf) {
    prelude_with_modal(Modal::MountDstChoice {
        target: crate::tui::state::FileBrowserTarget::CreateFirstMountSrc,
        state: crate::tui::components::mount_dst_choice::MountDstChoiceState::new("/workspace"),
    })
}

pub(crate) fn create_prelude_name_input() -> (ManagerState<'static>, AppConfig, PathBuf) {
    prelude_with_modal(Modal::TextInput {
        target: crate::tui::state::TextInputTarget::Name,
        state: crate::tui::components::TextInputState::new("Workspace name", "alpha"),
    })
}

// ── Account constructors ───────────────────────────────────────────────────
