// SPDX-FileCopyrightText: 2026 The jackin❯ Authors
// SPDX-License-Identifier: Apache-2.0

mod config;
mod files;
mod git;

use std::env;

use anyhow::{Context, Result};
use jackin_process_directory::RunningImage;

const OWNER_LIMIT: u64 = 256 * 1024 * 1024;

pub(crate) fn activation_failed_after_rename(error: &anyhow::Error) -> bool {
    config::activation_failed_after_rename(error)
}

pub(crate) fn install_hooks() -> Result<()> {
    let repository = git::repository_root()?;
    crate::precommit_snapshot::admit_install_toolchain_environment(&repository)?;
    let running_image = RunningImage::capture().context("capturing running hook owner image")?;
    let source_path = env::current_exe().context("resolving running hook owner")?;
    let owner = files::read_current_owner(&source_path, &running_image, OWNER_LIMIT)?;
    install_for_repository(&repository, &owner)?;
    Ok(())
}

fn install_for_repository(repository: &std::path::Path, owner: &[u8]) -> Result<()> {
    let (git_dir, common_config, selected_config) = git::installation_paths(repository)?;
    git::require_worktree_config(repository)?;
    git::require_named_hook_support(repository)?;
    git::require_events_enabled(repository)?;
    config::with_install_signals(|signals| {
        let installation = files::install_owner(&git_dir, owner, signals)?;
        config::activate(&common_config, &selected_config, &installation, signals)?;
        git::verify_installed_hooks(repository, installation.owner_path())
            .and_then(|()| installation.recheck())
            .map_err(config::after_rename_error)?;
        Ok(())
    })
}
