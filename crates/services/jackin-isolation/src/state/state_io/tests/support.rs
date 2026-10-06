// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;
pub(super) fn fixture() -> (tempfile::TempDir, PathBuf, StateDirectory) {
    let temp = tempfile::tempdir().unwrap();
    let state = temp.path().join("jk-a1b2c3d4-role");
    let directory = StateDirectory::open(&state, true).unwrap().unwrap();
    directory.write_file("isolation.json", b"original").unwrap();
    (temp, state, directory)
}
