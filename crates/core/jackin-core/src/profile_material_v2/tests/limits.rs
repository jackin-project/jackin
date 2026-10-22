// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::path::Path;

use serde_json::Value;

use crate::{
    Agent, ProfileCredentialV2Error, profile_credential_material_revision_v2,
    profile_credential_source_identity_v2,
};

use super::super::json::{MAX_CONTAINER_DEPTH, MAX_INPUT_BYTES, MAX_JSON_VALUES};

fn padded_null(size: usize) -> Vec<u8> {
    let mut raw = vec![b' '; size.saturating_sub(4)];
    raw.extend_from_slice(b"null");
    raw
}

fn null_array(items: usize) -> Vec<u8> {
    let mut raw = Vec::with_capacity(items.saturating_mul(5).saturating_add(2));
    raw.push(b'[');
    for index in 0..items {
        if index != 0 {
            raw.push(b',');
        }
        raw.extend_from_slice(b"null");
    }
    raw.push(b']');
    raw
}

fn scalar_object(fields: usize) -> Vec<u8> {
    let mut raw = Vec::new();
    raw.push(b'{');
    for index in 0..fields {
        if index != 0 {
            raw.push(b',');
        }
        raw.extend_from_slice(format!("\"k{index}\":null").as_bytes());
    }
    raw.push(b'}');
    raw
}

fn array_depth(depth: usize) -> Vec<u8> {
    let mut raw = Vec::with_capacity(depth.saturating_mul(2).saturating_add(4));
    raw.extend(std::iter::repeat_n(b'[', depth));
    raw.extend_from_slice(b"null");
    raw.extend(std::iter::repeat_n(b']', depth));
    raw
}

