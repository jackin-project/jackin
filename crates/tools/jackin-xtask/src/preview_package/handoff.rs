// SPDX-FileCopyrightText: 2026 The jackin❯ Authors
// SPDX-License-Identifier: Apache-2.0

use anyhow::{Context, Result, ensure};
use serde::Deserialize;
use std::{env, fs, io::Read, path::Path};
use velnor_actions_contract::{
    canonical_json_bytes, parse_strict_json,
    preview_production::{
        PreviewPayloadPair, PreviewProductionRequest, PreviewProductionStage,
        expected_preview_payload_pairs,
    },
};

const MAX_REQUEST_BYTES: u64 = 32 * 1024 * 1024;

#[derive(Debug, Deserialize, serde::Serialize)]
#[serde(deny_unknown_fields)]
pub(super) struct TaskInput {
    pub pair: PreviewPayloadPair,
    pub name: String,
    pub sha256: String,
    pub size: u64,
    pub path: String,
}

#[derive(Debug, Deserialize, serde::Serialize)]
#[serde(deny_unknown_fields)]
pub(super) struct TaskRequest {
    pub schema: String,
    pub request: PreviewProductionRequest,
    pub inputs: Vec<TaskInput>,
}

pub(super) fn load() -> Result<TaskRequest> {
    let path = env::var("VELNOR_PREVIEW_REQUEST_FILE")?;
    let expected = Path::new(&env::var("RUNNER_TEMP")?).join("velnor/preview-task-request.json");
    ensure!(
        Path::new(&path).is_absolute() && Path::new(&path) == expected,
        "unexpected native request path"
    );
    let metadata = fs::symlink_metadata(&path)?;
    ensure!(
        metadata.is_file()
            && !metadata.file_type().is_symlink()
            && metadata.len() <= MAX_REQUEST_BYTES,
        "native request must be a bounded regular file"
    );
    let descriptor = nix::fcntl::open(
        Path::new(&path),
        nix::fcntl::OFlag::O_RDONLY
            | nix::fcntl::OFlag::O_NOFOLLOW
            | nix::fcntl::OFlag::O_NONBLOCK
            | nix::fcntl::OFlag::O_CLOEXEC,
        nix::sys::stat::Mode::empty(),
    )?;
    let file = fs::File::from(descriptor);
    ensure!(
        file.metadata()?.is_file(),
        "native request descriptor must be regular"
    );
    let mut bytes = Vec::new();
    file.take(MAX_REQUEST_BYTES + 1).read_to_end(&mut bytes)?;
    parse(&bytes)
}

pub(super) fn parse(bytes: &[u8]) -> Result<TaskRequest> {
    ensure!(
        !bytes.is_empty() && bytes.len() as u64 <= MAX_REQUEST_BYTES,
        "native request size limit"
    );
    let value = parse_strict_json(std::str::from_utf8(bytes)?)?;
    let handoff: TaskRequest = serde_json::from_value(value)?;
    ensure!(
        canonical_json_bytes(&handoff)? == bytes,
        "native request must be canonical JSON"
    );
    ensure!(
        handoff.schema == "velnor.preview-task.v1",
        "unknown native task schema"
    );
    handoff.request.validate()?;
    validate_policy(&handoff.request)?;
    validate_inputs(&handoff)?;
    Ok(handoff)
}

pub(super) fn validate_environment(request: &PreviewProductionRequest) -> Result<()> {
    let context = &request.context;
    ensure!(
        env::var("VELNOR_RELEASE_SOURCE_COMMIT")? == context.source_sha,
        "source handoff mismatch"
    );
    ensure!(
        env::var("VELNOR_RELEASE_SOURCE_REF")? == context.source_ref,
        "ref handoff mismatch"
    );
    ensure!(
        env::var("VELNOR_RELEASE_VERSION")? == context.version,
        "version handoff mismatch"
    );
    ensure!(
        env::var("VELNOR_RELEASE_COMMIT_COUNT")? == context.commit_count.to_string(),
        "count handoff mismatch"
    );
    Ok(())
}

fn validate_policy(request: &PreviewProductionRequest) -> Result<()> {
    ensure!(
        request.plan.repository == "jackin-project/jackin"
            && request.plan.base_version == env!("CARGO_PKG_VERSION"),
        "unexpected source package policy"
    );
    let actual = expected_preview_payload_pairs(&request.plan)?;
    let mut expected = Vec::new();
    for payload in crate::release_archive::preview_payloads() {
        let product = request
            .plan
            .products
            .iter()
            .find(|product| product.id == payload.package)
            .context("missing declared product")?;
        ensure!(
            product.package == payload.package
                && product
                    .bins
                    .iter()
                    .map(String::as_str)
                    .eq(payload.binaries.iter().copied()),
            "product package/bin inventory differs from source"
        );
        expected.push(PreviewPayloadPair {
            product_id: payload.package.to_owned(),
            target: payload.target.to_owned(),
        });
    }
    expected.sort();
    ensure!(
        actual == expected,
        "full payload policy differs from source"
    );
    ensure!(
        request.plan.companions.len() == 1
            && request.plan.companions[0].id == "capsule-manifest"
            && request.plan.companions[0].path == "capsule-manifest.json"
            && request.plan.companions[0].asset_name == "capsule-manifest.json",
        "unexpected companion policy"
    );
    Ok(())
}

fn validate_inputs(handoff: &TaskRequest) -> Result<()> {
    if matches!(
        handoff.request.stage,
        PreviewProductionStage::Payload { .. }
    ) {
        ensure!(
            handoff.inputs.is_empty(),
            "payload tasks cannot consume leaves"
        );
        return Ok(());
    }
    let mut expected = Vec::new();
    for pair in expected_preview_payload_pairs(&handoff.request.plan)? {
        let archive = format!("{}-{}.tar.gz", pair.product_id, pair.target);
        expected.push((archive.clone(), pair.clone()));
        expected.push((format!("{archive}.sbom.json"), pair));
    }
    expected.sort();
    ensure!(
        handoff.inputs.len() == expected.len(),
        "incomplete admitted input union"
    );
    let mut paths = std::collections::BTreeSet::new();
    for (input, (name, pair)) in handoff.inputs.iter().zip(expected) {
        ensure!(
            input.name == name && input.pair == pair,
            "noncanonical admitted input inventory"
        );
        ensure!(
            input.size > 0 && input.size <= 1024 * 1024 * 1024,
            "invalid admitted input size"
        );
        ensure!(
            input.sha256.len() == 64
                && input
                    .sha256
                    .bytes()
                    .all(|b| b.is_ascii_digit() || matches!(b, b'a'..=b'f')),
            "invalid admitted input digest"
        );
        ensure!(
            Path::new(&input.path).is_absolute()
                && Path::new(&input.path)
                    .file_name()
                    .is_some_and(|base| base == input.name.as_str())
                && paths.insert(&input.path),
            "invalid or duplicate admitted input path"
        );
    }
    Ok(())
}
