// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

pub(super) fn codex_minimal_limits_value() -> serde_json::Value {
    serde_json::json!({
        "rateLimits": {
            "primary": {
                "usedPercent": 25.0,
                "windowDurationMins": 300,
                "resetsAt": 1_781_189_520_i64
            }
        }
    })
}

pub(super) fn test_jwt(payload: serde_json::Value) -> String {
    let header = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode("{}");
    let payload = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(payload.to_string());
    format!("{header}.{payload}.signature")
}

pub(super) const AMP_DAILY_FIXTURE: &str = "Signed in as user@example.com (example)\n\
     Amp Free: 61% remaining today (resets daily)\n\
     Individual credits: $9.86 remaining\n\
     Workspace example: $5.33 remaining";

pub(super) const AMP_TWO_WORKSPACE_FIXTURE: &str = "Amp Free: 61% remaining today (resets daily)\n\
     Individual credits: $9.86 remaining\n\
     Workspace alpha: $5.33 remaining\n\
     Workspace beta: $2.25 remaining";

pub(super) const AMP_TIER_FIXTURE: &str = "Signed in as user@example.com (example)\n\
     Amp Free: 61% remaining today (resets daily)\n\
     Amp Pro Tier: agent usage $80.00 of $100.00 remaining, orb usage 7.5h of 10h a1.small orb hours remaining, period 2026-09-01 to 2026-10-01, resets upon renewal in 12 days\n\
     Individual credits: $9.86 remaining";
