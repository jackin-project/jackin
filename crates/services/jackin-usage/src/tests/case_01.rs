// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

#[test]
fn contract_current_projection_v2_fixture_is_well_formed() {
    let fixture = read_json("usage-projection-v2-current.json");
    validate_projection_v2(&fixture).expect("canonical V2 fixture must satisfy contract");
}

#[test]
fn contract_v2_fixture_preserves_quota_observations_and_money_separately() {
    let fixture = read_json("usage-projection-v2-current.json");
    let projection: jackin_protocol::usage_broker::UsageProjectionV1 =
        serde_json::from_value(fixture).expect("current V2 fixture must decode");
    projection
        .validate()
        .expect("current V2 fixture must validate");

    let account = &projection.providers[0].accounts[0];
    let requests = &account.windows[0];
    assert_eq!(
        requests
            .remaining_percent
            .map(jackin_protocol::control::RawPercentPoints::get),
        Some(57.125)
    );
    assert_eq!(
        requests.remaining_percent_origin,
        Some(jackin_protocol::control::PercentObservationOrigin::ProviderReported)
    );
    assert_eq!(
        requests
            .used_percent
            .map(jackin_protocol::control::RawPercentPoints::get),
        Some(42.875)
    );
    assert_eq!(
        requests.used_percent_origin,
        Some(jackin_protocol::control::PercentObservationOrigin::Derived)
    );
    assert_eq!(
        requests
            .meter_percent
            .map(jackin_protocol::control::MeterPercentPoints::get),
        Some(25.0)
    );
    assert_eq!(
        requests.meter_side,
        Some(jackin_protocol::control::UsageMeterSide::Remaining)
    );
    let count = requests.count_quota.expect("full-width count quota");
    assert_eq!(count.used, Some(9_007_199_254_740_993));
    assert_eq!(count.limit, Some(u64::MAX));
    assert_eq!(count.remaining, Some(4_611_686_018_427_387_903));
    assert_eq!(
        requests
            .money_used_percent
            .map(jackin_protocol::control::RawPercentPoints::get),
        Some(132.875)
    );

    let unlimited = &account.windows[1];
    assert_eq!(
        unlimited.provider_quota_state,
        Some(jackin_protocol::control::ProviderQuotaState::Unlimited)
    );
    assert_eq!(
        unlimited.quota_state,
        jackin_protocol::usage_broker::UsageQuotaStateV1::NotApplicable
    );
    assert_eq!(unlimited.meter_percent, None);
    assert_eq!(unlimited.meter_side, None);

    let spend = account
        .metric_groups
        .iter()
        .find(|group| group.group_id == "account-spend")
        .expect("separate spend-cap group");
    assert!(matches!(
        &spend.value,
        jackin_protocol::usage_broker::UsageMetricValueV1::SpendCap {
            cap: Some(cap),
            spent: Some(spent),
            remaining: Some(remaining),
        } if cap == &jackin_protocol::control::Money::new(10_000, "USD", 2)
            && spent == &jackin_protocol::control::Money::new(132_875, "USD", 2)
            && remaining == &jackin_protocol::control::Money::new(-122_875, "USD", 2)
    ));
}

#[test]
fn contract_historical_v1_projection_remains_a_private_conversion_input() {
    let fixture = read_json("usage-projection-v1-current.json");
    validate_projection_v1(&fixture).expect("historical V1 fixture must remain valid");
    assert!(
        validate_projection_v2(&fixture).is_err(),
        "schema 1 input must not validate as the current V2 contract"
    );
}

#[test]
fn contract_projection_accepts_local_source_identity_kind() {
    let mut fixture = read_json("usage-projection-v2-current.json");
    let account = &mut fixture["providers"][0]["accounts"][0];
    account["canonical_account_id"] = Value::String("local-source-capability".to_owned());
    account["display_label"] = Value::String("Claude Code source".to_owned());
    account["identity_kind"] = Value::String("local_source_handle".to_owned());

    validate_projection_v2(&fixture).expect("local source identity has a distinct valid wire kind");
}

#[test]
fn contract_projection_accepts_unverified_identity_without_claiming_authority() {
    let mut fixture = read_json("usage-projection-v2-current.json");
    fixture["providers"][0]["accounts"][0]["identity_kind"] =
        Value::String("unverified_handle".to_owned());

    validate_projection_v2(&fixture)
        .expect("unverified legacy identity has an explicit non-authoritative wire kind");
}

#[test]
fn contract_current_projection_v2_rejects_invalid_fixtures() {
    let fixture = read_json("usage-projection-v2-invalid.json");
    let cases = fixture
        .as_array()
        .expect("invalid fixture must be a JSON array");
    assert!(
        !cases.is_empty(),
        "invalid fixture matrix must not be empty"
    );
    for case in cases {
        let id = required_string(case, "id").expect("invalid case needs an id");
        let projection = if let Some(projection) = case.get("projection") {
            projection.clone()
        } else {
            assert_eq!(
                id, "unpaired-remaining-percent-origin",
                "only the origin-pair case derives from the current fixture"
            );
            let mut current = read_json("usage-projection-v2-current.json");
            current["providers"][0]["accounts"][0]["windows"][0]
                .as_object_mut()
                .expect("current quota window must be an object")
                .remove("remaining_percent_origin")
                .expect("test mutation removes the paired source origin");
            current
        };
        let error = validate_projection_v2(&projection)
            .expect_err("invalid fixture must fail the current V2 contract");
        if id == "unpaired-remaining-percent-origin" {
            assert!(
                error.contains("remaining percent and origin without a matching pair"),
                "origin mismatch must fail for its own invariant: {error}"
            );
        }
        assert!(
            !error.is_empty(),
            "invalid case {id} must report a contract error"
        );
    }
}

#[test]
fn contract_historical_v1_invalid_vectors_remain_rejected_by_the_legacy_validator() {
    let fixture = read_json("usage-projection-v1-invalid.json");
    let cases = fixture
        .as_array()
        .expect("historical invalid fixture must be a JSON array");
    assert!(
        !cases.is_empty(),
        "historical invalid matrix must not be empty"
    );
    for case in cases {
        let id = required_string(case, "id").expect("invalid case needs an id");
        let projection = case
            .get("projection")
            .expect("historical invalid case needs a projection");
        assert!(
            validate_projection_v1(projection).is_err(),
            "historical invalid case {id} unexpectedly passed"
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
        r"use super::{unused_import_route};
fn unused_definition() {}
pub fn another_unused_definition() {}
fn bypass() {
    fetch_codex_rpc_usage();
    dispatch(first_callback, middle_callback, last_callback);
    not_fetch_codex_rpc_usage();
    not_unused_import_route();
    unused_import_route_suffix();
}
",
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
