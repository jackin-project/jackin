// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Tests for `selection`.
use super::{
    SelectionState, move_selection_end, selection_start_for_inner_rect, selection_was_dragged,
    visible_selection,
};
use crate::tui::layout::Rect;
use crate::tui::pane_snapshot::{CellSnapshot, RowSnapshot};
use crate::tui::selection::word_bounds_in_row;
use unicode_width::UnicodeWidthChar;

#[test]
fn selection_start_requires_inner_rect_hit() {
    let inner = Rect::new(10, 20, 5, 8);

    let sel = selection_start_for_inner_rect(7, inner, 12, 24, 0, 0).unwrap();
    assert_eq!(sel.session_id, 7);
    assert_eq!(sel.inner, inner);
    assert_eq!((sel.anchor_row, sel.anchor_col), (2, 4));
    assert_eq!((sel.end_row, sel.end_col), (2, 4));

    assert!(selection_start_for_inner_rect(7, inner, 9, 24, 0, 0).is_none());
    assert!(selection_start_for_inner_rect(7, inner, 12, 28, 0, 0).is_none());
}

#[test]
fn selection_start_records_content_row_when_scrolled() {
    let inner = Rect::new(10, 20, 5, 8);

    let sel = selection_start_for_inner_rect(7, inner, 12, 24, 12, 4).unwrap();

    assert_eq!(
        sel.anchor_row, 10,
        "content row = filled - offset + visible row"
    );
    assert_eq!((sel.anchor_col, sel.end_row, sel.end_col), (4, 10, 4));
}

#[test]
fn selection_motion_clamps_to_inner_rect() {
    let mut sel = SelectionState {
        session_id: 7,
        inner: Rect::new(10, 20, 5, 8),
        anchor_row: 1,
        anchor_col: 2,
        end_row: 1,
        end_col: 2,
    };

    move_selection_end(&mut sel, 99, 99, 0, 0);
    assert_eq!((sel.end_row, sel.end_col), (4, 7));

    move_selection_end(&mut sel, 0, 0, 0, 0);
    assert_eq!((sel.end_row, sel.end_col), (0, 0));
}

#[test]
fn visible_selection_projects_content_rows_into_viewport() {
    let sel = SelectionState {
        session_id: 7,
        inner: Rect::new(10, 20, 5, 8),
        anchor_row: 9,
        anchor_col: 1,
        end_row: 12,
        end_col: 3,
    };

    let visible = visible_selection(&sel, 12, 4).expect("selection intersects viewport");

    assert_eq!((visible.start_row, visible.start_col), (1, 1));
    assert_eq!((visible.end_row, visible.end_col), (4, 3));
}

#[test]
fn same_cell_selection_is_not_a_drag() {
    let mut sel = SelectionState {
        session_id: 7,
        inner: Rect::new(10, 20, 5, 8),
        anchor_row: 1,
        anchor_col: 2,
        end_row: 1,
        end_col: 2,
    };
    assert!(!selection_was_dragged(&sel));

    move_selection_end(&mut sel, 12, 24, 0, 0);
    assert!(selection_was_dragged(&sel));
}

/// Shared fixtures for the word-boundary suites: build a `RowSnapshot`
/// from a string (real unicode widths) and resolve the word under a click.
pub(super) fn row(text: &str) -> RowSnapshot {
    RowSnapshot {
        cells: text
            .chars()
            .map(|ch| CellSnapshot {
                contents: ch.to_string(),
                width: u16::try_from(UnicodeWidthChar::width(ch).unwrap_or(1)).unwrap_or(1),
            })
            .collect(),
    }
}

/// Resolve the word under a click on the first occurrence of `probe`
/// (clicking its middle cell) and return the selected text.
pub(super) fn word_at(text: &str, probe: &str) -> Option<String> {
    let snapshot = row(text);
    let probe_start = text.find(probe).expect("probe in line");
    let probe_char_idx = text[..probe_start].chars().count() + probe.chars().count() / 2;
    let cells = snapshot.display_cells();
    let col = cells[probe_char_idx].start_col;
    let (start, end) = word_bounds_in_row(&snapshot, col)?;
    Some(snapshot.text_range(start, end))
}

