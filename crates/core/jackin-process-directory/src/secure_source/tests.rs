// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

#![cfg(any(target_os = "linux", target_os = "macos"))]

use super::CheckedSourceRoot;
use crate::secure_install;
use rustix::fd::AsRawFd;
#[cfg(target_os = "linux")]
use rustix::fs::mkfifoat;
use rustix::fs::{Mode, OFlags, fcntl_getfl, open};
use rustix::io::{FdFlags, fcntl_getfd};
use std::fs::{self, File};
use std::io::{ErrorKind, Read, Seek};
use std::os::unix::fs::{FileExt, MetadataExt, PermissionsExt, symlink};
use std::path::{Path, PathBuf};

const FIXTURE_DIRECTORY_FLAGS: OFlags = OFlags::RDONLY
    .union(OFlags::DIRECTORY)
    .union(OFlags::NOFOLLOW)
    .union(OFlags::CLOEXEC);

#[cfg(target_os = "linux")]
fn fixture_base() -> tempfile::TempDir {
    let mut candidates = Vec::with_capacity(2);
    let cwd = std::env::current_dir().expect("read current test directory");
    candidates.push(cwd.clone());
    if let Some(home) = std::env::var_os("HOME") {
        let home = PathBuf::from(home);
        if !home.as_os_str().is_empty() && home != cwd {
            candidates.push(home);
        }
    }

    let mut failures = Vec::new();
    for parent in candidates.into_iter().take(2) {
        let temporary = match tempfile::Builder::new()
            .prefix(".jackin-source-root-")
            .permissions(fs::Permissions::from_mode(0o700))
            .tempdir_in(&parent)
        {
            Ok(temporary) => temporary,
            Err(error) => {
                failures.push(format!("fixture below {parent:?} failed: {error}"));
                continue;
            }
        };
        match CheckedSourceRoot::from_absolute_directory(temporary.path()) {
            Ok(root) => {
                drop(root);
                return temporary;
            }
            Err(error) => failures.push(format!(
                "production source-root walk rejected fixture {:?} below {parent:?}: {error}",
                temporary.path(),
            )),
        }
    }

    panic!(
        "no bounded fixture parent passed the production source-root walk: {}",
        failures.join("; "),
    );
}

#[cfg(target_os = "macos")]
fn fixture_base() -> tempfile::TempDir {
    let cwd = std::env::current_dir().expect("read current test directory");
    let temporary = tempfile::Builder::new()
        .prefix(".jackin-source-root-")
        .permissions(fs::Permissions::from_mode(0o700))
        .tempdir_in(&cwd)
        .expect("create source-root fixture below the checked-out working directory");
    CheckedSourceRoot::from_absolute_directory(temporary.path())
        .expect("production source-root walk accepts clean fixture chain");
    temporary
}

fn create_directory(path: &Path, mode: u32) {
    fs::create_dir(path).expect("create fixture directory");
    set_mode(path, mode);
}

fn write_file(path: &Path, bytes: &[u8], mode: u32) {
    fs::write(path, bytes).expect("write fixture file");
    set_mode(path, mode);
}

fn set_mode(path: &Path, mode: u32) {
    fs::set_permissions(path, fs::Permissions::from_mode(mode)).expect("set fixture mode");
}

fn open_directory(path: &Path) -> File {
    File::from(open(path, FIXTURE_DIRECTORY_FLAGS, Mode::empty()).expect("open fixture directory"))
}

fn error_kind<T>(result: std::io::Result<T>) -> ErrorKind {
    result
        .err()
        .expect("operation unexpectedly succeeded")
        .kind()
}

fn assert_private_cloexec_readonly(file: &File) {
    assert!(
        file.as_raw_fd() > 2,
        "owned descriptor must not alias stdio"
    );
    let status_flags = fcntl_getfl(file).expect("read source descriptor status flags");
    assert_eq!(
        status_flags & OFlags::ACCMODE,
        OFlags::RDONLY,
        "source descriptor must remain read-only",
    );
    assert!(
        status_flags.contains(OFlags::NONBLOCK),
        "source descriptor must retain nonblocking open status",
    );
    assert!(
        fcntl_getfd(file)
            .expect("read source descriptor flags")
            .contains(FdFlags::CLOEXEC),
        "source descriptor must be close-on-exec",
    );
}

