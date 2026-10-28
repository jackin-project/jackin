// SPDX-FileCopyrightText: 2026 The jackin❯ Authors
// SPDX-License-Identifier: Apache-2.0

use anyhow::{Context, Result, ensure};
use std::path::{Path, PathBuf};

pub(in crate::precommit_snapshot) fn canonical_temporary_parent(
    requested: &Path,
) -> Result<PathBuf> {
    let absolute = if requested.is_absolute() {
        requested.to_path_buf()
    } else {
        std::env::current_dir()?.join(requested)
    };
    let canonical =
        std::fs::canonicalize(&absolute).context("resolving physical temporary directory")?;
    ensure!(canonical.is_dir(), "temporary parent is not a directory");
    jackin_process_directory::validate_temporary_parent(&canonical)
        .context("checking temporary parent ownership, rename custody, and ACL")?;
    Ok(canonical)
}
