// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct LegacyFocusedAccountHeader {
    provider_label: String,
    account_label: String,
    #[serde(default)]
    username: Option<String>,
    plan_label: Option<String>,
    #[serde(default)]
    credential_origin: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct LegacyUsageProviderTab {
    #[serde(default)]
    id: String,
    label: String,
    status_label: String,
    account_label: String,
    plan_label: Option<String>,
    source_label: Option<String>,
    active: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct LegacyAccountUsageSnapshotView {
    provider: String,
    account_label: String,
    source: String,
    confidence: String,
    window_kind: String,
    used_amount: Option<i64>,
    used_unit: Option<String>,
    limit_amount: Option<i64>,
    limit_unit: Option<String>,
    resets_at: Option<i64>,
    fetched_at: i64,
    expires_at: Option<i64>,
    status: String,
    last_error: Option<String>,
    quota_bucket: Option<QuotaBucketView>,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
struct LegacyFocusedUsageView {
    focused_agent: Option<String>,
    focused_provider: Option<String>,
    account: LegacyFocusedAccountHeader,
    buckets: Vec<QuotaBucketView>,
    status: UsageSnapshotStatus,
    source: UsageSource,
    confidence: UsageConfidence,
    fetched_at_epoch: i64,
    updated_label: String,
    status_bar_label: String,
    tabs: Vec<LegacyUsageProviderTab>,
    last_error: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum LegacyServerMsg {
    UsageFocused {
        usage: Box<LegacyFocusedUsageView>,
    },
    UsageAccounts {
        accounts: Vec<LegacyAccountUsageSnapshotView>,
    },
}

fn capability() -> UsageAccountCapability {
    UsageAccountCapability {
        account_id: "acct-a".to_owned(),
        surface_id: "surface-a".to_owned(),
    }
}

fn legacy_header() -> LegacyFocusedAccountHeader {
    LegacyFocusedAccountHeader {
        provider_label: "OpenAI".to_owned(),
        account_label: "Shared label".to_owned(),
        username: None,
        plan_label: Some("Pro".to_owned()),
        credential_origin: None,
    }
}

fn current_header(account_capability: Option<UsageAccountCapability>) -> FocusedAccountHeader {
    FocusedAccountHeader {
        provider_label: "OpenAI".to_owned(),
        account_label: "Shared label".to_owned(),
        account_capability,
        username: None,
        plan_label: Some("Pro".to_owned()),
        credential_origin: None,
    }
}

fn legacy_tab() -> LegacyUsageProviderTab {
    LegacyUsageProviderTab {
        id: "sha256:legacy-label-key".to_owned(),
        label: "OpenAI".to_owned(),
        status_label: "fresh".to_owned(),
        account_label: "Shared label".to_owned(),
        plan_label: None,
        source_label: Some("provider API".to_owned()),
        active: true,
    }
}

fn current_tab(account_capability: Option<UsageAccountCapability>) -> UsageProviderTab {
    UsageProviderTab {
        id: "sha256:legacy-label-key".to_owned(),
        account_capability,
        label: "OpenAI".to_owned(),
        status_label: "fresh".to_owned(),
        account_label: "Shared label".to_owned(),
        plan_label: None,
        source_label: Some("provider API".to_owned()),
        active: true,
    }
}

fn count_bucket() -> QuotaBucketView {
    QuotaBucketView {
        label: "Requests".to_owned(),
        used_label: Some("18446744073709551615 / 18446744073709551615".to_owned()),
        limit_label: Some("18446744073709551615".to_owned()),
        used_percent: Some(CanonicalUsedPercent::new(43.375).unwrap()),
        used_percent_origin: Some(PercentObservationOrigin::ProviderReported),
        remaining_percent: Some(CanonicalRemainingPercent::new(56.625).unwrap()),
        remaining_percent_origin: Some(PercentObservationOrigin::Derived),
        count_quota: Some(CountQuota::new(
            Some(u64::MAX),
            Some(u64::MAX),
            Some(0),
            CountQuotaUnit::Requests,
            CountQuotaPeriod::UtcDaily,
            CountQuotaProvenance::ProviderReported,
        )),
        provider_quota_state: Some(ProviderQuotaState::Exhausted),
        reset_label: None,
        resets_at: Some(1_781_190_720),
        status_slot: Some(StatusSlot::Daily),
        pace_label: Some("on pace".to_owned()),
        status: UsageSnapshotStatus::Fresh,
        used_money: None,
        limit_money: None,
        severity: UsageSeverity::Danger,
    }
}

fn money_bucket() -> QuotaBucketView {
    QuotaBucketView {
        label: "Spend".to_owned(),
        used_label: None,
        limit_label: None,
        used_percent: Some(CanonicalUsedPercent::new(12.5).unwrap()),
        used_percent_origin: Some(PercentObservationOrigin::Derived),
        remaining_percent: Some(CanonicalRemainingPercent::new(87.5).unwrap()),
        remaining_percent_origin: Some(PercentObservationOrigin::ProviderReported),
        count_quota: None,
        provider_quota_state: Some(ProviderQuotaState::Unlimited),
        reset_label: None,
        resets_at: Some(1_781_190_720),
        status_slot: Some(StatusSlot::Spend),
        pace_label: None,
        status: UsageSnapshotStatus::Stale,
        used_money: Some(Money::new(1_234, "USD", 2)),
        limit_money: Some(Money::new(10_000, "USD", 2)),
        severity: UsageSeverity::Warn,
    }
}

fn legacy_account_snapshot() -> LegacyAccountUsageSnapshotView {
    LegacyAccountUsageSnapshotView {
        provider: "openai".to_owned(),
        account_label: "Shared label".to_owned(),
        source: "provider_api".to_owned(),
        confidence: "authoritative".to_owned(),
        window_kind: "session".to_owned(),
        used_amount: Some(1_234),
        used_unit: Some("cents".to_owned()),
        limit_amount: Some(10_000),
        limit_unit: Some("cents".to_owned()),
        resets_at: Some(1_781_190_720),
        fetched_at: 1_781_185_560,
        expires_at: Some(1_781_185_860),
        status: "fresh".to_owned(),
        last_error: None,
        quota_bucket: Some(money_bucket()),
    }
}

fn current_account_snapshot(
    account_capability: Option<UsageAccountCapability>,
) -> AccountUsageSnapshotView {
    AccountUsageSnapshotView {
        provider: "openai".to_owned(),
        account_label: "Shared label".to_owned(),
        account_capability,
        source: "provider_api".to_owned(),
        confidence: "authoritative".to_owned(),
        window_kind: "session".to_owned(),
        used_amount: Some(1_234),
        used_unit: Some("cents".to_owned()),
        limit_amount: Some(10_000),
        limit_unit: Some("cents".to_owned()),
        resets_at: Some(1_781_190_720),
        fetched_at: 1_781_185_560,
        expires_at: Some(1_781_185_860),
        status: "fresh".to_owned(),
        last_error: None,
        quota_bucket: Some(money_bucket()),
    }
}

fn current_focused_view(account_capability: Option<UsageAccountCapability>) -> FocusedUsageView {
    FocusedUsageView {
        focused_agent: Some("codex".to_owned()),
        focused_provider: Some("OpenAI".to_owned()),
        account: current_header(account_capability.clone()),
        buckets: vec![count_bucket(), money_bucket()],
        status: UsageSnapshotStatus::Fresh,
        source: UsageSource::ProviderApi,
        confidence: UsageConfidence::Authoritative,
        fetched_at_epoch: 1_781_185_560,
        updated_label: "Updated".to_owned(),
        status_bar_label: "usage current".to_owned(),
        tabs: vec![current_tab(account_capability)],
        last_error: None,
    }
}

#[test]
fn none_selector_preserves_exact_legacy_json_shape_for_all_carriers() {
    let old_header = legacy_header();
    let old_header_json = serde_json::to_value(&old_header).unwrap();
    let decoded_header: FocusedAccountHeader =
        serde_json::from_value(old_header_json.clone()).unwrap();
    assert_eq!(decoded_header.account_capability, None);
    assert_eq!(
        serde_json::to_value(decoded_header).unwrap(),
        old_header_json
    );
    assert_eq!(old_header_json.get("username"), Some(&Value::Null));
    assert_eq!(old_header_json.get("credential_origin"), Some(&Value::Null));

    let old_tab = legacy_tab();
    let old_tab_json = serde_json::to_value(&old_tab).unwrap();
    let decoded_tab: UsageProviderTab = serde_json::from_value(old_tab_json.clone()).unwrap();
    assert_eq!(decoded_tab.account_capability, None);
    assert_eq!(serde_json::to_value(decoded_tab).unwrap(), old_tab_json);
    assert_eq!(old_tab_json.get("plan_label"), Some(&Value::Null));

    let old_account = legacy_account_snapshot();
    let old_account_json = serde_json::to_value(&old_account).unwrap();
    let decoded_account: AccountUsageSnapshotView =
        serde_json::from_value(old_account_json.clone()).unwrap();
    assert_eq!(decoded_account.account_capability, None);
    assert_eq!(
        serde_json::to_value(decoded_account).unwrap(),
        old_account_json
    );
    assert_eq!(old_account_json.get("last_error"), Some(&Value::Null));
    let quota_bucket = old_account_json
        .get("quota_bucket")
        .expect("legacy snapshot retains its quota bucket");
    assert_eq!(quota_bucket.get("count_quota"), Some(&Value::Null));
    assert_eq!(
        quota_bucket
            .get("used_money")
            .and_then(|money| money.get("amount_minor")),
        Some(&json!(1_234))
    );
    assert_eq!(
        quota_bucket
            .get("limit_money")
            .and_then(|money| money.get("currency")),
        Some(&json!("USD"))
    );
}

fn assert_selector_json_contract<T>(none: &T, some: &T)
where
    T: Serialize + DeserializeOwned + PartialEq + std::fmt::Debug,
{
    let none_value = serde_json::to_value(none).unwrap();
    assert!(none_value.get("account_capability").is_none());
    let decoded_none: T = serde_json::from_value(none_value.clone()).unwrap();
    assert_eq!(&decoded_none, none);

    let mut explicit_null = none_value.clone();
    explicit_null
        .as_object_mut()
        .expect("DTO serializes as an object")
        .insert("account_capability".to_owned(), Value::Null);
    let decoded_null: T = serde_json::from_value(explicit_null).unwrap();
    assert_eq!(&decoded_null, none);
    assert_eq!(serde_json::to_value(decoded_null).unwrap(), none_value);

    let some_value = serde_json::to_value(some).unwrap();
    assert_eq!(
        some_value.get("account_capability"),
        Some(&json!({"account_id": "acct-a", "surface_id": "surface-a"}))
    );
    let decoded_some: T = serde_json::from_value(some_value.clone()).unwrap();
    assert_eq!(&decoded_some, some);
    assert_eq!(serde_json::to_value(decoded_some).unwrap(), some_value);

    for malformed in [
        json!({"account_id": "acct-a"}),
        json!({"surface_id": "surface-a"}),
        json!({"account_id": 7, "surface_id": "surface-a"}),
        json!({"account_id": "acct-a", "surface_id": false}),
    ] {
        let mut value = none_value.clone();
        value
            .as_object_mut()
            .expect("DTO serializes as an object")
            .insert("account_capability".to_owned(), malformed);
        assert!(
            serde_json::from_value::<T>(value).is_err(),
            "a present malformed selector must fail deserialization"
        );
    }
}

#[test]
fn null_some_and_malformed_selectors_are_explicit_for_all_carriers() {
    assert_selector_json_contract(&current_header(None), &current_header(Some(capability())));
    assert_selector_json_contract(&current_tab(None), &current_tab(Some(capability())));
    assert_selector_json_contract(
        &current_account_snapshot(None),
        &current_account_snapshot(Some(capability())),
    );
}

#[test]
fn old_control_readers_ignore_new_selectors_without_losing_required_fields() {
    let current = current_focused_view(Some(capability()));
    let new_message = serde_json::to_value(ServerMsg::UsageFocused {
        usage: Box::new(current.clone()),
    })
    .unwrap();
    match serde_json::from_value::<LegacyServerMsg>(new_message).unwrap() {
        LegacyServerMsg::UsageFocused { usage } => {
            assert_eq!(usage.focused_agent.as_deref(), Some("codex"));
            assert_eq!(usage.account.account_label, "Shared label");
            assert_eq!(usage.account.username, None);
            assert_eq!(usage.buckets.len(), 2);
            let count_bucket = usage.buckets.first().expect("count bucket is required");
            assert_eq!(
                count_bucket
                    .count_quota
                    .expect("count observation is required")
                    .used,
                Some(u64::MAX)
            );
            let money_bucket = usage.buckets.get(1).expect("money bucket is required");
            assert_eq!(
                money_bucket
                    .used_money
                    .as_ref()
                    .expect("money observation is required")
                    .amount_minor,
                1_234
            );
            let tab = usage.tabs.first().expect("old tab shape is required");
            assert_eq!(tab.account_label, "Shared label");
            assert_eq!(tab.source_label.as_deref(), Some("provider API"));
        }
        LegacyServerMsg::UsageAccounts { .. } => panic!("decoded the wrong control message"),
    }

    let account = current_account_snapshot(Some(capability()));
    let new_message = serde_json::to_value(ServerMsg::UsageAccounts {
        accounts: vec![account],
    })
    .unwrap();
    match serde_json::from_value::<LegacyServerMsg>(new_message).unwrap() {
        LegacyServerMsg::UsageAccounts { accounts } => {
            let account = accounts.first().expect("old account shape is required");
            assert_eq!(account.provider, "openai");
            assert_eq!(account.used_amount, Some(1_234));
            assert_eq!(account.last_error, None);
            assert_eq!(
                account
                    .quota_bucket
                    .as_ref()
                    .and_then(|bucket| bucket.limit_money.as_ref())
                    .map(|money| money.amount_minor),
                Some(10_000)
            );
        }
        LegacyServerMsg::UsageFocused { .. } => panic!("decoded the wrong control message"),
    }
}

#[test]
fn selector_metadata_does_not_match_synthetic_refresh_placeholders() {
    let unavailable = FocusedUsageView::unavailable("not available", 1);
    assert_eq!(unavailable.account.account_capability, None);
    assert!(!unavailable.is_refreshing_placeholder());

    let refreshing = FocusedUsageView::refreshing(Some("OpenAI"), 1);
    assert_eq!(refreshing.account.account_capability, None);
    assert!(refreshing.is_refreshing_placeholder());

    let mut selected_placeholder = refreshing;
    selected_placeholder.account.account_capability = Some(capability());
    assert_eq!(
        selected_placeholder.status,
        UsageSnapshotStatus::Unavailable
    );
    assert!(!selected_placeholder.is_refreshing_placeholder());
}
