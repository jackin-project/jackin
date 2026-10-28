// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

#![cfg(all(unix, any(target_os = "linux", target_os = "macos")))]

use std::{fs, path::Path};

use anyhow::{Context, Result};

mod custom_lints_support;

use custom_lints_support::{
    COMPLETE_COMPONENTS, FRESH_LIBRARY_BYTES, FRESH_LIBRARY_NAME, Fixture, MISSING_COMPONENTS,
    STALE_LIBRARY_BYTES, STALE_LIBRARY_NAME, assert_shim_receipt, command_text,
    install_pipeline_shims,
};

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
    let dylint_raw = fixture.receipt_text("dylint", &output)?;
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
    let cargo = fixture.receipt("cargo", &output)?;
    assert_eq!(
        cargo["selected_executable"],
        fs::canonicalize(fixture.tools.join("cargo"))?
            .to_string_lossy()
            .as_ref()
    );
    assert_eq!(cargo["args"], "build --locked");
    assert_eq!(cargo["cwd"], lint_dir.to_string_lossy().as_ref());
    assert_eq!(cargo["target"], library_target.to_string_lossy().as_ref());

    let mbx_build =
        fixture.assert_mbx_receipt("mbx-build", "build --locked", target_dir, &output)?;
    assert_shim_receipt(&fixture, &mbx_build, &output)?;
    let mbx_nextest =
        fixture.assert_mbx_receipt("mbx-nextest", "nextest run --locked", target_dir, &output)?;
    assert_eq!(
        mbx_nextest["cargo_nextest"],
        fs::canonicalize(fixture.tools.join("cargo-nextest"))?
            .to_string_lossy()
            .as_ref()
    );

    let nextest = fixture.receipt("nextest", &output)?;
    assert_eq!(
        nextest["selected_executable"],
        fs::canonicalize(fixture.tools.join("cargo-nextest"))?
            .to_string_lossy()
            .as_ref()
    );
    assert_eq!(nextest["args"], "nextest run --locked");
    assert_eq!(nextest["cwd"], lint_dir.to_string_lossy().as_ref());
    assert_eq!(nextest["target"], library_target.to_string_lossy().as_ref());
    assert_eq!(nextest["dylint_path"], "<unset>");

    let dylint = fixture.receipt("dylint", &output)?;
    assert_eq!(
        dylint["selected_executable"],
        fs::canonicalize(fixture.tools.join("cargo-dylint"))?
            .to_string_lossy()
            .as_ref()
    );
    assert_eq!(
        dylint["args"],
        "dylint --no-metadata --all --fail-on-no-libraries -- --workspace --locked"
    );
    assert_eq!(
        dylint["cwd"],
        fs::canonicalize(&fixture.root)?.to_string_lossy().as_ref()
    );
    assert_eq!(
        dylint["target"],
        canonical_target_root
            .join("workspace")
            .to_string_lossy()
            .as_ref()
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
    assert!(!fixture.receipts.join("cargo-decoy.marker").exists());
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
    fixture.assert_receipt("rustc", target_dir, &output)?;
    fixture.assert_receipt("components", target_dir, &output)?;
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
    fixture.assert_receipt("rustc", &target_dir, &output)?;
    fixture.assert_receipt("components", &target_dir, &output)?;
    fixture.assert_receipt("cargo", &target_dir, &output)?;
    let mbx = fixture.assert_mbx_receipt("mbx-build", "build --locked", &target_dir, &output)?;
    assert_shim_receipt(&fixture, &mbx, &output)?;
    let text = command_text(&output);
    assert!(!output.status.success());
    assert!(text.contains("custom-lints command failed"));
    assert!(text.contains("mbx build --locked"));
    assert!(text.contains("simulated nightly build failure"));
    Ok(())
}
