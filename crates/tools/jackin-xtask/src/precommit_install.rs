// SPDX-FileCopyrightText: 2026 The jackin❯ Authors
// SPDX-License-Identifier: Apache-2.0

//! Install the stable native hook owner for the current worktree.

mod config;
mod files;
mod git;

use std::env;

use anyhow::{Context, Result};

const OWNER_LIMIT: u64 = 256 * 1024 * 1024;

#[cfg(target_os = "linux")]
fn device_matches(named: nix::sys::stat::dev_t, opened: u64) -> bool {
    named == opened
}

#[cfg(target_os = "macos")]
fn device_matches(named: nix::sys::stat::dev_t, opened: u64) -> bool {
    named.cast_unsigned() == (opened as u32)
}

#[cfg(not(any(target_os = "linux", target_os = "macos")))]
fn device_matches(named: nix::sys::stat::dev_t, opened: u64) -> bool {
    u64::try_from(named).ok() == Some(opened)
}

pub(crate) fn activation_failed_after_rename(error: &anyhow::Error) -> bool {
    config::activation_failed_after_rename(error)
}

pub(crate) fn install_hooks() -> Result<()> {
    let repository = git::repository_root()?;
    crate::precommit_snapshot::admit_install_toolchain_environment(&repository)?;
    let source_path = env::current_exe().context("resolving running hook owner")?;
    let owner = files::read_current_owner(&source_path, OWNER_LIMIT)?;
    install_for_repository(&repository, &owner)?;
    Ok(())
}

fn install_for_repository(repository: &std::path::Path, owner: &[u8]) -> Result<()> {
    let (git_dir, common_config, selected_config) = git::installation_paths(repository)?;
    git::require_worktree_config(repository)?;
    git::require_named_hook_support(repository)?;
    git::require_events_enabled(repository)?;
    let installation = files::install_owner(&git_dir, owner)?;
    config::activate(&common_config, &selected_config, installation.owner_path())?;
    git::verify_installed_hooks(repository, installation.owner_path())?;
    Ok(())
}

#[cfg(test)]
mod tests;

#[cfg(all(test, target_os = "linux"))]
#[test]
fn linux_named_and_opened_device_values_match_directly() {
    let named = 4_294_967_297_u64;
    assert!(device_matches(named, named));
    assert!(!device_matches(named, named + 1));
}

#[cfg(all(test, target_os = "macos"))]
#[test]
fn macos_device_match_preserves_high_bit_patterns_on_both_sides() {
    let named: nix::sys::stat::dev_t = i32::MIN;
    let sign_extended_metadata = i64::from(i32::MIN).cast_unsigned();
    let zero_extended_metadata = u64::from(i32::MIN.cast_unsigned());

    assert!(device_matches(named, sign_extended_metadata));
    assert!(device_matches(named, zero_extended_metadata));
    assert!(!device_matches(named, zero_extended_metadata + 1));
}
