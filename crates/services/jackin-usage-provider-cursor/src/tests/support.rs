// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

pub(super) const PERIOD_FIXTURE: &str = r#"{
    "usage": {
        "enabled": true,
        "planUsage": {"limit": 20, "totalPercentUsed": 34, "totalSpend": 6.8},
        "spendLimitUsage": {"limitType": "personal", "pooledLimit": 0}
    }
}"#;

pub(super) const SUMMARY_FIXTURE: &str = r#"{
    "billingCycleStart": "2026-09-01T00:00:00Z",
    "billingCycleEnd": "2026-10-01T00:00:00Z",
    "membershipType": "pro",
    "individualUsage": {
        "plan": {"totalPercentUsed": 41, "autoPercentUsed": 12, "apiPercentUsed": 3},
        "onDemand": 4.25,
        "overall": 77
    },
    "teamUsage": {"onDemand": 1.5, "pooled": 9.75}
}"#;

pub(super) fn canned_dashboard_response(request: &[u8]) -> &'static str {
    let text = String::from_utf8_lossy(request);
    if text.contains("GetCurrentPeriodUsage") {
        PERIOD_FIXTURE
    } else if text.contains("GetPlanInfo") {
        // Live shape nests the label under `planInfo`.
        r#"{"planInfo": {"planName": "pro_plus"}}"#
    } else if text.contains("GetCreditGrantsBalance") {
        r#"{"grantTotal": 1500}"#
    } else {
        "{}"
    }
}
