use jackin_tui::ansi::RESET;
use jackin_tui::{display_cols, take_display_cols};

use crate::daemon::HoverTarget;
use crate::session::PullRequestInfo;

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
    // `branch` is the post-filter result from `Multiplexer::context_bar_branch`
    // (default-branch suppression already applied with the smart
    // `WorkdirContext::is_default_branch` check). Trust the input here.
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
