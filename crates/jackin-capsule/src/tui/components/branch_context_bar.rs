//! Branch context bar component: renders the current git branch and
//! ahead/behind counts in the capsule status bar.
//!
//! Not responsible for: fetching git state (caller provides `branch` and
//! `PullRequestInfo`) or mouse-event dispatch.
//!
//! Key invariant: `BRANCH_CONTEXT_BAR_ROWS` is the exact row budget callers
//! must reserve; rendering writes ANSI escape sequences directly into the
//! caller-supplied `buf` using absolute cursor positions.

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
    /// Click region for the debug run-id chip (rightmost, only when debug is active).
    pub(crate) debug_chip_region: Option<ColRange>,
}

pub(crate) fn visible_branch(branch: Option<&str>, is_default_branch: bool) -> Option<&str> {
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
        debug_chip_region: None, // populated by callers that have the run_id
    })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum BranchContextBarHit {
    Context,
    Container,
    /// Click on the debug run-id chip (only shown when `--debug` is active).
    DebugChip,
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
    // Check debug chip: when JACKIN_DEBUG is active, the rightmost N columns
    // show the red run-id chip. Detected here from the env var so callers
    // don't need a separate parameter.
    if crate::logging::debug_enabled() {
        if let Ok(run_id) = std::env::var("JACKIN_RUN_ID") {
            if !run_id.is_empty() {
                let chip = format!(" {run_id} ");
                let chip_cols = u16::try_from(display_cols(&chip)).unwrap_or(u16::MAX);
                let chip_start = term_cols.saturating_sub(chip_cols).saturating_add(1);
                if col >= chip_start && col <= term_cols {
                    return Some(BranchContextBarHit::DebugChip);
                }
            }
        }
    }
    if layout.debug_chip_region.is_some_and(|r| r.contains(col)) {
        return Some(BranchContextBarHit::DebugChip);
    }
    if layout.container_region.is_some_and(|r| r.contains(col)) {
        return Some(BranchContextBarHit::Container);
    }
    if layout.left_region.is_some_and(|r| r.contains(col)) {
        return Some(BranchContextBarHit::Context);
    }
    None
}

#[cfg(test)]
mod tests;