fn source_root_below(base: &tempfile::TempDir, name: &str, mode: u32) -> PathBuf {
    let path = base.path().join(name);
    create_directory(&path, mode);
    path
}

#[test]
fn source_root_accepts_safe_checkout_modes_and_keeps_install_root_private() {
    let base = fixture_base();
    let checkout = source_root_below(&base, "checkout", 0o755);
    let source = CheckedSourceRoot::from_absolute_directory(&checkout)
        .expect("accept a current-user-owned non-writable 0755 source root");
    source.recheck().expect("recheck selected source root");
    assert_eq!(
        error_kind(crate::CheckedInstallRoot::from_absolute_directory(
            &checkout
        )),
        ErrorKind::PermissionDenied,
        "a source checkout must not relax the exact-0700 install-root policy",
    );

    let private = source_root_below(&base, "private", 0o700);
    CheckedSourceRoot::from_absolute_directory(&private)
        .expect("accept a private 0700 source fixture");
}

#[test]
fn source_root_rejects_group_or_other_writable_mode() {
    let base = fixture_base();
    let writable = source_root_below(&base, "writable", 0o770);
    assert_eq!(
        error_kind(CheckedSourceRoot::from_absolute_directory(&writable)),
        ErrorKind::PermissionDenied,
    );
}

#[test]
fn source_owner_classifier_rejects_foreign_uid() {
    let current_uid = rustix::process::geteuid().as_raw();
    let foreign_uid = if current_uid == u32::MAX {
        current_uid - 1
    } else {
        current_uid + 1
    };
    assert!(secure_install::source_directory_owner_matches_for_test(
        current_uid,
        current_uid,
    ));
    assert!(!secure_install::source_directory_owner_matches_for_test(
        foreign_uid,
        current_uid,
    ));
}

#[test]
fn source_relative_paths_reject_escape_and_symlink_components() {
    let base = fixture_base();
    let root_path = source_root_below(&base, "tree", 0o755);
    create_directory(&root_path.join("real"), 0o755);
    write_file(&root_path.join("real/source.rs"), b"source", 0o644);
    symlink("real", root_path.join("linked-directory")).expect("create directory symlink");
    symlink("real/source.rs", root_path.join("linked-file")).expect("create file symlink");
    symlink("tree", base.path().join("linked-root")).expect("create root symlink");

    let source = CheckedSourceRoot::from_absolute_directory(&root_path)
        .expect("open clean selected source root");
    assert_eq!(
        error_kind(source.open_file(Path::new("../outside"))),
        ErrorKind::InvalidInput,
    );
    assert_eq!(
        error_kind(source.open_file(Path::new("/absolute/source.rs"))),
        ErrorKind::InvalidInput,
    );
    assert_eq!(
        error_kind(source.open_file(Path::new("linked-directory/source.rs"))),
        ErrorKind::PermissionDenied,
    );
    assert_eq!(
        error_kind(source.open_file(Path::new("linked-file"))),
        ErrorKind::PermissionDenied,
    );
    assert_eq!(
        error_kind(CheckedSourceRoot::from_absolute_directory(
            &base.path().join("linked-root"),
        )),
        ErrorKind::PermissionDenied,
    );
}

