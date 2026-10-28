// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

#![cfg(all(unix, any(target_os = "linux", target_os = "macos")))]

use std::{
    collections::BTreeMap,
    ffi::OsStr,
    fs,
    os::unix::fs::PermissionsExt,
    path::{Path, PathBuf},
    process::{Command, Output, Stdio},
};

use anyhow::{Context, Result, anyhow};
use tempfile::TempDir;

const COMPILER_VERSION_FIXTURE: &str = "commit-hash: 8006091eb71ae7d9e9bccb66c6122c9d68af4466\nrelease: 1.101.0-nightly\nhost: aarch64-apple-darwin\n";
const COMPLETE_COMPONENTS: &str = concat!(
    "clippy-aarch64-apple-darwin\n",
    "llvm-tools-aarch64-apple-darwin\n",
    "rustc-dev-aarch64-apple-darwin\n",
    "rustfmt-aarch64-apple-darwin\n",
);
const MISSING_COMPONENTS: &str = concat!(
    "clippy-aarch64-apple-darwin\n",
    "llvm-tools-aarch64-apple-darwin\n",
    "rustc-dev-aarch64-apple-darwin\n",
);
const FRESH_LIBRARY_BYTES: &str = "fresh qualified library";
const STALE_LIBRARY_BYTES: &str = "stale ABI library";
#[cfg(target_os = "linux")]
const FRESH_LIBRARY_NAME: &str = "libjackin_lints@nightly-2026-10-18-aarch64-apple-darwin.so";
#[cfg(target_os = "macos")]
const FRESH_LIBRARY_NAME: &str = "libjackin_lints@nightly-2026-10-18-aarch64-apple-darwin.dylib";
#[cfg(target_os = "linux")]
const STALE_LIBRARY_NAME: &str = "libjackin_lints@nightly-2026-10-17-aarch64-apple-darwin.so";
#[cfg(target_os = "macos")]
const STALE_LIBRARY_NAME: &str = "libjackin_lints@nightly-2026-10-17-aarch64-apple-darwin.dylib";
const RECEIPT_FUNCTION: &str = r#"record_receipt() {
    name=$1
    {
        printf 'cwd=%s\n' "$(pwd)"
        printf 'PATH=%s\n' "$PATH"
        printf 'CARGO_HOME=%s\n' "${CARGO_HOME-<unset>}"
        printf 'CARGO_TARGET_DIR=%s\n' "${CARGO_TARGET_DIR-<unset>}"
        printf 'MARKER=%s\n' "${CUSTOM_LINTS_TEST_MARKER-<unset>}"
        printf 'CARGO=%s\n' "${CARGO-<unset>}"
        printf 'RUSTUP_TOOLCHAIN=%s\n' "${RUSTUP_TOOLCHAIN-<unset>}"
        printf 'RUSTC=%s\n' "${RUSTC-<unset>}"
        printf 'RUSTDOC=%s\n' "${RUSTDOC-<unset>}"
        printf 'RUSTFLAGS=%s\n' "${RUSTFLAGS-<unset>}"
        printf 'RUSTC_WRAPPER=%s\n' "${RUSTC_WRAPPER-<unset>}"
        printf 'RUSTC_WORKSPACE_WRAPPER=%s\n' "${RUSTC_WORKSPACE_WRAPPER-<unset>}"
        printf 'DYLINT_DRIVER_PATH=%s\n' "${DYLINT_DRIVER_PATH-<unset>}"
        printf 'DYLINT_RUSTFLAGS=%s\n' "${DYLINT_RUSTFLAGS-<unset>}"
        printf 'DYLINT_LIBRARY_PATH=%s\n' "${DYLINT_LIBRARY_PATH-<unset>}"
        printf 'DYLINT_LINK_ENABLE_COPY_LIBRARY=%s\n' "${DYLINT_LINK_ENABLE_COPY_LIBRARY-<unset>}"
        printf 'CARGO_ENCODED_RUSTFLAGS=%s\n' "${CARGO_ENCODED_RUSTFLAGS-<unset>}"
    } > "$XTASK_TEST_RECEIPTS/$name.receipt"
}"#;

const RUSTUP_DISPATCH: &str = r#"
if [ "$1" = "run" ] && [ "$3" = "rustc" ]; then
    record_receipt rustc
    printf '%s' "$XTASK_TEST_COMPILER_VERSION"
    exit 0
fi
if [ "$1" = "component" ]; then
    record_receipt components
    printf '%s' "$XTASK_TEST_COMPONENTS"
    exit 0
