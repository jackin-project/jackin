use super::*;
use zeroize::Zeroize;

const BOOT_ID: &str = "01234567-89ab-4cde-8fab-0123456789ab";
const NONCE: &str = "fedcba98-7654-4abc-9def-0123456789ab";

fn valid_context<'a>(
    resource_id: &'a str,
    boot_id: &'a str,
    nonce: &'a str,
    generation: u64,
) -> Option<CredentialRefreshContextV2<'a>> {
    let result = CredentialRefreshContextV2::new(resource_id, boot_id, nonce, generation);
    assert!(result.is_ok(), "test context must be syntactically valid");
    result.ok()
}

fn encoded(
    phase: CredentialRefreshPhaseV2,
    context: &CredentialRefreshContextV2<'_>,
    payload: &[u8],
) -> Option<CredentialRefreshSigningBytesV2> {
    let result = encode_signing_bytes_v2(phase, context, payload);
    assert!(result.is_ok(), "test transcript must encode");
    result.ok()
}

#[test]
fn prepare_transcript_matches_independent_golden_bytes() {
    let Some(context) = valid_context("resource-A", BOOT_ID, NONCE, 0x0102_0304_0506_0708) else {
        return;
    };
    let payload = b"\0{\"x\":\"\\u0061\"}";
    let Some(encoded) = encoded(CredentialRefreshPhaseV2::Prepare, &context, payload) else {
        return;
    };
    const EXPECTED: &[u8] = b"jackin-signed-control-v2\0\x00\x00\x00\x00\x00\x00\x00\x1dcredential-refresh-prepare-v2\x00\x00\x00\x00\x00\x00\x00\x0aresource-A\x00\x00\x00\x00\x00\x00\x00\x2401234567-89ab-4cde-8fab-0123456789ab\x00\x00\x00\x00\x00\x00\x00\x24fedcba98-7654-4abc-9def-0123456789ab\x01\x02\x03\x04\x05\x06\x07\x08\x00\x00\x00\x00\x00\x00\x00\x0f\0{\"x\":\"\\u0061\"}";

    assert_eq!(encoded.as_bytes().len(), 199);
    assert_eq!(encoded.as_bytes(), EXPECTED);
}

#[test]
fn commit_and_abort_domains_match_independent_golden_bytes() {
    let Some(context) = valid_context("resource-A", BOOT_ID, NONCE, 0x0102_0304_0506_0708) else {
        return;
    };
    let Some(commit) = encoded(CredentialRefreshPhaseV2::Commit, &context, b"{}") else {
        return;
    };
    let Some(abort) = encoded(CredentialRefreshPhaseV2::Abort, &context, b"") else {
        return;
    };
    const COMMIT_EXPECTED: &[u8] = b"jackin-signed-control-v2\0\x00\x00\x00\x00\x00\x00\x00\x1ccredential-refresh-commit-v2\x00\x00\x00\x00\x00\x00\x00\x0aresource-A\x00\x00\x00\x00\x00\x00\x00\x2401234567-89ab-4cde-8fab-0123456789ab\x00\x00\x00\x00\x00\x00\x00\x24fedcba98-7654-4abc-9def-0123456789ab\x01\x02\x03\x04\x05\x06\x07\x08\x00\x00\x00\x00\x00\x00\x00\x02{}";
    const ABORT_EXPECTED: &[u8] = b"jackin-signed-control-v2\0\x00\x00\x00\x00\x00\x00\x00\x1bcredential-refresh-abort-v2\x00\x00\x00\x00\x00\x00\x00\x0aresource-A\x00\x00\x00\x00\x00\x00\x00\x2401234567-89ab-4cde-8fab-0123456789ab\x00\x00\x00\x00\x00\x00\x00\x24fedcba98-7654-4abc-9def-0123456789ab\x01\x02\x03\x04\x05\x06\x07\x08\x00\x00\x00\x00\x00\x00\x00\0";

    assert_eq!(commit.as_bytes().len(), 185);
    assert_eq!(commit.as_bytes(), COMMIT_EXPECTED);
    assert_eq!(abort.as_bytes().len(), 182);
    assert_eq!(abort.as_bytes(), ABORT_EXPECTED);
}