#[test]
fn source_leaf_rejects_hardlinks_directories_fifo_and_special_modes() {
    let base = fixture_base();
    let root_path = source_root_below(&base, "tree", 0o700);
    let source =
        CheckedSourceRoot::from_absolute_directory(&root_path).expect("open clean source root");

    write_file(&root_path.join("linked"), b"shared", 0o755);
    fs::hard_link(root_path.join("linked"), root_path.join("alias"))
        .expect("create hard-link fixture");
    assert_eq!(
        error_kind(source.open_file(Path::new("linked"))),
        ErrorKind::PermissionDenied,
    );

    create_directory(&root_path.join("directory-leaf"), 0o700);
    assert_eq!(
        error_kind(source.open_file(Path::new("directory-leaf"))),
        ErrorKind::PermissionDenied,
    );

    #[cfg(target_os = "linux")]
    {
        mkfifoat(
            &source.inner.directory,
            "fifo-leaf",
            Mode::from_bits_truncate(0o600),
        )
        .expect("create nonblocking FIFO fixture");
        assert_eq!(
            error_kind(source.open_file(Path::new("fifo-leaf"))),
            ErrorKind::PermissionDenied,
        );
    }

    write_file(&root_path.join("special"), b"mode", 0o4755);
    assert_ne!(
        fs::metadata(root_path.join("special")).unwrap().mode() & 0o7000,
        0,
        "filesystem must preserve the special-mode fixture",
    );
    assert_eq!(
        error_kind(source.open_file(Path::new("special"))),
        ErrorKind::PermissionDenied,
    );
}

#[test]
fn source_file_is_readonly_cloexec_and_offset_preserving() {
    let base = fixture_base();
    let root_path = source_root_below(&base, "tree", 0o755);
    let bytes = b"read-only-source-content";
    write_file(&root_path.join("source.rs"), bytes, 0o644);
    let source =
        CheckedSourceRoot::from_absolute_directory(&root_path).expect("open clean source root");
    let checked = source
        .open_file(Path::new("source.rs"))
        .expect("open source file without following links");
    assert_private_cloexec_readonly(checked.file());
    let metadata = checked.file().metadata().expect("read source metadata");
    assert_eq!(metadata.len(), bytes.len() as u64);
    assert_eq!(metadata.mode() & 0o7777, 0o644);

    let mut offset_probe = checked.file().try_clone().expect("clone offset probe");
    assert_eq!(
        offset_probe.stream_position().expect("read initial offset"),
        0
    );
    let mut observed = vec![0; bytes.len()];
    assert_eq!(
        checked
            .file()
            .read_at(&mut observed, 0)
            .expect("read at explicit source offset"),
        bytes.len(),
    );
    assert_eq!(observed, bytes);
    assert_eq!(
        offset_probe
            .stream_position()
            .expect("recheck shared file offset"),
        0,
        "offset-preserving inspection must not move the original open-file offset",
    );
}

#[test]
fn source_root_recheck_rejects_root_name_replacement() {
    let base = fixture_base();
    let selected = source_root_below(&base, "selected", 0o755);
    let source =
        CheckedSourceRoot::from_absolute_directory(&selected).expect("open selected source root");
    source
        .recheck()
        .expect("initial selected root name matches");

    fs::rename(&selected, base.path().join("moved")).expect("move original source root");
    create_directory(&selected, 0o755);
    assert_eq!(error_kind(source.recheck()), ErrorKind::PermissionDenied);
}

#[test]
fn source_file_recheck_rejects_relative_directory_replacement() {
    let base = fixture_base();
    let root_path = source_root_below(&base, "tree", 0o755);
    let nested = root_path.join("nested");
    create_directory(&nested, 0o755);
    write_file(&nested.join("source.rs"), b"original", 0o644);
    let source =
        CheckedSourceRoot::from_absolute_directory(&root_path).expect("open clean source root");
    let checked = source
        .open_file(Path::new("nested/source.rs"))
        .expect("open nested source file");

    fs::rename(&nested, root_path.join("moved")).expect("move original relative directory");
    create_directory(&nested, 0o755);
    write_file(&nested.join("source.rs"), b"replacement", 0o644);
    assert_eq!(error_kind(checked.recheck()), ErrorKind::PermissionDenied);
}

