// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

#[test]
fn snapshot_with_auth_refused_base_is_stale_never_fabricated() {
    let refused = std::net::TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0)).unwrap();
    let address = refused.local_addr().unwrap();
    drop(refused);
    let auth = cursor_auth_from_value(&serde_json::json!({"accessToken": "fixture-opaque"}))
        .expect("fixture auth parses");
    let view = cursor_snapshot_with_auth(
        "cursor",
        Some("Cursor"),
        &auth,
        None,
        "OAuth · configured profile",
        &format!("http://{address}"),
        1_781_728_000,
    );

    assert_eq!(view.status, UsageSnapshotStatus::Stale);
    assert_eq!(view.account.provider_label, "Cursor");
    assert!(view.last_error.is_some());
}
