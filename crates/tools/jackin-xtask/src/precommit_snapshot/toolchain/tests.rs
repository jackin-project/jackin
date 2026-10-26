// SPDX-FileCopyrightText: 2026 The jackin❯ Authors
// SPDX-License-Identifier: Apache-2.0

use super::*;
use std::collections::BTreeSet;
#[cfg(not(target_os = "macos"))]
use std::os::unix::ffi::OsStringExt;
use std::os::unix::{ffi::OsStrExt, fs::symlink, process::ExitStatusExt};
use std::process::{ExitStatus, Output};
use std::sync::{Arc, atomic::AtomicBool};

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
            OsString::from("MISE_CACHE_DIR"),
            OsString::from("/tmp/attacker-cache"),
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
            OsString::from("MISE_CACHE_DIR"),
            OsString::from("MISE_DATA_DIR"),
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
    assert_eq!(
        environment.get(OsStr::new("MISE_DATA_DIR")),
        Some(&OsString::from("/tmp/attacker-data"))
    );
    assert_eq!(
        environment.get(OsStr::new("MISE_CACHE_DIR")),
        Some(&OsString::from("/tmp/attacker-cache"))
    );
    for key in ["MBX_CACHE_DIR", "MBX_SESSION_PATH", "HK_LOG_FORMAT", "PATH"] {
        assert!(environment.contains_key(OsStr::new(key)), "lost {key}");
    }
}

#[test]
fn path_output_removes_only_one_terminal_protocol_lf() {
    let output = command_result(0, b"/tmp/path \xff with spaces\nembedded\n\n");
    let path = command_path(output, "reading path").expect("raw path record");
    assert_eq!(
        path.as_os_str().as_bytes(),
        b"/tmp/path \xff with spaces\nembedded\n"
    );
}