#[test]
fn source_file_recheck_rejects_leaf_replacement_and_keeps_original_descriptor() {
    let base = fixture_base();
    let root_path = source_root_below(&base, "tree", 0o755);
    write_file(&root_path.join("source.rs"), b"original", 0o644);
    let source =
        CheckedSourceRoot::from_absolute_directory(&root_path).expect("open clean source root");
    let checked = source
        .open_file(Path::new("source.rs"))
        .expect("open original source file");

    fs::rename(root_path.join("source.rs"), root_path.join("moved.rs"))
        .expect("move original source leaf");
    write_file(&root_path.join("source.rs"), b"replacement", 0o644);
    assert_eq!(error_kind(checked.recheck()), ErrorKind::PermissionDenied);

    let mut original = checked.file();
    let mut bytes = Vec::new();
    original
        .read_to_end(&mut bytes)
        .expect("read retained original descriptor");
    assert_eq!(bytes, b"original");
}

#[cfg(target_os = "linux")]
#[test]
fn source_linux_acl_controls_use_clean_fullwalk_baselines() {
    let base = fixture_base();

    let default_root = source_root_below(&base, "default-acl-root", 0o700);
    CheckedSourceRoot::from_absolute_directory(&default_root)
        .expect("establish a clean full-walk baseline before default ACL injection");
    let default_descriptor = open_directory(&default_root);
    secure_install::set_source_linux_acl_fixture(&default_descriptor, true)
        .expect("inject a default ACL on the selected current-user source root");
    assert_eq!(
        error_kind(CheckedSourceRoot::from_absolute_directory(&default_root)),
        ErrorKind::PermissionDenied,
    );

    let access_root = source_root_below(&base, "access-acl-root", 0o700);
    write_file(&access_root.join("source.rs"), b"acl", 0o644);
    let source = CheckedSourceRoot::from_absolute_directory(&access_root)
        .expect("establish a clean full-walk baseline before access ACL injection");
    source
        .open_file(Path::new("source.rs"))
        .expect("verify a clean leaf before ACL injection");
    let leaf = open(access_root.join("source.rs"), OFlags::RDONLY, Mode::empty())
        .expect("open source leaf for ACL fixture");
    secure_install::set_source_linux_acl_fixture(&leaf, false)
        .expect("inject an access ACL on the source leaf");
    assert_eq!(
        error_kind(source.open_file(Path::new("source.rs"))),
        ErrorKind::PermissionDenied,
    );
}

#[cfg(target_os = "macos")]
#[test]
fn source_darwin_acl_allows_read_only_entries_and_rejects_mutation() {
    use secure_install::SourceDarwinAclFixture;

    let base = fixture_base();
    for (name, fixture) in [
        ("deny-read", SourceDarwinAclFixture::DenyRead),
        ("allow-read", SourceDarwinAclFixture::AllowRead),
    ] {
        let root_path = source_root_below(&base, name, 0o700);
        let descriptor = open_directory(&root_path);
        secure_install::set_source_darwin_acl_fixture(&descriptor, fixture)
            .expect("install a non-mutating ACL fixture");
        CheckedSourceRoot::from_absolute_directory(&root_path)
            .expect("accept a deny or read-only ACL on the selected source root");
    }

    let root_path = source_root_below(&base, "mutating-root", 0o700);
    let descriptor = open_directory(&root_path);
    secure_install::set_source_darwin_acl_fixture(
        &descriptor,
        SourceDarwinAclFixture::AllowMutation,
    )
    .expect("install mutating ACL fixture");
    assert_eq!(
        error_kind(CheckedSourceRoot::from_absolute_directory(&root_path)),
        ErrorKind::PermissionDenied,
    );

    let root_path = source_root_below(&base, "mutating-leaf-root", 0o700);
    write_file(&root_path.join("source.rs"), b"acl", 0o644);
    let source =
        CheckedSourceRoot::from_absolute_directory(&root_path).expect("open clean source root");
    let descriptor = open(root_path.join("source.rs"), OFlags::RDONLY, Mode::empty())
        .expect("open source leaf for ACL fixture");
    secure_install::set_source_darwin_acl_fixture(
        &descriptor,
        SourceDarwinAclFixture::AllowMutation,
    )
    .expect("install mutating leaf ACL fixture");
    assert_eq!(
        error_kind(source.open_file(Path::new("source.rs"))),
        ErrorKind::PermissionDenied,
    );
}
