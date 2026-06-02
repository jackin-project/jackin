use jackin_tui::ansi::RESET;
use jackin_tui::{display_cols, take_display_cols};

use crate::pull_request::PullRequestInfo;
use crate::tui::app::HoverTarget;

pub const BRANCH_CONTEXT_BAR_ROWS: u16 = 1;

pub(crate) const BRANCH_CONTEXT_BAR_BG: &str = jackin_tui::ansi::rgb_bg(jackin_tui::WHITE);
pub(crate) const BRANCH_CONTEXT_BAR_HOVER_BG: &str = "\x1b[48;2;225;245;255m";
pub(crate) const BRANCH_CONTEXT_BAR_FG: &str = jackin_tui::ansi::rgb_fg(jackin_tui::BLACK);
pub(crate) const BRANCH_CONTEXT_BAR_LINK_FG: &str = jackin_tui::ansi::rgb_fg(jackin_tui::LINK_BLUE);
pub(crate) const BRANCH_CONTEXT_BAR_HOVER_FG: &str = "\x1b[38;2;0;55;140m";
pub(crate) const BRANCH_CONTEXT_BAR_BOLD: &str = jackin_tui::ansi::BOLD;

#[allow(clippy::too_many_arguments)]
pub(crate) fn render_branch_context_bar(
    buf: &mut Vec<u8>,
    term_rows: u16,
    term_cols: u16,
    branch: Option<&str>,
    pull_request: Option<&PullRequestInfo>,
    pull_request_loading: bool,
    container_name: &str,
    hover_target: Option<HoverTarget>,
) {
    let Some(layout) = branch_context_bar_layout(
        term_rows,
        term_cols,
        branch,
        pull_request,
        pull_request_loading,
        container_name,
    ) else {
        return;
    };

    let bar_row = term_rows.saturating_sub(1);
    jackin_tui::ansi::move_to(buf, bar_row, 0);
    buf.extend_from_slice(BRANCH_CONTEXT_BAR_BG.as_bytes());
    buf.extend_from_slice(BRANCH_CONTEXT_BAR_FG.as_bytes());
    for _ in 0..term_cols {
        buf.push(b' ');
    }

    paint_branch_bar_chunk(
        buf,
        bar_row,
        0,
        &layout.left,
        ChunkStyle::left(),
        hover_target == Some(HoverTarget::BranchContext),
    );
    if let Some(region) = layout.container_region {
        paint_branch_bar_chunk(
            buf,
            bar_row,
            region.start.saturating_sub(1),
            &layout.container,
            ChunkStyle::container(),
            hover_target == Some(HoverTarget::Container),
        );
    }
    buf.extend_from_slice(RESET.as_bytes());
}

/// Per-chunk colour selection rule for `render_branch_context_bar`.
/// The left chunk always emits bold; the container chunk emits bold
/// only on hover and uses the "link" foreground instead of the plain
/// foreground.
struct ChunkStyle {
    /// Idle foreground (`!hovered`).
    idle_fg: &'static str,
    /// Emit bold even when not hovered.
    always_bold: bool,
}

impl ChunkStyle {
    const fn left() -> Self {
        Self {
            idle_fg: BRANCH_CONTEXT_BAR_FG,
            always_bold: true,
        }
    }
    const fn container() -> Self {
        Self {
            idle_fg: BRANCH_CONTEXT_BAR_LINK_FG,
            always_bold: false,
        }
    }
}

fn paint_branch_bar_chunk(
    buf: &mut Vec<u8>,
    bar_row: u16,
    start_col: u16,
    label: &str,
    style: ChunkStyle,
    hovered: bool,
) {
    jackin_tui::ansi::move_to(buf, bar_row, start_col);
    let bg = if hovered {
        BRANCH_CONTEXT_BAR_HOVER_BG
    } else {
        BRANCH_CONTEXT_BAR_BG
    };
    let fg = if hovered {
        BRANCH_CONTEXT_BAR_HOVER_FG
    } else {
        style.idle_fg
    };
    buf.extend_from_slice(bg.as_bytes());
    buf.extend_from_slice(fg.as_bytes());
    if style.always_bold || hovered {
        buf.extend_from_slice(BRANCH_CONTEXT_BAR_BOLD.as_bytes());
    }
    buf.extend_from_slice(label.as_bytes());
}

