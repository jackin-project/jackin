// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::fs::File;

pub(super) fn spool_pair(
    dir: &tempfile::TempDir,
) -> anyhow::Result<(tempfile::NamedTempFile, tempfile::NamedTempFile, File, File)> {
    let stdout_guard = tempfile::NamedTempFile::new_in(dir.path())?;
    let stderr_guard = tempfile::NamedTempFile::new_in(dir.path())?;
    let stdout = stdout_guard.as_file().try_clone()?;
    let stderr = stderr_guard.as_file().try_clone()?;
    Ok((stdout_guard, stderr_guard, stdout, stderr))
}
