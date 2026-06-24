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

fn account(
    provider: &str,
    status: &str,
    source: &str,
    confidence: &str,
) -> AccountUsageSnapshotView {
    AccountUsageSnapshotView {
        provider: provider.to_owned(),
        account_label: format!("{provider} account"),
        source: source.to_owned(),
        confidence: confidence.to_owned(),
        window_kind: "Session".to_owned(),
        used_amount: Some(37),
        used_unit: Some("percent".to_owned()),
        limit_amount: Some(100),
        limit_unit: Some("percent".to_owned()),
        resets_at: None,
        fetched_at: 1_781_185_680,
        expires_at: None,
        status: status.to_owned(),
        last_error: None,
    }
}

#[test]
fn usage_verify_accepts_trusted_rows_for_every_provider() {
    let accounts = [
        account("Codex", "fresh", "provider_api", "authoritative"),
        account("Claude", "fresh", "cli", "authoritative"),
        account("Amp", "fresh", "provider_api", "authoritative"),
        account("Grok Build", "fresh", "cli", "authoritative"),
        account("GLM / Z.AI", "fresh", "provider_api", "authoritative"),
        account("Kimi", "fresh", "provider_api", "authoritative"),
        account("MiniMax", "fresh", "provider_api", "authoritative"),
    ];

    let checks = verify_usage_accounts(&accounts);

    assert_eq!(checks.len(), 7);
    assert!(
        checks.iter().all(|check| check.status == "ok"),
        "{checks:?}"
    );
}

#[test]
fn usage_verify_reports_missing_and_untrusted_providers() {
    let mut untrusted = account("Codex", "needs_login", "none", "none");
    untrusted.account_label = "needs Codex login".to_owned();
    untrusted.last_error = Some("Codex auth not available".to_owned());
    let accounts = [
        untrusted,
        account("Amp", "fresh", "provider_api", "authoritative"),
    ];

    let checks = verify_usage_accounts(&accounts);

    let codex = checks
        .iter()
        .find(|check| check.label == "OpenAI")
        .expect("OpenAI check");
    assert_eq!(codex.status, "untrusted");
    assert!(
        codex
            .detail
            .as_deref()
            .is_some_and(|detail| detail.contains("needs_login")),
        "{codex:?}"
    );
    let anthropic = checks
        .iter()
        .find(|check| check.label == "Anthropic")
        .expect("Anthropic check");
    assert_eq!(anthropic.status, "missing");
    let amp = checks
        .iter()
        .find(|check| check.label == "Amp")
        .expect("Amp check");
    assert_eq!(amp.status, "ok");
}

#[test]
fn truncates_long_values_with_ascii_ellipsis() {
    assert_eq!(truncate("abcdefghijkl", 8), "abcde...");
}
