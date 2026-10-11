// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! Double-click word bounds: token, quoted-path, and URL detection.

use crate::tui::pane_snapshot::{DisplayCell, RowSnapshot};

/// Inclusive display-column bounds of the word under a double-click.
///
/// Three passes, in priority order: a URL pass that keeps `http(s)://…`
/// spans whole (token separators like `;` legally appear inside URLs), a
/// quoted-path pass that keeps a quoted span containing `/` whole (spaces
/// included, quotes excluded), then a token pass that expands across
/// non-separator cells and strips wrapper punctuation from the edges — so a
/// click inside `{feature}` selects `feature`, while interior joiners
/// survive (`gpt-5.5`, `00:25`, `aaaaa:uuuuu`, `~/Projects/…/jackin`).
pub(crate) fn word_bounds_in_row(row: &RowSnapshot, col: u16) -> Option<(u16, u16)> {
    let cells = row.display_cells();
    let clicked = cells
        .iter()
        .position(|cell| cell.start_col <= col && col <= cell.end_col)?;
    url_word_bounds(&cells, clicked)
        .or_else(|| quoted_path_bounds(&cells, clicked))
        .or_else(|| token_word_bounds(&cells, clicked))
        .map(|(start, end)| (cells[start].start_col, cells[end].end_col))
}

/// First character of a cell's grapheme cluster, used to classify the cell
/// for boundary decisions. Blank/empty cells classify as `None`.
pub(crate) fn cell_char(cell: &DisplayCell<'_>) -> Option<char> {
    cell.contents.chars().next()
}

/// Cells that always end a token: whitespace, blanks, and structural
/// punctuation that never appears inside the things operators copy
/// (commands, paths, versions, timestamps). `<` / `>` break like the
/// bracket family — Alacritty, kitty, and VS Code agree, and it is what
/// makes a double-click on `String` in `Vec<String>` select `String`
/// (herdr's expand-then-trim yields `Vec<String` there). Quotes are not
/// separators: they expand into the token and the edge trim strips them,
/// so `don't` survives whole and the quoted-path pass can own quoted spans.
pub(crate) fn is_token_separator(cell: &DisplayCell<'_>) -> bool {
    match cell_char(cell) {
        None => true,
        Some(ch) => {
            ch.is_whitespace()
                || matches!(
                    ch,
                    '|' | '(' | ')' | '[' | ']' | '{' | '}' | '<' | '>' | ',' | ';' | '!'
                )
        }
    }
}

/// Opening wrapper punctuation stripped from a token's left edge.
pub(crate) fn is_leading_wrapper(ch: char) -> bool {
    matches!(ch, '"' | '\'' | '`')
}

/// Closing wrapper and clause punctuation stripped from a token's right
/// edge. `:` and `.` are interior joiners (`00:25`, `AGENTS.md`) but trim
/// away when trailing (`Agents.md:` → `Agents.md`).
pub(crate) fn is_trailing_wrapper(ch: char) -> bool {
    matches!(ch, '"' | '\'' | '`' | '.' | ':' | '!' | '?')
}

