// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! Auth mount admission for provisioned credential paths.

use std::path::Path;

use crate::auth_directory;

pub fn mount_file_present(path: &Path) -> anyhow::Result<bool> {
    auth_directory::mount_file_present(path)
}

pub fn mount_directory_present(path: &Path) -> anyhow::Result<bool> {
    auth_directory::mount_directory_present(path)
}
