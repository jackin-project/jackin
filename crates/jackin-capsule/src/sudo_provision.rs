//! `sudo-provision` subcommand — called host-side via `docker exec --user root`
//! after container start to enforce the per-profile sudo grant.
//!
//! The construct image ships `/etc/sudoers.d/agent` (passwordless sudo) so role
//! Dockerfiles can use `sudo apt-get` during `docker build` (WP-SUDO build compat).
//! At runtime this subcommand removes the entry for profiles that do not grant
//! sudo, and is a no-op for profiles that do (`JACKIN_SUDO=1`).

use anyhow::Result;
use std::fs;
use std::path::Path;

const SUDOERS_PATH: &str = "/etc/sudoers.d/agent";
const SUDOERS_ENTRY: &[u8] = b"agent ALL=(ALL) NOPASSWD:ALL\n";

pub fn provision() -> Result<()> {
    let granted = std::env::var(jackin_core::env_model::JACKIN_SUDO_ENV_NAME)
        .as_deref()
        == Ok("1");
    if granted {
        if !Path::new(SUDOERS_PATH).exists() {
            write_sudoers()?;
        }
    } else {
        match fs::remove_file(SUDOERS_PATH) {
            Ok(()) => {
                crate::output::stdout_line(format_args!(
                    "[sudo-provision] sudo revoked (no JACKIN_SUDO grant)"
                ));
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(e.into()),
        }
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
