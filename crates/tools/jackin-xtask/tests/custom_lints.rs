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
