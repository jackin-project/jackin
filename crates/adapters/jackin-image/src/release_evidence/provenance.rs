// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::collections::BTreeMap;

use anyhow::{Context, Result, bail, ensure};
use serde_json::Value;
use sigstore_types::{Artifact, Bundle, Sha256Hash, SignatureContent};
use sigstore_verify::Verifier;
use velnor_actions_contract::package_release::PackageReleaseReceipt;

use super::{EVIDENCE_LIMIT, parse_bundle, policy, signing_policy, transport::Transport};

#[derive(serde::Deserialize)]
struct Attestations {
    attestations: Vec<Attestation>,
}
#[derive(serde::Deserialize)]
struct Attestation {
    bundle: Value,
}

pub(super) async fn verify(
    transport: &impl Transport,
    verifier: &Verifier,
    receipt: &PackageReleaseReceipt,
    selected_digest: &str,
    subjects: &BTreeMap<String, String>,
) -> Result<()> {
    let url = format!(
        "https://api.github.com/repos/jackin-project/jackin/attestations/sha256:{selected_digest}?per_page=100"
    );
    let bytes = transport.get(&url, EVIDENCE_LIMIT).await?;
    let response: Attestations =
        serde_json::from_slice(&bytes).context("parsing GitHub artifact attestations response")?;
    for attestation in response.attestations {
        let accepted = (|| {
            let bundle = parse_bundle(&serde_json::to_vec(&attestation.bundle)?)?;
            verify_bundle(verifier, &bundle, receipt, selected_digest, subjects)
        })();
        if accepted.is_ok() {
            return Ok(());
        }
    }
    bail!(
        "GitHub has no valid production Sigstore SLSA attestation binding the exact eight release subjects and authenticated release workflow/source/run; refusing capsule installation"
    )
}

fn verify_bundle(
    verifier: &Verifier,
    bundle: &Bundle,
    receipt: &PackageReleaseReceipt,
    selected_digest: &str,
    subjects: &BTreeMap<String, String>,
) -> Result<()> {
    let SignatureContent::DsseEnvelope(envelope) = &bundle.content else {
        bail!("artifact provenance requires DSSE");
    };
    let result = verifier.verify(
        Artifact::from_digest(Sha256Hash::from_hex(selected_digest)?),
        bundle,
        &signing_policy(receipt.channel),
    )?;
    policy::validate_claims(&result, receipt)?;
    let statement: Value = serde_json::from_slice(envelope.payload.as_bytes())?;
    validate_statement(&statement, receipt, subjects)
}

fn value<'a>(statement: &'a Value, pointer: &str) -> Result<&'a str> {
    statement
        .pointer(pointer)
        .and_then(Value::as_str)
        .with_context(|| format!("missing provenance claim {pointer}"))
}

pub(super) fn validate_statement(
    statement: &Value,
    receipt: &PackageReleaseReceipt,
    expected_subjects: &BTreeMap<String, String>,
) -> Result<()> {
    ensure!(
        value(statement, "/_type")? == "https://in-toto.io/Statement/v1"
            && value(statement, "/predicateType")? == "https://slsa.dev/provenance/v1",
        "unsupported attestation statement/predicate"
    );
    validate_subjects(statement, expected_subjects)?;
    let invocation = format!(
        "{}/actions/runs/{}/attempts/{}",
        policy::REPOSITORY,
        receipt.workflow.run_id,
        receipt.workflow.run_attempt
    );
    let expected = [
        (
            "/predicate/buildDefinition/buildType",
            "https://actions.github.io/buildtypes/workflow/v1",
        ),
        (
            "/predicate/buildDefinition/externalParameters/workflow/ref",
            "refs/heads/main",
        ),
        (
            "/predicate/buildDefinition/externalParameters/workflow/repository",
            policy::REPOSITORY,
        ),
        (
            "/predicate/buildDefinition/externalParameters/workflow/path",
            receipt.channel.workflow_path(),
        ),
        (
            "/predicate/buildDefinition/internalParameters/github/event_name",
            receipt.channel.event().as_str(),
        ),
        (
            "/predicate/buildDefinition/internalParameters/github/repository_id",
            "1197700841",
        ),
        (
            "/predicate/buildDefinition/internalParameters/github/repository_owner_id",
            "273541039",
        ),
        (
            "/predicate/buildDefinition/internalParameters/github/runner_environment",
            "github-hosted",
        ),
        (
            "/predicate/runDetails/builder/id",
            policy::identity(receipt.channel),
        ),
        (
            "/predicate/runDetails/metadata/invocationId",
            invocation.as_str(),
        ),
    ];
    for (pointer, expected) in expected {
        ensure!(
            value(statement, pointer)? == expected,
            "provenance claim mismatch at {pointer}"
        );
    }
    validate_source_dependency(statement, receipt)?;
    Ok(())
}

fn validate_subjects(
    statement: &Value,
    expected_subjects: &BTreeMap<String, String>,
) -> Result<()> {
    let subjects = statement
        .get("subject")
        .and_then(Value::as_array)
        .context("missing attestation subjects")?;
    let mut actual = BTreeMap::new();
    for subject in subjects {
        let name = value(subject, "/name")?;
        let digest = subject
            .get("digest")
            .and_then(Value::as_object)
            .context("missing subject digest")?;
        ensure!(
            digest.len() == 1,
            "attestation subject must carry exactly SHA-256"
        );
        let sha = value(subject, "/digest/sha256")?;
        ensure!(
            actual.insert(name.to_owned(), sha.to_owned()).is_none(),
            "duplicate attestation subject"
        );
    }
    ensure!(
        &actual == expected_subjects,
        "attestation subject name/digest inventory mismatch"
    );
    Ok(())
}

fn validate_source_dependency(statement: &Value, receipt: &PackageReleaseReceipt) -> Result<()> {
    let dependencies = statement
        .pointer("/predicate/buildDefinition/resolvedDependencies")
        .and_then(Value::as_array)
        .context("missing provenance source dependency")?;
    let source_uri = format!("git+{}@refs/heads/main", policy::REPOSITORY);
    ensure!(
        dependencies
            .iter()
            .any(
                |dependency| value(dependency, "/uri").is_ok_and(|uri| uri == source_uri)
                    && value(dependency, "/digest/gitCommit")
                        .is_ok_and(|sha| sha == receipt.source.commit_sha)
            ),
        "provenance source commit mismatch"
    );
    Ok(())
}
