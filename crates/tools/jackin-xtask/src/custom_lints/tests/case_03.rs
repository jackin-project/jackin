use std::ffi::{OsStr, OsString};
use std::fmt::Write as _;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, Result, ensure};

use super::{COMPONENTS, Environment, command_tools, host, library_name, run_with, version};

const CHILD_MODE: &str = "JACKIN_CUSTOM_LINTS_COLON_PATH_CHILD";
const CHILD_ROOT: &str = "JACKIN_CUSTOM_LINTS_COLON_PATH_ROOT";
const CHILD_TOOLS: &str = "JACKIN_CUSTOM_LINTS_COLON_PATH_TOOLS";
const CHILD_TMPDIR: &str = "JACKIN_CUSTOM_LINTS_COLON_PATH_TMPDIR";
const CHILD_CARGO_HOME: &str = "JACKIN_CUSTOM_LINTS_COLON_PATH_CARGO_HOME";
const CHILD_PATH_MARKER: &str = "JACKIN_CUSTOM_LINTS_COLON_PATH_PATH_MARKER";
const CHILD_MBX_MARKER: &str = "JACKIN_CUSTOM_LINTS_COLON_PATH_MBX_MARKER";
const CHILD_NEXTEST_MARKER: &str = "JACKIN_CUSTOM_LINTS_COLON_PATH_NEXTEST_MARKER";
const CHILD_CARGO_HOME_MARKER: &str = "JACKIN_CUSTOM_LINTS_COLON_PATH_CARGO_HOME_MARKER";

#[test]
fn custom_lint_cargo_shim_aliases_colon_paths_and_preserves_provider_order() {
    if std::env::var_os(CHILD_MODE).is_some() {
        run_colon_path_child().expect("colon PATH custom-lint pipeline");
        return;
    }

    let fixture = tempfile::tempdir().expect("fixture directory");
    let fixture_root = fs::canonicalize(fixture.path()).expect("physical fixture directory");
    let root = fixture_root.join("repository");
    let tools = fixture_root.join("pinned tools");
    let tempdir = fixture_root.join("temp:directory");
    let cargo_home = fixture_root.join("cargo:home");
    let nextest_parent = fixture_root.join("nextest:bin");
    let path_marker = fixture_root.join("observed PATH");
    let mbx_marker = fixture_root.join("MBX provider");
    let nextest_marker = fixture_root.join("nextest provider");
    let cargo_home_marker = fixture_root.join("Cargo home provider");
    fs::create_dir_all(root.join("crates/tools/jackin-lints")).expect("lint crate directory");
    fs::create_dir_all(&tempdir).expect("colon TMPDIR directory");
    fs::create_dir_all(cargo_home.join("bin")).expect("colon Cargo home bin");
    fs::create_dir_all(&nextest_parent).expect("colon nextest tool parent");
    command_tools(&tools);
    write_executable(
        &nextest_parent.join("cargo-nextest"),
        b"#!/bin/sh\nprintf 'nextest\\n' > \"$JACKIN_CUSTOM_LINTS_COLON_PATH_NEXTEST_MARKER\"\n",
    );
    fs::remove_file(tools.join("cargo-nextest")).expect("remove ordinary nextest fixture");
    std::os::unix::fs::symlink(
        nextest_parent.join("cargo-nextest"),
        tools.join("cargo-nextest"),
    )
    .expect("select nextest from colon-containing tool parent");
    write_executable(
        &cargo_home.join("bin/cargo-home-probe"),
        b"#!/bin/sh\nprintf 'cargo-home\\n' > \"$JACKIN_CUSTOM_LINTS_COLON_PATH_CARGO_HOME_MARKER\"\n",
    );
    write_executable(
        &tools.join("mbx"),
        b"#!/bin/sh\n[ \"${1-}\" = --shim-probe ] || exit 91\nprintf '%s\\n%s\\n' \"$MBX_CARGO_SHIM_MODE\" \"$MBX_CARGO_SHIM_PATH\" > \"$JACKIN_CUSTOM_LINTS_COLON_PATH_MBX_MARKER\"\n",
    );
    write_executable(
        &tools.join("cargo-dylint"),
        b"#!/bin/sh\nset -eu\npath_rest=${PATH#*:}\nnextest_entry=${path_rest%%:*}\npath_rest=${path_rest#*:}\ncargo_home_entry=${path_rest%%:*}\nprintf '%s\\n' \"$PATH\" > \"$JACKIN_CUSTOM_LINTS_COLON_PATH_PATH_MARKER\"\n/usr/bin/readlink \"$nextest_entry\" >> \"$JACKIN_CUSTOM_LINTS_COLON_PATH_PATH_MARKER\"\n/usr/bin/readlink \"$cargo_home_entry\" >> \"$JACKIN_CUSTOM_LINTS_COLON_PATH_PATH_MARKER\"\ncargo --shim-probe\ncargo-nextest --shim-probe\ncargo-home-probe\n",
    );

    let status = Command::new(std::env::current_exe().expect("current test executable"))
        .args([
            "custom_lint_cargo_shim_aliases_colon_paths_and_preserves_provider_order",
            "--nocapture",
        ])
        .env(CHILD_MODE, "1")
        .env(CHILD_ROOT, &root)
        .env(CHILD_TOOLS, &tools)
        .env(CHILD_TMPDIR, &tempdir)
        .env("TMPDIR", &tempdir)
        .env(CHILD_CARGO_HOME, &cargo_home)
        .env(CHILD_PATH_MARKER, &path_marker)
        .env(CHILD_MBX_MARKER, &mbx_marker)
        .env(CHILD_NEXTEST_MARKER, &nextest_marker)
        .env(CHILD_CARGO_HOME_MARKER, &cargo_home_marker)
        .status()
        .expect("spawn isolated colon PATH test process");
    assert!(status.success(), "colon PATH child failed: {status}");

    let path_record = fs::read_to_string(&path_marker).expect("captured shim PATH");
    let mut path_lines = path_record.lines();
    let path = std::env::split_paths(OsStr::new(path_lines.next().expect("captured PATH line")))
        .collect::<Vec<_>>();
    assert_eq!(path.len(), 4, "Cargo PATH projection: {path:?}");
    assert!(
        path.iter()
            .all(|entry| !entry.as_os_str().as_encoded_bytes().contains(&b':')),
        "every injected PATH entry is delimiter-safe: {path:?}"
    );
    assert_eq!(
        PathBuf::from(path_lines.next().expect("nextest alias target")),
        nextest_parent
    );
    assert_eq!(
        PathBuf::from(path_lines.next().expect("Cargo home alias target")),
        cargo_home.join("bin")
    );
    assert!(path_lines.next().is_none(), "unexpected PATH marker output");
    assert_eq!(
        path[3],
        fs::canonicalize(&tools).expect("original provider directory")
    );
    let mbx_output = fs::read_to_string(&mbx_marker).expect("MBX provider marker");
    let mut mbx_lines = mbx_output.lines();
    assert_eq!(mbx_lines.next(), Some("1"));
    assert_eq!(
        mbx_lines.next().map(PathBuf::from),
        Some(path[0].join("cargo"))
    );
    assert!(mbx_lines.next().is_none());
    assert_eq!(
        fs::read(&nextest_marker).expect("nextest provider marker"),
        b"nextest\n"
    );
    assert_eq!(
        fs::read(&cargo_home_marker).expect("Cargo home provider marker"),
        b"cargo-home\n"
    );
    assert!(
        !path[0].exists(),
        "private shim and all PATH aliases are removed after the pipeline"
    );
}

