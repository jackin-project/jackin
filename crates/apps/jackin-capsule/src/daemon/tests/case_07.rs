// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::path::Path;

use crate::pull_request::PullRequestInfo;
use crate::tui::title::compose_outer_terminal_title;

fn pull_request() -> PullRequestInfo {
    PullRequestInfo {
        number: 42,
        title: "Add widgets".to_owned(),
        url: "https://example.com/pr/42".to_owned(),
        is_draft: false,
        checks: None,
    }
}

#[test]
fn outer_terminal_title_uses_workspace_and_pr_title() {
    let title = compose_outer_terminal_title(
        Path::new("/srv/checkouts/payments"),
        Some("feature-x"),
        Some(&pull_request()),
    );
    assert!(title.contains("payments"), "{title:?}");
    assert!(title.contains("PR #42"), "{title:?}");
    assert!(title.contains("Add widgets"), "{title:?}");
}

#[test]
fn outer_terminal_title_falls_back_to_branch_without_pr() {
    let title = compose_outer_terminal_title(
        Path::new("/srv/checkouts/payments"),
        Some("feature-x"),
        None,
    );
    assert!(title.contains("payments"), "{title:?}");
    assert!(title.contains("feature-x"), "{title:?}");
    assert!(!title.contains("PR #"), "{title:?}");
}

#[test]
fn outer_terminal_title_sanitizes_control_bytes() {
    let title = compose_outer_terminal_title(
        Path::new("/srv/checkouts/payments"),
        Some("br\x01anch\x07name"),
        None,
    );
    assert!(!title.chars().any(char::is_control), "{title:?}");
    assert!(title.contains("anch"), "{title:?}");
}
