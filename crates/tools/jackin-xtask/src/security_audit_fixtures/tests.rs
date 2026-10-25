// SPDX-FileCopyrightText: 2026 The jackin❯ Authors
// SPDX-License-Identifier: Apache-2.0

use super::*;
use serde_json::json;

#[test]
fn manifests_are_compact_and_finding_filter_is_specific() {
    assert_eq!(
        manifest("matrix_key", SYNTHETIC_ID).expect("manifest"),
        "{\"matrix_key\":\"m-0123456789abcdef\"}\n"
    );
    let findings = vec![
        json!({"RuleID":"generic-api-key"}),
        json!({"RuleID":"private-key"}),
        json!({"RuleID":"other"}),
    ];
    assert_eq!(generic_findings(&findings).len(), 1);
}
