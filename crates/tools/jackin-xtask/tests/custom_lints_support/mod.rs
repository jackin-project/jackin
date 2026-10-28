// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

mod transport;

use std::{
    collections::BTreeMap,
    fs,
    os::unix::fs::PermissionsExt,
    path::{Path, PathBuf},
    process::{Command, Output, Stdio},
};

use anyhow::{Context, Result};
use tempfile::TempDir;

pub(super) use transport::{
    assert_fixture_path, command_text, install_pipeline_shims, read_receipt,
};

pub(super) const COMPILER_VERSION_FIXTURE: &str = "commit-hash: 8006091eb71ae7d9e9bccb66c6122c9d68af4466\nrelease: 1.101.0-nightly\nhost: aarch64-apple-darwin\n";
pub(super) const COMPLETE_COMPONENTS: &str = concat!(
    "clippy-aarch64-apple-darwin\n",
    "llvm-tools-aarch64-apple-darwin\n",
    "rustc-dev-aarch64-apple-darwin\n",
    "rustfmt-aarch64-apple-darwin\n",
);
pub(super) const MISSING_COMPONENTS: &str = concat!(
    "clippy-aarch64-apple-darwin\n",
    "llvm-tools-aarch64-apple-darwin\n",
    "rustc-dev-aarch64-apple-darwin\n",
);
pub(super) const FRESH_LIBRARY_BYTES: &str = "fresh qualified library";
pub(super) const STALE_LIBRARY_BYTES: &str = "stale ABI library";
#[cfg(target_os = "linux")]
pub(super) const FRESH_LIBRARY_NAME: &str =
    "libjackin_lints@nightly-2026-10-18-aarch64-apple-darwin.so";
#[cfg(target_os = "macos")]
pub(super) const FRESH_LIBRARY_NAME: &str =
    "libjackin_lints@nightly-2026-10-18-aarch64-apple-darwin.dylib";
#[cfg(target_os = "linux")]
pub(super) const STALE_LIBRARY_NAME: &str =
    "libjackin_lints@nightly-2026-10-17-aarch64-apple-darwin.so";
#[cfg(target_os = "macos")]
pub(super) const STALE_LIBRARY_NAME: &str =
    "libjackin_lints@nightly-2026-10-17-aarch64-apple-darwin.dylib";

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

pub(super) struct Fixture {
    pub(super) temp: TempDir,
    pub(super) root: PathBuf,
    pub(super) tools: PathBuf,
    cargo_home: PathBuf,
    pub(super) receipts: PathBuf,
}

impl Fixture {
    pub(super) fn new() -> Result<Self> {
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

        write_executable(
            &tools.join("rustup"),
            &format!("#!/bin/sh\nset -eu\n{RECEIPT_FUNCTION}{RUSTUP_DISPATCH}"),
        )?;
        write_executable(&tools.join("cargo"), &failed_cargo())?;
        write_executable(&tools.join("mbx"), &mbx_transport())?;
        for (name, marker) in [
            ("cargo-nextest", "nextest.marker"),
            ("cargo-dylint", "dylint.marker"),
            ("dylint-link", "linker.marker"),
        ] {
            write_executable(
                &tools.join(name),
                &format!(
                    "#!/bin/sh\nset -eu\nprintf 'reached\\n' > \"$XTASK_TEST_RECEIPTS/{marker}\"\nexit 0\n"
                ),
            )?;
        }

        Ok(Self {
            temp,
            root,
            tools,
            cargo_home,
            receipts,
        })
    }

