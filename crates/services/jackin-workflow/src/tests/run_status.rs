// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use crate::RunStatus;

#[test]
fn status_parse_roundtrips_all_variants() {
    let cases = [
        (RunStatus::Active, "active"),
        (RunStatus::Paused, "paused"),
        (RunStatus::NeedsOperator, "needs_operator"),
        (RunStatus::Accepted, "accepted"),
        (RunStatus::Failed, "failed"),
        (RunStatus::Cancelled, "cancelled"),
    ];
    for (status, expected) in cases {
        assert_eq!(status.as_str(), expected);
        assert_eq!(RunStatus::parse(expected), Some(status));
    }
    assert_eq!(RunStatus::parse("bogus"), None);
}

#[test]
fn terminal_flags_cover_finished_states() {
    assert!(!RunStatus::Active.is_terminal());
    assert!(!RunStatus::Paused.is_terminal());
    assert!(!RunStatus::NeedsOperator.is_terminal());
    assert!(RunStatus::Accepted.is_terminal());
    assert!(RunStatus::Failed.is_terminal());
    assert!(RunStatus::Cancelled.is_terminal());
}
