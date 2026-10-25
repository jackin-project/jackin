// SPDX-FileCopyrightText: 2026 The jackin❯ Authors
// SPDX-License-Identifier: Apache-2.0

use super::*;
use std::collections::BTreeSet;
#[cfg(not(target_os = "macos"))]
use std::os::unix::ffi::OsStringExt;
use std::os::unix::{ffi::OsStrExt, process::ExitStatusExt};
use std::process::{ExitStatus, Output};

fn write_executable(path: &Path, contents: &[u8]) {
    fs::write(path, contents).expect("tool file");
    fs::set_permissions(path, fs::Permissions::from_mode(0o755)).expect("executable mode");
}

fn command_result(status: i32, stdout: &[u8]) -> Output {
    Output {
        status: ExitStatus::from_raw(status << 8),
        stdout: stdout.to_vec(),
        stderr: Vec::new(),
    }
}

#[test]
fn path_record_parser_preserves_raw_unix_path_bytes_and_one_trailing_newline() {
    let path = b" leading \tpath\r\nwith internal line \xff\n";
    let mut record = path.to_vec();
    record.push(b'\n');

    let resolved = command_path_output(command_result(0, &record), "resolving fixture path")
        .expect("parse path record");

    assert_eq!(resolved.as_os_str().as_bytes(), path);
}

#[test]
fn path_record_parser_rejects_missing_empty_and_nul_records_and_nonzero_status() {
    for record in [&b"/repo"[..], b"\n", b"/repo\0path\n"] {
        command_path_output(command_result(0, record), "fixture path")
            .expect_err("malformed path record must fail");
    }

    let error = command_path_output(command_result(7, b"/repo\n"), "fixture path")
        .expect_err("nonzero command status must fail before path decoding");
    assert!(error.to_string().contains("failed with exit status: 7"));
    verify::command_output(command_result(0, b"/repo/\xff\n"), "fixture text")
        .expect_err("text output must reject non-UTF-8 bytes");
}

#[test]
fn rustup_path_resolution_handles_actual_raw_unix_paths() {
    let temporary = tempfile::tempdir().expect("temporary directory");
    #[cfg(target_os = "macos")]
    let raw_component = OsString::from(" rustup \t\r\n ");
    #[cfg(not(target_os = "macos"))]
    let raw_component = OsString::from_vec(b" rustup \t\r\n\xff ".to_vec());
    let rustup_home = temporary.path().join(raw_component);
    let toolchain_bin = rustup_home.join("toolchains/fixture-pin/bin");
    fs::create_dir_all(&toolchain_bin).expect("raw path toolchain directory");
    for name in ["rustc", "rustdoc", "clippy-driver", "rustfmt"] {
        write_executable(&toolchain_bin.join(name), b"fixture tool");
    }
    let rustup = temporary.path().join("rustup");
    let rustup_script = concat!(
        "#!/bin/sh\n",
        "set -eu\n",
        "[ \"$1\" = which ] && [ \"$2\" = --toolchain ] && [ \"$3\" = fixture-pin ] || exit 73\n",
        "path=\"$RUSTUP_HOME/toolchains/$3/bin/$4\"\n",
        "[ -f \"$path\" ] || exit 74\n",
        "printf '%s\\n' \"$path\"\n",
    );
    write_executable(&rustup, rustup_script.as_bytes());

    let resolved = resolve_rust_tools(
        &rustup,
        "fixture-pin",
        OsStr::new("/inherited/path"),
        Some(rustup_home.as_os_str()),
        None,
    )
    .expect("resolve raw Unix tool paths");

    for name in ["rustc", "rustdoc", "clippy-driver", "rustfmt"] {
        assert_eq!(
            resolved[name],
            fs::canonicalize(toolchain_bin.join(name)).expect("physical raw tool path")
        );
    }
}