fi
if [ "$1" = "run" ]; then
    shift 2
    exec "$@"
fi
printf 'unexpected rustup request\n' >&2
exit 90
"#;

const CARGO_SCRIPT: &str = r"#!/bin/sh
set -eu
";

struct Fixture {
    temp: TempDir,
    root: PathBuf,
    tools: PathBuf,
    cargo_home: PathBuf,
    receipts: PathBuf,
}

impl Fixture {
    fn new() -> Result<Self> {
        let temp = tempfile::tempdir()?;
        let root = temp.path().join("repository with spaces");
        let tools = temp.path().join("tools with spaces");
        let cargo_home = temp.path().join("cargo home");
        let receipts = temp.path().join("child receipts");
        fs::create_dir_all(root.join("docs/content"))?;
        fs::create_dir_all(root.join("crates/tools/jackin-lints"))?;
        fs::create_dir_all(cargo_home.join("bin"))?;
        fs::create_dir_all(&tools)?;
        fs::create_dir_all(&receipts)?;

        let rustup = format!("#!/bin/sh\nset -eu\n{RECEIPT_FUNCTION}{RUSTUP_DISPATCH}");
        write_executable(&tools.join("rustup"), &rustup)?;
        write_executable(
            &tools.join("cargo"),
            &format!(
                "{CARGO_SCRIPT}{RECEIPT_FUNCTION}\nrecord_receipt cargo\nprintf 'simulated nightly build failure\\n' >&2\nexit 37\n"
            ),
        )?;
        for (name, marker) in [
            ("cargo-nextest", "nextest.marker"),
            ("cargo-dylint", "dylint.marker"),
            ("dylint-link", "linker.marker"),
        ] {
            let script = format!(
                "#!/bin/sh\nset -eu\nprintf 'reached\\n' > \"$XTASK_TEST_RECEIPTS/{marker}\"\nexit 0\n"
            );
            write_executable(&tools.join(name), &script)?;
        }

        Ok(Self {
            temp,
            root,
            tools,
            cargo_home,
            receipts,
        })
    }

    fn run(&self, components: &str, target_dir: &Path) -> Result<Output> {
        let target_root = if target_dir.is_absolute() {
            target_dir.to_path_buf()
        } else {
            self.root.join(target_dir)
        };
        fs::create_dir_all(&target_root)?;
        let path = std::env::join_paths([
            self.tools.as_path(),
            Path::new("/usr/bin"),
            Path::new("/bin"),
        ])
        .context("joining fixture PATH")?;
        let mut command = Command::new(env!("CARGO_BIN_EXE_jackin-xtask"));
        command
            .arg("custom-lints")
            .current_dir(&self.root)
            .env_clear()
            .env("PATH", path)
            .env("CARGO_HOME", &self.cargo_home)
            .env("CARGO_TARGET_DIR", target_dir)
            .env("XTASK_TEST_RECEIPTS", &self.receipts)
            .env("XTASK_TEST_COMPILER_VERSION", COMPILER_VERSION_FIXTURE)
            .env("XTASK_TEST_COMPONENTS", components)
            .env("CUSTOM_LINTS_TEST_MARKER", "preserved marker")
            .env("CARGO", "inherited cargo")
            .env("RUSTUP_TOOLCHAIN", "inherited toolchain")
            .env("RUSTC", "inherited rustc")
            .env("RUSTDOC", "inherited rustdoc")
            .env("RUSTFLAGS", "inherited rustflags")
            .env("RUSTC_WRAPPER", "inherited rustc wrapper")
            .env("RUSTC_WORKSPACE_WRAPPER", "inherited workspace wrapper")
            .env("DYLINT_DRIVER_PATH", "inherited driver")
            .env("DYLINT_RUSTFLAGS", "inherited dylint flags")
            .env("DYLINT_LIBRARY_PATH", "inherited library")
            .env("DYLINT_LINK_ENABLE_COPY_LIBRARY", "inherited copy setting")
            .env("CARGO_ENCODED_RUSTFLAGS", "inherited encoded flags");
        command
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .context("spawn public custom-lints command")?
            .wait_with_output()
            .context("wait for public custom-lints command")
    }

