// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

#[cfg(any(target_os = "linux", target_os = "macos"))]
use super::*;
#[cfg(any(target_os = "linux", target_os = "macos"))]
use rustix::fs::{OFlags, open};
#[cfg(any(target_os = "linux", target_os = "macos"))]
use std::{fs, os::unix::fs::PermissionsExt, path::Path};

#[cfg(any(target_os = "linux", target_os = "macos"))]
fn directory(path: &Path, mode: u32) -> io::Result<File> {
    fs::create_dir(path)?;
    fs::set_permissions(path, fs::Permissions::from_mode(mode))?;
    Ok(File::from(open(
        path,
        OFlags::RDONLY | OFlags::DIRECTORY | OFlags::NOFOLLOW | OFlags::CLOEXEC,
        Mode::empty(),
    )?))
}

#[cfg(any(target_os = "linux", target_os = "macos"))]
fn regular_file(path: &Path) -> io::Result<File> {
    fs::write(path, b"private")?;
    fs::set_permissions(path, fs::Permissions::from_mode(0o600))?;
    Ok(File::from(open(
        path,
        OFlags::RDONLY | OFlags::NOFOLLOW | OFlags::CLOEXEC,
        Mode::empty(),
    )?))
}

#[cfg(any(target_os = "linux", target_os = "macos"))]
#[test]
fn confidential_descriptor_validation_accepts_clean_private_objects() -> io::Result<()> {
    let temporary = tempfile::tempdir()?;
    let directory = directory(&temporary.path().join("private"), 0o700)?;
    let file = regular_file(&temporary.path().join("private-file"))?;
    CheckedInstallRoot::validate_confidential_directory(&directory)?;
    CheckedInstallRoot::validate_confidential_file(&file)?;
    Ok(())
}

#[cfg(target_os = "linux")]
#[test]
fn confidential_directory_and_file_validation_rejects_present_acls() -> io::Result<()> {
    let temporary = tempfile::tempdir()?;
    let private_directory = directory(&temporary.path().join("acl-directory"), 0o700)?;
    super::super::linux_acl::set_confidential_acl_fixture(&private_directory, 0x07)?;
    assert_eq!(
        CheckedInstallRoot::validate_confidential_directory(&private_directory)
            .expect_err("private data directory must have no ACL entries")
            .kind(),
        io::ErrorKind::PermissionDenied
    );

    let default_acl_directory = directory(&temporary.path().join("default-acl-directory"), 0o700)?;
    super::super::linux_acl::set_extended_acl_fixture(&default_acl_directory, true)?;
    assert_eq!(
        CheckedInstallRoot::validate_confidential_directory(&default_acl_directory)
            .expect_err("private data directory must reject default ACL entries")
            .kind(),
        io::ErrorKind::PermissionDenied
    );

    let file = regular_file(&temporary.path().join("acl-file"))?;
    super::super::linux_acl::set_confidential_acl_fixture(&file, 0x06)?;
    assert_eq!(
        CheckedInstallRoot::validate_confidential_file(&file)
            .expect_err("private data file must have no ACL entries")
            .kind(),
        io::ErrorKind::PermissionDenied
    );
    Ok(())
}

#[cfg(target_os = "macos")]
#[test]
fn confidential_validation_rejects_read_acl_without_changing_generic_policy() -> io::Result<()> {
    let temporary = tempfile::tempdir()?;
    let acl_directory = directory(&temporary.path().join("acl-directory"), 0o700)?;
    super::super::darwin_acl::set_test_acl(
        &acl_directory,
        super::super::darwin_acl::FixtureAcl::AllowRead,
    )?;
    CheckedInstallRoot::from_pinned_directory(acl_directory.try_clone()?)?;
    assert_eq!(
        CheckedInstallRoot::validate_confidential_directory(&acl_directory)
            .expect_err("confidential directory must reject every ACL entry")
            .kind(),
        io::ErrorKind::PermissionDenied
    );

    let plain_root_path = temporary.path().join("plain-root");
    let plain_root = directory(&plain_root_path, 0o700)?;
    let file = regular_file(&plain_root_path.join("acl-file"))?;
    super::super::darwin_acl::set_test_acl(&file, super::super::darwin_acl::FixtureAcl::AllowRead)?;
    let root = CheckedInstallRoot::from_pinned_directory(plain_root)?;
    let checked_file = root.open_file(Path::new("acl-file"), InstallFileKind::Receipt)?;
    assert_eq!(
        CheckedInstallRoot::validate_confidential_file(&file)
            .expect_err("confidential file must reject every ACL entry")
            .kind(),
        io::ErrorKind::PermissionDenied
    );
    drop(checked_file);
    Ok(())
}

#[cfg(any(target_os = "linux", target_os = "macos"))]
#[test]
fn sealed_validation_preserves_the_private_root_mode_contract() -> io::Result<()> {
    let temporary = tempfile::tempdir()?;
    let sealed = directory(&temporary.path().join("sealed"), 0o500)?;
    CheckedInstallRoot::validate_pinned_sealed_directory(&sealed)?;
    assert_eq!(
        CheckedInstallRoot::from_pinned_directory(sealed.try_clone()?)
            .expect_err("sealed directory is not a writable private root")
            .kind(),
        io::ErrorKind::PermissionDenied
    );
    let private = directory(&temporary.path().join("private"), 0o700)?;
    assert_eq!(
        CheckedInstallRoot::validate_pinned_sealed_directory(&private)
            .expect_err("writable directory is not sealed")
            .kind(),
        io::ErrorKind::PermissionDenied
    );
    CheckedInstallRoot::from_pinned_directory(private)?;
    Ok(())
}

#[cfg(any(target_os = "linux", target_os = "macos"))]
#[test]
fn sealed_validation_keeps_the_original_descriptor_after_name_replacement() -> io::Result<()> {
    let temporary = tempfile::tempdir()?;
    let name = temporary.path().join("generation");
    let sealed = directory(&name, 0o500)?;
    fs::rename(&name, temporary.path().join("retained-generation"))?;
    let replacement = directory(&name, 0o700)?;
    CheckedInstallRoot::validate_pinned_sealed_directory(&sealed)?;
    CheckedInstallRoot::validate_pinned_sealed_directory(&replacement)
        .expect_err("replacement must not substitute for the sealed descriptor");
    Ok(())
}

#[cfg(any(target_os = "linux", target_os = "macos"))]
#[test]
fn sealed_validation_rejects_an_actual_native_acl_without_mode_drift() -> io::Result<()> {
    let temporary = tempfile::tempdir()?;
    let descriptor = directory(&temporary.path().join("generation"), 0o500)?;
    #[cfg(target_os = "macos")]
    super::super::darwin_acl::set_test_acl(
        &descriptor,
        super::super::darwin_acl::FixtureAcl::AllowMutation,
    )?;
    #[cfg(target_os = "linux")]
    super::super::linux_acl::set_extended_acl_fixture(&descriptor, true)?;
    assert_eq!(descriptor.metadata()?.permissions().mode() & 0o777, 0o500);
    assert_eq!(
        CheckedInstallRoot::validate_pinned_sealed_directory(&descriptor)
            .expect_err("native ACL must be checked even when exact mode is sealed")
            .kind(),
        io::ErrorKind::PermissionDenied
    );
    Ok(())
}
