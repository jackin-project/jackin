use super::spend_headline_label;
use jackin_protocol::control::{
    Money, QuotaBucketView, StatusSlot, UsageSeverity, UsageSnapshotStatus,
};

fn spend_bucket(limit: Money) -> QuotaBucketView {
    QuotaBucketView {
        label: "Spend".to_owned(),
        used_label: None,
        limit_label: None,
        used_percent: None,
        used_percent_origin: None,
        remaining_percent: None,
        remaining_percent_origin: None,
        count_quota: None,
        provider_quota_state: None,
        reset_label: None,
        resets_at: None,
        status_slot: Some(StatusSlot::Spend),
        pace_label: None,
        status: UsageSnapshotStatus::Fresh,
        used_money: Some(Money::new(1, "USD", 0)),
        limit_money: Some(limit),
        severity: UsageSeverity::Normal,
    }
}

#[test]
fn spend_headline_preserves_exact_major_amount() {
    let bucket = spend_bucket(Money::new(9_007_199_254_740_993, "USD", 0));

    assert_eq!(
        spend_headline_label(&[bucket]).as_deref(),
        Some("$1 of 9007199254740993")
    );
}
