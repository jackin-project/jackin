// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::path::Path;

use crate::auth_directory;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PermissionRepairFailure {
    Stat,
    Chmod,
    Verify,
}

pub fn maybe_inject_permission_repair_failure(
    stage: PermissionRepairFailure,
) -> anyhow::Result<()> {
    let _ = stage;
    Ok(())
}

pub fn repair_permissions(path: &Path) -> anyhow::Result<()> {
    #[cfg(unix)]
    {
        auth_directory::repair_file_permissions(path)
    }

    #[cfg(not(unix))]
    {
        reject_auth_path(path)?;
        let _ = path;
        Ok(())
    }
}