    pub(super) fn run(&self, components: &str, target_dir: &Path) -> Result<Output> {
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
        let fake_cargo = fs::canonicalize(self.tools.join("cargo"))?;
        let mut command = Command::new(env!("CARGO_BIN_EXE_jackin-xtask"));
        command
            .arg("custom-lints")
            .current_dir(&self.root)
            .env_clear()
            .env("PATH", path)
            .env("CARGO_HOME", &self.cargo_home)
            .env("CARGO_TARGET_DIR", target_dir)
            .env("XTASK_TEST_RECEIPTS", &self.receipts)
            .env("XTASK_TEST_FAKE_CARGO", fake_cargo)
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

    pub(super) fn assert_receipt(
        &self,
        name: &str,
        target_dir: &Path,
        output: &Output,
    ) -> Result<()> {
        let receipt = self.receipt(name, output)?;
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
        assert_fixture_path(&receipt["PATH"], &self.tools, &self.cargo_home)?;
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

    pub(super) fn assert_no_later_markers(&self, expected_cargo: bool) {
        assert_eq!(
            self.receipts.join("cargo.receipt").exists(),
            expected_cargo,
            "cargo build marker state"
        );
        for marker in [
            "nextest.marker",
            "dylint.marker",
            "linker.marker",
            "cargo-decoy.marker",
        ] {
            assert!(
                !self.receipts.join(marker).exists(),
                "unexpected later tool marker: {marker}"
            );
        }
        assert_eq!(
            self.receipts.join("mbx-build.receipt").exists(),
            expected_cargo,
            "MBX build dispatch marker state"
        );
        assert!(!self.receipts.join("mbx-nextest.receipt").exists());
        assert_eq!(
            self.receipts.join("shim.receipt").exists(),
            expected_cargo,
            "private Cargo alias marker state"
        );
    }

    pub(super) fn receipt(&self, name: &str, output: &Output) -> Result<BTreeMap<String, String>> {
        read_receipt(&self.receipts.join(format!("{name}.receipt"))).with_context(|| {
            format!(
                "reading {name} receipt after custom-lints output: {}",
                command_text(output)
            )
        })
    }

    pub(super) fn receipt_text(&self, name: &str, output: &Output) -> Result<String> {
        fs::read_to_string(self.receipts.join(format!("{name}.receipt"))).with_context(|| {
            format!(
                "reading {name} receipt after custom-lints output: {}",
                command_text(output)
            )
        })
    }

    pub(super) fn assert_mbx_receipt(
        &self,
        name: &str,
        expected_args: &str,
        target_dir: &Path,
        output: &Output,
    ) -> Result<BTreeMap<String, String>> {
        let receipt = self.receipt(name, output)?;
        let mbx = fs::canonicalize(self.tools.join("mbx"))?;
        assert_eq!(
            receipt["selected_executable"],
            mbx.to_string_lossy().as_ref()
        );
        assert_eq!(receipt["args"], expected_args);
        let lint_dir = fs::canonicalize(self.root.join("crates/tools/jackin-lints"))?;
        assert_eq!(receipt["cwd"], lint_dir.to_string_lossy().as_ref());
        let expected_target = fs::canonicalize(if target_dir.is_absolute() {
            target_dir.to_path_buf()
        } else {
            self.root.join(target_dir)
        })?
        .join("library");
        assert_eq!(
            receipt["CARGO_TARGET_DIR"],
            expected_target.to_string_lossy().as_ref()
        );
        assert_eq!(
            receipt["CARGO_HOME"],
            fs::canonicalize(&self.cargo_home)?
                .to_string_lossy()
                .as_ref()
        );
        assert_eq!(receipt["CARGO"], "<unset>");
        assert_eq!(receipt["RUSTUP_TOOLCHAIN"], "<unset>");
        assert_eq!(receipt["mode"], "1");
        assert_eq!(receipt["cargo"], receipt["shim_path"]);
        assert_eq!(
            receipt["cargo_nextest"],
            fs::canonicalize(self.tools.join("cargo-nextest"))?.to_string_lossy()
        );
        assert_fixture_path(&receipt["PATH"], &self.tools, &self.cargo_home)?;
        Ok(receipt)
    }
}

pub(super) fn assert_shim_receipt(
    fixture: &Fixture,
    mbx_receipt: &BTreeMap<String, String>,
    output: &Output,
) -> Result<()> {
    let receipt = fixture.receipt("shim", output)?;
    let mbx = fs::canonicalize(fixture.tools.join("mbx"))?;
    let fake_cargo = fs::canonicalize(fixture.tools.join("cargo"))?;
    assert_eq!(receipt["selected_executable"], mbx_receipt["shim_path"]);
    assert_eq!(receipt["shim_path"], mbx_receipt["cargo"]);
    assert_eq!(receipt["shim_target"], mbx.to_string_lossy().as_ref());
    assert_eq!(receipt["fake_cargo"], fake_cargo.to_string_lossy().as_ref());
    assert_eq!(receipt["shim_parent_mode"], "700");
    assert_eq!(receipt["mode"], "1");
    Ok(())
}

fn write_executable(path: &Path, contents: &str) -> std::io::Result<()> {
    fs::write(path, contents)?;
    fs::set_permissions(path, fs::Permissions::from_mode(0o755))
}

fn failed_cargo() -> String {
    format!(
        "#!/bin/sh\nset -eu\n{RECEIPT_FUNCTION}\nrecord_receipt cargo\nprintf 'selected_executable=%s\\n' \"$0\" >> \"$XTASK_TEST_RECEIPTS/cargo.receipt\"\nprintf 'args=%s\\n' \"$*\" >> \"$XTASK_TEST_RECEIPTS/cargo.receipt\"\nprintf 'target=%s\\n' \"$CARGO_TARGET_DIR\" >> \"$XTASK_TEST_RECEIPTS/cargo.receipt\"\nprintf 'simulated nightly build failure\\n' >&2\nexit 37\n"
    )
}

fn mbx_transport() -> String {
    let stat_mode = if cfg!(target_os = "macos") {
        "stat -f '%Lp'"
    } else {
        "stat -c '%a'"
    };
    format!(
        r#"#!/bin/sh
set -eu
if [ "$(basename "$0")" = cargo ]; then
    {{ printf 'selected_executable=%s\n' "$0"; printf 'args=%s\n' "$*"; printf 'shim_path=%s\n' "$MBX_CARGO_SHIM_PATH"; printf 'shim_target=%s\n' "$(readlink "$MBX_CARGO_SHIM_PATH")"; printf 'shim_parent_mode=%s\n' "$({stat_mode} "$(dirname "$MBX_CARGO_SHIM_PATH")")"; printf 'mode=%s\n' "$MBX_CARGO_SHIM_MODE"; printf 'fake_cargo=%s\n' "$XTASK_TEST_FAKE_CARGO"; }} > "$XTASK_TEST_RECEIPTS/shim.receipt"
    exec "$XTASK_TEST_FAKE_CARGO" "$@"
fi
name=
case "$1" in
    build) name=mbx-build ;;
    nextest) name=mbx-nextest ;;
    *) printf 'unexpected MBX request: %s\n' "$*" >&2; exit 90 ;;
esac
{{ printf 'selected_executable=%s\n' "$0"; printf 'args=%s\n' "$*"; printf 'cwd=%s\n' "$(pwd)"; printf 'PATH=%s\n' "$PATH"; printf 'CARGO_HOME=%s\n' "$CARGO_HOME"; printf 'CARGO_TARGET_DIR=%s\n' "$CARGO_TARGET_DIR"; printf 'CARGO=%s\n' "${{CARGO-<unset>}}"; printf 'RUSTUP_TOOLCHAIN=%s\n' "${{RUSTUP_TOOLCHAIN-<unset>}}"; printf 'mode=%s\n' "$MBX_CARGO_SHIM_MODE"; printf 'shim_path=%s\n' "$MBX_CARGO_SHIM_PATH"; printf 'cargo=%s\n' "$(command -v cargo)"; printf 'cargo_nextest=%s\n' "$(command -v cargo-nextest)"; }} > "$XTASK_TEST_RECEIPTS/$name.receipt"
if [ "$name" = mbx-build ]; then
    exec "$(command -v cargo)" "$@"
fi
exec "$(command -v cargo-nextest)" "$@"
"#
    )
}
