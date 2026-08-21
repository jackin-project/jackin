use std::{
    collections::BTreeSet,
    fs,
    path::{Path, PathBuf},
};

use serde::Deserialize;
use serde_json::Value;

const FIXTURES: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/contracts");

#[derive(Debug, Deserialize)]
struct SurfaceMatrix {
    schema_version: u64,
    cases: Vec<SurfaceCase>,
}

#[derive(Debug, Deserialize)]
struct SurfaceCase {
    id: String,
    surfaces: Vec<String>,
    state: String,
    dimensions: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct BypassAllowlist {
    schema_version: u64,
    calls: Vec<AllowedCall>,
}

#[derive(Debug, Deserialize)]
struct AllowedCall {
    path: String,
    symbol: String,
    classification: String,
}

#[test]
fn contract_baseline_projection_fixture_is_well_formed() {
    let fixture = read_json("usage-projection-v1-current.json");
    validate_projection_v1(&fixture).expect("canonical V1 fixture must satisfy contract");
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
        "fn bypass() {\n    fetch_codex_rpc_usage();\n}\n",
    )
    .expect("fixture source must be writable");
    let symbols = ["fetch_codex_rpc_usage"].into_iter().collect();
    let calls = scan_production_calls(workspace.path(), &symbols);
    assert_eq!(
        calls,
        ["crates/consumer/src/lib.rs|fetch_codex_rpc_usage".to_owned()]
            .into_iter()
            .collect()
    );
}

fn validate_projection_v1(value: &Value) -> Result<(), String> {
    let object = value
        .as_object()
        .ok_or_else(|| "projection must be an object".to_owned())?;
    if object.get("schema_version").and_then(Value::as_u64) != Some(1) {
        return Err("schema_version must be 1".to_owned());
    }
    for key in ["projection_id", "discovery_revision", "broker_instance_id"] {
        required_string(value, key)?;
    }
    required_i64(value, "generated_at_epoch")?;
    required_u64(value, "broker_generation")?;
    required_enum(value, "refresh_state", &["idle", "refreshing"])?;
    for key in ["providers", "unresolved", "issues"] {
        if !object.get(key).is_some_and(Value::is_array) {
            return Err(format!("{key} must be an array"));
        }
    }
    for provider in object["providers"]
        .as_array()
        .expect("providers checked above")
    {
        validate_provider(provider)?;
    }
    Ok(())
}

fn validate_provider(provider: &Value) -> Result<(), String> {
    required_string(provider, "provider_id")?;
    required_string(provider, "display_name")?;
    required_u64(provider, "rank")?;
    required_enum(provider, "membership_state", &["current"])?;
    validate_freshness(provider.get("freshness"))?;
    let accounts = provider
        .get("accounts")
        .and_then(Value::as_array)
        .ok_or_else(|| "provider accounts must be an array".to_owned())?;
    for account in accounts {
        validate_account(account)?;
    }
    Ok(())
}

fn validate_account(account: &Value) -> Result<(), String> {
    required_string(account, "canonical_account_id")?;
    required_u64(account, "rank")?;
    required_string(account, "display_label")?;
    required_enum(
        account,
        "identity_kind",
        &["provider_account_id", "provider_stable_handle"],
    )?;
    required_enum(
        account,
        "lifecycle",
        &[
            "available",
            "agent_uninitialized",
            "needs_login",
            "needs_secret",
            "unsupported",
            "unavailable",
            "error",
        ],
    )?;
    validate_freshness(account.get("freshness"))?;
    let windows = account
        .get("windows")
        .and_then(Value::as_array)
        .ok_or_else(|| "account windows must be an array".to_owned())?;
    for window in windows {
        validate_window(window)?;
    }
    Ok(())
}