/// Resolve the word under a click at an explicit display column.
pub(super) fn word_at_col(text: &str, col: u16) -> Option<String> {
    let snapshot = row(text);
    let (start, end) = word_bounds_in_row(&snapshot, col)?;
    Some(snapshot.text_range(start, end))
}

#[test]
fn plain_words_and_versions() {
    let line = ">_ OpenAI Codex (v0.139.0)";
    assert_eq!(word_at(line, "OpenAI").as_deref(), Some("OpenAI"));
    assert_eq!(word_at(line, "v0.139.0").as_deref(), Some("v0.139.0"));
}

#[test]
fn slash_commands_models_and_hyphens() {
    let line = "model:       gpt-5.5   /model to change";
    assert_eq!(word_at(line, "/model").as_deref(), Some("/model"));
    assert_eq!(word_at(line, "gpt-5.5").as_deref(), Some("gpt-5.5"));
    assert_eq!(
        word_at("Visit it for up-to-date information", "up-to-date").as_deref(),
        Some("up-to-date")
    );
    assert_eq!(
        word_at("GPT-5.3-Codex-Spark limit:", "GPT-5.3-Codex-Spark").as_deref(),
        Some("GPT-5.3-Codex-Spark")
    );
}

#[test]
fn paths_stay_whole_including_ellipsis() {
    let line = "directory:   ~/Projects/jackin-project/\u{2026}/pr-555/jackin ";
    assert_eq!(
        word_at(line, "pr-555").as_deref(),
        Some("~/Projects/jackin-project/\u{2026}/pr-555/jackin")
    );
}

#[test]
fn urls_take_priority_over_token_separators() {
    let line = "Visit https://chatgpt.com/codex/settings/usage for up-to-date";
    assert_eq!(
        word_at(line, "settings").as_deref(),
        Some("https://chatgpt.com/codex/settings/usage")
    );
    assert_eq!(
        word_at("(see https://x.test/a;b).", "x.test").as_deref(),
        Some("https://x.test/a;b"),
        "trailing `).` is prose; `;` inside the URL is address"
    );
    assert_eq!(
        word_at(
            "https://en.wikipedia.org/wiki/Rust_(language) is fine",
            "wiki"
        )
        .as_deref(),
        Some("https://en.wikipedia.org/wiki/Rust_(language)"),
        "balanced closing paren stays in the URL"
    );
}

#[test]
fn wrappers_trim_but_joiners_hold() {
    let line = "\u{203a} (xxx) [yyyy] <xxxxx;ddddd>   -xxx_xxxx-";
    assert_eq!(word_at(line, "xxx)").as_deref(), Some("xxx"));
    assert_eq!(word_at(line, "yyyy").as_deref(), Some("yyyy"));
    assert_eq!(word_at(line, "xxxxx;").as_deref(), Some("xxxxx"));
    assert_eq!(word_at(line, "ddddd").as_deref(), Some("ddddd"));
    assert_eq!(word_at(line, "-xxx_xxxx-").as_deref(), Some("-xxx_xxxx-"));
    assert_eq!(
        word_at("\u{203a} Implement {feature}", "feature").as_deref(),
        Some("feature")
    );
}

#[test]
fn interior_colons_join_trailing_colons_trim() {
    assert_eq!(
        word_at("(resets 00:25 on 12 Jun)", "00:25").as_deref(),
        Some("00:25")
    );
    assert_eq!(
        word_at("  aaaaa:uuuuu", "aaaaa").as_deref(),
        Some("aaaaa:uuuuu")
    );
    let line = "  Agents.md:                   AGENTS.md";
    assert_eq!(word_at(line, "Agents.md").as_deref(), Some("Agents.md"));
    assert_eq!(word_at(line, "AGENTS.md").as_deref(), Some("AGENTS.md"));
}