#[test]
fn path_output_rejects_missing_terminator_and_nul() {
    for (bytes, reason) in [
        (
            b"/tmp/path".to_vec(),
            "path record is missing its LF terminator",
        ),
        (b"/tmp\0/path\n".to_vec(), "path record contains NUL"),
        (b"\n".to_vec(), "empty path record"),
    ] {
        let output = command_result(0, &bytes);
        command_path(output, "reading path").expect_err(reason);
    }

    let error = command_path(command_result(7, b"/repo\n"), "reading path")
        .expect_err("nonzero command must fail before path decoding");
    assert!(error.to_string().contains("failed with exit status: 7"));
    verify::command_output(command_result(0, b"/repo/\xff\n"), "fixture text")
        .expect_err("text output must reject non-UTF-8 bytes");
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
fn inherited_path_anchors_empty_and_relative_entries_and_preserves_order() {
    let temporary = tempfile::tempdir().expect("hook directory");
    let hook_cwd = fs::canonicalize(temporary.path()).expect("physical hook directory");
    let absolute = PathBuf::from("/absolute/inherited/bin");
    let inherited = env::join_paths([
        PathBuf::new(),
        PathBuf::from("relative/bin"),
        absolute.clone(),
        PathBuf::new(),
    ])
    .expect("inherited PATH with empty components");

    let anchored = anchor_inherited_path(&hook_cwd, &inherited);
    assert_eq!(
        anchored,
        vec![
            hook_cwd.clone(),
            hook_cwd.join("relative/bin"),
            absolute,
            hook_cwd,
        ]
    );
}

#[test]
fn inherited_path_aliases_colon_cwd_entries_without_changing_order() {
    let temporary = tempfile::tempdir().expect("hook parent");
    let hook_cwd = temporary.path().join("hook:cwd");
    fs::create_dir(&hook_cwd).expect("colon-containing hook directory");
    let hook_cwd = fs::canonicalize(hook_cwd).expect("physical colon hook directory");
    let inherited = env::join_paths([PathBuf::new(), PathBuf::from("relative/bin")])
        .expect("empty and relative PATH entries");
    let anchored = anchor_inherited_path(&hook_cwd, &inherited);
    let aliases = tempfile::tempdir().expect("separator-safe alias directory");
    let aliases_fd = open_private_tool_directory(aliases.path()).expect("open alias directory");
    populate_inherited_path_aliases(&aliases_fd, &anchored)
        .expect("populate inherited delimiter aliases");

    let qualified =
        qualified_path_entries(aliases.path(), &anchored).expect("qualified delimiter-safe PATH");
    let entries = env::split_paths(&qualified).collect::<Vec<_>>();
    assert_eq!(
        entries,
        vec![
            aliases.path().to_path_buf(),
            inherited_path_alias(aliases.path(), 0),
            inherited_path_alias(aliases.path(), 1),
        ]
    );
    assert_eq!(
        fs::read_link(&entries[1]).expect("empty PATH alias target"),
        hook_cwd
    );
    assert_eq!(
        fs::read_link(&entries[2]).expect("relative PATH alias target"),
        hook_cwd.join("relative/bin")
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
fn rust_release_reads_a_bounded_regular_target_and_preserves_staged_symlinks() {
    let temporary = tempfile::tempdir().expect("temporary directory");
    let target = temporary.path().join("toolchain.toml");
    let link = temporary.path().join("rust-toolchain.toml");
    fs::write(&target, "[toolchain]\nchannel = \"1.99.1\"\n").expect("toolchain pin");
    symlink(&target, &link).expect("staged toolchain link");

    let pin = rust_release(
        &link,
        &SetupControls::for_test(std::time::Duration::from_secs(10)),
    )
    .expect("read pin through admitted staged symlink");
    assert_eq!(pin, "1.99.1");
}

#[test]
fn rust_release_rejects_control_files_larger_than_one_megabyte() {
    let temporary = tempfile::tempdir().expect("temporary directory");
    let path = temporary.path().join("rust-toolchain.toml");
    let mut contents = b"[toolchain]\nchannel = \"1.99.1\"\n#".to_vec();
    contents.resize(1024 * 1024 + 1, b'x');
    fs::write(&path, contents).expect("oversized staged toolchain file");

    let error = rust_release(
        &path,
        &SetupControls::for_test(std::time::Duration::from_secs(10)),
    )
    .expect_err("oversized toolchain metadata must fail before parsing");
    assert!(error.to_string().contains("exceeds its byte limit"));
}

#[test]
fn rust_release_checks_cancellation_before_reading() {
    let temporary = tempfile::tempdir().expect("temporary directory");
    let path = temporary.path().join("rust-toolchain.toml");
    fs::write(&path, "[toolchain]\nchannel = \"1.99.1\"\n").expect("toolchain pin");
    let controls = SetupControls::new(
        Arc::new(AtomicBool::new(true)),
        std::time::Duration::from_secs(10),
    );

    let error =
        rust_release(&path, &controls).expect_err("cancelled setup must stop metadata read");
    assert!(error.to_string().contains("cancelled"));
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
    let inherited = env::split_paths(&inherited).collect::<Vec<_>>();
    let path_aliases = tempfile::tempdir().expect("separator-safe PATH directory");
    let path_aliases_fd =
        open_private_tool_directory(path_aliases.path()).expect("open PATH aliases");
    populate_path_aliases(&path_aliases_fd, &managed, &BTreeMap::new())
        .expect("populate separator-safe PATH directory");
    let path = qualified_path_entries(path_aliases.path(), &inherited).expect("qualified PATH");
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
    let path_aliases = tempfile::tempdir().expect("separator-safe PATH directory");
    let path_aliases_fd =
        open_private_tool_directory(path_aliases.path()).expect("open PATH aliases");
    populate_path_aliases(&path_aliases_fd, &managed, &BTreeMap::new())
        .expect("populate separator-safe PATH directory");
    let inherited_entries = env::split_paths(&inherited).collect::<Vec<_>>();
    let qualified =
        qualified_path_entries(path_aliases.path(), &inherited_entries).expect("qualified PATH");
    let selected = find_executable("rust-build-guard", &qualified).expect("qualified guard");
    assert_eq!(
        fs::canonicalize(selected).expect("selected physical guard"),
        resolved
    );
}

#[test]
fn colon_paths_use_private_separator_safe_aliases_and_run_qualified_tools() {
    let temporary = tempfile::tempdir().expect("temporary directory");
    let requested_tmp = temporary.path().join("requested:tmp");
    let fallback = temporary.path().join("safe");
    let managed = requested_tmp.join("managed tools");
    let rust_bin = requested_tmp.join("rust toolchain/bin");
    fs::create_dir_all(&managed).expect("managed tool directory");
    fs::create_dir_all(&rust_bin).expect("colon-containing Rust bin directory");
    fs::create_dir_all(&fallback).expect("separator-free fallback root");
    let cargo_marker = temporary.path().join("cargo-provider");
    let rustc_marker = temporary.path().join("rustc-provider");
    let cargo = managed.join("cargo");
    let rustc = rust_bin.join("rustc");
    write_executable(&cargo, b"#!/bin/sh\nprintf 'mbx-cargo\\n' > \"$CARGO_PROVIDER_MARKER\"\nprintf 'cargo 1.99.1 (fixture)\\n'\n");
    write_executable(&rustc, b"#!/bin/sh\nprintf 'pinned-rustc\\n' > \"$RUSTC_PROVIDER_MARKER\"\nprintf 'rustc 1.99.1 (fixture)\\n'\n");

    let aliases = create_path_aliases(&requested_tmp, &fallback).expect("separator-safe aliases");
    let aliases_fd =
        open_private_tool_directory(aliases.path()).expect("open separator-safe aliases");
    let rust_tools = BTreeMap::from([("rustc", rustc.clone())]);
    let inherited = env::var_os("PATH").unwrap_or_default();
    let inherited = env::split_paths(&inherited).collect::<Vec<_>>();
    populate_inherited_path_aliases(&aliases_fd, &inherited)
        .expect("populate inherited PATH aliases");
    populate_path_aliases(&aliases_fd, &managed, &rust_tools)
        .expect("populate qualified tool aliases");
    let path = qualified_path_entries(aliases.path(), &inherited).expect("qualified PATH");
    assert!(
        !aliases
            .path()
            .as_os_str()
            .as_encoded_bytes()
            .contains(&b':')
    );
    assert_eq!(
        env::split_paths(&path).next().expect("first PATH entry"),
        aliases.path()
    );

    let cargo_output = super::super::process::capture(
        Command::new("cargo")
            .arg("--version")
            .env("PATH", &path)
            .env("CARGO_PROVIDER_MARKER", &cargo_marker),
    )
    .expect("run aliased MBX Cargo provider");
    assert!(cargo_output.status.success());
    assert_eq!(
        fs::read(&cargo_marker).expect("Cargo provider marker"),
        b"mbx-cargo\n"
    );
    assert!(
        String::from_utf8(cargo_output.stdout)
            .expect("Cargo provider output")
            .starts_with("cargo 1.99.1")
    );

    let rustc_output = super::super::process::capture(
        Command::new("rustc")
            .arg("--version")
            .env("PATH", &path)
            .env("RUSTC_PROVIDER_MARKER", &rustc_marker),
    )
    .expect("run aliased pinned rustc provider");
    assert!(rustc_output.status.success());
    assert_eq!(
        fs::read(&rustc_marker).expect("rustc provider marker"),
        b"pinned-rustc\n"
    );
    assert!(
        String::from_utf8(rustc_output.stdout)
            .expect("rustc provider output")
            .starts_with("rustc 1.99.1")
    );

    let alias_path = aliases.path().to_path_buf();
    drop(aliases);
    assert!(!alias_path.exists(), "private PATH aliases are removed");
    assert!(
        requested_tmp.is_dir(),
        "requested scratch root remains available"
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
fn colon_path_alias_fallback_rejects_unsafe_parent_before_creating_child() {
    let temporary = tempfile::tempdir().expect("temporary directory");
    let requested_tmp = temporary.path().join("requested:tmp");
    let fallback = temporary.path().join("fallback");
    fs::create_dir(&requested_tmp).expect("colon-containing preferred temporary directory");
    fs::create_dir(&fallback).expect("fallback directory");
    fs::set_permissions(&fallback, fs::Permissions::from_mode(0o777))
        .expect("make fallback parent writable by others");

    let result = create_path_aliases(&requested_tmp, &fallback);
    let children = crate::fs_util::read_dir_sorted(&fallback)
        .expect("read fallback after validation")
        .len();
    fs::set_permissions(&fallback, fs::Permissions::from_mode(0o700))
        .expect("restore private fallback mode");

    assert!(result.is_err(), "unsafe fallback parent must be rejected");
    assert_eq!(
        children, 0,
        "no alias child may be created under an unsafe parent"
    );
}

#[test]
fn colon_path_alias_fallback_accepts_root_sticky_tmp_and_cleans_up() {
    let temporary = tempfile::tempdir().expect("temporary directory");
    let requested_tmp = temporary.path().join("requested:tmp");
    let fallback = fs::canonicalize("/tmp").expect("canonical standard temporary directory");
    fs::create_dir(&requested_tmp).expect("colon-containing preferred temporary directory");

    let aliases =
        create_path_aliases(&requested_tmp, &fallback).expect("root-owned sticky fallback parent");
    let alias_path = aliases.path().to_path_buf();
    assert_eq!(alias_path.parent(), Some(fallback.as_path()));

    drop(aliases);
    assert!(
        !alias_path.exists(),
        "temporary aliases must be removed on drop"
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