fn validate_window(window: &Value) -> Result<(), String> {
    required_string(window, "window_id")?;
    required_u64(window, "rank")?;
    required_string(window, "label")?;
    required_string(window, "value_label")?;
    required_string(window, "reset_label")?;
    required_enum(
        window,
        "quota_state",
        &[
            "available",
            "not_started",
            "warning",
            "exhausted",
            "unsupported",
            "unavailable",
            "error",
        ],
    )?;
    let remaining = optional_percent(window, "remaining_percent")?;
    let used = optional_percent(window, "used_percent")?;
    if remaining.is_some() == used.is_some() {
        return Err("window needs exactly one percent representation".to_owned());
    }
    Ok(())
}

fn validate_freshness(value: Option<&Value>) -> Result<(), String> {
    let value = value.ok_or_else(|| "freshness is required".to_owned())?;
    required_u64(value, "generation")?;
    required_enum(
        value,
        "phase",
        &["current", "stale", "refreshing", "failed"],
    )?;
    if !value.get("is_stale").is_some_and(Value::is_boolean) {
        return Err("freshness is_stale must be boolean".to_owned());
    }
    Ok(())
}

fn optional_percent(value: &Value, key: &str) -> Result<Option<u64>, String> {
    match value.get(key) {
        None => Ok(None),
        Some(Value::Null) => Err(format!("{key} must be omitted, not null")),
        Some(value) => {
            let percent = value
                .as_u64()
                .ok_or_else(|| format!("{key} must be an unsigned integer"))?;
            if percent > 100 {
                return Err(format!("{key} exceeds 100"));
            }
            Ok(Some(percent))
        }
    }
}

fn required_enum(value: &Value, key: &str, allowed: &[&str]) -> Result<(), String> {
    let found = required_string(value, key)?;
    if allowed.contains(&found) {
        Ok(())
    } else {
        Err(format!("invalid {key}: {found}"))
    }
}

fn required_string<'a>(value: &'a Value, key: &str) -> Result<&'a str, String> {
    value
        .get(key)
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| format!("{key} must be a non-empty string"))
}

fn required_u64(value: &Value, key: &str) -> Result<u64, String> {
    value
        .get(key)
        .and_then(Value::as_u64)
        .ok_or_else(|| format!("{key} must be an unsigned integer"))
}

fn required_i64(value: &Value, key: &str) -> Result<i64, String> {
    value
        .get(key)
        .and_then(Value::as_i64)
        .ok_or_else(|| format!("{key} must be an integer"))
}

fn read_json(name: &str) -> Value {
    let path = Path::new(FIXTURES).join(name);
    serde_json::from_str(&fs::read_to_string(&path).expect("contract fixture must exist"))
        .unwrap_or_else(|error| panic!("{}: {error}", path.display()))
}

fn scan_production_calls(root: &Path, symbols: &BTreeSet<&str>) -> BTreeSet<String> {
    let mut files = Vec::new();
    collect_rust_files(&root.join("crates"), &mut files);
    let mut calls = BTreeSet::new();
    for path in files {
        let relative = path
            .strip_prefix(root)
            .expect("scanned path must be below workspace root");
        let relative_text = relative.to_string_lossy().replace('\\', "/");
        if relative_text.contains("/tests/") || relative_text.ends_with("/tests.rs") {
            continue;
        }
        if relative_text == "crates/jackin-usage/src/contract_baseline.rs" {
            continue;
        }
        let source = fs::read_to_string(&path).expect("Rust source must be readable");
        for line in source.lines() {
            let trimmed = line.trim_start();
            if trimmed.starts_with("fn ")
                || trimmed.starts_with("pub fn ")
                || trimmed.starts_with("pub(crate) fn ")
            {
                continue;
            }
            for symbol in symbols {
                if line.contains(&format!("{symbol}(")) {
                    calls.insert(format!("{relative_text}|{symbol}"));
                }
            }
        }
    }
    calls
}

fn collect_rust_files(directory: &Path, files: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(directory) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_rust_files(&path, files);
        } else if path.extension().is_some_and(|extension| extension == "rs") {
            files.push(path);
        }
    }
}
