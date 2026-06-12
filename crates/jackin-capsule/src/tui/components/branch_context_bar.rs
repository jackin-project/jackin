//! Branch context bar component: renders the current git branch and
//! ahead/behind counts in the capsule status bar.
//!
//! Not responsible for: fetching git state (caller provides `branch` and
//! `PullRequestInfo`) or mouse-event dispatch.
//!
//! Key invariant: `BRANCH_CONTEXT_BAR_ROWS` is the exact row budget callers
//! must reserve; rendering writes ANSI escape sequences directly into the
//! caller-supplied `buf` using absolute cursor positions.

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
    usage_status_label: Option<&str>,
    pull_request: Option<&PullRequestInfo>,
    pull_request_loading: bool,
    container_name: &str,
) -> Option<BranchContextBarLayout> {
    if term_rows == 0 || term_cols == 0 {
        return None;
    }
    // `branch` is the post-filter visible branch. Trust the input here so
    // renderer / layout / hit-test helpers stay default-branch-agnostic.
    let (context_left, mut left_clickable) = match (pull_request, branch) {
        (Some(pr), _) => (format!(" PR {} · {} ", pr.number_label(), pr.title), true),
        (None, Some(b)) if pull_request_loading => (format!(" Resolving PR · {b} "), true),
        (None, Some(b)) => (format!(" Branch · {b} "), true),
        (None, None) => (String::new(), false),
    };
    let container = if container_name.is_empty() {
        String::new()
    } else {
        format!(" {container_name} ")
    };
    let term_cols_usize = usize::from(term_cols);
    let container_cols = display_cols(&container);
    let container_fits = container_cols > 0 && container_cols + 2 < term_cols_usize;
    let left_max_cols = if container_fits {
        term_cols_usize.saturating_sub(container_cols + 1)
    } else {
        term_cols_usize
    };
    let usage = usage_status_label.filter(|s| !s.is_empty());
    let mut left = branch_context_left_label(&context_left, usage);
    if let Some(usage) = usage
        && display_cols(&left) > left_max_cols
    {
        let compact_usage = compact_usage_status_label(usage);
        left = branch_context_left_label(&context_left, Some(&compact_usage));
        if !context_left.is_empty() && display_cols(&left) > left_max_cols {
            left = branch_context_left_label("", Some(&compact_usage));
            left_clickable = false;
        }
    }
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
        debug_chip_region: debug_chip_range(term_cols),
    })
}

fn branch_context_left_label(context_left: &str, usage: Option<&str>) -> String {
    match (context_left.is_empty(), usage) {
        (true, Some(usage)) => format!(" {usage} "),
        (false, Some(usage)) => format!("{} · {usage} ", context_left.trim_end()),
        (false, None) => context_left.to_owned(),
        (true, None) => String::new(),
    }
}

fn compact_usage_status_label(label: &str) -> String {
    let provider = label
        .split(" · ")
        .next()
        .and_then(|head| head.split_whitespace().next())
        .unwrap_or("Usage");
    let parts = label
        .split(" · ")
        .map(str::trim)
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>();
    let remaining = parts
        .iter()
        .find(|part| part.contains("% left"))
        .map(|part| (*part).to_owned());
    let state = parts
        .iter()
        .rev()
        .find_map(|part| usage_lifecycle_word(part));
    match (remaining, state) {
        (Some(remaining), Some(state)) => format!("{provider} {remaining} · {state}"),
        (Some(remaining), None) => format!("{provider} {remaining}"),
        (None, Some(state)) => format!("{provider} {state}"),
        (None, None) => provider.to_owned(),
    }
}

fn usage_lifecycle_word(part: &str) -> Option<&'static str> {
    let lower = part.to_ascii_lowercase();
    [
        "login",
        "secret",
        "stale",
        "unsupported",
        "unavailable",
        "error",
    ]
    .into_iter()
    .find(|word| lower.contains(word))
}

pub(crate) fn debug_chip_range(term_cols: u16) -> Option<ColRange> {
    if !crate::logging::debug_enabled() {
        return None;
    }
    let Ok(run_id) = std::env::var("JACKIN_RUN_ID") else {
        return None;
    };
    if run_id.is_empty() {
        return None;
    }
    let chip = format!(" {run_id} ");
    let chip_cols = u16::try_from(display_cols(&chip)).unwrap_or(u16::MAX);
    let chip_start = term_cols.saturating_sub(chip_cols).saturating_add(1);
    ColRange::new(chip_start, term_cols.saturating_add(1))
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
        None,
        pull_request,
        pull_request_loading,
        container_name,
    )?;
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
