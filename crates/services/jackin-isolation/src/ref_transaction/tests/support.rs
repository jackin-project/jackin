// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;
pub(super) const OLD: &str = "1111111111111111111111111111111111111111";

pub(super) const NEW: &str = "2222222222222222222222222222222222222222";

pub(super) const OTHER: &str = "3333333333333333333333333333333333333333";

pub(super) const NAME: &str = "refs/heads/jackin/scratch";

pub(super) fn fixture() -> (tempfile::TempDir, OwnedFd) {
    let dir = tempfile::tempdir().expect("tempdir");
    std::fs::create_dir_all(dir.path().join("refs/heads/jackin")).expect("refs");
    let fd = nix::fcntl::open(
        dir.path(),
        OFlag::O_RDONLY | OFlag::O_DIRECTORY | OFlag::O_NOFOLLOW,
        Mode::empty(),
    )
    .expect("pin");
    (dir, fd)
}
