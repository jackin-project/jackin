// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;
use serde_json::{Value, json};
use support::verifier;
use velnor_actions_contract::package_release::PackageReleaseChannel;

// Synthetic policy bytes use b"test" digests. Separate immutable upstream
// fixtures below provide real cryptographic positive/negative controls.

const RECEIPT: &[u8] = include_bytes!("fixtures/jackin-policy-receipt.json");
const COSIGN_BYTES: &[u8] = include_bytes!("fixtures/cosign-3.1.3-checksums.txt");
const COSIGN_BUNDLE: &[u8] = include_bytes!("fixtures/cosign-3.1.3-checksums.bundle.json");
const DSSE_BYTES: &[u8] = include_bytes!("fixtures/velnor-0.1.13-release-manifest.json");
const DSSE_BUNDLE: &[u8] = include_bytes!("fixtures/velnor-0.1.13-provenance.bundle.json");

fn receipt() -> PackageReleaseReceipt {
    channel_receipt(PackageReleaseChannel::Preview)
}

fn cosign_policy() -> VerificationPolicy {
    VerificationPolicy::new(
        "keyless@projectsigstore.iam.gserviceaccount.com",
        "https://accounts.google.com",
    )
}

fn dsse_policy() -> VerificationPolicy {
    VerificationPolicy::new(
        "https://github.com/tailrocks/velnor-new/.github/workflows/product-release-generator.yml@refs/heads/main",
        policy::ISSUER,
    )
}

#[test]
fn real_cosign_message_signature_verifies_certificate_sct_inclusion_set_and_timestamp() {
    let bundle = parse_bundle(COSIGN_BUNDLE).unwrap();
    let result = verifier()
        .verify(COSIGN_BYTES, &bundle, &cosign_policy())
        .unwrap();
    assert!(
        result.certificate_verified()
            && result.sct_verified()
            && result.tlog_verified()
            && result.identity_policy_checked()
    );
    assert!(!result.verified_timestamps().is_empty());
    assert!(result.integrated_time().is_some());
    verifier()
        .verify(b"tampered".as_slice(), &bundle, &cosign_policy())
        .expect_err("invalid evidence must be rejected");
    verifier()
        .verify(
            COSIGN_BYTES,
            &bundle,
            &signing_policy(PackageReleaseChannel::Preview),
        )
        .expect_err("invalid evidence must be rejected");
}

#[test]
fn identity_and_issuer_are_independently_exact() {
    let verifier = verifier();
    let bundle = parse_bundle(COSIGN_BUNDLE).unwrap();
    for policy in [
        VerificationPolicy::new("wrong@example.com", "https://accounts.google.com"),
        VerificationPolicy::new(
            "keyless@projectsigstore.iam.gserviceaccount.com",
            "https://wrong.example",
        ),
    ] {
        verifier
            .verify(COSIGN_BYTES, &bundle, &policy)
            .expect_err("identity and issuer must both match exactly");
    }
}

#[test]
fn real_dsse_verifies_with_digest_only_and_rejects_transplants() {
    let bundle = parse_bundle(DSSE_BUNDLE).unwrap();
    let hash = Sha256Hash::from_hex(&digest(DSSE_BYTES)).unwrap();
    let result = verifier()
        .verify(Artifact::from_digest(hash), &bundle, &dsse_policy())
        .unwrap();
    assert!(result.certificate_verified() && result.sct_verified() && result.tlog_verified());
    assert!(result.integrated_time().is_some());
    assert!(
        policy::validate_claims(&result, &receipt()).is_err(),
        "authentic Velnor claims do not authorize Jackin"
    );
    verifier()
        .verify(
            Artifact::from_digest(Sha256Hash::new([0; 32])),
            &bundle,
            &dsse_policy(),
        )
        .expect_err("invalid evidence must be rejected");
    verifier()
        .verify(
            DSSE_BYTES,
            &bundle,
            &signing_policy(PackageReleaseChannel::Preview),
        )
        .expect_err("invalid evidence must be rejected");
}

