use jackin_agent_status::rules::{PackSource, RulePackRegistry, SignedPackBundle, SignedPackEntry};

const FORGED_IDENTITY: &str = "jackin-project/agent-status-packs";
const FORGED_SIGNATURE: &str =
    "jackin-agent-status-pack-bundle:v1:jackin-project/agent-status-packs";

fn forged_bundle(marker: &str) -> SignedPackBundle {
    SignedPackBundle {
        signer_identity: FORGED_IDENTITY.to_owned(),
        signature: FORGED_SIGNATURE.to_owned(),
        packs: vec![SignedPackEntry {
            label: "claude-remote".to_owned(),
            content: format!(
                r#"
schema_version = 1
agent = "claude"
validated_versions = ">=1.0.0, <2.0.0"

[[rule]]
id = "forged-remote"
state = "blocked"
priority = 100
region = "bottom:12"
strength = "strong"
requires_all = ["{marker}"]
"#
            ),
        }],
    }
}

fn assert_rejected(bundle: SignedPackBundle, marker: &str) {
    let result = RulePackRegistry::from_sources([
        PackSource::Embedded,
        PackSource::SignedRemoteBundle(bundle),
    ]);
    std::assert_matches!(&result, Ok(_));
    let Ok(registry) = result else { return };
    assert!(
        registry
            .evaluate(Some("claude"), &[marker.to_owned()])
            .is_none()
    );
    assert!(
        registry
            .evaluate(Some("claude"), &["esc to interrupt".to_owned()])
            .is_some()
    );
    assert_eq!(
        registry.notes(),
        [
            "remote pack bundle failed verification - using baked packs: remote pack bundles require a production signature verifier"
        ]
    );
}

#[test]
fn production_registry_rejects_predictable_identity_marker() {
    assert_rejected(
        forged_bundle("original remote marker"),
        "original remote marker",
    );
}

#[test]
fn production_registry_rejects_swapped_payload_with_same_marker() {
    let mut bundle = forged_bundle("original remote marker");
    bundle.packs = forged_bundle("swapped remote marker").packs;
    assert_eq!(bundle.signer_identity, FORGED_IDENTITY);
    assert_eq!(bundle.signature, FORGED_SIGNATURE);
    assert_rejected(bundle, "swapped remote marker");
}

#[test]
fn production_registry_rejects_other_identity_and_signature() {
    let mut bundle = forged_bundle("untrusted remote marker");
    bundle.signer_identity = "attacker".to_owned();
    bundle.signature = "arbitrary".to_owned();
    assert_rejected(bundle, "untrusted remote marker");
}

#[test]
fn rejected_bundle_cannot_create_registry_without_floor() {
    let result = RulePackRegistry::from_sources([PackSource::SignedRemoteBundle(forged_bundle(
        "remote marker",
    ))]);
    assert_eq!(
        result.unwrap_err().to_string(),
        "no agent-status rule packs loaded"
    );
}
