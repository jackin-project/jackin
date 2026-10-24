// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

#![cfg(any(target_os = "linux", target_os = "macos"))]

#[cfg(target_os = "linux")]
use super::super::linux_acl::{XattrProbe, classify_probe};
use super::super::{
    CheckedInstallFile, CheckedInstallRoot, InstallFileKind, MetadataFacts, MetadataProblem,
    NodeRole, classify_metadata,
};
use super::{absolute_components, open_directory_component, open_filesystem_root};
use rustix::fd::AsRawFd;
use rustix::fs::{FileType, Mode, OFlags, fcntl_getfl, open};
#[cfg(target_os = "linux")]
use rustix::fs::{fgetxattr, fstat, openat};
use rustix::io::{FdFlags, fcntl_getfd};
#[cfg(target_os = "linux")]
use std::ffi::CStr;
#[cfg(target_os = "linux")]
use std::fmt::Write as _;
use std::fs::{self, File};
use std::io::{self, Read};
use std::os::unix::fs::{PermissionsExt, symlink};
use std::path::Path;
#[cfg(target_os = "linux")]
use std::path::{Component, PathBuf};

const FIXTURE_DIRECTORY_FLAGS: OFlags = OFlags::RDONLY
    .union(OFlags::DIRECTORY)
    .union(OFlags::NOFOLLOW)
    .union(OFlags::CLOEXEC);

#[cfg(target_os = "macos")]
fn fixture_base() -> tempfile::TempDir {
    let cwd = std::env::current_dir().expect("read the physical test working directory");
    tempfile::Builder::new()
        .prefix(".jackin-absolute-root-")
        .tempdir_in(cwd)
        .expect("create an owned fixture below the checked-out working directory")
}

#[cfg(target_os = "linux")]
fn fixture_base() -> tempfile::TempDir {
    let mut candidates = Vec::with_capacity(2);
    let mut failures = Vec::with_capacity(4);
    let mut cwd_candidate = None;

    match std::env::current_dir() {
        Ok(cwd) => {
            cwd_candidate = Some(cwd.clone());
            if let Some(reason) = fixture_parent_problem(&cwd) {
                failures.push(format!(
                    "current_dir candidate {cwd:?} rejected: {reason}; chain diagnostics: {}",
                    fixture_chain_diagnostics(&cwd),
                ));
            } else {
                candidates.push(cwd);
            }
        }
        Err(error) => failures.push(format!(
            "reading physical current directory failed: {error}"
        )),
    }

    match std::env::var_os("HOME") {
        Some(home) if !home.is_empty() => {
            let home = PathBuf::from(home);
            if cwd_candidate.as_ref() == Some(&home) {
                failures.push(format!(
                    "HOME candidate {home:?} duplicates current_dir and was not retried"
                ));
            } else if let Some(reason) = fixture_parent_problem(&home) {
                failures.push(format!(
                    "HOME candidate {home:?} rejected: {reason}; chain diagnostics: {}",
                    fixture_chain_diagnostics(&home),
                ));
            } else {
                candidates.push(home);
            }
        }
        Some(_) => failures.push("HOME candidate is empty".to_owned()),
        None => failures.push("HOME candidate is unset".to_owned()),
    }

    for parent in candidates.into_iter().take(2) {
        let temporary = match tempfile::Builder::new()
            .prefix(".jackin-absolute-root-")
            .tempdir_in(&parent)
        {
            Ok(temporary) => temporary,
            Err(error) => {
                failures.push(format!(
                    "creating a private fixture below {parent:?} failed: {error}; chain diagnostics: {}",
                    fixture_chain_diagnostics(&parent),
                ));
                continue;
            }
        };

        match CheckedInstallRoot::from_absolute_directory(temporary.path()) {
            Ok(root) => {
                drop(root);
                return temporary;
            }
            Err(error) => failures.push(format!(
                "production clean-chain check rejected candidate {parent:?} at {:?}: {error}; chain diagnostics: {}",
                temporary.path(),
                fixture_chain_diagnostics(temporary.path()),
            )),
        }
    }

    panic!(
        "no candidate parent provided a production-accepted clean absolute fixture chain:\n{}",
        failures.join("\n"),
    );
}

#[cfg(target_os = "linux")]
fn fixture_parent_problem(parent: &Path) -> Option<String> {
    if !parent.is_absolute() {
        return Some("candidate parent is not absolute".to_owned());
    }
    if parent
        .components()
        .any(|component| matches!(component, Component::ParentDir | Component::Prefix(_)))
    {
        return Some("candidate parent contains parent traversal".to_owned());
    }
    match fs::metadata(parent) {
        Ok(metadata) if metadata.is_dir() => None,
        Ok(_) => Some("candidate parent is not a directory".to_owned()),
        Err(error) => Some(format!("reading candidate parent metadata failed: {error}")),
    }
}