fn object_depth(depth: usize) -> Vec<u8> {
    let mut raw = Vec::new();
    for _ in 0..depth {
        raw.extend_from_slice(br#"{"k":"#);
    }
    raw.extend_from_slice(b"null");
    raw.extend(std::iter::repeat_n(b'}', depth));
    raw
}

fn mixed_depth(depth: usize) -> Vec<u8> {
    let mut raw = Vec::new();
    let mut object_flags = Vec::new();
    for index in 0..depth {
        let object = index % 2 == 1;
        object_flags.push(object);
        if object {
            raw.extend_from_slice(br#"{"k":"#);
        } else {
            raw.push(b'[');
        }
    }
    raw.extend_from_slice(b"null");
    for object in object_flags.into_iter().rev() {
        raw.push(if object { b'}' } else { b']' });
    }
    raw
}

fn amp_with_ignored_string(size: usize) -> Vec<u8> {
    let prefix = br#"{"apiKey@https://ampcode.com/":"token","ignored":""#;
    let suffix = b"\"}";
    let payload_len = size
        .saturating_sub(prefix.len())
        .saturating_sub(suffix.len());
    let mut raw = Vec::with_capacity(size);
    raw.extend_from_slice(prefix);
    raw.extend(std::iter::repeat_n(b'x', payload_len));
    raw.extend_from_slice(suffix);
    raw
}

fn assert_valid_amp_ignored_fixture(raw: &[u8]) {
    let parsed: Value = serde_json::from_slice(raw).expect("large Amp fixture must be valid JSON");
    assert_eq!(
        parsed
            .get("apiKey@https://ampcode.com/")
            .and_then(Value::as_str),
        Some("token")
    );
    let ignored = parsed
        .get("ignored")
        .and_then(Value::as_str)
        .expect("large Amp fixture must contain an ignored string");
    assert!(ignored.bytes().all(|byte| byte == b'x'));
}

fn large_key_object(key_len: usize) -> Vec<u8> {
    let mut raw = Vec::with_capacity(key_len.saturating_add(9));
    raw.extend_from_slice(br"{");
    raw.push(b'"');
    raw.extend(std::iter::repeat_n(b'k', key_len));
    raw.extend_from_slice(br#"":null}"#);
    raw
}

fn mixed_value_object(items: usize) -> Vec<u8> {
    let mut raw = Vec::with_capacity(items.saturating_mul(5).saturating_add(16));
    raw.extend_from_slice(br#"{"values":["#);
    for index in 0..items {
        if index != 0 {
            raw.push(b',');
        }
        raw.extend_from_slice(b"null");
    }
    raw.extend_from_slice(b"]}");
    raw
}

#[test]
fn enforces_eight_mib_per_selector_and_json_material_input() {
    let exact = padded_null(MAX_INPUT_BYTES);
    let over = padded_null(MAX_INPUT_BYTES + 1);
    assert_eq!(exact.len(), MAX_INPUT_BYTES);
    assert_eq!(over.len(), MAX_INPUT_BYTES + 1);
    profile_credential_material_revision_v2(Agent::Codex, &exact)
        .expect("exact-size JSON material must be accepted");
    assert_eq!(
        profile_credential_material_revision_v2(Agent::Codex, &over).err(),
        Some(ProfileCredentialV2Error::InputTooLarge)
    );
    profile_credential_source_identity_v2(
        Agent::Codex,
        "openai",
        Path::new("/profiles/main"),
        Some(&exact),
    )
    .expect("exact-size selector JSON must be accepted");
    assert_eq!(
        profile_credential_source_identity_v2(
            Agent::Codex,
            "openai",
            Path::new("/profiles/main"),
            Some(&over),
        )
        .err(),
        Some(ProfileCredentialV2Error::InputTooLarge)
    );

    let malformed_over = vec![b'!'; MAX_INPUT_BYTES + 1];
    assert_eq!(
        profile_credential_material_revision_v2(Agent::Codex, &malformed_over).err(),
        Some(ProfileCredentialV2Error::InputTooLarge)
    );
    assert_eq!(
        profile_credential_source_identity_v2(
            Agent::Codex,
            "openai",
            Path::new("/profiles/main"),
            Some(&malformed_over),
        )
        .err(),
        Some(ProfileCredentialV2Error::InputTooLarge)
    );
}

#[test]
fn byte_limit_includes_long_keys_and_raw_numeric_tokens() {
    let exact_key = large_key_object(MAX_INPUT_BYTES - 9);
    let over_key = large_key_object(MAX_INPUT_BYTES - 8);
    assert_eq!(exact_key.len(), MAX_INPUT_BYTES);
    assert_eq!(over_key.len(), MAX_INPUT_BYTES + 1);
    profile_credential_material_revision_v2(Agent::Codex, &exact_key)
        .expect("exact-size JSON key must be accepted");
    assert_eq!(
        profile_credential_material_revision_v2(Agent::Codex, &over_key).err(),
        Some(ProfileCredentialV2Error::InputTooLarge)
    );

    let exact_number = format!("1{}", "2".repeat(MAX_INPUT_BYTES - 1));
    let over_number = format!("1{}", "2".repeat(MAX_INPUT_BYTES));
    profile_credential_material_revision_v2(Agent::Codex, exact_number.as_bytes())
        .expect("exact-size JSON number must be accepted");
    assert_eq!(
        profile_credential_material_revision_v2(Agent::Codex, over_number.as_bytes()).err(),
        Some(ProfileCredentialV2Error::InputTooLarge)
    );
}

#[test]
fn enforces_eight_mib_for_amp_ignored_metadata_and_opaque_omp_bytes() {
    let amp_exact = amp_with_ignored_string(MAX_INPUT_BYTES);
    let amp_over = amp_with_ignored_string(MAX_INPUT_BYTES + 1);
    assert_valid_amp_ignored_fixture(&amp_exact);
    assert_valid_amp_ignored_fixture(&amp_over);
    assert_eq!(amp_exact.len(), MAX_INPUT_BYTES);
    assert_eq!(amp_over.len(), MAX_INPUT_BYTES + 1);
    profile_credential_material_revision_v2(Agent::Amp, &amp_exact)
        .expect("exact-size Amp JSON material must be accepted");
    assert_eq!(
        profile_credential_material_revision_v2(Agent::Amp, &amp_over).err(),
        Some(ProfileCredentialV2Error::InputTooLarge)
    );

    let omp_exact = vec![0; MAX_INPUT_BYTES];
    let omp_over = vec![0; MAX_INPUT_BYTES + 1];
    profile_credential_material_revision_v2(Agent::Omp, &omp_exact)
        .expect("exact-size opaque OMP bytes must be accepted");
    assert_eq!(
        profile_credential_material_revision_v2(Agent::Omp, &omp_over).err(),
        Some(ProfileCredentialV2Error::InputTooLarge)
    );
}

#[test]
fn counts_root_containers_and_scalar_values_once_but_not_object_keys() {
    assert_eq!(MAX_JSON_VALUES, 65_536);
    profile_credential_material_revision_v2(Agent::Codex, b"null")
        .expect("a root scalar must count as one JSON value");
    profile_credential_material_revision_v2(Agent::Codex, b"[]")
        .expect("empty JSON array must be accepted");
    profile_credential_material_revision_v2(Agent::Codex, b"{}")
        .expect("empty JSON object must be accepted");
    profile_credential_material_revision_v2(Agent::Codex, b"{\"key\":null}")
        .expect("object with one null value must be accepted");

    let array_at_limit = null_array(MAX_JSON_VALUES - 1);
    let array_over_limit = null_array(MAX_JSON_VALUES);
    profile_credential_material_revision_v2(Agent::Codex, &array_at_limit)
        .expect("array at value limit must be accepted");
    assert_eq!(
        profile_credential_material_revision_v2(Agent::Codex, &array_over_limit).err(),
        Some(ProfileCredentialV2Error::TooManyValues)
    );

    let object_at_limit = scalar_object(MAX_JSON_VALUES - 1);
    let object_over_limit = scalar_object(MAX_JSON_VALUES);
    profile_credential_material_revision_v2(Agent::Codex, &object_at_limit)
        .expect("object at value limit must be accepted");
    assert_eq!(
        profile_credential_material_revision_v2(Agent::Codex, &object_over_limit).err(),
        Some(ProfileCredentialV2Error::TooManyValues)
    );

    let mixed_at_limit = mixed_value_object(MAX_JSON_VALUES - 2);
    let mixed_over_limit = mixed_value_object(MAX_JSON_VALUES - 1);
    profile_credential_material_revision_v2(Agent::Codex, &mixed_at_limit)
        .expect("mixed object at value limit must be accepted");
    assert_eq!(
        profile_credential_material_revision_v2(Agent::Codex, &mixed_over_limit).err(),
        Some(ProfileCredentialV2Error::TooManyValues)
    );

    profile_credential_source_identity_v2(
        Agent::Codex,
        "openai",
        Path::new("/profiles/main"),
        Some(&array_at_limit),
    )
    .expect("selector array at value limit must be accepted");
    assert_eq!(
        profile_credential_source_identity_v2(
            Agent::Codex,
            "openai",
            Path::new("/profiles/main"),
            Some(&array_over_limit),
        )
        .err(),
        Some(ProfileCredentialV2Error::TooManyValues)
    );
}

#[test]
fn enforces_container_depth_for_arrays_objects_and_mixed_nesting() {
    assert_eq!(MAX_CONTAINER_DEPTH, 127);
    for valid in [
        array_depth(MAX_CONTAINER_DEPTH),
        object_depth(MAX_CONTAINER_DEPTH),
        mixed_depth(MAX_CONTAINER_DEPTH),
    ] {
        profile_credential_material_revision_v2(Agent::Codex, &valid)
            .expect("JSON at container-depth limit must be accepted");
    }
    for too_deep in [
        array_depth(MAX_CONTAINER_DEPTH + 1),
        object_depth(MAX_CONTAINER_DEPTH + 1),
        mixed_depth(MAX_CONTAINER_DEPTH + 1),
    ] {
        assert_eq!(
            profile_credential_material_revision_v2(Agent::Codex, &too_deep).err(),
            Some(ProfileCredentialV2Error::TooDeep)
        );
    }
}

#[test]
fn sibling_branches_receive_independent_depth_with_shared_value_budget() {
    let branch = array_depth(MAX_CONTAINER_DEPTH - 1);
    let mut raw = Vec::with_capacity(branch.len().saturating_mul(2).saturating_add(3));
    raw.push(b'[');
    raw.extend_from_slice(&branch);
    raw.push(b',');
    raw.extend_from_slice(&branch);
    raw.push(b']');
    profile_credential_material_revision_v2(Agent::Codex, &raw)
        .expect("value-limit object with unique keys must be accepted");

    let left = null_array(MAX_JSON_VALUES / 2 - 1);
    let right = null_array(MAX_JSON_VALUES / 2 - 1);
    let mut shared_budget =
        Vec::with_capacity(left.len().saturating_add(right.len()).saturating_add(3));
    shared_budget.push(b'[');
    shared_budget.extend_from_slice(&left);
    shared_budget.push(b',');
    shared_budget.extend_from_slice(&right);
    shared_budget.push(b']');
    assert_eq!(
        profile_credential_material_revision_v2(Agent::Codex, &shared_budget).err(),
        Some(ProfileCredentialV2Error::TooManyValues)
    );
}

#[test]
fn late_duplicate_near_value_limit_returns_no_revision() {
    let canary_key = "PROFILE_V2_LATE_DUPLICATE_CANARY";
    let mut raw = String::from("{");
    for index in 0..MAX_JSON_VALUES - 5 {
        if index != 0 {
            raw.push(',');
        }
        raw.push_str(&format!("\"k{index}\":null"));
    }
    raw.push_str(&format!(",\"{canary_key}\":null,\"{canary_key}\":null}}"));
    let Err(error) = profile_credential_material_revision_v2(Agent::Codex, raw.as_bytes()) else {
        panic!("late duplicate-key JSON unexpectedly succeeded");
    };
    assert_eq!(error, ProfileCredentialV2Error::DuplicateKey);
    assert!(!error.to_string().contains(canary_key));
    assert!(!format!("{error:?}").contains(canary_key));
}

#[test]
fn object_value_budget_precedes_next_key_decoding_and_duplicate_checks() {
    let mut malformed_key = scalar_object(MAX_JSON_VALUES - 1);
    malformed_key.pop();
    malformed_key.extend_from_slice(br#","bad\q":null}"#);
    assert!(malformed_key.len() < MAX_INPUT_BYTES);
    assert_eq!(
        profile_credential_material_revision_v2(Agent::Codex, &malformed_key).err(),
        Some(ProfileCredentialV2Error::TooManyValues)
    );

    let mut duplicate_key = scalar_object(MAX_JSON_VALUES - 1);
    duplicate_key.pop();
    duplicate_key.extend_from_slice(br#","k0":null}"#);
    assert!(duplicate_key.len() < MAX_INPUT_BYTES);
    assert_eq!(
        profile_credential_material_revision_v2(Agent::Codex, &duplicate_key).err(),
        Some(ProfileCredentialV2Error::TooManyValues)
    );

    let mut nested_duplicate_key = null_array(MAX_JSON_VALUES - 3);
    nested_duplicate_key.pop();
    nested_duplicate_key.push(b',');
    nested_duplicate_key.extend_from_slice(br#"{"k":null,"k":null}]"#);
    assert!(nested_duplicate_key.len() < MAX_INPUT_BYTES);
    assert_eq!(
        profile_credential_material_revision_v2(Agent::Codex, &nested_duplicate_key).err(),
        Some(ProfileCredentialV2Error::TooManyValues)
    );
}
