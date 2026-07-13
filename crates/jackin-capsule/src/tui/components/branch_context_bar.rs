//! Branch context bar component: renders the current git branch and
//! ahead/behind counts in the capsule status bar.
//!
//! Not responsible for: fetching git state (caller provides `branch` and
//! `PullRequestInfo`) or mouse-event dispatch.
//!
//! Key invariant: `BRANCH_CONTEXT_BAR_ROWS` is the exact row budget callers
//! must reserve; rendering writes ANSI escape sequences directly into the
//! caller-supplied `buf` using absolute cursor positions.

use jackin_tui::components::{StatusRightChunk, StatusRightGroup, status_right_group_layout};
use jackin_tui::{display_cols, take_display_cols};

use crate::pull_request::PullRequestInfo;

pub const BRANCH_CONTEXT_BAR_ROWS: u16 = 1;

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
    pub(crate) usage_region: Option<ColRange>,
    pub(crate) debug_chip_region: Option<ColRange>,
    pub(crate) container_region: Option<ColRange>,
}

pub(crate) fn visible_branch(branch: Option<&str>, is_default_branch: bool) -> Option<&str> {
    branch.filter(|_| !is_default_branch)
}

/// Convert a placed right-group chunk into its clickable column range.
fn chunk_region(chunk: Option<&StatusRightChunk>) -> Option<ColRange> {
    chunk.and_then(|chunk| ColRange::new(chunk.start, chunk.end))
}

#[allow(
    clippy::too_many_arguments,
    reason = "documented residual allow; prefer expect when site is lint-true"
)]
pub(crate) fn branch_context_bar_layout(
    term_rows: u16,
    term_cols: u16,
    branch: Option<&str>,
    usage_status_label: Option<&str>,
    pull_request: Option<&PullRequestInfo>,
    pull_request_loading: bool,
    debug_run_id: Option<&str>,
    container_name: &str,
) -> Option<BranchContextBarLayout> {
    if term_rows == 0 || term_cols == 0 {
        return None;
    }
    // `branch` is the post-filter visible branch. Trust the input here so
    // renderer / layout / hit-test helpers stay default-branch-agnostic.
    let (context_left, left_clickable) = match (pull_request, branch) {
        (Some(pr), _) => (format!(" PR {} · {} ", pr.number_label(), pr.title), true),
        (None, Some(b)) if pull_request_loading => (format!(" Resolving PR · {b} "), true),
        (None, Some(b)) => (format!(" Branch · {b} "), true),
        (None, None) => (String::new(), false),
    };
    let term_cols_usize = usize::from(term_cols);
    let right = status_right_group_layout(
        term_cols,
        StatusRightGroup {
            usage: usage_status_label,
            container: container_name,
            run_id: debug_run_id,
        },
    );

    let right_start = right.start(term_cols_usize.saturating_add(1));
    let left_max_cols = right_start.saturating_sub(2);
    let left = take_display_cols(&context_left, left_max_cols);
    let left_cols = display_cols(&left);
    let left_region = if left_clickable && left_cols > 0 {
        let end = u16::try_from(left_cols.saturating_add(1)).unwrap_or(u16::MAX);
        ColRange::new(1, end)
    } else {
        None
    };
    let usage_region = chunk_region(right.usage.as_ref());
    let debug_chip_region = chunk_region(right.run_id.as_ref());
    let container_region = chunk_region(right.container.as_ref());
    Some(BranchContextBarLayout {
        left,
        left_region,
        usage_region,
        debug_chip_region,
        container_region,
    })
}

pub(crate) fn debug_run_id_label() -> Option<String> {
    if !crate::logging::debug_enabled() {
        return None;
    }
    std::env::var("JACKIN_RUN_ID")
        .ok()
        .filter(|id| !id.is_empty())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum BranchContextBarHit {
    Context,
    UsageStatus,
    Container,
    /// Click on the debug run-id chip (only shown when `--debug` is active).
    DebugChip,
}

#[allow(
    clippy::too_many_arguments,
    reason = "documented residual allow; prefer expect when site is lint-true"
)]
pub(crate) fn branch_context_bar_hit(
    row: u16,
    col: u16,
    term_rows: u16,
    term_cols: u16,
    branch: Option<&str>,
    usage_status_label: Option<&str>,
    pull_request: Option<&PullRequestInfo>,
    pull_request_loading: bool,
    debug_run_id: Option<&str>,
    container_name: &str,
) -> Option<BranchContextBarHit> {
    if row != term_rows {
        return None;
    }
    let layout = branch_context_bar_layout(
        term_rows,
        term_cols,
        branch,
        usage_status_label,
        pull_request,
        pull_request_loading,
        debug_run_id,
        container_name,
    )?;
    if layout.debug_chip_region.is_some_and(|r| r.contains(col)) {
        return Some(BranchContextBarHit::DebugChip);
    }
    if layout.container_region.is_some_and(|r| r.contains(col)) {
        return Some(BranchContextBarHit::Container);
    }
    if layout.usage_region.is_some_and(|r| r.contains(col)) {
        return Some(BranchContextBarHit::UsageStatus);
    }
    if layout.left_region.is_some_and(|r| r.contains(col)) {
        return Some(BranchContextBarHit::Context);
    }
    None
}

#[cfg(test)]
mod tests;