#[test]
fn every_context_and_payload_field_changes_the_transcript() {
    let Some(base_context) = valid_context("resource-A", BOOT_ID, NONCE, 7) else {
        return;
    };
    let Some(base) = encoded(CredentialRefreshPhaseV2::Prepare, &base_context, b"payload") else {
        return;
    };

    let Some(other_phase) = encoded(CredentialRefreshPhaseV2::Commit, &base_context, b"payload")
    else {
        return;
    };
    let Some(other_resource_context) = valid_context("resource-B", BOOT_ID, NONCE, 7) else {
        return;
    };
    let Some(other_resource) = encoded(
        CredentialRefreshPhaseV2::Prepare,
        &other_resource_context,
        b"payload",
    ) else {
        return;
    };
    let Some(other_boot_context) = valid_context(
        "resource-A",
        "01234567-89ab-4cde-8fab-1123456789ab",
        NONCE,
        7,
    ) else {
        return;
    };
    let Some(other_boot) = encoded(
        CredentialRefreshPhaseV2::Prepare,
        &other_boot_context,
        b"payload",
    ) else {
        return;
    };
    let Some(other_nonce_context) = valid_context(
        "resource-A",
        BOOT_ID,
        "fedcba98-7654-4abc-9def-1123456789ab",
        7,
    ) else {
        return;
    };
    let Some(other_nonce) = encoded(
        CredentialRefreshPhaseV2::Prepare,
        &other_nonce_context,
        b"payload",
    ) else {
        return;
    };
    let Some(other_generation_context) = valid_context("resource-A", BOOT_ID, NONCE, 8) else {
        return;
    };
    let Some(other_generation) = encoded(
        CredentialRefreshPhaseV2::Prepare,
        &other_generation_context,
        b"payload",
    ) else {
        return;
    };
    let Some(other_payload) = encoded(
        CredentialRefreshPhaseV2::Prepare,
        &base_context,
        b"payload!",
    ) else {
        return;
    };

    assert_ne!(base.as_bytes(), other_phase.as_bytes());
    assert_ne!(base.as_bytes(), other_resource.as_bytes());
    assert_ne!(base.as_bytes(), other_boot.as_bytes());
    assert_ne!(base.as_bytes(), other_nonce.as_bytes());
    assert_ne!(base.as_bytes(), other_generation.as_bytes());
    assert_ne!(base.as_bytes(), other_payload.as_bytes());
}

#[test]
fn length_framing_separates_ambiguous_field_boundaries() {
    let boot = "01234567-89ab-4cde-8fab-0123456789ab";
    let nonce = "fedcba98-7654-4abc-9def-0123456789ab";
    let generation = u64::from_be_bytes(*b"abcdefgh");
    let generation_bytes: &[u8] = b"abcdefgh";
    let resource_one: &[u8] = b"a";
    let tail: &[u8] = b"tail";
    let ambiguous = format!("{boot}{nonce}abcdefgh");
    let resource_two = format!("a{ambiguous}");
    let payload_one = [ambiguous.as_bytes(), tail].concat();
    let payload_two = tail;
    let unframed_one = [
        resource_one,
        boot.as_bytes(),
        nonce.as_bytes(),
        generation_bytes,
        payload_one.as_slice(),
    ]
    .concat();
    let unframed_two = [
        resource_two.as_bytes(),
        boot.as_bytes(),
        nonce.as_bytes(),
        generation_bytes,
        payload_two,
    ]
    .concat();
    let Some(context_one) = valid_context("a", boot, nonce, generation) else {
        return;
    };
    let Some(context_two) = valid_context(&resource_two, boot, nonce, generation) else {
        return;
    };
    let Some(encoded_one) = encoded(
        CredentialRefreshPhaseV2::Prepare,
        &context_one,
        &payload_one,
    ) else {
        return;
    };
    let Some(encoded_two) = encoded(CredentialRefreshPhaseV2::Prepare, &context_two, payload_two)
    else {
        return;
    };

    assert_eq!(ambiguous.len(), 80);
    assert_eq!(resource_two.len(), 81);
    assert_eq!(payload_one.len(), 84);
    assert_eq!(payload_two.len(), 4);
    assert_eq!(unframed_one, unframed_two);
    assert_ne!(encoded_one.as_bytes(), encoded_two.as_bytes());
}

