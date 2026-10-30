// SPDX-FileCopyrightText: 2026 The jackin❯ Authors
// SPDX-License-Identifier: Apache-2.0

use super::*;

#[test]
fn real_crypto_evidence_mutations_fail_closed() {
    let paths = [
        "/verificationMaterial/certificate/rawBytes",
        "/verificationMaterial/tlogEntries/0/canonicalizedBody",
        "/verificationMaterial/tlogEntries/0/inclusionPromise/signedEntryTimestamp",
        "/verificationMaterial/tlogEntries/0/inclusionProof/checkpoint/envelope",
        "/verificationMaterial/tlogEntries/0/inclusionProof/rootHash",
        "/messageSignature/signature",
        "/verificationMaterial/timestampVerificationData/rfc3161Timestamps/0/signedTimestamp",
    ];
    let verifier = verifier();
    for path in paths {
        let mut value: Value = serde_json::from_slice(COSIGN_BUNDLE).unwrap();
        let field = value
            .pointer_mut(path)
            .unwrap_or_else(|| panic!("missing fixture field {path}"));
        let original = field.as_str().unwrap();
        let mut changed = original.as_bytes().to_vec();
        changed[0] = if changed[0] == b'A' { b'B' } else { b'A' };
        *field = Value::String(String::from_utf8(changed).unwrap());
        let bytes = serde_json::to_vec(&value).unwrap();
        assert!(
            parse_bundle(&bytes)
                .and_then(|bundle| verifier
                    .verify(COSIGN_BYTES, &bundle, &cosign_policy())
                    .map_err(Into::into))
                .is_err(),
            "{path}"
        );
    }
    let mut value: Value = serde_json::from_slice(DSSE_BUNDLE).unwrap();
    value["dsseEnvelope"]["payload"] = json!("e30=");
    let bundle = parse_bundle(&serde_json::to_vec(&value).unwrap()).unwrap();
    verifier
        .verify(DSSE_BYTES, &bundle, &dsse_policy())
        .expect_err("invalid evidence must be rejected");
}

#[test]
fn official_rekor_v2_message_dsse_and_tsa_verify_without_skips() {
    // These are immutable upstream staging conformance fixtures, not production
    // Jackin release evidence. Production always fetches the public-good TUF root.
    let root = sigstore_trust_root::TrustedRoot::from_json(include_str!(
        "../fixtures/sigstore-rust-staging-root.json"
    ))
    .unwrap();
    let verifier = Verifier::new(&root).unwrap();
    let fixtures: [(&[u8], &str, &str, &str); 2] = [
        (
            include_bytes!("../fixtures/sigstore-python-rekor2.bundle.json"),
            "jku@goto.fi",
            "https://github.com/login/oauth",
            &digest(include_bytes!("../fixtures/sigstore-python-rekor2.txt")),
        ),
        (
            include_bytes!("../fixtures/sigstore-python-rekor2-dsse.bundle.json"),
            "https://github.com/sigstore-conformance/extremely-dangerous-public-oidc-beacon/.github/workflows/extremely-dangerous-oidc-beacon.yml@refs/heads/main",
            policy::ISSUER,
            "a0cfc71271d6e278e57cd332ff957c3f7043fdda354c4cbb190a30d56efa01bf",
        ),
    ];
    for (bundle_bytes, identity, issuer, hash) in fixtures {
        let artifact = || Artifact::from_digest(Sha256Hash::from_hex(hash).unwrap());
        let policy = VerificationPolicy::new(identity, issuer);
        let bundle = parse_bundle(bundle_bytes).unwrap();
        let result = verifier.verify(artifact(), &bundle, &policy).unwrap();
        assert!(
            result.certificate_verified()
                && result.sct_verified()
                && result.tlog_verified()
                && result.identity_policy_checked()
        );
        assert!(result.integrated_time().is_none());
        assert!(!result.verified_timestamps().is_empty());
        verifier
            .verify(b"wrong".as_slice(), &bundle, &policy)
            .expect_err("v2 evidence must bind exact artifact");
        let mut mutated: Value = serde_json::from_slice(bundle_bytes).unwrap();
        mutated["verificationMaterial"]["timestampVerificationData"] = json!({});
        let result = parse_bundle(&serde_json::to_vec(&mutated).unwrap()).and_then(|bundle| {
            verifier
                .verify(artifact(), &bundle, &policy)
                .map_err(Into::into)
        });
        result.expect_err("Rekor v2 without authenticated timestamp must fail");
    }
}