fn run_colon_path_child() -> Result<()> {
    let root = PathBuf::from(std::env::var_os(CHILD_ROOT).context("fixture root")?);
    let tools = PathBuf::from(std::env::var_os(CHILD_TOOLS).context("fixture tools")?);
    let tmpdir = PathBuf::from(std::env::var_os(CHILD_TMPDIR).context("fixture TMPDIR")?);
    let cargo_home = PathBuf::from(std::env::var_os(CHILD_CARGO_HOME).context("Cargo home")?);
    let pinned_dylint = fs::canonicalize(tools.join("cargo-dylint"))?.into_os_string();
    let host = host();
    let mut inherited = Environment::new();
    inherited.insert(OsString::from("PATH"), tools.as_os_str().to_owned());
    inherited.insert(OsString::from("HOME"), root.as_os_str().to_owned());
    inherited.insert(OsString::from("CARGO_HOME"), cargo_home.into_os_string());
    for key in [
        CHILD_PATH_MARKER,
        CHILD_MBX_MARKER,
        CHILD_NEXTEST_MARKER,
        CHILD_CARGO_HOME_MARKER,
    ] {
        inherited.insert(
            OsString::from(key),
            std::env::var_os(key).context("fixture marker")?,
        );
    }

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
            let library_path = env
                .get(OsStr::new("DYLINT_LIBRARY_PATH"))
                .context("isolated Dylint library path")?;
            let library_dirs = std::env::split_paths(library_path).collect::<Vec<_>>();
            ensure!(
                library_dirs.len() == 1,
                "Dylint library path must be one entry"
            );
            let library_dir = &library_dirs[0];
            ensure!(
                library_dir.is_absolute() && library_dir.is_dir(),
                "Dylint library directory must exist and be absolute"
            );
            ensure!(
                !library_dir.as_os_str().as_encoded_bytes().contains(&b':'),
                "Dylint library directory must be delimiter-safe"
            );
            let shim_dir = std::env::split_paths(
                env.get(OsStr::new("PATH"))
                    .context("projected Cargo PATH")?,
            )
            .next()
            .context("private Cargo shim PATH entry")?;
            ensure!(
                library_dir.starts_with(shim_dir),
                "Dylint library directory must use the validated private parent"
            );
            let libraries = crate::fs_util::read_dir_sorted(library_dir)
                .context("reading isolated Dylint library directory")?;
            ensure!(
                libraries.len() == 1
                    && libraries[0].file_name().to_string_lossy() == library_name(&host),
                "isolated Dylint library must contain the qualified library"
            );
            let status = Command::new(&argv[0])
                .args(&argv[1..])
                .current_dir(cwd)
                .env_clear()
                .envs(env.iter())
                .env("TMPDIR", &tmpdir)
                .status()
                .context("run Dylint gate fixture with colon TMPDIR")?;
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
