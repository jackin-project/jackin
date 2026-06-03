//! Tests for `branch_context_bar`.
use super::*;
use crate::pull_request::PullRequestInfo;

fn pull_request_fixture(number: u64) -> PullRequestInfo {
    PullRequestInfo {
        number,
        title: "Surface PR context in Capsule".to_string(),
        url: format!("https://github.com/jackin-project/jackin/pull/{number}"),
        is_draft: false,
        checks: None,
    }
}

#[test]
fn renders_pr_id_title_and_container_without_url() {
    let pr = pull_request_fixture(434);
    let mut buf = Vec::new();
    render_branch_context_bar(
        &mut buf,
        24,
        120,
        Some("asa/pr-context"),
        Some(&pr),
        false,
        "jk-test-container",
        None,
    );
    let rendered = String::from_utf8_lossy(&buf);

    assert!(rendered.contains("\x1b[24;1H"));
    assert!(rendered.contains("\x1b[48;2;255;255;255m"));
    assert!(rendered.contains("PR #434"));
    assert!(!rendered.contains("asa/pr-context"));
    assert!(rendered.contains("Surface PR context in Capsule"));
    assert!(rendered.contains("jk-test-container"));
    assert!(!rendered.contains("https://github.com/jackin-project/jackin/pull/434"));
    assert!(!rendered.contains("\x1b]8;;"));
}

#[test]
fn renders_non_default_branch_without_pr() {
    let mut buf = Vec::new();
    render_branch_context_bar(
        &mut buf,
        24,
        80,
        Some("feature/no-pr"),
        None,
        false,
        "jk-test-container",
        None,
    );
    let rendered = String::from_utf8_lossy(&buf);

    assert!(rendered.contains("Branch · feature/no-pr"));
    assert!(rendered.contains("jk-test-container"));
    assert!(!rendered.contains("\x1b]8;;"));
}

#[test]
fn shows_pr_lookup_in_progress() {
    let mut buf = Vec::new();
    render_branch_context_bar(
        &mut buf,
        24,
        100,
        Some("feature/slow-gh"),
        None,
        true,
        "jk-test-container",
        None,
    );
    let rendered = String::from_utf8_lossy(&buf);

    assert!(rendered.contains("Resolving PR · feature/slow-gh"));
    assert!(!rendered.contains("Branch · feature/slow-gh"));
}

#[test]
fn truncates_left_chunk_on_narrow_terminal() {
    let mut pr = pull_request_fixture(999);
    pr.title = "Implement enormous feature with very long title that exceeds the bar".to_string();
    let mut buf = Vec::new();
    render_branch_context_bar(
        &mut buf,
        24,
        20,
        Some("feature/x"),
        Some(&pr),
        false,
        "jk-test-container-with-extra-long-suffix",
        None,
    );
    let rendered = String::from_utf8_lossy(&buf);
    assert!(rendered.contains("PR #999"));
    assert!(
        !rendered.contains("jk-test-container-with-extra-long-suffix"),
        "narrow terminal must drop container chunk: {rendered:?}"
    );
}

#[test]
fn layout_returns_none_for_zero_dimensions() {
    let pr = pull_request_fixture(1);
    assert!(
        branch_context_bar_layout(0, 80, Some("feature/x"), Some(&pr), false, "jk-test").is_none()
    );
    assert!(
        branch_context_bar_layout(24, 0, Some("feature/x"), Some(&pr), false, "jk-test").is_none()
    );
}

#[test]
fn hit_rejects_columns_outside_region() {
    let pr = pull_request_fixture(7);
    let layout = branch_context_bar_layout(24, 120, Some("feature/x"), Some(&pr), false, "jk-test")
        .expect("layout fits");
    let region = layout.left_region.expect("left region present");
    let left_start = region.start;
    let left_end = region.end;
    assert_eq!(
        branch_context_bar_hit(
            24,
            left_start,
            24,
            120,
            Some("feature/x"),
            Some(&pr),
            false,
            "jk-test"
        ),
        Some(BranchContextBarHit::Context)
    );
    assert_eq!(
        branch_context_bar_hit(
            24,
            left_end - 1,
            24,
            120,
            Some("feature/x"),
            Some(&pr),
            false,
            "jk-test"
        ),
        Some(BranchContextBarHit::Context)
    );
    let outside_left = branch_context_bar_hit(
        24,
        left_end,
        24,
        120,
        Some("feature/x"),
        Some(&pr),
        false,
        "jk-test",
    );
    assert!(matches!(
        outside_left,
        None | Some(BranchContextBarHit::Container)
    ));
    assert_eq!(
        branch_context_bar_hit(
            23,
            left_start,
            24,
            120,
            Some("feature/x"),
            Some(&pr),
            false,
            "jk-test"
        ),
        None
    );
}

#[test]
fn hover_highlights_click_targets() {
    let pr = pull_request_fixture(434);
    let mut context_buf = Vec::new();
    render_branch_context_bar(
        &mut context_buf,
        24,
        120,
        Some("asa/pr-context"),
        Some(&pr),
        false,
        "jk-test-container",
        Some(HoverTarget::BranchContext),
    );
    let context_rendered = String::from_utf8_lossy(&context_buf);
    assert!(context_rendered.contains(BRANCH_CONTEXT_BAR_HOVER_BG));
    assert!(context_rendered.contains(BRANCH_CONTEXT_BAR_HOVER_FG));

    let mut container_buf = Vec::new();
    render_branch_context_bar(
        &mut container_buf,
        24,
        120,
        Some("asa/pr-context"),
        Some(&pr),
        false,
        "jk-test-container",
        Some(HoverTarget::Container),
    );
    let container_rendered = String::from_utf8_lossy(&container_buf);
    assert!(container_rendered.contains(BRANCH_CONTEXT_BAR_HOVER_BG));
    assert!(container_rendered.contains("jk-test-container"));
}

#[test]
fn leaves_left_side_empty_when_branch_filtered_out() {
    let mut buf = Vec::new();
    render_branch_context_bar(
        &mut buf,
        24,
        80,
        None,
        None,
        false,
        "jk-test-container",
        None,
    );
    let rendered = String::from_utf8_lossy(&buf);
    assert!(rendered.contains("jk-test-container"));
    assert!(!rendered.contains("jackin"));
    assert!(!rendered.contains("Branch ·"));
    assert!(!rendered.contains("Resolving PR"));
    assert!(!rendered.contains("PR #"));
    assert_eq!(
        branch_context_bar_hit(24, 2, 24, 80, None, None, false, "jk-test-container"),
        None
    );
}

#[test]
fn visible_branch_suppresses_default_branch_only() {
    assert_eq!(visible_branch(Some("main"), true), None);
    assert_eq!(
        visible_branch(Some("feature/tui"), false),
        Some("feature/tui")
    );
    assert_eq!(visible_branch(None, false), None);
}