#[test]
fn managed_mise_gate_discards_ambient_config_overrides_and_preserves_gate_context() {
    let trusted_root = Path::new("/private/staged/repository");
    let mut environment = BTreeMap::from([
        (
            OsString::from("MISE_ENV"),
            OsString::from("attacker-profile"),
        ),
        (
            OsString::from("MISE_ENV_FILE"),
            OsString::from("/tmp/attacker.env"),
        ),
        (
            OsString::from("MISE_CONFIG_DIR"),
            OsString::from("/tmp/attacker-config"),
        ),
        (
            OsString::from("MISE_CONFIG_FILE"),
            OsString::from("/tmp/attacker.toml"),
        ),
        (
            OsString::from("MISE_GLOBAL_CONFIG_FILE"),
            OsString::from("/tmp/attacker-global.toml"),
        ),
        (
            OsString::from("MISE_DATA_DIR"),
            OsString::from("/tmp/attacker-data"),
        ),
        (
            OsString::from("MBX_CACHE_DIR"),
            OsString::from("/cache/mbx"),
        ),
        (
            OsString::from("MBX_SESSION_PATH"),
            OsString::from("/session/mbx"),
        ),
        (OsString::from("HK_LOG_FORMAT"), OsString::from("json")),
        (OsString::from("PATH"), OsString::from("/qualified/path")),
    ]);

    configure_managed_mise_environment(&mut environment, trusted_root);

    assert_eq!(
        environment
            .keys()
            .filter(|key| key.to_string_lossy().starts_with("MISE_"))
            .cloned()
            .collect::<BTreeSet<_>>(),
        BTreeSet::from([
            OsString::from("MISE_AUTO_INSTALL"),
            OsString::from("MISE_EXEC_AUTO_INSTALL"),
            OsString::from("MISE_TRUSTED_CONFIG_PATHS"),
        ])
    );
    assert_eq!(
        environment.get(OsStr::new("MISE_TRUSTED_CONFIG_PATHS")),
        Some(&trusted_root.as_os_str().to_owned())
    );
    assert_eq!(
        environment.get(OsStr::new("MISE_AUTO_INSTALL")),
        Some(&OsString::from("0"))
    );
    assert_eq!(
        environment.get(OsStr::new("MISE_EXEC_AUTO_INSTALL")),
        Some(&OsString::from("false"))
    );
    for key in ["MBX_CACHE_DIR", "MBX_SESSION_PATH", "HK_LOG_FORMAT", "PATH"] {
        assert!(environment.contains_key(OsStr::new(key)), "lost {key}");
    }
}

#[test]
fn cargo_path_selects_the_qualified_mbx_wrapper_before_rustup_cargo() {
    let temporary = tempfile::tempdir().unwrap();
    let managed = temporary.path().join("managed");
    let rust_bin = temporary.path().join("rust-bin");
    fs::create_dir(&managed).unwrap();
    fs::create_dir(&rust_bin).unwrap();
    let mbx_cargo = temporary.path().join("mbx-cargo");
    write_executable(&mbx_cargo, b"MBX wrapper");
    write_executable(&rust_bin.join("cargo"), b"rustup cargo");
    symlink(&mbx_cargo, managed.join("cargo")).unwrap();
    let path = env::join_paths([&managed, &rust_bin]).unwrap();
    let selected = find_executable("cargo", &path).unwrap();
    assert_eq!(
        fs::canonicalize(selected).unwrap(),
        fs::canonicalize(mbx_cargo).unwrap()
    );
}

