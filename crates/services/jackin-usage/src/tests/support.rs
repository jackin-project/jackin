// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;
pub(super) const FIXTURES: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/contracts");

#[derive(Debug, Deserialize)]
pub(super) struct SurfaceMatrix {
    pub(super) schema_version: u64,
    pub(super) cases: Vec<SurfaceCase>,
}

#[derive(Debug, Deserialize)]
pub(super) struct SurfaceCase {
    pub(super) id: String,
    pub(super) surfaces: Vec<String>,
    pub(super) state: String,
    pub(super) dimensions: Vec<String>,
}

#[derive(Debug, Deserialize)]
pub(super) struct BypassAllowlist {
    pub(super) schema_version: u64,
    pub(super) calls: Vec<AllowedCall>,
}

#[derive(Debug, Deserialize)]
pub(super) struct AllowedCall {
    pub(super) path: String,
    pub(super) symbol: String,
    pub(super) classification: String,
}

pub(super) fn validate_projection_v1(value: &Value) -> Result<(), String> {
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

pub(super) fn validate_provider(provider: &Value) -> Result<(), String> {
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

pub(super) fn validate_account(account: &Value) -> Result<(), String> {
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

pub(super) fn validate_window(window: &Value) -> Result<(), String> {
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

pub(super) fn validate_freshness(value: Option<&Value>) -> Result<(), String> {
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

pub(super) fn optional_percent(value: &Value, key: &str) -> Result<Option<u64>, String> {
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

pub(super) fn required_enum(value: &Value, key: &str, allowed: &[&str]) -> Result<(), String> {
    let found = required_string(value, key)?;
    if allowed.contains(&found) {
        Ok(())
    } else {
        Err(format!("invalid {key}: {found}"))
    }
}

pub(super) fn required_string<'a>(value: &'a Value, key: &str) -> Result<&'a str, String> {
    value
        .get(key)
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| format!("{key} must be a non-empty string"))
}

pub(super) fn required_u64(value: &Value, key: &str) -> Result<u64, String> {
    value
        .get(key)
        .and_then(Value::as_u64)
        .ok_or_else(|| format!("{key} must be an unsigned integer"))
}

pub(super) fn required_i64(value: &Value, key: &str) -> Result<i64, String> {
    value
        .get(key)
        .and_then(Value::as_i64)
        .ok_or_else(|| format!("{key} must be an integer"))
}

pub(super) fn read_json(name: &str) -> Value {
    let path = Path::new(FIXTURES).join(name);
    serde_json::from_str(&fs::read_to_string(&path).expect("contract fixture must exist"))
        .unwrap_or_else(|error| panic!("{}: {error}", path.display()))
}

pub(super) fn scan_production_calls(root: &Path, symbols: &BTreeSet<&str>) -> BTreeSet<String> {
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
        if relative_text == "crates/services/jackin-usage/src/contract_baseline.rs" {
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

pub(super) fn collect_rust_files(directory: &Path, files: &mut Vec<PathBuf>) {
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
