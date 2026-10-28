// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::{
    collections::BTreeMap,
    ffi::OsStr,
    fs,
    path::{Path, PathBuf},
    process::Output,
};

use anyhow::{Context, Result, anyhow};

use super::{FRESH_LIBRARY_BYTES, FRESH_LIBRARY_NAME, Fixture, write_executable};

pub(crate) fn install_pipeline_shims(fixture: &Fixture) -> Result<()> {
    let cargo = format!(
        "#!/bin/sh\nset -eu\n{{ printf 'selected_executable=%s\\n' \"$0\"; printf 'args=%s\\n' \"$*\"; printf 'cwd=%s\\n' \"$(pwd)\"; printf 'target=%s\\n' \"$CARGO_TARGET_DIR\"; }} > \"$XTASK_TEST_RECEIPTS/cargo.receipt\"\nmkdir -p \"$CARGO_TARGET_DIR/debug\"\nprintf '%s' '{FRESH_LIBRARY_BYTES}' > \"$CARGO_TARGET_DIR/debug/{FRESH_LIBRARY_NAME}\"\n"
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
        ("cargo", "cargo-decoy.marker"),
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

pub(crate) fn read_receipt(path: &Path) -> Result<BTreeMap<String, String>> {
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

pub(crate) fn command_text(output: &Output) -> String {
    format!(
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    )
}

pub(crate) fn assert_fixture_path(path: &str, tools: &Path, cargo_home: &Path) -> Result<()> {
    let entries = std::env::split_paths(OsStr::new(path)).collect::<Vec<_>>();
    let shim = entries
        .first()
        .context("private MBX shim path is missing from PATH")?;
    let shim_name = shim.file_name().and_then(OsStr::to_str).unwrap_or_default();
    anyhow::ensure!(
        shim_name.starts_with("jackin-custom-lints-mbx-"),
        "unexpected private Cargo shim PATH entry"
    );
    let expected = [
        tools.to_path_buf(),
        cargo_home.join("bin"),
        tools.to_path_buf(),
        PathBuf::from("/usr/bin"),
        PathBuf::from("/bin"),
    ]
    .into_iter()
    .map(fs::canonicalize)
    .collect::<std::io::Result<Vec<_>>>()?;
    let actual = entries
        .into_iter()
        .skip(1)
        .map(fs::canonicalize)
        .collect::<std::io::Result<Vec<_>>>()?;
    assert_eq!(actual, expected);
    Ok(())
}