    fn assert_receipt(&self, name: &str, target_dir: &Path) -> Result<()> {
        let receipt = read_receipt(&self.receipts.join(format!("{name}.receipt")))?;
        let lint_dir = fs::canonicalize(self.root.join("crates/tools/jackin-lints"))?;
        assert_eq!(
            receipt["cwd"],
            lint_dir.to_str().context("UTF-8 lint path")?
        );
        let cargo_home = fs::canonicalize(&self.cargo_home)?;
        let receipt_cargo_home = fs::canonicalize(&receipt["CARGO_HOME"])?;
        assert_eq!(receipt_cargo_home, cargo_home);
        let target_root = if target_dir.is_absolute() {
            target_dir.to_path_buf()
        } else {
            self.root.join(target_dir)
        };
        let expected_target = fs::canonicalize(target_root)?.join("library");
        assert_eq!(
            receipt["CARGO_TARGET_DIR"],
            expected_target.to_str().context("UTF-8 target path")?
        );
        assert_eq!(receipt["MARKER"], "preserved marker");
        let path = std::env::split_paths(OsStr::new(&receipt["PATH"]))
            .map(fs::canonicalize)
            .collect::<std::io::Result<Vec<_>>>()?;
        let expected_path = [
            self.cargo_home.join("bin"),
            self.tools.clone(),
            PathBuf::from("/usr/bin"),
            PathBuf::from("/bin"),
        ]
        .into_iter()
        .map(fs::canonicalize)
        .collect::<std::io::Result<Vec<_>>>()?;
        assert_eq!(path, expected_path);
        for key in [
            "CARGO",
            "RUSTUP_TOOLCHAIN",
            "RUSTC",
            "RUSTDOC",
            "RUSTFLAGS",
            "RUSTC_WRAPPER",
            "RUSTC_WORKSPACE_WRAPPER",
            "DYLINT_DRIVER_PATH",
            "DYLINT_RUSTFLAGS",
            "DYLINT_LIBRARY_PATH",
        ] {
            assert_eq!(receipt[key], "<unset>", "{key} leaked to child");
        }
        assert_eq!(receipt["DYLINT_LINK_ENABLE_COPY_LIBRARY"], "1");
        let linker = fs::canonicalize(self.tools.join("dylint-link"))?;
        assert_eq!(
            receipt["CARGO_ENCODED_RUSTFLAGS"],
            format!("-C\u{1f}linker={}", linker.display())
        );
        Ok(())
    }

    fn assert_no_later_markers(&self, expected_cargo: bool) {
        assert_eq!(
            self.receipts.join("cargo.receipt").exists(),
            expected_cargo,
            "cargo build marker state"
        );
        for marker in ["nextest.marker", "dylint.marker", "linker.marker"] {
            assert!(
                !self.receipts.join(marker).exists(),
                "unexpected later tool marker: {marker}"
            );
        }
    }
}

fn write_executable(path: &Path, contents: &str) -> std::io::Result<()> {
    fs::write(path, contents)?;
    fs::set_permissions(path, fs::Permissions::from_mode(0o755))
}

fn read_receipt(path: &Path) -> Result<BTreeMap<String, String>> {
    fs::read_to_string(path)
        .context("read child receipt")?
        .lines()
        .map(|line| {
            let (key, value) = line
                .split_once('=')
                .ok_or_else(|| anyhow!("malformed child receipt field"))?;
            Ok((key.to_owned(), value.to_owned()))
        })
        .collect()
}

fn command_text(output: &Output) -> String {
    format!(
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    )
}