#[test]
fn identifiers_require_bounded_canonical_resource_and_uuid_text() {
    let resource_512 = "r".repeat(512);
    let resource_513 = "r".repeat(513);
    let multibyte_512 = "é".repeat(256);
    let multibyte_514 = "é".repeat(257);

    assert!(valid_context(&resource_512, BOOT_ID, NONCE, 0).is_some());
    assert!(valid_context(&multibyte_512, BOOT_ID, NONCE, 0).is_some());
    assert!(valid_context("odd/name:✓", BOOT_ID, NONCE, 0).is_some());
    assert!(matches!(
        CredentialRefreshContextV2::new("", BOOT_ID, NONCE, 0),
        Err(CredentialRefreshEncodingErrorV2::InvalidContext)
    ));
    assert!(matches!(
        CredentialRefreshContextV2::new(&resource_513, BOOT_ID, NONCE, 0),
        Err(CredentialRefreshEncodingErrorV2::InvalidContext)
    ));
    assert!(matches!(
        CredentialRefreshContextV2::new(&multibyte_514, BOOT_ID, NONCE, 0),
        Err(CredentialRefreshEncodingErrorV2::InvalidContext)
    ));
    assert!(matches!(
        CredentialRefreshContextV2::new("resource\u{7f}", BOOT_ID, NONCE, 0),
        Err(CredentialRefreshEncodingErrorV2::InvalidContext)
    ));

    let malformed_pairs = [
        ("01234567-89AB-4CDE-8FAB-0123456789AB", NONCE),
        ("01234567-89ab-5cde-8fab-0123456789ab", NONCE),
        ("01234567-89ab-4cde-7fab-0123456789ab", NONCE),
        (BOOT_ID, "fedcba98-7654-4abc-9def-0123456789a"),
        (BOOT_ID, "FEDCBA98-7654-4ABC-9DEF-0123456789AB"),
        (BOOT_ID, "fedcba98-7654-4abc-7def-0123456789ab"),
    ];
    for (boot_id, nonce) in malformed_pairs {
        assert!(matches!(
            CredentialRefreshContextV2::new("resource", boot_id, nonce, 0),
            Err(CredentialRefreshEncodingErrorV2::InvalidContext)
        ));
    }
}

#[test]
fn payload_exact_limit_is_accepted_and_plus_one_is_rejected() {
    let Some(context) = valid_context("resource-A", BOOT_ID, NONCE, 0) else {
        return;
    };
    let mut payload = vec![0x5a; MAX_CREDENTIAL_REFRESH_PAYLOAD_BYTES_V2];
    let Some(encoded) = encoded(CredentialRefreshPhaseV2::Prepare, &context, &payload) else {
        return;
    };
    assert_eq!(encoded.as_bytes().len(), 4 * 1024 * 1024 + 184);
    drop(encoded);

    payload.push(0x5a);
    assert!(matches!(
        encode_signing_bytes_v2(CredentialRefreshPhaseV2::Prepare, &context, &payload),
        Err(CredentialRefreshEncodingErrorV2::PayloadTooLarge)
    ));
}

#[test]
fn abort_rejects_nonempty_payload() {
    let Some(context) = valid_context("resource-A", BOOT_ID, NONCE, 0) else {
        return;
    };
    assert!(matches!(
        encode_signing_bytes_v2(CredentialRefreshPhaseV2::Abort, &context, b"x"),
        Err(CredentialRefreshEncodingErrorV2::AbortPayloadMustBeEmpty)
    ));
}

#[test]
fn debug_and_errors_redact_sentinel_payload_and_context() {
    const CONTEXT_SENTINEL: &str = "context-secret-sentinel";
    const PAYLOAD_SENTINEL: &[u8] = b"payload-secret-sentinel";
    let Some(context) = valid_context(CONTEXT_SENTINEL, BOOT_ID, NONCE, 0) else {
        return;
    };
    let Some(encoded) = encoded(
        CredentialRefreshPhaseV2::Prepare,
        &context,
        PAYLOAD_SENTINEL,
    ) else {
        return;
    };
    let debug = format!("{encoded:?}");
    let context_debug = format!("{context:?}");
    assert!(!debug.contains(CONTEXT_SENTINEL));
    assert!(!debug.contains("payload-secret-sentinel"));
    assert!(!context_debug.contains(CONTEXT_SENTINEL));

    let invalid_context =
        CredentialRefreshContextV2::new("error-secret-sentinel\u{0}", BOOT_ID, NONCE, 0);
    assert!(matches!(
        &invalid_context,
        Err(CredentialRefreshEncodingErrorV2::InvalidContext)
    ));
    if let Some(error) = invalid_context.err() {
        let display = error.to_string();
        let debug = format!("{error:?}");
        assert!(!display.contains("error-secret-sentinel"));
        assert!(!debug.contains("error-secret-sentinel"));
    }
}

#[test]
fn clearing_owned_transcript_zeroizes_its_initialized_bytes() {
    let Some(context) = valid_context("resource-A", BOOT_ID, NONCE, 0) else {
        return;
    };
    let Some(mut encoded) = encoded(
        CredentialRefreshPhaseV2::Prepare,
        &context,
        b"zeroize-owned-transcript-fixture",
    ) else {
        return;
    };

    encoded.bytes.as_mut_slice().zeroize();
    assert!(encoded.as_bytes().iter().all(|byte| *byte == 0));
}
