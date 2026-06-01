//! Terminal title and pane-label helpers for the capsule multiplexer.
//!
//! These pure functions drive the outer terminal's OSC 2 title and the
//! per-pane title bars. Keeping them here lets the compositor and the
//! session-spawn path share the same display rules without duplication.

use std::path::Path;

use jackin_tui::sanitize_terminal_title;

use crate::pull_request::PullRequestInfo;
use crate::session::Session;

const OUTER_TERMINAL_TITLE_MAX_CHARS: usize = 180;

/// First letter of `slug` capitalised; the rest preserved unchanged.
pub(crate) fn capitalize(s: &str) -> String {
    let mut c = s.chars();
    match c.next() {
        None => String::new(),
        Some(f) => f.to_uppercase().to_string() + c.as_str(),
    }
}

/// Human-readable label for the agent running in `session`.
///
/// Returns "Shell" when no agent is present, `"Slug (provider)"` when
/// a provider label is known, or `"Slug"` otherwise.
pub(crate) fn session_agent_label(session: &Session) -> String {
    let Some(slug) = session.agent.as_deref() else {
        return "Shell".to_string();
    };
    match session.provider.as_ref() {
        Some(provider) => format!("{} ({})", capitalize(slug), provider.label),
        None => capitalize(slug),
    }
}

/// Human-readable title for the pane box drawn above the session.
///
/// Priority: OSC 2 title > shortened cwd > session label.
pub(crate) fn display_title(session: &Session) -> String {
    let title = session.title().filter(|title| !title.trim().is_empty());
    let cwd = session.cwd().map(jackin_tui::shorten_home);
    title
        .map(str::to_string)
        .or(cwd)
        .unwrap_or_else(|| session.label.clone())
}

/// Compose the outer terminal's OSC 2 window title from the workspace
/// path plus the current branch or PR context.
pub(crate) fn compose_outer_terminal_title(
    workdir: &Path,
    branch: Option<&str>,
    pull_request: Option<&PullRequestInfo>,
) -> String {
    let workspace = workspace_title(workdir);
    let context = pull_request
        .map(|pr| format!("PR #{} · {}", pr.number, pr.title))
        .or_else(|| branch.map(ToOwned::to_owned))
        .filter(|value| !value.trim().is_empty());

    let raw_title = match context {
        Some(context) => format!("{workspace} · {context}"),
        None => workspace,
    };
    trim_title_chars(
        &sanitize_terminal_title(&raw_title),
        OUTER_TERMINAL_TITLE_MAX_CHARS,
    )
}

/// Last path component of `workdir`, falling back to the full path.
pub(crate) fn workspace_title(workdir: &Path) -> String {
    workdir
        .file_name()
        .and_then(|name| name.to_str())
        .filter(|name| !name.trim().is_empty())
        .map(ToOwned::to_owned)
        .unwrap_or_else(|| workdir.display().to_string())
}

/// Truncate `title` to `max_chars` Unicode scalar values, appending
/// `…` when truncated.
pub(crate) fn trim_title_chars(title: &str, max_chars: usize) -> String {
    if title.chars().count() <= max_chars {
        return title.to_string();
    }
    let keep = max_chars.saturating_sub(1);
    let mut trimmed = title.chars().take(keep).collect::<String>();
    trimmed.push('…');
    trimmed
}

/// Emit an OSC 2 sequence for the given title to `buf`.
pub(crate) fn append_osc_window_title(buf: &mut Vec<u8>, title: &str) {
    buf.extend_from_slice(b"\x1b]2;");
    buf.extend_from_slice(title.as_bytes());
    buf.extend_from_slice(b"\x1b\\");
}
