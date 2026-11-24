// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin_usage::accounts::{AccountPreferences, ConnectedAccount, UsageProvider};
use jackin_usage::export::snapshot_account;
use serde_json::json;
use uuid::Uuid;

#[test]
fn public_snapshot_keeps_quantities_and_excludes_credentials_inventory_and_history() {
    let account = ConnectedAccount {
        id: Uuid::new_v4(),
        provider: UsageProvider::Claude,
        label: "Work".into(),
        position: 2,
        preferences: AccountPreferences::default(),
    };
    let private = json!({
        "plan":"Enterprise","resources":{
            "extraUsage":{"kind":"consumption","unit":"jpy","used":0,"limit":4000,"resetsAt":null,"windowSeconds":18000,"header":"secret"},
            "rateLimitResets":{"kind":"balance","unit":"resets","available":2}},
        "resets":[{"id":"private-credit","expiresAt":40},{"id":"another","expiresAt":30}],
        "project":"private-project","history":{"byModel":{"private-name":1}},
        "planReadAt":12,"access_token":"private-token","url":"private-hub"});
    assert_eq!(
        snapshot_account(&account, Some(20), Some(&private), 620),
        json!({
            "account":{"id":account.id,"label":"Work","provider":"claude"},"fetched_at":20,"expires_at":620,"stale":true,
            "usage":{"plan":"Enterprise","resources":{
                "extraUsage":{"kind":"consumption","unit":"jpy","used":0,"limit":4000,"utilization":0.0,"window_seconds":18000},
                "rateLimitResets":{"kind":"balance","unit":"resets","available":2,"expires_at":[30,40]}}}
        })
    );
    let second = ConnectedAccount {
        id: Uuid::new_v4(),
        ..account
    };
    let missing = snapshot_account(&second, None, None, 620);
    assert_eq!(missing["account"]["id"], second.id.to_string());
    assert!(missing["usage"].is_null());
    assert!(missing.get("fetched_at").is_none());
}

#[test]
fn exact_string_amounts_pass_through_the_snapshot_and_the_details_without_quotes() {
    use jackin_usage::dashboard::DashboardSettings;
    use jackin_usage::export::details_account;

    let account = ConnectedAccount {
        id: Uuid::new_v4(),
        provider: UsageProvider::OpenRouter,
        label: "Router".into(),
        position: 0,
        preferences: AccountPreferences::default(),
    };
    let snapshot = json!({
        "resources":{
            "keyLimit":{"kind":"consumption","unit":"usd","used":"25","limit":"100","remaining":"75"},
            "balance":{"kind":"balance","unit":"usd","available":"0.0000000000000000001"}},
        "period_spend":{"today":{"spend":"0.1000000000000000001","byokSpend":"0.2","tokens":3}},
        "activity":{"as_of":"2025-01-05","expected_days":1,"observed_days":1,
            "prior30Days":{"spend":"0.1000000000000000001"},
            "days":{"2025-01-05":{"totals":{"spend":"0.1000000000000000001","tokens":3},
                "by_model":{"alpha":{"spend":"0.1000000000000000001","tokens":3}}}}}
    });
    let public = snapshot_account(&account, Some(20), Some(&snapshot), 620);
    assert_eq!(
        public["usage"]["resources"],
        json!({
            "keyLimit":{"kind":"consumption","unit":"usd","used":"25","limit":"100","remaining":"75","utilization":0.25},
            "balance":{"kind":"balance","unit":"usd","available":"0.0000000000000000001"}})
    );
    let row = (account, Some(20), Some(snapshot));
    let report = details_account(
        row,
        0,
        &DashboardSettings::default(),
        None,
        jackin_core::Timestamp(620),
    );
    assert_eq!(
        report["usage"]["period_spend"]["today"],
        json!({"spend":"0.1000000000000000001","byokSpend":"0.2","tokens":3})
    );
    assert_eq!(
        report["usage"]["activity"]["days"]["2025-01-05"]["by_model"]["alpha"]["spend"],
        "0.1000000000000000001"
    );
    assert_eq!(
        report["usage"]["activity"]["prior30Days"]["spend"],
        "0.1000000000000000001"
    );
    let lines: Vec<String> = serde_json::from_value(report["details"].clone()).unwrap_or_default();
    assert!(
        lines
            .iter()
            .any(|line| line == "Activity · 2025-01-05 · spend 0.1000000000000000001 · tokens 3"),
        "{lines:?}"
    );
    assert!(lines.iter().all(|line| !line.contains('"')), "{lines:?}");
}