fn install_pipeline_shims(fixture: &Fixture) -> Result<()> {
    let rustup = r#"#!/bin/sh
set -eu
if [ "$1" = "run" ] && [ "$3" = "rustc" ]; then
    printf '%s' "$XTASK_TEST_COMPILER_VERSION"
    exit 0
fi
if [ "$1" = "component" ]; then
    printf '%s' "$XTASK_TEST_COMPONENTS"
    exit 0
fi
if [ "$1" = "run" ]; then
    shift 2
    exec "$@"
fi
printf 'unexpected rustup request\n' >&2
exit 90
"#;
    write_executable(&fixture.tools.join("rustup"), rustup)?;

    let cargo = format!(
        "#!/bin/sh\nset -eu\n{{ printf 'args=%s\\n' \"$*\"; printf 'cwd=%s\\n' \"$(pwd)\"; printf 'target=%s\\n' \"$CARGO_TARGET_DIR\"; }} > \"$XTASK_TEST_RECEIPTS/cargo.receipt\"\nmkdir -p \"$CARGO_TARGET_DIR/debug\"\nprintf '%s' '{FRESH_LIBRARY_BYTES}' > \"$CARGO_TARGET_DIR/debug/{FRESH_LIBRARY_NAME}\"\n"
    );
    write_executable(&fixture.tools.join("cargo"), &cargo)?;

    let nextest = r#"#!/bin/sh
set -eu
{
    printf 'selected_executable=%s\n' "$0"
    printf 'args=%s\n' "$*"
    printf 'cwd=%s\n' "$(pwd)"
    printf 'target=%s\n' "$CARGO_TARGET_DIR"
    printf 'dylint_path=%s\n' "${DYLINT_LIBRARY_PATH-<unset>}"
} > "$XTASK_TEST_RECEIPTS/nextest.receipt"
printf 'reached\n' > "$XTASK_TEST_RECEIPTS/nextest.marker"
"#;
    write_executable(&fixture.tools.join("cargo-nextest"), nextest)?;

    let dylint = r#"#!/bin/sh
set -eu
{
    printf 'selected_executable=%s\n' "$0"
    printf 'args=%s\n' "$*"
    printf 'cwd=%s\n' "$(pwd)"
    printf 'target=%s\n' "$CARGO_TARGET_DIR"
    printf 'library=%s\n' "$DYLINT_LIBRARY_PATH"
    printf 'encoded_flags=%s\n' "${CARGO_ENCODED_RUSTFLAGS-<unset>}"
    printf 'rustflags=%s\n' "${RUSTFLAGS-<unset>}"
    printf 'copy_library=%s\n' "${DYLINT_LINK_ENABLE_COPY_LIBRARY-<unset>}"
    printf 'toolchain=%s\n' "${RUSTUP_TOOLCHAIN-<unset>}"
    for artifact in "$DYLINT_LIBRARY_PATH"/*; do
        [ -f "$artifact" ] || continue
        printf 'entry=%s:%s\n' "$(basename "$artifact")" "$(cat "$artifact")"
    done
} > "$XTASK_TEST_RECEIPTS/dylint.receipt"
printf 'reached\n' > "$XTASK_TEST_RECEIPTS/dylint.marker"
if [ -f "$XTASK_TEST_RECEIPTS/fail-dylint" ]; then
    printf 'simulated workspace failure\n' >&2
    exit 37
fi
"#;
    write_executable(&fixture.tools.join("cargo-dylint"), dylint)?;

    for (name, marker) in [
        ("cargo-nextest", "nextest-decoy.marker"),
        ("cargo-dylint", "dylint-decoy.marker"),
    ] {
        let decoy = format!(
            "#!/bin/sh\nset -eu\nprintf 'selected\\n' > \"$XTASK_TEST_RECEIPTS/{marker}\"\nexit 91\n"
        );
        write_executable(&fixture.cargo_home.join("bin").join(name), &decoy)?;
    }

    Ok(())
}

fn assert_pipeline_case(workspace_failure: bool) -> Result<()> {
    let fixture = Fixture::new()?;
    install_pipeline_shims(&fixture)?;
    let target_dir = Path::new("build target");
    let target_root = fixture.root.join(target_dir);
    let build_library_dir = target_root.join("library/debug");
    fs::create_dir_all(&build_library_dir)?;
    let stale_library = build_library_dir.join(STALE_LIBRARY_NAME);
    fs::write(&stale_library, STALE_LIBRARY_BYTES)?;
    if workspace_failure {
        fs::write(fixture.receipts.join("fail-dylint"), b"")?;
    }

    let output = fixture.run(COMPLETE_COMPONENTS, target_dir)?;
    let dylint_raw = fs::read_to_string(fixture.receipts.join("dylint.receipt"))?;
    let entries = dylint_raw
        .lines()
        .filter_map(|line| line.strip_prefix("entry="))
        .map(str::to_owned)
        .collect::<Vec<_>>();
    assert_eq!(
        entries,
        [format!("{FRESH_LIBRARY_NAME}:{FRESH_LIBRARY_BYTES}")],
        "Dylint must receive only the fresh library and its bytes"
    );

    let library_target = fs::canonicalize(target_root.join("library"))?;
    let canonical_target_root = library_target
        .parent()
        .context("library target has a parent")?;
    let lint_dir = fs::canonicalize(fixture.root.join("crates/tools/jackin-lints"))?;
    let cargo = read_receipt(&fixture.receipts.join("cargo.receipt"))?;
    assert_eq!(cargo["args"], "build --locked");
    assert_eq!(cargo["cwd"], lint_dir.to_str().context("UTF-8 lint path")?);
    assert_eq!(
        cargo["target"],
        library_target.to_str().context("UTF-8 library target")?
    );

    let nextest = read_receipt(&fixture.receipts.join("nextest.receipt"))?;
    assert_eq!(
        nextest["selected_executable"],
        fs::canonicalize(fixture.tools.join("cargo-nextest"))?
            .to_str()
            .context("UTF-8 Nextest path")?
    );
    assert_eq!(nextest["args"], "nextest run --locked");
    assert_eq!(
        nextest["cwd"],
        lint_dir.to_str().context("UTF-8 lint path")?
    );
    assert_eq!(
        nextest["target"],
        library_target.to_str().context("UTF-8 library target")?
    );
    assert_eq!(nextest["dylint_path"], "<unset>");

    let dylint = read_receipt(&fixture.receipts.join("dylint.receipt"))?;
    assert_eq!(
        dylint["selected_executable"],
        fs::canonicalize(fixture.tools.join("cargo-dylint"))?
            .to_str()
            .context("UTF-8 Dylint path")?
    );
    assert_eq!(
        dylint["args"],
        "dylint --no-metadata --all --fail-on-no-libraries -- --workspace --locked"
    );
    assert_eq!(
        dylint["cwd"],
        fs::canonicalize(&fixture.root)?
            .to_str()
            .context("UTF-8 workspace path")?
    );
    assert_eq!(
        dylint["target"],
        canonical_target_root
            .join("workspace")
            .to_str()
            .context("UTF-8 workspace target")?
    );
    assert_eq!(dylint["encoded_flags"], "<unset>");
    assert_eq!(dylint["rustflags"], "<unset>");
    assert_eq!(dylint["copy_library"], "<unset>");
    assert_eq!(dylint["toolchain"], "<unset>");
    assert_eq!(fs::read(&stale_library)?, STALE_LIBRARY_BYTES.as_bytes());
    assert_eq!(
        fs::read(build_library_dir.join(FRESH_LIBRARY_NAME))?,
        FRESH_LIBRARY_BYTES.as_bytes()
    );
    assert!(
        !Path::new(&dylint["library"]).exists(),
        "temporary Dylint library directory survived the workspace outcome"
    );
    assert!(fixture.receipts.join("nextest.marker").is_file());
    assert!(fixture.receipts.join("dylint.marker").is_file());
    assert!(!fixture.receipts.join("nextest-decoy.marker").exists());
    assert!(!fixture.receipts.join("dylint-decoy.marker").exists());
    assert_eq!(output.status.success(), !workspace_failure);
    if workspace_failure {
        let text = command_text(&output);
        assert!(text.contains("exit status: 37"));
        assert!(text.contains("simulated workspace failure"));
    }

    Ok(())
}

#[test]
fn full_pipeline_hands_off_only_fresh_library_and_cleans_after_workspace_outcomes() -> Result<()> {
    assert_pipeline_case(false)?;
    assert_pipeline_case(true)
}

#[test]
fn missing_component_stops_before_build_and_receives_configured_environment() -> Result<()> {
    let fixture = Fixture::new()?;
    let target_dir = Path::new("target dir");
    let output = fixture.run(MISSING_COMPONENTS, target_dir)?;
    fixture.assert_no_later_markers(false);
    fixture.assert_receipt("rustc", target_dir)?;
    fixture.assert_receipt("components", target_dir)?;
    assert!(!output.status.success());
    assert!(command_text(&output).contains("missing pinned nightly component:"));
    Ok(())
}

#[test]
fn build_failure_stops_before_nextest_and_receives_configured_environment() -> Result<()> {
    let fixture = Fixture::new()?;
    let target_dir = fixture.temp.path().join("absolute target dir");
    let output = fixture.run(COMPLETE_COMPONENTS, &target_dir)?;
    fixture.assert_no_later_markers(true);
    fixture.assert_receipt("rustc", &target_dir)?;
    fixture.assert_receipt("components", &target_dir)?;
    fixture.assert_receipt("cargo", &target_dir)?;
    assert!(!output.status.success());
    let text = command_text(&output);
    assert!(text.contains("custom-lints command failed"));
    assert!(text.contains("cargo build"));
    assert!(text.contains("simulated nightly build failure"));
    Ok(())
}

#[test]
fn empty_path_is_rejected_before_child_execution() -> Result<()> {
    let temp = TempDir::new()?;
    fs::create_dir_all(temp.path().join("docs/content"))?;
    let output = Command::new(env!("CARGO_BIN_EXE_jackin-xtask"))
        .arg("custom-lints")
        .current_dir(temp.path())
        .env_clear()
        .env("PATH", "")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .context("spawn public custom-lints command")?
        .wait_with_output()
        .context("wait for public custom-lints command")?;
    assert!(!output.status.success());
    assert!(String::from_utf8(output.stderr)?.contains("PATH is empty"));
    assert!(output.stdout.is_empty());
    Ok(())
}
