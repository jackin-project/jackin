use std::ffi::{OsStr, OsString};
use std::fmt::Write as _;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, Result, ensure};

use super::{COMPONENTS, Environment, command_tools, host, library_name, run_with, version};

const CHILD_MODE: &str = "JACKIN_CUSTOM_LINTS_RELATIVE_TMPDIR_CHILD";
const CHILD_ROOT: &str = "JACKIN_CUSTOM_LINTS_RELATIVE_TMPDIR_ROOT";
const CHILD_TOOLS: &str = "JACKIN_CUSTOM_LINTS_RELATIVE_TMPDIR_TOOLS";
const CHILD_MARKER: &str = "JACKIN_CUSTOM_LINTS_RELATIVE_TMPDIR_MARKER";
const CHILD_LIBRARY: &str = "JACKIN_CUSTOM_LINTS_RELATIVE_TMPDIR_LIBRARY";
const CHILD_CWD: &str = "JACKIN_CUSTOM_LINTS_RELATIVE_TMPDIR_CWD";

#[test]
fn private_temp_paths_work_after_child_changes_directory_with_relative_tmpdir() {
    if std::env::var_os(CHILD_MODE).is_some() {
        run_relative_tmpdir_child().expect("relative TMPDIR pipeline");
        return;
    }

    let fixture = tempfile::tempdir().expect("fixture directory");
    let fixture_root = fs::canonicalize(fixture.path()).expect("physical fixture directory");
    let launch = fixture_root.join("launch");
    let root = fixture_root.join("repository");
    let child_cwd = root.join("different-child-cwd");
    let relative_temp = launch.join("relative-temp");
    let tools = fixture_root.join("pinned tools");
    let marker = fixture_root.join("observed shim invocation");
    fs::create_dir_all(&relative_temp).expect("relative TMPDIR target");
    fs::create_dir_all(root.join("crates/tools/jackin-lints")).expect("lint crate directory");
    fs::create_dir_all(&child_cwd).expect("different child working directory");
    command_tools(&tools);
    write_executable(
        &tools.join("mbx"),
        b"#!/bin/sh\nset -eu\ntest \"${1-}\" = --shim-probe\nprintf '%s\\n%s\\n' \"$PWD\" \"$MBX_CARGO_SHIM_PATH\" > \"$JACKIN_CUSTOM_LINTS_RELATIVE_TMPDIR_MARKER\"\n",
    );
    write_executable(
        &tools.join("cargo-dylint"),
        b"#!/bin/sh\nset -eu\ncd \"$JACKIN_CUSTOM_LINTS_RELATIVE_TMPDIR_CWD\"\ntest -d \"$DYLINT_LIBRARY_PATH\"\ntest -f \"$DYLINT_LIBRARY_PATH/$JACKIN_CUSTOM_LINTS_RELATIVE_TMPDIR_LIBRARY\"\ncargo --shim-probe\n",
    );

    let status = Command::new(std::env::current_exe().expect("test executable"))
        .args([
            "private_temp_paths_work_after_child_changes_directory_with_relative_tmpdir",
            "--nocapture",
        ])
        .current_dir(&launch)
        .env("TMPDIR", "relative-temp")
        .env(CHILD_MODE, "1")
        .env(CHILD_ROOT, &root)
        .env(CHILD_TOOLS, &tools)
        .env(CHILD_MARKER, &marker)
        .env(CHILD_LIBRARY, library_name(&host()))
        .env(CHILD_CWD, &child_cwd)
        .status()
        .expect("spawn isolated test process");
    assert!(status.success(), "relative TMPDIR child failed: {status}");

    let observed = fs::read_to_string(&marker).expect("pinned MBX shim invocation");
    let mut lines = observed.lines();
    assert_eq!(
        lines.next(),
        Some(child_cwd.to_str().expect("UTF-8 fixture path"))
    );
    let shim = PathBuf::from(lines.next().expect("shim path"));
    assert!(shim.is_absolute(), "MBX Cargo shim path must be absolute");
    assert!(
        shim.starts_with(&relative_temp),
        "shim must remain inside the relative TMPDIR fixture: {shim:?}"
    );
    assert!(lines.next().is_none(), "unexpected shim output");
    assert!(
        !shim.exists(),
        "private Cargo shim directory must be removed after the pipeline"
    );
}

fn run_relative_tmpdir_child() -> Result<()> {
    let root = PathBuf::from(std::env::var_os(CHILD_ROOT).context("fixture root")?);
    let tools = PathBuf::from(std::env::var_os(CHILD_TOOLS).context("fixture tools")?);
    let pinned_dylint = fs::canonicalize(tools.join("cargo-dylint"))?.into_os_string();
    let host = host();
    let mut inherited = Environment::new();
    inherited.insert(OsString::from("PATH"), tools.as_os_str().to_owned());
    inherited.insert(OsString::from("HOME"), root.as_os_str().to_owned());
    inherited.insert(
        OsString::from(CHILD_MARKER),
        std::env::var_os(CHILD_MARKER).context("fixture marker")?,
    );
    inherited.insert(
        OsString::from(CHILD_LIBRARY),
        std::env::var_os(CHILD_LIBRARY).context("fixture library")?,
    );
    inherited.insert(
        OsString::from(CHILD_CWD),
        std::env::var_os(CHILD_CWD).context("fixture child working directory")?,
    );

    run_with(&root, inherited, |argv, cwd, env, _| {
        if argv.ends_with(&[OsString::from("rustc"), OsString::from("-Vv")]) {
            return Ok(version(&host).into_bytes());
        }
        if argv.ends_with(&[OsString::from("--installed")]) {
            let mut installed = String::new();
            for component in COMPONENTS {
                writeln!(installed, "{component}-{host}")?;
            }
            return Ok(installed.into_bytes());
        }
        if argv.ends_with(&[OsString::from("build"), OsString::from("--locked")]) {
            let target = PathBuf::from(
                env.get(OsStr::new("CARGO_TARGET_DIR"))
                    .context("library build target")?,
            );
            let library = target.join("debug").join(library_name(&host));
            fs::create_dir_all(library.parent().context("library parent")?)?;
            fs::write(library, b"qualified lint library")?;
            return Ok(Vec::new());
        }
        if argv.first() == Some(&pinned_dylint) {
            let status = Command::new(&argv[0])
                .args(&argv[1..])
                .current_dir(cwd)
                .env_clear()
                .envs(env.iter())
                .status()
                .context("run Dylint gate from its selected working directory")?;
            ensure!(status.success(), "Dylint gate fixture exited with {status}");
        }
        Ok(Vec::new())
    })?;
    Ok(())
}

fn write_executable(path: &Path, bytes: &[u8]) {
    use std::os::unix::fs::PermissionsExt;

    fs::write(path, bytes).expect("write executable fixture");
    fs::set_permissions(path, fs::Permissions::from_mode(0o700))
        .expect("set executable fixture mode");
}