#[test]
fn pinned_rustup_resolution_preserves_only_the_admitted_rustup_home() {
    let temporary = tempfile::tempdir().expect("temporary directory");
    let rustup_home = temporary.path().join("custom rustup home");
    let toolchain_bin = rustup_home.join("toolchains/fixture-pin/bin");
    fs::create_dir_all(&toolchain_bin).expect("custom rustup toolchain directory");
    for name in ["rustc", "rustdoc", "clippy-driver", "rustfmt"] {
        write_executable(&toolchain_bin.join(name), b"fixture tool");
    }
    let rustup = temporary.path().join("rustup");
    let rustup_script = concat!(
        "#!/bin/sh\n",
        "set -eu\n",
        "if [ -n \"${RUSTUP_HOME:-}\" ]; then\n",
        "  [ \"${HOME+x}\" != x ] || exit 71\n",
        "else\n",
        "  [ -n \"${HOME:-}\" ] || exit 70\n",
        "  RUSTUP_HOME=\"$HOME/.rustup\"\n",
        "fi\n",
        "[ \"${RUSTUP_TOOLCHAIN+x}\" != x ] || exit 72\n",
        "[ \"$1\" = which ] && [ \"$2\" = --toolchain ] && [ \"$3\" = fixture-pin ] || exit 73\n",
        "path=\"$RUSTUP_HOME/toolchains/$3/bin/$4\"\n",
        "[ -f \"$path\" ] || exit 74\n",
        "printf '%s\\n' \"$path\"\n",
    );
    write_executable(&rustup, rustup_script.as_bytes());

    let resolved = resolve_rust_tools(
        &rustup,
        "fixture-pin",
        OsStr::new("/inherited/path"),
        Some(rustup_home.as_os_str()),
        Some(OsStr::new("/unselected/home")),
    )
    .expect("resolve tools from the admitted custom rustup root");

    assert_eq!(resolved.len(), 4);
    for name in ["rustc", "rustdoc", "clippy-driver", "rustfmt"] {
        assert_eq!(
            resolved[name],
            fs::canonicalize(toolchain_bin.join(name)).expect("physical fixture tool")
        );
    }
    let home = temporary.path().join("default rustup home");
    fs::create_dir(&home).expect("custom HOME");
    symlink(&rustup_home, home.join(".rustup")).expect("HOME-selected rustup root");
    let default_resolved = resolve_rust_tools(
        &rustup,
        "fixture-pin",
        OsStr::new("/inherited/path"),
        None,
        Some(home.as_os_str()),
    )
    .expect("resolve tools from the admitted HOME without RUSTUP_HOME");
    assert_eq!(default_resolved, resolved);
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
    write_executable(&pinned, b"Mise-pinned SwiftLint 0.65.1");
    let ambient_tool = ambient.join("swiftlint");
    write_executable(&ambient_tool, b"ambient SwiftLint");
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

#[test]
fn local_guard_in_home_is_admitted_and_qualified_without_inherited_path_entry() {
    let temporary = tempfile::tempdir().expect("temporary directory");
    let home = temporary.path().join("home");
    let local_bin = home.join(".local/bin");
    let inherited_bin = temporary.path().join("inherited");
    let managed = temporary.path().join("managed");
    let rust_bin = temporary.path().join("rust-bin");
    for directory in [&local_bin, &inherited_bin, &managed, &rust_bin] {
        fs::create_dir_all(directory).expect("tool directory");
    }
    let guard = local_bin.join("rust-build-guard");
    write_executable(&guard, b"trusted guard");
    let inherited = env::join_paths([&inherited_bin]).expect("inherited PATH");

    let resolved = resolve_guard(&inherited, Some(&home)).expect("home guard fallback");
    assert_eq!(
        resolved,
        fs::canonicalize(&guard).expect("physical guard path")
    );
    symlink(&resolved, managed.join("rust-build-guard")).expect("managed guard link");
    let qualified = qualified_path(&managed, &rust_bin, &inherited).expect("qualified PATH");
    let selected = find_executable("rust-build-guard", &qualified).expect("qualified guard");
    assert_eq!(
        fs::canonicalize(selected).expect("selected physical guard"),
        resolved
    );
}

#[test]
fn missing_local_guard_fails_closed() {
    let temporary = tempfile::tempdir().expect("temporary directory");
    let home = temporary.path().join("home");
    let inherited_bin = temporary.path().join("inherited");
    fs::create_dir_all(&home).expect("home directory");
    fs::create_dir_all(&inherited_bin).expect("inherited PATH directory");
    let inherited = env::join_paths([&inherited_bin]).expect("inherited PATH");

    let error = resolve_guard(&inherited, Some(&home)).expect_err("missing guard must fail");
    assert!(
        error
            .to_string()
            .contains("rust-build-guard is not installed")
    );
}

#[test]
fn inherited_path_guard_precedes_the_home_fallback() {
    let temporary = tempfile::tempdir().expect("temporary directory");
    let home = temporary.path().join("home");
    let home_bin = home.join(".local/bin");
    let inherited_bin = temporary.path().join("inherited");
    fs::create_dir_all(&home_bin).expect("home guard directory");
    fs::create_dir_all(&inherited_bin).expect("inherited guard directory");
    let home_guard = home_bin.join("rust-build-guard");
    let path_guard = inherited_bin.join("rust-build-guard");
    write_executable(&home_guard, b"home guard");
    write_executable(&path_guard, b"PATH guard");
    let inherited = env::join_paths([&inherited_bin]).expect("inherited PATH");

    let resolved = resolve_guard(&inherited, Some(&home)).expect("PATH guard");
    assert_eq!(
        resolved,
        fs::canonicalize(path_guard).expect("physical PATH guard")
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
