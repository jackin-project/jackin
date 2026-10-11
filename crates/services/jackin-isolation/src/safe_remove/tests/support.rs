// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;
pub(super) fn tempdir() -> tempfile::TempDir {
    // macOS TMPDIR commonly traverses /var -> /private/var. Fixtures use
    // canonical trusted roots; the resolver intentionally rejects aliases.
    tempfile::tempdir_in(std::fs::canonicalize(std::env::temp_dir()).unwrap()).unwrap()
}

pub(super) fn canary_tree() -> (tempfile::TempDir, PathBuf, PathBuf) {
    let temp = tempdir();
    let outside = temp.path().join("outside");
    std::fs::create_dir_all(outside.join("dir")).unwrap();
    std::fs::write(outside.join("file.txt"), "canary-file").unwrap();
    std::fs::write(outside.join("dir").join("nested.txt"), "canary-nested").unwrap();
    let victim = temp.path().join("victim");
    (temp, outside, victim)
}

pub(super) fn canaries_intact(outside: &Path) -> bool {
    std::fs::read_to_string(outside.join("file.txt"))
        .is_ok_and(|contents| contents == "canary-file")
        && std::fs::read_to_string(outside.join("dir").join("nested.txt"))
            .is_ok_and(|contents| contents == "canary-nested")
}
