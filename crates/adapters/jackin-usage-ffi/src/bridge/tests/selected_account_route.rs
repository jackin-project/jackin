// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

const MISSING_ACCOUNT_KEY: &str = "removed-claude-account-key";
const SIBLING_ACCOUNT_ID: &str = "current-claude-sibling";
const CREDENTIAL_SENTINEL: &str = "fixture-current-claude-sibling-secret";

#[test]
fn desktop_projection_keeps_missing_selected_route_without_sibling_fallback() {
    let dir = tempfile::tempdir().expect("tempdir");
    write_account_config(&dir.path().join("config"), SIBLING_ACCOUNT_ID, "anthropic");

    let usage_state = dir.path().join("usage-menu-bar");
    std::fs::create_dir_all(&usage_state).expect("usage state directory");
    std::fs::write(
        usage_state.join("selected-accounts.json"),
        format!(r#"{{"selected":{{"claude":"{MISSING_ACCOUNT_KEY}"}}}}"#),
    )
    .expect("persisted selected account");

    // open_bridge sets `allow_live_probes` to false. The configured Claude
    // account is discoverable, but neither its credential nor provider API is
    // used to produce quota during this fixture.
    let bridge = open_bridge(dir.path());
    assert!(!bridge.refresh_due().expect("refresh due"));

    let projection = bridge.desktop_projection(3).expect("desktop projection");
    let provider = projection
        .providers
        .iter()
        .find(|provider| provider.group.surface_id == "claude")
        .expect("configured Claude provider");

    assert_eq!(provider.selected_account_route.status, "unavailable");
    assert_eq!(
        provider.selected_account_route.account_key.as_deref(),
        Some(MISSING_ACCOUNT_KEY)
    );
    assert_eq!(
        provider.selected_account_route.notice.as_deref(),
        Some(jackin_usage::host::SELECTED_ACCOUNT_UNAVAILABLE_NOTICE)
    );

    assert_eq!(provider.group.accounts.len(), 1);
    let sibling = &provider.group.accounts[0];
    assert_eq!(sibling.account_label, SIBLING_ACCOUNT_ID);
    assert!(!sibling.selected);
    assert_eq!(sibling.lifecycle, "current");
    assert_ne!(sibling.account_key, MISSING_ACCOUNT_KEY);

    let selected_usage = &provider.selected_usage;
    assert_eq!(selected_usage.status, "unavailable");
    assert_eq!(
        selected_usage.last_error.as_deref(),
        Some(jackin_usage::host::SELECTED_ACCOUNT_UNAVAILABLE_NOTICE)
    );
    assert_ne!(selected_usage.account_label, sibling.account_label);
    assert_ne!(selected_usage.identity.account_label, sibling.account_label);
    assert!(selected_usage.buckets.is_empty());
    assert!(
        selected_usage
            .detail_presentation
            .rows
            .iter()
            .all(|row| { !row.display_label.contains(SIBLING_ACCOUNT_ID) })
    );
    assert_eq!(selected_usage.username, None);
    assert_eq!(selected_usage.plan_label, None);
    assert_eq!(selected_usage.credential_origin, None);
    assert!(
        !format!("{selected_usage:?}").contains(SIBLING_ACCOUNT_ID),
        "selected view exposed the sibling identity"
    );

    let dto_debug = format!("{projection:?}");
    assert!(
        !dto_debug.contains(CREDENTIAL_SENTINEL),
        "Desktop projection exposed a credential value"
    );

    bridge.shutdown().expect("shutdown");
}