#[test]
fn canonical_receipt_policy_validates_fixed_jackin_inventory() {
    let original = receipt();
    policy::validate_receipt(&original, &original.version.value).unwrap();
    policy::validate_receipt(&original, "0.6.4-preview.2+abcdef0")
        .expect_err("invalid evidence must be rejected");
    policy::validate_receipt(&original, "0.6.4").expect_err("invalid evidence must be rejected");
    for pointer in [
        "/repository/id",
        "/repository/slug",
        "/source/ref",
        "/source/event",
        "/source/commit_sha",
        "/workflow/authority_sha",
        "/workflow/signer_workflow",
        "/workflow/run_attempt",
        "/products/0/bins/1",
        "/products/1/payloads/0/target",
        "/evidence/attested_subject_names/0",
    ] {
        let mut value = serde_json::to_value(&original).unwrap();
        let slot = value.pointer_mut(pointer).unwrap();
        *slot = if slot.is_number() {
            json!(999)
        } else {
            json!("wrong")
        };
        let rejected = serde_json::from_value::<PackageReleaseReceipt>(value)
            .map_or(true, |parsed| {
                policy::validate_receipt(&parsed, &original.version.value).is_err()
            });
        assert!(rejected, "{pointer}");
    }
    let text = String::from_utf8(RECEIPT.to_vec()).unwrap();
    parse_package_release_receipt(format!(" {text}").as_bytes())
        .expect_err("invalid evidence must be rejected");
    parse_package_release_receipt(text.replacen('{', "{\"channel\":\"stable\",", 1).as_bytes())
        .expect_err("invalid evidence must be rejected");
}

fn claims(receipt: &PackageReleaseReceipt) -> sigstore_crypto::FulcioCiClaims {
    let mut claims = sigstore_crypto::FulcioCiClaims::default();
    claims.source_repository_uri = Some(policy::REPOSITORY.into());
    claims.source_repository_identifier = Some("1197700841".into());
    claims.source_repository_ref = Some("refs/heads/main".into());
    claims.source_repository_digest = Some(receipt.source.commit_sha.clone());
    claims.source_repository_owner_uri = Some("https://github.com/jackin-project".into());
    claims.source_repository_owner_identifier = Some("273541039".into());
    claims.build_signer_uri = Some(policy::identity(receipt.channel).into());
    claims.build_signer_digest = Some(receipt.workflow.authority_sha.clone());
    claims.build_config_uri = Some(policy::identity(receipt.channel).into());
    claims.build_config_digest = Some(receipt.workflow.authority_sha.clone());
    claims.build_trigger = Some(receipt.channel.event().as_str().into());
    claims.runner_environment = Some("github-hosted".into());
    claims.run_invocation_uri = Some(format!(
        "{}/actions/runs/{}/attempts/{}",
        policy::REPOSITORY,
        receipt.workflow.run_id,
        receipt.workflow.run_attempt
    ));
    claims.source_repository_visibility_at_signing = Some("public".into());
    claims
}

#[test]
fn exact_ci_claim_policy_rejects_every_missing_or_changed_claim() {
    let receipt = receipt();
    policy::validate_ci_claims(&claims(&receipt), &receipt).unwrap();
    for trigger in ["push", "pull_request", "workflow_dispatch"] {
        let mut invalid = claims(&receipt);
        invalid.build_trigger = Some(trigger.into());
        policy::validate_ci_claims(&invalid, &receipt)
            .expect_err("only the native completion-triggered preview signer is trusted");
    }
    for index in 0..14 {
        for replacement in [None, Some("wrong".into())] {
            let mut claims = claims(&receipt);
            let slots = [
                &mut claims.source_repository_uri,
                &mut claims.source_repository_identifier,
                &mut claims.source_repository_ref,
                &mut claims.source_repository_digest,
                &mut claims.source_repository_owner_uri,
                &mut claims.source_repository_owner_identifier,
                &mut claims.build_signer_uri,
                &mut claims.build_signer_digest,
                &mut claims.build_config_uri,
                &mut claims.build_config_digest,
                &mut claims.build_trigger,
                &mut claims.runner_environment,
                &mut claims.run_invocation_uri,
                &mut claims.source_repository_visibility_at_signing,
            ];
            *slots.into_iter().nth(index).unwrap() = replacement;
            assert!(
                policy::validate_ci_claims(&claims, &receipt).is_err(),
                "claim {index}"
            );
        }
    }
}