#[cfg(target_os = "linux")]
fn fixture_chain_diagnostics(candidate: &Path) -> String {
    const ACCESS_ACL: &CStr = c"system.posix_acl_access";
    const DEFAULT_ACL: &CStr = c"system.posix_acl_default";

    let mut report = format!("candidate={candidate:?}\n");
    if !candidate.is_absolute() {
        writeln!(report, "  traversal=not-absolute")
            .expect("formatting a diagnostic String cannot fail");
        return report;
    }

    let mut components = Vec::new();
    for component in candidate.components() {
        match component {
            Component::RootDir => {}
            Component::Normal(name) => components.push(name.to_os_string()),
            Component::CurDir => {}
            Component::ParentDir | Component::Prefix(_) => {
                writeln!(report, "  traversal=refused-unsafe-component:{component:?}")
                    .expect("formatting a diagnostic String cannot fail");
                return report;
            }
        }
    }

    let mut parent = match open("/", FIXTURE_DIRECTORY_FLAGS, Mode::empty()) {
        Ok(root) => File::from(root),
        Err(error) => {
            writeln!(report, "  component=/ open=error({error:?})")
                .expect("formatting a diagnostic String cannot fail");
            return report;
        }
    };
    append_fixture_component_diagnostic(
        &mut report,
        Path::new("/"),
        &parent,
        ACCESS_ACL,
        DEFAULT_ACL,
    );

    let mut opened_path = PathBuf::from("/");
    for component in components {
        let next = match openat(&parent, &component, FIXTURE_DIRECTORY_FLAGS, Mode::empty()) {
            Ok(next) => File::from(next),
            Err(error) => {
                writeln!(report, "  component={component:?} openat=error({error:?})")
                    .expect("formatting a diagnostic String cannot fail");
                break;
            }
        };
        opened_path.push(&component);
        append_fixture_component_diagnostic(
            &mut report,
            &opened_path,
            &next,
            ACCESS_ACL,
            DEFAULT_ACL,
        );
        parent = next;
    }
    report
}

#[cfg(target_os = "linux")]
fn append_fixture_component_diagnostic(
    report: &mut String,
    path: &Path,
    descriptor: &File,
    access_acl: &CStr,
    default_acl: &CStr,
) {
    write!(report, "  component={path:?}").expect("formatting a diagnostic String cannot fail");
    match fstat(descriptor) {
        Ok(metadata) => {
            write!(
                report,
                " uid={} mode={:04o} dev={} ino={}",
                metadata.st_uid,
                metadata.st_mode & 0o7777,
                metadata.st_dev,
                metadata.st_ino,
            )
            .expect("formatting a diagnostic String cannot fail");
        }
        Err(error) => {
            write!(report, " stat=error({error:?})")
                .expect("formatting a diagnostic String cannot fail");
        }
    }
    writeln!(
        report,
        " {}={} {}={}",
        access_acl.to_string_lossy(),
        fixture_xattr_probe(descriptor, access_acl),
        default_acl.to_string_lossy(),
        fixture_xattr_probe(descriptor, default_acl),
    )
    .expect("formatting a diagnostic String cannot fail");
}

#[cfg(target_os = "linux")]
fn fixture_xattr_probe(descriptor: &File, name: &CStr) -> String {
    let mut value = [0_u8; 1];
    match classify_probe(fgetxattr(descriptor, name, &mut value)) {
        XattrProbe::Absent => "absent".to_owned(),
        XattrProbe::Present => "present".to_owned(),
        XattrProbe::Unsupported => "unsupported".to_owned(),
        XattrProbe::Failed(error) => format!("error({error:?})"),
    }
}

fn create_directory(path: &Path, mode: u32) {
    fs::create_dir(path).expect("create private fixture directory");
    set_mode(path, mode);
}

fn write_file(path: &Path, contents: &[u8], mode: u32) {
    fs::write(path, contents).expect("write private fixture file");
    set_mode(path, mode);
}

fn set_mode(path: &Path, mode: u32) {
    fs::set_permissions(path, fs::Permissions::from_mode(mode)).expect("set private fixture mode");
}

fn open_fixture_directory(path: &Path) -> File {
    File::from(open(path, FIXTURE_DIRECTORY_FLAGS, Mode::empty()).expect("open fixture directory"))
}

