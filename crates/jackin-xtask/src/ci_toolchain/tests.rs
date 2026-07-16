use std::fs;
use std::os::unix::fs::PermissionsExt;

use tempfile::tempdir;

use super::valid_toolchain;

#[test]
fn toolchain_requires_both_rustc_and_cargo() {
    let temp = tempdir().unwrap();
    let bin = temp.path().join("bin");
    fs::create_dir_all(&bin).unwrap();
    let rustc = bin.join("rustc");
    fs::write(&rustc, b"rustc").unwrap();
    fs::set_permissions(&rustc, fs::Permissions::from_mode(0o755)).unwrap();
    assert!(!valid_toolchain(temp.path()));

    let cargo = bin.join("cargo");
    fs::write(&cargo, b"cargo").unwrap();
    fs::set_permissions(&cargo, fs::Permissions::from_mode(0o755)).unwrap();
    assert!(valid_toolchain(temp.path()));
}
