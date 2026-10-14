// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use anyhow::{Context, Result};
use std::fs;
use std::path::Path;

const SUDOERS_PATH: &str = "/etc/sudoers.d/agent";
const SUDOERS_ENTRY: &[u8] = b"agent ALL=(ALL) NOPASSWD:ALL\n";

#[cfg(test)]
mod tests;

#[derive(Debug, PartialEq, Eq)]
enum SudoAction {
    Write,
    Remove,
    Noop,
}

fn sudo_action(granted: bool, present: bool) -> SudoAction {
    match (granted, present) {
        (true, false) => SudoAction::Write,
        (false, true) => SudoAction::Remove,
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