#[test]
fn companion_and_asset_bytes_bind_exact_size_digest_and_version() {
    let receipt = receipt();
    let a = asset(&receipt, "capsule-manifest.json").unwrap();
    verify_bytes(a, b"test").unwrap();
    verify_bytes(a, b"Test").expect_err("invalid evidence must be rejected");
    verify_bytes(a, b"test\n").expect_err("invalid evidence must be rejected");
    let mut companion = json!({"version":receipt.version.value,"targets":{}});
    for target in policy::LINUX_TARGETS {
        companion["targets"][target] = json!(
            asset(&receipt, &format!("jackin-capsule-{target}.tar.gz"))
                .unwrap()
                .sha256
        );
    }
    validate_companion(
        &serde_json::to_vec(&companion).unwrap(),
        &receipt,
        &receipt.version.value,
    )
    .unwrap();
    for pointer in [
        "/version",
        "/targets/aarch64-unknown-linux-gnu",
        "/targets/x86_64-unknown-linux-gnu",
    ] {
        let mut invalid = companion.clone();
        *invalid.pointer_mut(pointer).unwrap() = json!("wrong");
        validate_companion(
            &serde_json::to_vec(&invalid).unwrap(),
            &receipt,
            &receipt.version.value,
        )
        .expect_err("invalid evidence must be rejected");
    }
    companion["targets"]["x86_64-apple-darwin"] = json!("a".repeat(64));
    validate_companion(
        &serde_json::to_vec(&companion).unwrap(),
        &receipt,
        &receipt.version.value,
    )
    .expect_err("invalid evidence must be rejected");
}

struct MockTransport {
    requests: std::sync::Mutex<Vec<String>>,
    receipt: Vec<u8>,
}
impl Transport for MockTransport {
    #[expect(
        clippy::unused_async_trait_impl,
        reason = "in-memory HTTP mock implements async transport without I/O"
    )]
    async fn get(&self, url: &str, _limit: usize) -> Result<Vec<u8>> {
        self.requests.lock().unwrap().push(url.into());
        if url.ends_with("/release.json") {
            Ok(self.receipt.clone())
        } else if url.ends_with("/release.json.bundle") {
            Ok(COSIGN_BUNDLE.to_vec())
        } else {
            anyhow::bail!("unverified receipt must block all downstream downloads")
        }
    }
}

#[tokio::test]
async fn mocked_http_authenticates_receipt_before_payload_download() {
    let mock = MockTransport {
        requests: std::sync::Mutex::new(Vec::new()),
        receipt: RECEIPT.to_vec(),
    };
    verify_with_transport(
        &mock,
        &verifier(),
        &receipt().version.value,
        "x86_64-unknown-linux-gnu",
    )
    .await
    .expect_err("invalid evidence must be rejected");
    let requests = mock.requests.lock().unwrap();
    assert_eq!(requests.len(), 2);
    assert!(
        requests[0].ends_with("/release.json") && requests[1].ends_with("/release.json.bundle")
    );
}

#[tokio::test]
async fn unsupported_remote_versions_never_request_http() {
    let mock = MockTransport {
        requests: std::sync::Mutex::new(Vec::new()),
        receipt: Vec::new(),
    };
    for version in ["dev", "preview", "0.6.4-dev+abcdef0", "00.6.4"] {
        verify_with_transport(&mock, &verifier(), version, "x86_64-unknown-linux-gnu")
            .await
            .expect_err("invalid evidence must be rejected");
    }
    assert!(mock.requests.lock().unwrap().is_empty());
}