fn error_kind<T>(result: io::Result<T>) -> io::ErrorKind {
    result
        .err()
        .expect("operation unexpectedly succeeded")
        .kind()
}

fn read_checked(file: &CheckedInstallFile) -> Vec<u8> {
    let mut contents = Vec::new();
    file.file()
        .read_to_end(&mut contents)
        .expect("read original checked descriptor");
    contents
}

fn assert_descriptor_is_private_and_cloexec(file: &File) {
    assert!(
        file.as_raw_fd() > 2,
        "owned descriptor must not alias stdio"
    );
    assert_eq!(
        fcntl_getfl(file).expect("read descriptor access mode") & OFlags::ACCMODE,
        OFlags::RDONLY,
        "checked descriptor must remain read-only",
    );
    assert!(
        fcntl_getfd(file)
            .expect("read descriptor flags")
            .contains(FdFlags::CLOEXEC),
        "owned descriptor must be close-on-exec",
    );
}

#[test]
fn absolute_private_root_and_b21_leaf_preserve_original_descriptor() {
    let temporary = fixture_base();
    let selected = temporary.path().join("selected-é");
    create_directory(&selected, 0o700);
    let bin = selected.join("bin");
    create_directory(&bin, 0o700);
    let owner = bin.join("owner");
    write_file(&owner, b"absolute-root-marker", 0o755);

    let root = CheckedInstallRoot::from_absolute_directory(&selected)
        .expect("open absolute private root through checked ancestors");
    assert_descriptor_is_private_and_cloexec(&root.directory);
    let checked = root
        .open_file(Path::new("bin/owner"), InstallFileKind::Executable)
        .expect("preserve B2.1 descriptor-relative inspection");
    assert_descriptor_is_private_and_cloexec(checked.file());
    assert_eq!(read_checked(&checked), b"absolute-root-marker");
    checked
        .recheck()
        .expect("recheck the original leaf descriptor");
}

#[test]
fn absolute_lexical_escape_is_rejected_before_lookup() {
    let temporary = fixture_base();
    let nonexistent_parent = temporary.path().join("missing").join("..").join("root");
    for path in [
        Path::new(""),
        Path::new("relative/root"),
        Path::new("/"),
        nonexistent_parent.as_path(),
    ] {
        assert_eq!(
            error_kind(CheckedInstallRoot::from_absolute_directory(path)),
            io::ErrorKind::InvalidInput,
            "lexically invalid path must fail before descendant lookup: {path:?}",
        );
    }
}

#[test]
fn absolute_intermediate_and_final_symlinks_rejected() {
    let temporary = fixture_base();
    let real_parent = temporary.path().join("real-parent");
    create_directory(&real_parent, 0o700);
    let intermediate_root = real_parent.join("root");
    create_directory(&intermediate_root, 0o700);
    let intermediate_marker = intermediate_root.join("marker");
    write_file(&intermediate_marker, b"intermediate-target", 0o600);
    let final_parent = temporary.path().join("final-parent");
    create_directory(&final_parent, 0o700);
    let final_root = final_parent.join("root");
    create_directory(&final_root, 0o700);
    let final_marker = final_root.join("marker");
    write_file(&final_marker, b"final-target", 0o600);

    let linked_parent = temporary.path().join("linked-parent");
    symlink(&real_parent, &linked_parent).expect("create intermediate symlink fixture");
    let linked_root = temporary.path().join("linked-root");
    symlink(&final_root, &linked_root).expect("create final symlink fixture");

    assert_eq!(
        error_kind(CheckedInstallRoot::from_absolute_directory(
            &linked_parent.join("root"),
        )),
        io::ErrorKind::PermissionDenied,
    );
    assert_eq!(
        error_kind(CheckedInstallRoot::from_absolute_directory(&linked_root)),
        io::ErrorKind::PermissionDenied,
    );
    assert_eq!(
        fs::read(&intermediate_marker).expect("read intermediate symlink target"),
        b"intermediate-target",
        "intermediate symlink rejection must leave its target unchanged",
    );
    assert_eq!(
        fs::read(&final_marker).expect("read final symlink target"),
        b"final-target",
        "final symlink rejection must leave its target unchanged",
    );
}

