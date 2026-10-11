// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

pub(super) fn window_fixture(status: &str, body: serde_json::Value) -> serde_json::Value {
    let mut window = serde_json::Map::new();
    window.insert(
        "status".to_owned(),
        serde_json::Value::String(status.to_owned()),
    );
    if let serde_json::Value::Object(extra) = body {
        window.extend(extra);
    }
    serde_json::Value::Object(window)
}

pub(super) fn usage_fixture(
    rolling: serde_json::Value,
    weekly: serde_json::Value,
    monthly: serde_json::Value,
) -> serde_json::Value {
    serde_json::json!({
        "usage": {"rolling": rolling, "weekly": weekly, "monthly": monthly}
    })
}