pub(crate) fn token_word_bounds(
    cells: &[DisplayCell<'_>],
    clicked: usize,
) -> Option<(usize, usize)> {
    if is_token_separator(&cells[clicked]) {
        return None;
    }
    let mut start = clicked;
    while start > 0 && !is_token_separator(&cells[start - 1]) {
        start -= 1;
    }
    let mut end = clicked;
    while end + 1 < cells.len() && !is_token_separator(&cells[end + 1]) {
        end += 1;
    }
    while start <= end && cell_char(&cells[start]).is_some_and(is_leading_wrapper) {
        start += 1;
    }
    while start <= end && cell_char(&cells[end]).is_some_and(is_trailing_wrapper) {
        end = end.checked_sub(1)?;
    }
    (start <= end && (start..=end).contains(&clicked)).then_some((start, end))
}

/// A quoted span containing a `/` selects as one path, spaces included and
/// quotes excluded — `cp "/my docs/file one.txt" …` double-clicks whole.
/// Quotes pair left-to-right per quote kind; a backslash-escaped quote is
/// content, not a delimiter. Clicking on a quote character itself falls
/// through to the token pass.
pub(crate) fn quoted_path_bounds(
    cells: &[DisplayCell<'_>],
    clicked: usize,
) -> Option<(usize, usize)> {
    let clicked_ch = cell_char(&cells[clicked])?;
    if matches!(clicked_ch, '"' | '\'' | '`') {
        return None;
    }
    for quote in ['"', '\'', '`'] {
        let mut open: Option<usize> = None;
        for idx in 0..cells.len() {
            if cell_char(&cells[idx]) != Some(quote) || quote_is_escaped(cells, idx) {
                continue;
            }
            let Some(open_idx) = open else {
                open = Some(idx);
                continue;
            };
            if clicked > open_idx
                && clicked < idx
                && cells[open_idx + 1..idx]
                    .iter()
                    .any(|cell| cell_char(cell) == Some('/'))
            {
                return Some((open_idx + 1, idx - 1));
            }
            open = None;
        }
    }
    None
}

/// True when an odd run of backslashes precedes the cell — the quote at
/// `idx` is escaped content rather than a delimiter.
pub(crate) fn quote_is_escaped(cells: &[DisplayCell<'_>], idx: usize) -> bool {
    let mut backslashes = 0;
    let mut cursor = idx;
    while cursor > 0 && cell_char(&cells[cursor - 1]) == Some('\\') {
        backslashes += 1;
        cursor -= 1;
    }
    backslashes % 2 == 1
}

pub(crate) fn url_word_bounds(cells: &[DisplayCell<'_>], clicked: usize) -> Option<(usize, usize)> {
    let mut idx = 0;
    while idx < cells.len() {
        if !cells_start_with(&cells[idx..], "http://")
            && !cells_start_with(&cells[idx..], "https://")
        {
            idx += 1;
            continue;
        }
        let mut end = idx;
        while end + 1 < cells.len()
            && cell_char(&cells[end + 1]).is_some_and(|ch| !ch.is_whitespace())
        {
            end += 1;
        }
        if !(idx..=end).contains(&clicked) {
            idx = end + 1;
            continue;
        }
        // Sentence punctuation and unbalanced closers after the URL are
        // prose, not address: `(see https://x.y/z).` must yield the bare
        // URL. A closer with a matching opener inside the span stays —
        // wiki-style URLs end in `)`.
        while end > idx {
            let Some(ch) = cell_char(&cells[end]) else {
                break;
            };
            let trim = match ch {
                '"' | '\'' | '`' | '.' | ',' | ';' | ':' | '!' | '?' => true,
                ')' => !closer_is_balanced(&cells[idx..=end], '(', ')'),
                ']' => !closer_is_balanced(&cells[idx..=end], '[', ']'),
                '}' => !closer_is_balanced(&cells[idx..=end], '{', '}'),
                _ => false,
            };
            if !trim {
                break;
            }
            end -= 1;
        }
        return (idx..=end).contains(&clicked).then_some((idx, end));
    }
    None
}

pub(crate) fn cells_start_with(cells: &[DisplayCell<'_>], prefix: &str) -> bool {
    let mut prefix_chars = prefix.chars();
    for cell in cells {
        let Some(expected) = prefix_chars.next() else {
            return true;
        };
        if cell_char(cell) != Some(expected) {
            return false;
        }
    }
    prefix_chars.next().is_none()
}

pub(crate) fn closer_is_balanced(span: &[DisplayCell<'_>], open: char, close: char) -> bool {
    let mut depth = 0i32;
    for cell in span {
        match cell_char(cell) {
            Some(ch) if ch == open => depth += 1,
            Some(ch) if ch == close => depth -= 1,
            _ => {}
        }
    }
    depth >= 0
}
