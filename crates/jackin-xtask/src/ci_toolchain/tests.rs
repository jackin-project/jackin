use std::fs;
use std::os::unix::fs::PermissionsExt;

use tempfile::tempdir;

use super::{ToolchainConfig, valid_toolchain};

#[test]
fn toolchain_requires_both_rustc_and_cargo() {
    let config = ToolchainConfig {
        channel: "1.97.0".to_owned(),
        components: Vec::new(),
        targets: Vec::new(),
    };
    let temp = tempdir().unwrap();
    let bin = temp.path().join("bin");
    fs::create_dir_all(&bin).unwrap();
    let rustc = bin.join("rustc");
    fs::write(&rustc, b"rustc").unwrap();
    fs::set_permissions(&rustc, fs::Permissions::from_mode(0o755)).unwrap();
    assert!(!valid_toolchain(temp.path(), &config));

    let cargo = bin.join("cargo");
    fs::write(&cargo, b"cargo").unwrap();
    fs::set_permissions(&cargo, fs::Permissions::from_mode(0o755)).unwrap();
    assert!(valid_toolchain(temp.path(), &config));
}
