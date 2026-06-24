//! Tests for `branch_context_bar`.
use super::*;
use crate::pull_request::PullRequestInfo;

fn pull_request_fixture(number: u64) -> PullRequestInfo {
    PullRequestInfo {
        number,
        title: "Surface PR context in Capsule".to_owned(),
        url: format!("https://github.com/jackin-project/jackin/pull/{number}"),
        is_draft: false,
        checks: None,
    }
}

/// Paint the bar through the chrome widget into a test buffer and return
/// `(bar_row_text, buffer)` for content + style assertions.
fn widget_bar(
    cols: u16,
    branch: Option<&str>,
    usage_status_label: Option<&str>,
    pull_request: Option<&PullRequestInfo>,
    loading: bool,
    container: &str,
    hover: Option<crate::tui::app::HoverTarget>,
) -> (String, ratatui::buffer::Buffer) {
    use ratatui::widgets::Widget as _;
    let area = ratatui::layout::Rect::new(0, 0, cols, 24);
    let mut buf = ratatui::buffer::Buffer::empty(area);
    crate::tui::components::chrome::BottomChromeWidget {
        branch,
        usage_status_label,
        pull_request,
        pull_request_loading: loading,
        instance_id_label: container,
        hover_target: hover,
        scrollback_active: false,
        scroll_axes: jackin_tui::scroll::ScrollAxes::none(),
        debug_run_id: None,
        prefix_awaiting: false,
        palette_key: 0x1C,
    }
    .render(area, &mut buf);
    let text: String = (0..cols)
        .map(|x| buf[(x, 23)].symbol().to_owned())
        .collect();
    (text, buf)
}

#[test]
fn renders_pr_id_title_and_container_without_url() {
    let pr = pull_request_fixture(434);
    let (text, buf) = widget_bar(
        120,
        Some("asa/pr-context"),
        None,
        Some(&pr),
        false,
        "jk-test-container",
        None,
    );

    assert!(text.contains("PR #434"));
    assert!(!text.contains("asa/pr-context"));
    assert!(text.contains("Surface PR context in Capsule"));
    assert!(text.contains("jk-test-container"));
    assert!(!text.contains("https://github.com/jackin-project/jackin/pull/434"));
    assert_eq!(
        buf[(0, 23)].bg,
        ratatui::style::Color::Rgb(255, 255, 255),
        "bar row paints the white background"
    );
}

#[test]
fn renders_non_default_branch_without_pr() {
    let (text, _) = widget_bar(
        80,
        Some("feature/no-pr"),
        None,
        None,
        false,
        "jk-test-container",
        None,
    );
    assert!(text.contains("Branch · feature/no-pr"));
    assert!(text.contains("jk-test-container"));
}

#[test]
fn shows_pr_lookup_in_progress() {
    let (text, _) = widget_bar(
        100,
        Some("feature/slow-gh"),
        None,
        None,
        true,
        "jk-test-container",
        None,
    );
    assert!(text.contains("Resolving PR · feature/slow-gh"));
    assert!(!text.contains("Branch · feature/slow-gh"));
}

#[test]
fn truncates_left_chunk_on_narrow_terminal() {
    let mut pr = pull_request_fixture(999);
    pr.title = "Implement enormous feature with very long title that exceeds the bar".to_owned();
    let (text, _) = widget_bar(
        20,
        Some("feature/x"),
        None,
        Some(&pr),
        false,
        "jk-test-container-with-extra-long-suffix",
        None,
    );
    assert!(text.contains("PR #999"));
    assert!(
        !text.contains("jk-test-container-with-extra-long-suffix"),
        "narrow terminal must drop container chunk: {text:?}"
    );
}

#[test]
fn renders_usage_signal_with_branch_context() {
    let (text, _) = widget_bar(
        120,
        Some("feature/usage"),
        Some("Codex Session: 63% used · 37% left"),
        None,
        false,
        "jk-test-container",
        None,
    );
    assert!(text.contains("Branch · feature/usage"));
    assert!(text.contains("Codex Session: 63% used · 37% left"));
    assert!(
        text.find("Branch · feature/usage") < text.find("Codex Session: 63% used · 37% left"),
        "{text:?}"
    );
}

#[test]
fn narrow_usage_signal_keeps_session_quota_before_weekly() {
    let (text, _) = widget_bar(
        44,
        Some("feature/very-long-branch-name"),
        Some("Session 37% · Weekly 10%"),
        None,
        false,
        "jk-test-container",
        None,
    );

    assert!(text.contains("Session 37%"), "{text:?}");
    assert!(!text.contains("Weekly 10%"), "{text:?}");
    assert!(!text.contains("feature/very-long-branch-name"), "{text:?}");
}

