// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::os::unix::fs::{MetadataExt as _, PermissionsExt as _};
use std::path::PathBuf;

use tempfile::TempDir;

use crate::UNIX_SOCKET_PATH_LIMIT;

use super::super::default_socket_alias_root;

pub(super) fn overlong_socket_path() -> PathBuf {
    PathBuf::from(format!("/{}", "x".repeat(UNIX_SOCKET_PATH_LIMIT - 1)))
}

pub(super) fn short_private_root() -> TempDir {
    let root = tempfile::Builder::new()
        .prefix("j")
        .rand_bytes(1)
        .tempdir_in(default_socket_alias_root())
        .expect("create short private alias root");
    std::fs::set_permissions(root.path(), std::fs::Permissions::from_mode(0o700))
        .expect("secure private root fixture");
    let metadata = std::fs::metadata(root.path()).expect("private root metadata");
    assert_eq!(metadata.uid(), nix::unistd::geteuid().as_raw());
    assert_eq!(metadata.mode() & 0o7777, 0o700);
    root
}

pub(super) fn alias_child_name() -> String {
    format!(
        "{}{}",
        crate::BROKER_SOCKET_ALIAS_DIR_PREFIX,
        nix::unistd::geteuid().as_raw()
    )
}
