// SPDX-FileCopyrightText: 2026 The jackin❯ Authors
// SPDX-License-Identifier: Apache-2.0

use super::*;

#[test]
fn cargo_path_selects_the_qualified_mbx_wrapper_before_rustup_cargo() {
    let temporary = tempfile::tempdir().unwrap();
    let managed = temporary.path().join("managed");
    let rust_bin = temporary.path().join("rust-bin");
    fs::create_dir(&managed).unwrap();
    fs::create_dir(&rust_bin).unwrap();
    let mbx_cargo = temporary.path().join("mbx-cargo");
    fs::write(&mbx_cargo, b"MBX wrapper").unwrap();
    fs::write(rust_bin.join("cargo"), b"rustup cargo").unwrap();
    symlink(&mbx_cargo, managed.join("cargo")).unwrap();
    let path = env::join_paths([&managed, &rust_bin]).unwrap();
    let selected = find_executable("cargo", &path).unwrap();
    assert_eq!(
        fs::canonicalize(selected).unwrap(),
        fs::canonicalize(mbx_cargo).unwrap()
    );
}