#[test]
fn absolute_unsafe_ancestor_mode_and_foreign_owner_are_rejected() {
    let temporary = fixture_base();
    let writable = temporary.path().join("writable-ancestor");
    create_directory(&writable, 0o770);
    let selected = writable.join("selected");
    create_directory(&selected, 0o700);
    for mode in [0o770, 0o1777] {
        set_mode(&writable, mode);
        assert_eq!(
            error_kind(CheckedInstallRoot::from_absolute_directory(&selected)),
            io::ErrorKind::PermissionDenied,
            "writable ancestor mode {mode:o} must fail even when sticky",
        );
    }

    let effective_uid = super::super::geteuid().as_raw();
    let foreign_uid = if effective_uid == u32::MAX {
        effective_uid - 1
    } else {
        effective_uid + 1
    };
    let facts = MetadataFacts {
        file_type: FileType::Directory,
        mode: Mode::from_bits_truncate(0o755),
        uid: 0,
        single_link: true,
    };
    assert_eq!(
        classify_metadata(facts, effective_uid, NodeRole::Ancestor),
        Ok(()),
        "a root-owned read-only ancestor is accepted",
    );
    assert_eq!(
        classify_metadata(
            MetadataFacts {
                uid: effective_uid,
                ..facts
            },
            effective_uid,
            NodeRole::Ancestor,
        ),
        Ok(()),
        "an effective-user-owned read-only ancestor is accepted",
    );
    assert_eq!(
        classify_metadata(
            MetadataFacts {
                uid: foreign_uid,
                ..facts
            },
            effective_uid,
            NodeRole::Ancestor,
        ),
        Err(MetadataProblem::WrongAncestorOwner),
        "a foreign-owned ancestor is rejected",
    );
}

#[test]
fn absolute_parent_rebind_uses_pinned_chain() {
    let temporary = fixture_base();
    let parent = temporary.path().join("parent");
    create_directory(&parent, 0o700);
    let selected = parent.join("selected");
    create_directory(&selected, 0o700);
    write_file(&selected.join("marker"), b"original-parent", 0o600);

    let components = absolute_components(&selected).expect("preflight fixture path");
    let mut parent_descriptor = open_filesystem_root().expect("open pinned OS root");
    for component in &components[..components.len() - 1] {
        parent_descriptor =
            open_directory_component(&parent_descriptor, component, NodeRole::Ancestor)
                .expect("walk to pinned parent descriptor");
    }

    let parked_parent = temporary.path().join("parked-parent");
    fs::rename(&parent, &parked_parent).expect("rename selected parent while it is pinned");
    create_directory(&parent, 0o700);
    let replacement = parent.join("selected");
    create_directory(&replacement, 0o700);
    write_file(&replacement.join("marker"), b"replacement-parent", 0o600);

    let final_name = components.last().expect("fixture root component");
    let final_descriptor = open_directory_component(&parent_descriptor, final_name, NodeRole::Root)
        .expect("continue through the original parent descriptor");
    let pinned_root = CheckedInstallRoot {
        directory: final_descriptor,
    };
    let checked = pinned_root
        .open_file(Path::new("marker"), InstallFileKind::Receipt)
        .expect("open original child beneath the retained descriptor");
    assert_eq!(read_checked(&checked), b"original-parent");
    assert_eq!(
        fs::read(replacement.join("marker")).expect("read replacement namespace"),
        b"replacement-parent",
    );
}

#[test]
fn absolute_root_still_requires_private_final_metadata() {
    let temporary = fixture_base();
    let private_root = temporary.path().join("private-root");
    create_directory(&private_root, 0o700);
    CheckedInstallRoot::from_absolute_directory(&private_root)
        .expect("effective-user-owned mode-0700 final directory is accepted");

    let wrong_mode = temporary.path().join("wrong-mode");
    create_directory(&wrong_mode, 0o750);
    assert_eq!(
        error_kind(CheckedInstallRoot::from_absolute_directory(&wrong_mode)),
        io::ErrorKind::PermissionDenied,
    );

    let regular_file = temporary.path().join("not-a-directory");
    write_file(&regular_file, b"not a directory", 0o600);
    assert_eq!(
        error_kind(CheckedInstallRoot::from_absolute_directory(&regular_file)),
        io::ErrorKind::PermissionDenied,
    );

    let effective_uid = super::super::geteuid().as_raw();
    let root_owned_final = MetadataFacts {
        file_type: FileType::Directory,
        mode: Mode::from_bits_truncate(0o700),
        uid: 0,
        single_link: true,
    };
    assert_eq!(
        classify_metadata(root_owned_final, effective_uid, NodeRole::Root),
        if effective_uid == 0 {
            Ok(())
        } else {
            Err(MetadataProblem::WrongOwner)
        },
        "root is accepted as final owner only when it is the effective user",
    );
}

