// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin_usage::accounts::{ConnectedAccount, UsageProvider};
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
        preferences: Default::default(),
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
