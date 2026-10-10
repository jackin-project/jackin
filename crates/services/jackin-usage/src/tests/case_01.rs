// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

#[test]
fn contract_baseline_projection_fixture_is_well_formed() {
    let fixture = read_json("usage-projection-v1-current.json");
    validate_projection_v1(&fixture).expect("canonical V1 fixture must satisfy contract");
}

#[test]
fn contract_projection_accepts_local_source_identity_kind() {
    let fixture = read_json("usage-projection-v1-current.json");
    let mut account = fixture["providers"][0]["accounts"][0].clone();
    account["canonical_account_id"] = Value::String("local-source-capability".to_owned());
    account["display_label"] = Value::String("Claude Code source".to_owned());
    account["identity_kind"] = Value::String("local_source_handle".to_owned());

    validate_account(&account).expect("local source identity has a distinct valid wire kind");
}

#[test]
fn contract_baseline_projection_rejects_invalid_fixtures() {
    let fixture = read_json("usage-projection-v1-invalid.json");
    let cases = fixture
        .as_array()
        .expect("invalid fixture must be a JSON array");
    assert!(
        !cases.is_empty(),
        "invalid fixture matrix must not be empty"
    );
    for case in cases {
        let id = required_string(case, "id").expect("invalid case needs an id");
        let projection = case
            .get("projection")
            .expect("invalid case needs a projection");
        assert!(
            validate_projection_v1(projection).is_err(),
            "invalid case {id} unexpectedly passed"
        );
    }
}

#[test]
fn contract_baseline_surface_matrix_names_every_state_family() {
    let matrix: SurfaceMatrix = serde_json::from_value(read_json("surface-matrix.json"))
        .expect("surface matrix must parse");
    assert_eq!(matrix.schema_version, 1);
    let actual = matrix
        .cases
        .iter()
        .map(|case| case.id.as_str())
        .collect::<BTreeSet<_>>();
    let expected = [
        "cli-human-json",
        "console-major-states",
        "capsule-lifecycle",
        "desktop-runtime-accessibility",
        "cross-surface-partial-stale",
    ]
    .into_iter()
    .collect::<BTreeSet<_>>();
    assert_eq!(actual, expected);
    for case in &matrix.cases {
        assert!(!case.surfaces.is_empty(), "{} has no surface", case.id);
        assert!(!case.state.is_empty(), "{} has no state", case.id);
        assert!(!case.dimensions.is_empty(), "{} has no dimensions", case.id);
    }
}

#[test]
fn contract_baseline_provider_calls_have_no_unclassified_route() {
    let allowlist: BypassAllowlist =
        serde_json::from_value(read_json("provider-call-allowlist.json"))
            .expect("provider call allowlist must parse");
    assert_eq!(allowlist.schema_version, 1);
    for call in &allowlist.calls {
        assert!(
            matches!(
                call.classification.as_str(),
                "broker_executor" | "adapter_internal" | "legacy_bypass"
            ),
            "{}:{} has unknown classification {}",
            call.path,
            call.symbol,
            call.classification
        );
    }
    let expected = allowlist
        .calls
        .iter()
        .map(|call| format!("{}|{}", call.path, call.symbol))
        .collect::<BTreeSet<_>>();
    assert_eq!(
        expected.len(),
        allowlist.calls.len(),
        "provider call allowlist contains duplicates"
    );

    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .and_then(Path::parent)
        .expect("crate must live below workspace root");
    let symbols = allowlist
        .calls
        .iter()
        .map(|call| call.symbol.as_str())
        .collect::<BTreeSet<_>>();
    let actual = scan_production_calls(root, &symbols);
    assert_eq!(actual, expected, "provider-call inventory drifted");
}

#[test]
fn contract_baseline_provider_calls_detect_injected_route() {
    let workspace = tempfile::tempdir().expect("temporary workspace must exist");
    let source_dir = workspace.path().join("crates/consumer/src");
    fs::create_dir_all(&source_dir).expect("fixture source directory must exist");
    fs::write(
        source_dir.join("lib.rs"),
        r#"use super::{unused_import_route};
fn unused_definition() {}
pub fn another_unused_definition() {}
fn bypass() {
    fetch_codex_rpc_usage();
    dispatch(first_callback, middle_callback, last_callback);
    not_fetch_codex_rpc_usage();
    not_unused_import_route();
    unused_import_route_suffix();
}
"#,
    )
    .expect("fixture source must be writable");
    let symbols = [
        "fetch_codex_rpc_usage",
        "first_callback",
        "middle_callback",
        "last_callback",
        "unused_definition",
        "another_unused_definition",
        "unused_import_route",
    ]
    .into_iter()
    .collect();
    let calls = scan_production_calls(workspace.path(), &symbols);
    assert_eq!(
        calls,
        [
            "crates/consumer/src/lib.rs|fetch_codex_rpc_usage".to_owned(),
            "crates/consumer/src/lib.rs|first_callback".to_owned(),
            "crates/consumer/src/lib.rs|middle_callback".to_owned(),
            "crates/consumer/src/lib.rs|last_callback".to_owned(),
        ]
        .into_iter()
        .collect()
    );
}
