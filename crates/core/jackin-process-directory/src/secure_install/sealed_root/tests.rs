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
