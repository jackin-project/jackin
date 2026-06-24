use super::*;

#[test]
fn formats_account_usage_with_shared_unit() {
    let account = AccountUsageSnapshotView {
        provider: "codex".to_owned(),
        account_label: "alexey@example.com".to_owned(),
        source: "codex-rpc".to_owned(),
        confidence: "authoritative".to_owned(),
        window_kind: "session".to_owned(),
        used_amount: Some(37),
        used_unit: Some("percent".to_owned()),
        limit_amount: Some(100),
        limit_unit: Some("percent".to_owned()),
        resets_at: None,
        fetched_at: 0,
        expires_at: None,
        status: "fresh".to_owned(),
        last_error: None,
    };

    assert_eq!(usage_amount_label(&account), "37/100 percent");
}

#[test]
fn truncates_long_values_with_ascii_ellipsis() {
    assert_eq!(truncate("abcdefghijkl", 8), "abcde...");
}
