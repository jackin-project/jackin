// SPDX-FileCopyrightText: 2026 The jackin❯ Authors
// SPDX-License-Identifier: Apache-2.0

//! Source-first installation for the immutable native pre-commit owner.

mod bundle;
mod write;

#[cfg(test)]
mod tests;

pub(crate) fn run(
    bootstrap_root: &std::path::Path,
    repository: &std::path::Path,
    reviewed_tree: &str,
) -> anyhow::Result<String> {
    let verified = bundle::verify(bootstrap_root, repository, reviewed_tree)?;
    write::install(verified)
}

pub(crate) fn validate_hook_config(repository: &std::path::Path) -> anyhow::Result<()> {
    let repository = bundle::physical_directory(repository, "repository")?;
    let (common_config, selected_config) = bundle::hook_config_paths(&repository)?;
    write::validate_hook_config_custody(&common_config, &selected_config)
}

pub(crate) fn activate_hook_config(repository: &std::path::Path) -> anyhow::Result<()> {
    let repository = bundle::physical_directory(repository, "repository")?;
    let (common_config, selected_config) = bundle::hook_config_paths(&repository)?;
    write::activate_hook_config(&common_config, &selected_config)
}

pub(crate) fn activation_failed_after_rename(error: &anyhow::Error) -> bool {
    write::activation_failed_after_rename(error)
}
