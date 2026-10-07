// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

#[test]
fn account_projection_preserves_error_text_without_inventing_retry_policy() {
    let mut view = view_with_buckets(UsageSnapshotStatus::Stale, vec![bucket("Weekly")]);
    view.last_error = Some("Provider message mentions HTTP 429 retry at 999999".to_owned());
    let account = project_account(&catalog_entry(view, None), 0, 1).unwrap();
    assert_eq!(account.issues[0].code, "provider_unavailable");
    assert_eq!(
        account.issues[0].message,
        "Provider message mentions HTTP 429 retry at 999999"
    );
    assert_eq!(account.issues[0].retry_at_epoch, None);
    assert_eq!(account.freshness.retry_at_epoch, None);
}
