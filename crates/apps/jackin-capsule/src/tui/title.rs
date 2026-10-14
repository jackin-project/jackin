// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::path::Path;

use termrock::text::sanitize_terminal_title;

use crate::pull_request::PullRequestInfo;

const OUTER_TERMINAL_TITLE_MAX_CHARS: usize = 180;

pub(crate) fn pane_display_title(
    title: Option<&str>,
    cwd: Option<&str>,
    fallback_label: &str,
) -> String {
    let title = title.filter(|title| !title.trim().is_empty());
    let cwd = cwd.map(jackin_core::shorten_home);
    title
        .map(str::to_owned)
        .or(cwd)
        .unwrap_or_else(|| fallback_label.to_owned())
}

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

pub(crate) fn workspace_title(workdir: &Path) -> String {
    workdir
        .file_name()
        .and_then(|name| name.to_str())
        .filter(|name| !name.trim().is_empty())
        .map_or_else(|| workdir.display().to_string(), ToOwned::to_owned)
}

pub(crate) fn trim_title_chars(title: &str, max_chars: usize) -> String {
    if title.chars().count() <= max_chars {
        return title.to_owned();
    }
    let keep = max_chars.saturating_sub(1);
    let mut trimmed = title.chars().take(keep).collect::<String>();
    trimmed.push('…');
    trimmed
}

pub(crate) fn append_osc_window_title(buf: &mut Vec<u8>, title: &str) {
    buf.extend_from_slice(b"\x1b]2;");
    buf.extend_from_slice(title.as_bytes());
    buf.extend_from_slice(b"\x1b\\");
}
