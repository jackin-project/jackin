// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! `sudo-provision` subcommand — called host-side via `docker exec --user root`
//! after container start to enforce the per-profile sudo grant.
//!
//! The base construct image ships **no** `/etc/sudoers.d/agent` entry (the baked
//! `NOPASSWD:ALL` was removed in WP-SUDO). At runtime this subcommand writes the
//! passwordless-sudo entry when the profile grants sudo (`JACKIN_SUDO=1`) and
//! removes any stray entry otherwise. The launch path only execs this on
//! sudo-granted profiles (no jackin❯ image bakes a sudoers file, so non-sudo
//! profiles have nothing to provision); the removal arm remains the safety net
//! for a hand-authored image that ships one.

use anyhow::{Context, Result};
use std::fs;
use std::path::Path;

const SUDOERS_PATH: &str = "/etc/sudoers.d/agent";
const SUDOERS_ENTRY: &[u8] = b"agent ALL=(ALL) NOPASSWD:ALL\n";

#[cfg(test)]
mod tests;

/// The filesystem action to take, derived purely from the grant + whether the
/// sudoers entry already exists. Kept pure so the read-only-root edge is tested
/// without touching `/etc`.
#[derive(Debug, PartialEq, Eq)]
enum SudoAction {
    /// Grant on, entry missing — write it.
    Write,
    /// Grant off, entry present — strip it.
    Remove,
    /// Nothing to do.
    Noop,
}

fn sudo_action(granted: bool, present: bool) -> SudoAction {
    match (granted, present) {
        (true, false) => SudoAction::Write,
        (false, true) => SudoAction::Remove,
        // Grant on + already present, or grant off + already absent: no change.
        // The (false, false) arm is the load-bearing one — it must NOT attempt a
        // removal, because on a read-only root (hardened/locked) `unlink` returns
        // EROFS even for a missing file (the parent dir is read-only), which would
        // fail the launch fail-closed for the common no-sudo case.
        (true, true) | (false, false) => SudoAction::Noop,
    }
}

pub fn provision() -> Result<()> {
    let granted = std::env::var(jackin_core::JACKIN_SUDO_ENV_NAME).as_deref() == Ok("1");
    let present = Path::new(SUDOERS_PATH).exists();
    match sudo_action(granted, present) {
        SudoAction::Write => write_sudoers()?,
        SudoAction::Remove => {
            fs::remove_file(SUDOERS_PATH)
                .with_context(|| format!("removing stray {SUDOERS_PATH}"))?;
            crate::output::stdout_line(format_args!(
                "[sudo-provision] sudo revoked (no JACKIN_SUDO grant)"
            ));
        }
        SudoAction::Noop => {}
    }
    Ok(())
}

fn write_sudoers() -> Result<()> {
    fs::write(SUDOERS_PATH, SUDOERS_ENTRY)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;
        fs::set_permissions(SUDOERS_PATH, fs::Permissions::from_mode(0o440))?;
    }
    Ok(())
}