#[test]
fn separators_and_blanks_yield_nothing() {
    let snapshot = row("a b");
    assert_eq!(word_bounds_in_row(&snapshot, 1), None, "click on space");
    assert_eq!(
        word_bounds_in_row(&row("(  )"), 0),
        None,
        "click on a separator"
    );
}

#[test]
fn wide_cells_map_columns_correctly() {
    // "你好 jackin" — the two wide cells occupy columns 0..=3, so the
    // ASCII word starts at display column 5.
    let line = "\u{4f60}\u{597d} jackin";
    let snapshot = row(line);
    let (start, end) = word_bounds_in_row(&snapshot, 7).expect("word under click");
    assert_eq!((start, end), (5, 10));
    assert_eq!(snapshot.text_range(start, end), "jackin");
    let (start, end) = word_bounds_in_row(&snapshot, 1).expect("wide word under click");
    assert_eq!(snapshot.text_range(start, end), "\u{4f60}\u{597d}");
}

#[test]
fn quoted_paths_with_spaces_select_whole_without_quotes() {
    let line = r#"cp "/my docs/file one.txt" /dest"#;
    // Click inside "docs".
    assert_eq!(
        word_at_col(line, 9).as_deref(),
        Some("/my docs/file one.txt")
    );
    // Single quotes and backticks pair the same way.
    assert_eq!(
        word_at_col("see '/tmp/a b' now", 7).as_deref(),
        Some("/tmp/a b")
    );
}

#[test]
fn quoted_text_without_a_slash_is_a_plain_token() {
    // No `/` inside the quotes: the token pass owns the click.
    assert_eq!(
        word_at_col(r#"say "hello there""#, 6).as_deref(),
        Some("hello")
    );
}

#[test]
fn escaped_quotes_are_content_not_delimiters() {
    // The escaped quote must not close the pair early.
    let line = r#"x "/a \"b\" c/d" y"#;
    assert_eq!(word_at_col(line, 5).as_deref(), Some(r#"/a \"b\" c/d"#));
}

#[test]
fn bang_separates_tokens() {
    let line = "wow!yes";
    assert_eq!(word_at_col(line, 1).as_deref(), Some("wow"));
    assert_eq!(word_at_col(line, 5).as_deref(), Some("yes"));
    assert_eq!(word_at_col(line, 3), None, "click on the bang itself");
}

#[test]
fn blank_rows_select_nothing() {
    assert_eq!(word_at_col("", 0), None);
    assert_eq!(word_at_col("      ", 3), None);
}

#[test]
fn angle_brackets_break_like_the_bracket_family() {
    // Alacritty / kitty / VS Code semantics: generics and includes
    // double-click to the inner identifier.
    let line = "let v: Vec<String> = read();";
    assert_eq!(word_at_col(line, 13).as_deref(), Some("String"));
    assert_eq!(word_at_col(line, 8).as_deref(), Some("Vec"));
    assert_eq!(
        word_at_col("#include <stdio.h>", 13).as_deref(),
        Some("stdio.h")
    );
    assert_eq!(
        word_at_col("from <user@host.test> inbox", 8).as_deref(),
        Some("user@host.test")
    );
}

#[test]
fn kitty_word_characters_all_join() {
    // kitty's select_by_word_characters set: @ - . / _ ~ ? & = % + #
    // must all extend a token so URLs-ish and env-ish tokens copy whole.
    let line = "FOO=bar+baz%2F~x#frag&q?y@host";
    assert_eq!(word_at_col(line, 12).as_deref(), Some(line));
}

#[test]
fn apostrophes_survive_inside_prose_words() {
    assert_eq!(word_at_col("it don't break", 6).as_deref(), Some("don't"));
}
