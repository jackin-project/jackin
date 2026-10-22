// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use crate::{Agent, profile_credential_material_revision};

#[test]
fn v1_number_normalization_and_large_integer_collapse_remain_unchanged() {
    let integer = profile_credential_material_revision(Agent::Codex, b"1")
        .expect("V1 integer fixture must remain accepted");
    let normalized_float = profile_credential_material_revision(Agent::Codex, b"1.0")
        .expect("V1 floating-point fixture must remain accepted");
    assert_ne!(integer, normalized_float);

    for raw in [b"1.0".as_slice(), b"1e0".as_slice(), b"1E+0".as_slice()] {
        let normalized = profile_credential_material_revision(Agent::Codex, raw)
            .expect("V1 number normalization fixture must remain accepted");
        assert_eq!(normalized, normalized_float);
    }
    let first_large_integer =
        profile_credential_material_revision(Agent::Codex, b"18446744073709551616")
            .expect("V1 large integer fixture must remain accepted");
    let second_large_integer =
        profile_credential_material_revision(Agent::Codex, b"18446744073709551617")
            .expect("V1 adjacent large integer fixture must remain accepted");
    assert_eq!(first_large_integer, second_large_integer);
}

#[test]
fn v1_still_accepts_whitespace_and_arrays_larger_than_new_v2_limits() {
    let mut padded = vec![b' '; 8 * 1024 * 1024 + 1];
    padded.extend_from_slice(b"null");
    profile_credential_material_revision(Agent::Codex, &padded)
        .expect("V1 must accept whitespace beyond the new V2 byte limit");

    let mut values = Vec::with_capacity(65_536 * 5 + 2);
    values.push(b'[');
    for index in 0..65_536 {
        if index != 0 {
            values.push(b',');
        }
        values.extend_from_slice(b"null");
    }
    values.push(b']');
    profile_credential_material_revision(Agent::Codex, &values)
        .expect("V1 must accept arrays beyond the new V2 value limit");
}

#[test]
fn v1_amp_still_accepts_large_unambiguous_ignored_metadata() {
    let prefix = br#"{"apiKey@https://ampcode.com/":"token","foreign":""#;
    let suffix = b"\"}";
    let mut raw = Vec::with_capacity(8 * 1024 * 1024 + 64);
    raw.extend_from_slice(prefix);
    raw.extend(std::iter::repeat_n(b'x', 8 * 1024 * 1024));
    raw.extend_from_slice(suffix);
    let parsed: serde_json::Value =
        serde_json::from_slice(&raw).expect("large V1 Amp fixture must be valid JSON");
    assert_eq!(
        parsed
            .get("apiKey@https://ampcode.com/")
            .and_then(serde_json::Value::as_str),
        Some("token")
    );
    assert_eq!(
        parsed
            .get("foreign")
            .and_then(serde_json::Value::as_str)
            .map(str::len),
        Some(8 * 1024 * 1024)
    );
    assert!(raw.len() > 8 * 1024 * 1024);
    profile_credential_material_revision(Agent::Amp, &raw)
        .expect("V1 must accept large unambiguous Amp metadata");
}
