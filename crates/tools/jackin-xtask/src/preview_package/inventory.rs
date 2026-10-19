// SPDX-FileCopyrightText: 2026 The jackin❯ Authors
// SPDX-License-Identifier: Apache-2.0

use crate::release_sbom::archive_sha256;
use anyhow::Result;
use serde_json::{Value, json};
use std::{fs, path::Path};
use velnor_actions_contract::preview_production::{
    PreviewPayloadPair, PreviewProductionRequest, expected_preview_payload_pairs,
};

pub(super) fn write_leaf(
    output: &Path,
    request: &PreviewProductionRequest,
    pair: &PreviewPayloadPair,
) -> Result<()> {
    let (payloads, sboms) = rows(output, std::slice::from_ref(pair))?;
    write_manifest(
        output,
        request,
        "velnor.package-payload-output.v1",
        payloads,
        sboms,
        Vec::new(),
    )
}

pub(super) fn write_complete(output: &Path, request: &PreviewProductionRequest) -> Result<()> {
    let pairs = expected_preview_payload_pairs(&request.plan)?;
    let (payloads, sboms) = rows(output, &pairs)?;
    let mut capsule_targets = serde_json::Map::new();
    for pair in pairs
        .iter()
        .filter(|pair| pair.product_id == "jackin-capsule")
    {
        let archive = format!("{}-{}.tar.gz", pair.product_id, pair.target);
        capsule_targets.insert(
            pair.target.clone(),
            Value::String(archive_sha256(&output.join(archive))?),
        );
    }
    let companion = output.join("capsule-manifest.json");
    fs::write(
        &companion,
        serde_json::to_vec(&json!({"version":request.context.version,"targets":capsule_targets}))?,
    )?;
    let companions = vec![
        json!({"id":"capsule-manifest", "path":"capsule-manifest.json",
        "name":"capsule-manifest.json", "sha256":archive_sha256(&companion)?}),
    ];
    write_manifest(
        output,
        request,
        "velnor.package-output.v1",
        payloads,
        sboms,
        companions,
    )
}

fn rows(output: &Path, pairs: &[PreviewPayloadPair]) -> Result<(Vec<Value>, Vec<Value>)> {
    let mut payloads = Vec::new();
    let mut sboms = Vec::new();
    for pair in pairs {
        let name = format!("{}-{}.tar.gz", pair.product_id, pair.target);
        let sbom = format!("{name}.sbom.json");
        crate::release_sbom::verify(&output.join(&name), &output.join(&sbom))?;
        payloads.push(json!({"product_id":pair.product_id, "target":pair.target,
            "format":"tar-gz", "name":name, "sha256":archive_sha256(&output.join(&name))?}));
        sboms.push(json!({"payload_name":name, "name":sbom, "sha256":archive_sha256(&output.join(&sbom))?}));
    }
    payloads.sort_by_key(|row| row["name"].as_str().unwrap_or_default().to_owned());
    sboms.sort_by_key(|row| row["payload_name"].as_str().unwrap_or_default().to_owned());
    Ok((payloads, sboms))
}

fn write_manifest(
    output: &Path,
    request: &PreviewProductionRequest,
    schema: &str,
    payloads: Vec<Value>,
    sboms: Vec<Value>,
    companions: Vec<Value>,
) -> Result<()> {
    let context = &request.context;
    let manifest = json!({"schema":schema, "source_commit":context.source_sha,
        "source_ref":context.source_ref, "version":context.version,
        "payloads":payloads,"companions":companions,"sboms":sboms});
    fs::write(
        output.join("package-output.json"),
        serde_json::to_vec(&manifest)?,
    )?;
    Ok(())
}
