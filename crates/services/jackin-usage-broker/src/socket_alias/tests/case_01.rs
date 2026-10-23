// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::fs;
use std::os::unix::ffi::OsStringExt as _;
use std::path::{Path, PathBuf};

use super::super::*;
use super::support::overlong_socket_path;

fn full_socket_path(data_dir: &Path) -> PathBuf {
    data_dir
        .join(BROKER_DIR)
        .join(BROKER_RUN_DIR)
        .join(BROKER_SOCKET)
}

#[test]
fn canonical_socket_path_unifies_relative_symlink_and_parent_spellings() {
    use std::os::unix::fs::symlink;

    let temp = tempfile::tempdir().expect("tempdir");
    let real_data_dir = temp.path().join("real/data");
    fs::create_dir_all(&real_data_dir).expect("create real data directory");
    fs::create_dir(temp.path().join("segment")).expect("create path segment");
    symlink(temp.path().join("real"), temp.path().join("alias")).expect("create alias symlink");

    let real = broker_socket_path_for_data_dir(Path::new("real/data"), temp.path())
        .expect("relative spelling");
    let through_symlink = broker_socket_path_for_data_dir(Path::new("alias/data"), temp.path())
        .expect("symlink spelling");
    let through_parent =
        broker_socket_path_for_data_dir(Path::new("segment/../real/data"), temp.path())
            .expect("parent spelling");
    let canonical_data_dir = fs::canonicalize(real_data_dir).expect("canonical data directory");

    assert_eq!(real, full_socket_path(&canonical_data_dir));
    assert_eq!(real, through_symlink);
    assert_eq!(real, through_parent);
    assert_eq!(
        socket_alias_name(&real),
        socket_alias_name(&through_symlink)
    );
    assert_eq!(socket_alias_name(&real), socket_alias_name(&through_parent));
}

#[test]
fn canonical_socket_path_anchors_relative_paths_and_rejects_ambiguous_missing_tails() {
    let relative = Path::new("profiles/current/data");
    let first_cwd = Path::new("/tmp/jackin-cwd-one");
    let second_cwd = Path::new("/tmp/jackin-cwd-two");
    let first = broker_socket_path_for_data_dir(relative, first_cwd).expect("first cwd");
    let second = broker_socket_path_for_data_dir(relative, second_cwd).expect("second cwd");
    assert!(first.is_absolute());
    assert!(second.is_absolute());
    assert_ne!(first, second);
    assert_ne!(socket_alias_name(&first), socket_alias_name(&second));

    let temp = tempfile::tempdir().expect("tempdir");
    let ambiguous = Path::new("missing/../existing");
    let _error = broker_socket_path_for_data_dir(ambiguous, temp.path()).unwrap_err();
    let unambiguous = broker_socket_path_for_data_dir(Path::new("missing/tail"), temp.path())
        .expect("preserve ordinary missing tail");
    assert!(unambiguous.is_absolute());
}

#[test]
fn alias_identity_uses_full_raw_path_bytes_and_full_sha256() {
    use sha2::{Digest as _, Sha256};

    let path = overlong_socket_path();
    let name = socket_alias_name(&path);
    let digest = Sha256::digest(path.as_os_str().as_encoded_bytes());
    let mut expected = String::from("jk-");
    for byte in digest {
        use std::fmt::Write as _;
        let _written = write!(expected, "{byte:02x}");
    }
    expected.push_str(".sock");
    assert_eq!(name, expected);
    assert_eq!(name.len(), 3 + 64 + 5);

    let first = PathBuf::from(std::ffi::OsString::from_vec(b"/long/data-\x80".to_vec()));
    let second = PathBuf::from(std::ffi::OsString::from_vec(b"/long/data-\x81".to_vec()));
    assert_eq!(
        first.as_os_str().to_string_lossy(),
        second.as_os_str().to_string_lossy()
    );
    assert_ne!(socket_alias_name(&first), socket_alias_name(&second));
}

#[test]
fn alias_uses_native_path_boundary_and_maximum_uid_fits_default_root() {
    let short = Path::new("/tmp/short.sock");
    assert_eq!(
        short_socket_alias_with_root(short, Some(Path::new("/missing"))).unwrap(),
        None
    );

    let path = overlong_socket_path();
    assert_eq!(
        path.as_os_str().as_encoded_bytes().len(),
        UNIX_SOCKET_PATH_LIMIT
    );
    let _error = short_socket_alias_with_root(&path, Some(Path::new("/missing"))).unwrap_err();

    let max_uid_alias = default_socket_alias_root()
        .join(format!("{BROKER_SOCKET_ALIAS_DIR_PREFIX}{}", u32::MAX))
        .join(format!("jk-{}.sock", "f".repeat(64)));
    assert!(
        max_uid_alias.as_os_str().as_encoded_bytes().len() < UNIX_SOCKET_PATH_LIMIT,
        "full SHA-256 alias must fit the default root with a ten-digit uid"
    );
}

#[test]
fn explicit_root_is_not_checked_when_full_socket_path_fits() {
    let full =
        broker_socket_path_for_data_dir(Path::new("/tmp"), Path::new("/")).expect("socket path");
    assert!(full.as_os_str().as_encoded_bytes().len() < UNIX_SOCKET_PATH_LIMIT);
    assert_eq!(
        socket_path_for_data_dir(Path::new("/tmp"), Some(Path::new("/missing/alias-root")))
            .expect("short socket path ignores unavailable alias root"),
        full
    );
}