#[cfg(target_os = "macos")]
#[test]
fn absolute_darwin_read_acl_ancestor_is_accepted() {
    use super::super::darwin_acl::{FixtureAcl, set_test_acl};

    for acl in [FixtureAcl::DenyRead, FixtureAcl::AllowRead] {
        let temporary = fixture_base();
        let ancestor = temporary.path().join("acl-ancestor");
        create_directory(&ancestor, 0o755);
        let selected = ancestor.join("selected");
        create_directory(&selected, 0o700);
        let descriptor = open_fixture_directory(&ancestor);
        set_test_acl(&descriptor, acl).expect("set read-only or deny ACL fixture");
        CheckedInstallRoot::from_absolute_directory(&selected)
            .expect("nonmutating native ACL must remain acceptable");
    }
}

#[cfg(target_os = "macos")]
#[test]
fn absolute_darwin_mutating_ancestor_acl_is_rejected() {
    use super::super::darwin_acl::{FixtureAcl, set_test_acl};

    for acl in [
        FixtureAcl::AllowMutation,
        FixtureAcl::InheritableMutation,
        FixtureAcl::DeleteChild,
    ] {
        let temporary = fixture_base();
        let ancestor = temporary.path().join("acl-ancestor");
        create_directory(&ancestor, 0o755);
        let selected = ancestor.join("selected");
        create_directory(&selected, 0o700);
        CheckedInstallRoot::from_absolute_directory(&selected)
            .expect("unmodified ancestor chain must be accepted before adding the ACL");
        let descriptor = open_fixture_directory(&ancestor);
        set_test_acl(&descriptor, acl).expect("set mutating native ACL fixture");
        assert_eq!(
            fs::metadata(&ancestor)
                .expect("read ACL ancestor mode")
                .permissions()
                .mode()
                & 0o7777,
            0o755,
            "mutating ACL fixture must preserve valid ancestor mode",
        );
        assert_eq!(
            fs::metadata(&selected)
                .expect("read selected root mode")
                .permissions()
                .mode()
                & 0o7777,
            0o700,
            "selected root must remain private after setting the ancestor ACL",
        );
        assert_eq!(
            error_kind(CheckedInstallRoot::from_absolute_directory(&selected)),
            io::ErrorKind::PermissionDenied,
        );
    }
}

#[cfg(target_os = "linux")]
#[test]
fn absolute_linux_access_acl_ancestor_is_rejected() {
    use super::super::linux_acl::set_extended_acl_fixture;

    let temporary = fixture_base();
    let ancestor = temporary.path().join("access-acl-ancestor");
    create_directory(&ancestor, 0o755);
    let selected = ancestor.join("selected");
    create_directory(&selected, 0o700);
    drop(
        CheckedInstallRoot::from_absolute_directory(&selected)
            .expect("full absolute chain must be clean before access ACL injection"),
    );
    let descriptor = open_fixture_directory(&ancestor);
    set_extended_acl_fixture(&descriptor, false).expect("set access ACL fixture");
    assert_eq!(
        fs::metadata(&ancestor)
            .expect("read access ACL ancestor metadata")
            .permissions()
            .mode()
            & 0o777,
        0o755,
        "access ACL fixture must preserve the reviewed ancestor mode",
    );
    assert_eq!(
        error_kind(CheckedInstallRoot::from_absolute_directory(&selected)),
        io::ErrorKind::PermissionDenied,
    );
}

#[cfg(target_os = "linux")]
#[test]
fn absolute_linux_default_acl_on_private_ancestor_is_rejected() {
    use super::super::linux_acl::{ensure_no_extended_acl, set_extended_acl_fixture};
    use rustix::fd::AsFd;

    let temporary = fixture_base();
    let ancestor = temporary.path().join("default-acl-ancestor");
    create_directory(&ancestor, 0o700);
    let selected = ancestor.join("selected");
    create_directory(&selected, 0o700);
    drop(
        CheckedInstallRoot::from_absolute_directory(&selected)
            .expect("full absolute chain must be clean before default ACL injection"),
    );
    let descriptor = open_fixture_directory(&ancestor);
    set_extended_acl_fixture(&descriptor, true).expect("set default ACL fixture");
    set_mode(&ancestor, 0o700);
    assert_eq!(
        fs::metadata(&ancestor)
            .expect("read default ACL ancestor metadata")
            .permissions()
            .mode()
            & 0o777,
        0o700,
        "default ACL fixture must retain the reviewed private mode",
    );
    let selected_descriptor = open_fixture_directory(&selected);
    ensure_no_extended_acl(selected_descriptor.as_fd(), true)
        .expect("create selected root before the default ACL to avoid inheritance");

    assert_eq!(
        error_kind(CheckedInstallRoot::from_absolute_directory(&selected)),
        io::ErrorKind::PermissionDenied,
    );
}
