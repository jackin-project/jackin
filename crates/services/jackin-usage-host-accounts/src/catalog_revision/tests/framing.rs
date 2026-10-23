use hmac::{Hmac, KeyInit, Mac};
use sha2::Sha256;

use crate::catalog_revision::framing::{
    checked_frame_size_for_test, fixed_frame_bytes_for_test, max_fields_for_test,
    max_frame_bytes_for_test, revision,
};
use crate::catalog_revision::{CatalogAuthorityError, RevisionDomain};

#[test]
fn rfc4231_hmac_sha256_known_answer_uses_constant_time_library_verification() {
    let key = [0x0b_u8; 20];
    let expected = [
        0xb0, 0x34, 0x4c, 0x61, 0xd8, 0xdb, 0x38, 0x53, 0x5c, 0xa8, 0xaf, 0xce, 0xaf, 0x0b, 0xf1,
        0x2b, 0x88, 0x1d, 0xc2, 0x00, 0xc9, 0x83, 0x3d, 0xa7, 0x26, 0xe9, 0x37, 0x6c, 0x2e, 0x32,
        0xcf, 0xf7,
    ];
    let mut mac = Hmac::<Sha256>::new_from_slice(&key).expect("valid RFC 4231 key");
    mac.update(b"Hi There");
    mac.verify_slice(&expected)
        .expect("RFC 4231 test case 1 matches");

    let second_key = b"Jefe";
    let second_message = b"what do ya want for nothing?";
    let second_expected = [
        0x5b, 0xdc, 0xc1, 0x46, 0xbf, 0x60, 0x75, 0x4e, 0x6a, 0x04, 0x24, 0x26, 0x08, 0x95, 0x75,
        0xc7, 0x5a, 0x00, 0x3f, 0x08, 0x9d, 0x27, 0x39, 0x83, 0x9d, 0xec, 0x58, 0xb9, 0x64, 0xec,
        0x38, 0x43,
    ];
    let mut second_mac = Hmac::<Sha256>::new_from_slice(second_key).expect("valid RFC 4231 key");
    second_mac.update(second_message);
    second_mac
        .verify_slice(&second_expected)
        .expect("RFC 4231 test case 2 matches");
}

#[test]
fn revision_separates_domain_epoch_order_empty_and_field_boundaries() {
    let key = std::array::from_fn(|index| u8::try_from(index).expect("32-byte key index fits"));
    let epoch =
        std::array::from_fn(|index| u8::try_from(index + 32).expect("epoch fixture index fits"));
    let next_epoch = std::array::from_fn(|index| {
        u8::try_from(index + 33).expect("next epoch fixture index fits")
    });
    let first = revision(
        &key,
        &epoch,
        RevisionDomain::HostSourceEvidenceV1,
        &[b"ab", b"c"],
    )
    .expect("valid bounded frame");
    let split = revision(
        &key,
        &epoch,
        RevisionDomain::HostSourceEvidenceV1,
        &[b"a", b"bc"],
    )
    .expect("valid split frame");
    let empty = revision(
        &key,
        &epoch,
        RevisionDomain::HostSourceEvidenceV1,
        &[b"ab", b"", b"c"],
    )
    .expect("valid frame with empty field");
    let no_fields = revision(&key, &epoch, RevisionDomain::HostSourceEvidenceV1, &[])
        .expect("valid frame with no fields");
    let one_empty_field = revision(&key, &epoch, RevisionDomain::HostSourceEvidenceV1, &[b""])
        .expect("valid frame with one empty field");
    let reordered = revision(
        &key,
        &epoch,
        RevisionDomain::HostSourceEvidenceV1,
        &[b"c", b"ab"],
    )
    .expect("valid reordered frame");
    let other_domain = revision(
        &key,
        &epoch,
        RevisionDomain::CatalogEvidenceV1,
        &[b"ab", b"c"],
    )
    .expect("valid catalog-domain frame");
    let other_epoch = revision(
        &key,
        &next_epoch,
        RevisionDomain::HostSourceEvidenceV1,
        &[b"ab", b"c"],
    )
    .expect("valid next-epoch frame");

    assert_ne!(first, split);
    assert_ne!(first, empty);
    assert_ne!(no_fields, one_empty_field);
    assert_ne!(first, reordered);
    assert_ne!(first, other_domain);
    assert_ne!(first, other_epoch);
    assert!(first.as_str().starts_with("hmac-sha256:"));
    assert_eq!(first.as_str().len(), 76);
    assert!(
        first.as_str()[12..]
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
    );
}

#[test]
fn revision_uses_the_frozen_v1_framing_vector() {
    let key = std::array::from_fn(|index| u8::try_from(index).expect("32-byte key index fits"));
    let epoch =
        std::array::from_fn(|index| u8::try_from(index + 32).expect("epoch fixture index fits"));
    let revision = revision(
        &key,
        &epoch,
        RevisionDomain::HostSourceEvidenceV1,
        &[b"alpha", b""],
    )
    .expect("fixed bounded vector is accepted");

    assert_eq!(
        revision.as_str(),
        "hmac-sha256:ee9a7c99c9fc974f2f163c1201eb1f6e4ac52cf0111b29470a081087f8482a7c"
    );
}

#[test]
fn framing_limit_accepts_exact_total_and_rejects_one_byte_over() {
    let maximum_payload = max_frame_bytes_for_test() - fixed_frame_bytes_for_test() - 8;
    assert_eq!(
        checked_frame_size_for_test(&[maximum_payload]).expect("exact limit is allowed"),
        max_frame_bytes_for_test()
    );
    assert_eq!(
        checked_frame_size_for_test(&[maximum_payload + 1]),
        Err(CatalogAuthorityError::InvalidEvidence)
    );
}

#[test]
fn framing_field_count_and_checked_arithmetic_fail_closed_without_large_payloads() {
    let accepted = vec![0; max_fields_for_test()];
    assert_eq!(
        checked_frame_size_for_test(&accepted).expect("maximum field count is accepted"),
        fixed_frame_bytes_for_test() + max_fields_for_test() * 8
    );
    let too_many = vec![0; max_fields_for_test() + 1];
    assert_eq!(
        checked_frame_size_for_test(&too_many),
        Err(CatalogAuthorityError::InvalidEvidence)
    );
    assert_eq!(
        checked_frame_size_for_test(&[usize::MAX]),
        Err(CatalogAuthorityError::InvalidEvidence)
    );
}
