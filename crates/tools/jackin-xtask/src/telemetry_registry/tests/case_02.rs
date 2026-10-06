// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

#[test]
fn namespace_scan_detects_telemetry_literals_without_flagging_identifiers() {
    let path = "fixture.rs";
    assert!(contains_legacy_telemetry_name(
        path,
        "const FIELD: &str = \"jackin.unregistered.field\";"
    ));
    assert!(contains_legacy_telemetry_name(
        path,
        "record.insert(\"parallax.unregistered\", value);"
    ));
    assert!(contains_legacy_telemetry_name(
        path,
        "pub const LABEL_ROLE_KEY: &str = \"jackin.role\";"
    ));
    assert!(!contains_legacy_telemetry_name(
        "crates/services/jackin-runtime/src/runtime/naming.rs",
        "pub const LABEL_ROLE_KEY: &str = \"jackin.role\";"
    ));
    assert!(contains_legacy_telemetry_name(
        "crates/services/jackin-runtime/src/runtime/naming.rs",
        "pub const DIFFERENT_SYMBOL: &str = \"jackin.role\";"
    ));
    assert!(contains_legacy_telemetry_name(
        path,
        "attrs.get(\"jackin.unregistered\")"
    ));
    assert!(contains_legacy_telemetry_name(
        path,
        "map.get(\"jackin.role\"); emit(\"jackin.unregistered\")"
    ));
    assert!(contains_legacy_telemetry_name(
        path,
        "const LABEL_ROLE: &str = \"jackin.role\"; const FIELD: &str = \"parallax.bad\";"
    ));
    assert!(contains_legacy_telemetry_name(
        path,
        "path.join(\"jackin.state\"); record(\"jackin.bad\")"
    ));
    let negative_fixture = "crates/testing/jackin-otlp-testbed/src/tests.rs";
    assert!(!contains_legacy_telemetry_name(
        negative_fixture,
        "fn namespace_detector_rejects_synthetic_legacy_attribute() { let _ = \"jackin.synthetic\"; }"
    ));
    assert!(contains_legacy_telemetry_name(
        negative_fixture,
        "fn different_test() { let _ = \"jackin.synthetic\"; }"
    ));
}

#[test]
fn namespace_scan_handles_rust_literal_forms_and_macro_construction() {
    let path = "fixture.rs";
    assert!(contains_legacy_telemetry_name(
        path,
        r##"let _ = r#"jackin.raw.field"#;"##
    ));
    assert!(contains_legacy_telemetry_name(
        path,
        r#"let _ = "jackin.\u{72}aw.field";"#
    ));
    assert!(contains_legacy_telemetry_name(
        path,
        r#"let _ = b"parallax.byte.field";"#
    ));
    assert!(contains_legacy_telemetry_name(
        path,
        r#"let _ = concat!("jackin.", "concat.field");"#
    ));
    assert!(contains_legacy_telemetry_name(
        path,
        "let _ = stringify!(parallax.stringify.field);"
    ));
    assert!(contains_legacy_telemetry_name(
        path,
        "let _ = \"jackin.multiline\\\n.field\";"
    ));
}