fn statement(receipt: &PackageReleaseReceipt) -> (Value, BTreeMap<String, String>) {
    let subjects: BTreeMap<_, _> = receipt
        .evidence
        .attested_subject_names
        .iter()
        .map(|name| (name.clone(), "a".repeat(64)))
        .collect();
    let statement = json!({
        "_type":"https://in-toto.io/Statement/v1", "predicateType":"https://slsa.dev/provenance/v1",
        "subject":subjects.iter().map(|(name, sha)| json!({"name":name,"digest":{"sha256":sha}})).collect::<Vec<_>>(),
        "predicate":{
            "buildDefinition":{
                "buildType":"https://actions.github.io/buildtypes/workflow/v1",
                "externalParameters":{"workflow":{"ref":"refs/heads/main","repository":policy::REPOSITORY,"path":receipt.channel.workflow_path()}},
                "internalParameters":{"github":{"event_name":receipt.channel.event().as_str(),"repository_id":"1197700841","repository_owner_id":"273541039","runner_environment":"github-hosted"}},
                "resolvedDependencies":[{"uri":format!("git+{}@refs/heads/main",policy::REPOSITORY),"digest":{"gitCommit":receipt.source.commit_sha}}]
            },
            "runDetails":{"builder":{"id":policy::identity(receipt.channel)},"metadata":{"invocationId":format!("{}/actions/runs/{}/attempts/{}",policy::REPOSITORY,receipt.workflow.run_id,receipt.workflow.run_attempt)}}
        }
    });
    (statement, subjects)
}

#[test]
fn provenance_requires_exact_complete_eight_subject_inventory() {
    let receipt = receipt();
    let (statement, subjects) = statement(&receipt);
    assert_eq!(subjects.len(), 8);
    provenance::validate_statement(&statement, &receipt, &subjects).unwrap();
    for pointer in [
        "/subject/0/name",
        "/subject/0/digest/sha256",
        "/_type",
        "/predicateType",
        "/predicate/buildDefinition/buildType",
        "/predicate/buildDefinition/externalParameters/workflow/ref",
        "/predicate/buildDefinition/externalParameters/workflow/path",
        "/predicate/buildDefinition/externalParameters/workflow/repository",
        "/predicate/buildDefinition/internalParameters/github/event_name",
        "/predicate/buildDefinition/internalParameters/github/repository_id",
        "/predicate/buildDefinition/internalParameters/github/repository_owner_id",
        "/predicate/buildDefinition/internalParameters/github/runner_environment",
        "/predicate/buildDefinition/resolvedDependencies/0/digest/gitCommit",
        "/predicate/runDetails/builder/id",
        "/predicate/runDetails/metadata/invocationId",
    ] {
        let mut invalid = statement.clone();
        *invalid.pointer_mut(pointer).unwrap() = json!("wrong");
        assert!(
            provenance::validate_statement(&invalid, &receipt, &subjects).is_err(),
            "{pointer}"
        );
    }
    for trigger in ["push", "pull_request", "workflow_dispatch"] {
        let mut invalid = statement.clone();
        invalid["predicate"]["buildDefinition"]["internalParameters"]["github"]["event_name"] =
            json!(trigger);
        provenance::validate_statement(&invalid, &receipt, &subjects)
            .expect_err("only the native completion-triggered preview provenance is trusted");
    }
    let mut missing = statement.clone();
    missing["subject"].as_array_mut().unwrap().pop();
    provenance::validate_statement(&missing, &receipt, &subjects)
        .expect_err("invalid evidence must be rejected");
    let mut duplicate = statement.clone();
    let subject = duplicate["subject"][0].clone();
    duplicate["subject"].as_array_mut().unwrap().push(subject);
    provenance::validate_statement(&duplicate, &receipt, &subjects)
        .expect_err("invalid evidence must be rejected");
    let mut extra = statement;
    extra["subject"]
        .as_array_mut()
        .unwrap()
        .push(json!({"name":"unexpected","digest":{"sha256":"a".repeat(64)}}));
    provenance::validate_statement(&extra, &receipt, &subjects)
        .expect_err("invalid evidence must be rejected");
}

mod channels;
mod crypto;

mod support;

pub(super) fn channel_receipt(channel: PackageReleaseChannel) -> PackageReleaseReceipt {
    support::receipt(channel)
}