/// Half-open `[start, end)` column range. Constructor returns `None`
/// when `end <= start` so the renderer / hit-tester can rely on
/// `end > start` for every alive region without re-checking.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct ColRange {
    pub(crate) start: u16,
    pub(crate) end: u16,
}

impl ColRange {
    pub(crate) fn new(start: u16, end: u16) -> Option<Self> {
        (end > start).then_some(Self { start, end })
    }

    pub(crate) fn contains(self, col: u16) -> bool {
        col >= self.start && col < self.end
    }
}

pub(crate) struct BranchContextBarLayout {
    pub(crate) left: String,
    pub(crate) left_region: Option<ColRange>,
    pub(crate) container: String,
    pub(crate) container_region: Option<ColRange>,
}

pub(crate) fn visible_branch<'a>(
    branch: Option<&'a str>,
    is_default_branch: bool,
) -> Option<&'a str> {
    branch.filter(|_| !is_default_branch)
}

pub(crate) fn branch_context_bar_layout(
    term_rows: u16,
    term_cols: u16,
    branch: Option<&str>,
    pull_request: Option<&PullRequestInfo>,
    pull_request_loading: bool,
    container_name: &str,
) -> Option<BranchContextBarLayout> {
    if term_rows == 0 || term_cols == 0 {
        return None;
    }
    // `branch` is the post-filter visible branch. Trust the input here so
    // renderer / layout / hit-test helpers stay default-branch-agnostic.
    let (left, left_clickable) = match (pull_request, branch) {
        (Some(pr), _) => (format!(" PR {} · {} ", pr.number_label(), pr.title), true),
        (None, Some(b)) if pull_request_loading => (format!(" Resolving PR · {b} "), true),
        (None, Some(b)) => (format!(" Branch · {b} "), true),
        (None, None) => (String::new(), false),
    };
    let container = if container_name.is_empty() {
        String::new()
    } else {
        format!(" {} ", container_name)
    };
    let term_cols_usize = usize::from(term_cols);
    let container_cols = display_cols(&container);
    let container_fits = container_cols > 0 && container_cols + 2 < term_cols_usize;
    let left_max_cols = if container_fits {
        term_cols_usize.saturating_sub(container_cols + 1)
    } else {
        term_cols_usize
    };
    let left = take_display_cols(&left, left_max_cols);
    let left_cols = display_cols(&left);
    let left_region = if left_clickable && left_cols > 0 {
        let end = u16::try_from(left_cols.saturating_add(1)).unwrap_or(u16::MAX);
        ColRange::new(1, end)
    } else {
        None
    };
    let container_region = if container_fits {
        let start = term_cols_usize
            .saturating_sub(container_cols)
            .saturating_add(1);
        let end = start.saturating_add(container_cols);
        ColRange::new(
            u16::try_from(start).unwrap_or(u16::MAX),
            u16::try_from(end).unwrap_or(u16::MAX),
        )
    } else {
        None
    };
    Some(BranchContextBarLayout {
        left,
        left_region,
        container,
        container_region,
    })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum BranchContextBarHit {
    Context,
    Container,
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn branch_context_bar_hit(
    row: u16,
    col: u16,
    term_rows: u16,
    term_cols: u16,
    branch: Option<&str>,
    pull_request: Option<&PullRequestInfo>,
    pull_request_loading: bool,
    container_name: &str,
) -> Option<BranchContextBarHit> {
    if row != term_rows {
        return None;
    }
    let layout = branch_context_bar_layout(
        term_rows,
        term_cols,
        branch,
        pull_request,
        pull_request_loading,
        container_name,
    )?;
    if layout.container_region.is_some_and(|r| r.contains(col)) {
        return Some(BranchContextBarHit::Container);
    }
    if layout.left_region.is_some_and(|r| r.contains(col)) {
        return Some(BranchContextBarHit::Context);
    }
    None
}

#[cfg(test)]
mod tests {
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
        pr.title =
            "Implement enormous feature with very long title that exceeds the bar".to_string();
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
            branch_context_bar_layout(0, 80, Some("feature/x"), Some(&pr), false, "jk-test")
                .is_none()
        );
        assert!(
            branch_context_bar_layout(24, 0, Some("feature/x"), Some(&pr), false, "jk-test")
                .is_none()
        );
    }

    #[test]
    fn hit_rejects_columns_outside_region() {
        let pr = pull_request_fixture(7);
        let layout =
            branch_context_bar_layout(24, 120, Some("feature/x"), Some(&pr), false, "jk-test")
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
}
