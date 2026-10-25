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

#[test]
fn pinned_swiftlint_precedes_an_ambient_global_tool() {
    let temporary = tempfile::tempdir().expect("temporary directory");
    let managed = temporary.path().join("managed");
    let rust_bin = temporary.path().join("rust-bin");
    let ambient = temporary.path().join("ambient");
    let pinned_bin = temporary.path().join("mise/installs/swiftlint/0.65.1/bin");
    for directory in [&managed, &rust_bin, &ambient, &pinned_bin] {
        fs::create_dir_all(directory).expect("tool directory");
    }
    let pinned = pinned_bin.join("swiftlint");
    fs::write(&pinned, b"Mise-pinned SwiftLint 0.65.1").expect("pinned SwiftLint");
    let ambient_tool = ambient.join("swiftlint");
    fs::write(&ambient_tool, b"ambient SwiftLint").expect("ambient SwiftLint");
    let resolved =
        resolve_tool_paths(&["swiftlint"], |_| Ok(pinned.clone())).expect("mock Mise resolution");
    symlink(&resolved["swiftlint"], managed.join("swiftlint")).expect("managed SwiftLint link");
    let inherited = env::join_paths([&ambient]).expect("ambient PATH");
    let path = qualified_path(&managed, &rust_bin, &inherited).expect("qualified PATH");
    let selected = find_executable("swiftlint", &path).expect("qualified SwiftLint");

    assert_eq!(
        fs::canonicalize(selected).expect("selected SwiftLint"),
        fs::canonicalize(pinned).expect("physical pinned SwiftLint")
    );
}

#[cfg(target_os = "macos")]
#[test]
fn missing_swiftlint_resolution_fails_before_resolving_hk() {
    assert_eq!(MANAGED_TOOLS.first(), Some(&"swiftlint"));
    let mut requested = Vec::new();
    let result = resolve_tool_paths(MANAGED_TOOLS, |name| {
        requested.push(name);
        if name == "swiftlint" {
            anyhow::bail!("Mise could not resolve pinned SwiftLint");
        }
        Ok(PathBuf::from(name))
    });

    assert!(
        result
            .expect_err("missing pinned SwiftLint must stop resolution")
            .to_string()
            .contains("Mise could not resolve pinned SwiftLint")
    );
    assert_eq!(requested, ["swiftlint"]);
}

#[cfg(target_os = "linux")]
#[test]
fn linux_managed_tools_do_not_require_macos_only_swiftlint() {
    assert!(!MANAGED_TOOLS.contains(&"swiftlint"));
}
