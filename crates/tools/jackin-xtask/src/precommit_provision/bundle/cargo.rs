// SPDX-FileCopyrightText: 2026 The jackin❯ Authors
// SPDX-License-Identifier: Apache-2.0

//! Canonical bootstrap request and Cargo artifact validation.

use std::{
    fs,
    path::{Path, PathBuf},
};

use anyhow::{Context, Result, ensure};
use serde::Deserialize;

use super::{JSON_LINE_LIMIT, RequestFields};

#[derive(Deserialize)]
struct CargoMessage {
    reason: String,
    package_id: Option<String>,
    manifest_path: Option<PathBuf>,
    target: Option<CargoTarget>,
    executable: Option<PathBuf>,
}

#[derive(Deserialize)]
struct CargoTarget {
    name: String,
    kind: Vec<String>,
    src_path: PathBuf,
}

pub(in crate::precommit_provision) fn parse_request(bytes: &[u8]) -> Result<RequestFields> {
    ensure!(
        bytes.is_ascii() && !bytes.contains(&b'\r') && bytes.last() == Some(&b'\n'),
        "bootstrap request is not canonical ASCII"
    );
    let text = std::str::from_utf8(bytes)?;
    let mut lines = text.lines();
    ensure!(
        lines.next() == Some("jackin-native-precommit-bootstrap/1"),
        "bootstrap request header mismatch"
    );
    let expected = [
        "reviewed-tree",
        "build-commit",
        "index-sha256",
        "source-sha256",
        "cargo-sha256",
        "rustc-sha256",
        "build-sha256",
        "target-sha256",
    ];
    let mut values = Vec::with_capacity(expected.len());
    for name in expected {
        let line = lines
            .next()
            .context("bootstrap request has missing fields")?;
        let (key, value) = line
            .split_once('=')
            .context("invalid bootstrap request field")?;
        ensure!(
            key == name && !value.is_empty(),
            "bootstrap request field order or value is invalid"
        );
        values.push(value.to_owned());
    }
    ensure!(
        lines.next().is_none() && text.matches('\n').count() == 9,
        "bootstrap request has extra records"
    );
    let mut values = values.into_iter();
    let fields = RequestFields {
        reviewed_tree: next_value(&mut values)?,
        build_commit: next_value(&mut values)?,
        index_sha256: next_value(&mut values)?,
        source_sha256: next_value(&mut values)?,
        cargo_sha256: next_value(&mut values)?,
        rustc_sha256: next_value(&mut values)?,
        build_sha256: next_value(&mut values)?,
        target_sha256: next_value(&mut values)?,
    };
    super::files::validate_oid(&fields.reviewed_tree)?;
    super::files::validate_oid(&fields.build_commit)?;
    for digest in [
        &fields.index_sha256,
        &fields.source_sha256,
        &fields.cargo_sha256,
        &fields.rustc_sha256,
        &fields.build_sha256,
        &fields.target_sha256,
    ] {
        super::files::validate_digest(digest)?;
    }
    Ok(fields)
}

fn next_value(values: &mut impl Iterator<Item = String>) -> Result<String> {
    values
        .next()
        .context("bootstrap request has missing fields")
}

pub(super) fn validate_build_argv(bytes: &[u8]) -> Result<()> {
    ensure!(bytes.last() == Some(&0), "build argv lacks its final NUL");
    let args = bytes
        .split(|byte| *byte == 0)
        .filter(|arg| !arg.is_empty())
        .collect::<Vec<_>>();
    ensure!(
        args.len() == 13,
        "build argv has an unexpected argument count"
    );
    ensure!(
        args[0].ends_with(b"/rust-build-guard"),
        "build argv does not use rust-build-guard"
    );
    ensure!(
        args[1..]
            == [
                b"--".as_slice(),
                b"mise",
                b"exec",
                b"--",
                b"cargo",
                b"build",
                b"--locked",
                b"-p",
                b"jackin-xtask",
                b"--bin",
                b"jackin-precommit-owner",
                b"--message-format=json-render-diagnostics"
            ],
        "build argv differs from the fixed locked build command"
    );
    Ok(())
}

pub(super) fn validate_cargo_artifact(
    bytes: &[u8],
    source: &Path,
    target_path: &Path,
) -> Result<()> {
    let expected_manifest = source.join("crates/tools/jackin-xtask/Cargo.toml");
    let expected_src = source.join("crates/tools/jackin-xtask/src/precommit_owner_main.rs");
    let mut matches = 0;
    for line in bytes
        .split(|byte| *byte == b'\n')
        .filter(|line| !line.is_empty())
    {
        ensure!(
            line.len() <= JSON_LINE_LIMIT,
            "Cargo JSON record exceeds its size limit"
        );
        let message: CargoMessage =
            serde_json::from_slice(line).context("parsing Cargo build JSON")?;
        if message.reason != "compiler-artifact" {
            continue;
        }
        let (Some(target), Some(manifest), Some(executable)) =
            (message.target, message.manifest_path, message.executable)
        else {
            continue;
        };
        if target.name == "jackin-precommit-owner"
            && target.kind.iter().any(|kind| kind == "bin")
            && fs::canonicalize(manifest).ok().as_deref() == Some(expected_manifest.as_path())
            && fs::canonicalize(&target.src_path).ok().as_deref() == Some(expected_src.as_path())
            && fs::canonicalize(executable).ok().as_deref() == Some(target_path)
        {
            ensure!(
                message
                    .package_id
                    .as_deref()
                    .is_some_and(|id| !id.is_empty()),
                "Cargo artifact has no package identity"
            );
            matches += 1;
        }
    }
    ensure!(
        matches == 1,
        "Cargo JSON does not identify exactly one reviewed owner artifact"
    );
    Ok(())
}
