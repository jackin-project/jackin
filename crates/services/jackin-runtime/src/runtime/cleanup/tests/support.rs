// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;
pub(super) fn tempdir() -> std::io::Result<TempDir> {
    tempfile::tempdir_in(std::fs::canonicalize(std::env::temp_dir())?)
}