#[test]
fn narrow_usage_signal_keeps_state_when_no_quota_exists() {
    let (text, _) = widget_bar(
        42,
        Some("feature/very-long-branch-name"),
        Some("Amp · account unavailable login"),
        None,
        false,
        "jk-test-container",
        None,
    );

    assert!(text.contains("login"), "{text:?}");
    assert!(!text.contains("feature/very-long-branch-name"), "{text:?}");
}

#[test]
fn right_chunks_order_usage_container_run_id() {
    let layout = branch_context_bar_layout(
        24,
        100,
        Some("feature/status"),
        Some("Session 37%"),
        None,
        false,
        Some("18bc138751b01628"),
        "sx02yp2x",
    )
    .expect("layout");

    let usage = layout.usage_region.expect("usage region");
    let container = layout.container_region.expect("container region");
    let run = layout.debug_chip_region.expect("run id region");

    assert!(
        usage.start < container.start,
        "usage should render left of container"
    );
    assert!(
        container.start < run.start,
        "container should render left of run id"
    );
}

#[test]
fn layout_returns_none_for_zero_dimensions() {
    let pr = pull_request_fixture(1);
    assert!(
        branch_context_bar_layout(
            0,
            80,
            Some("feature/x"),
            None,
            Some(&pr),
            false,
            None,
            "jk-test"
        )
        .is_none()
    );
    assert!(
        branch_context_bar_layout(
            24,
            0,
            Some("feature/x"),
            None,
            Some(&pr),
            false,
            None,
            "jk-test"
        )
        .is_none()
    );
}

#[test]
fn hit_rejects_columns_outside_region() {
    let pr = pull_request_fixture(7);
    let layout = branch_context_bar_layout(
        24,
        120,
        Some("feature/x"),
        None,
        Some(&pr),
        false,
        None,
        "jk-test",
    )
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
            None,
            Some(&pr),
            false,
            None,
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
            None,
            Some(&pr),
            false,
            None,
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
        None,
        Some(&pr),
        false,
        None,
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
            None,
            Some(&pr),
            false,
            None,
            "jk-test"
        ),
        None
    );
}

#[test]
fn hover_highlights_click_targets() {
    let pr = pull_request_fixture(434);
    let hover_bg = ratatui::style::Color::Rgb(225, 245, 255);
    let (_, ctx) = widget_bar(
        120,
        Some("asa/pr-context"),
        None,
        Some(&pr),
        false,
        "jk-test-container",
        Some(crate::tui::app::HoverTarget::BranchContext),
    );
    assert_eq!(ctx[(1, 23)].bg, hover_bg, "hovered context chunk lifts");

    let (text, container) = widget_bar(
        120,
        Some("asa/pr-context"),
        None,
        Some(&pr),
        false,
        "jk-test-container",
        Some(crate::tui::app::HoverTarget::Container),
    );
    assert!(text.contains("jk-test-container"));
    let chunk_x = text.find("jk-test-container").expect("container chunk") as u16;
    assert_eq!(
        container[(chunk_x, 23)].bg,
        hover_bg,
        "hovered container chunk lifts"
    );

    let (text, usage) = widget_bar(
        120,
        Some("asa/pr-context"),
        Some("Session 37% · Weekly 10%"),
        Some(&pr),
        false,
        "jk-test-container",
        Some(crate::tui::app::HoverTarget::UsageStatus),
    );
    let chunk_x = text.find("Session 37%").expect("usage chunk") as u16;
    assert_eq!(
        usage[(chunk_x, 23)].bg,
        hover_bg,
        "hovered usage chunk lifts"
    );
}

#[test]
fn leaves_left_side_empty_when_branch_filtered_out() {
    let (text, _) = widget_bar(80, None, None, None, false, "jk-test-container", None);
    assert!(text.contains("jk-test-container"));
    assert!(!text.contains("jackin"));
    assert!(!text.contains("Branch ·"));
    assert!(!text.contains("Resolving PR"));
    assert!(!text.contains("PR #"));
    assert_eq!(
        branch_context_bar_hit(
            24,
            2,
            24,
            80,
            None,
            None,
            None,
            false,
            None,
            "jk-test-container"
        ),
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

#[test]
fn snapshot_branch_context_bar_with_pr_120x24() {
    let pr = pull_request_fixture(434);
    let (text, _) = widget_bar(
        120,
        Some("feature/tui-architecture"),
        None,
        Some(&pr),
        false,
        "jk-test-container",
        None,
    );
    insta::assert_snapshot!("branch_context_bar_with_pr_120x24", text);
}

#[test]
fn snapshot_branch_context_bar_no_pr_80x24() {
    let (text, _) = widget_bar(
        80,
        Some("feature/tui-architecture"),
        None,
        None,
        false,
        "jk-test-container",
        None,
    );
    insta::assert_snapshot!("branch_context_bar_no_pr_80x24", text);
}
