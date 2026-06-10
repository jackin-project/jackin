//! Claude Code screen-state detector.
//!
//! Patterns verified against Claude Code TUI output (`CCManager` / ccmux /
//! Agent Session Manager source reviews). Inspects only the bottom rows of
//! the current viewport — never historical scrollback.

use super::{Detector, bottom_rows, contains_ci};
use crate::agent_status::AgentRawState;

/// Spinner glyphs Claude Code cycles through while working. Followed by a
/// word ending in `ing` and an ellipsis (U+2026), e.g. `✻ Tempering…`.
const SPINNER_GLYPHS: &[char] = &[
    '✱', '✲', '✳', '✴', '✵', '✶', '✷', '✸', '✹', '✺', '✻', '✼', '✽', '✾', '✿', '❀', '❁', '❂', '❃',
    '❇', '❈', '❉', '❊', '❋', '✢', '✣', '✤', '✥', '✦', '✧', '✨', '⊛', '⊕', '⊙', '◉', '◎', '◍', '⁂',
    '⁕', '※', '⍟', '☼', '★', '☆', '·', '•', '⏺', '▸', '▹', '∙', '⋅', '○', '●',
];

#[derive(Debug)]
pub struct ClaudeDetector;

impl Detector for ClaudeDetector {
    fn agent_slug(&self) -> Option<&str> {
        Some("claude")
    }

    fn detect(&self, screen_rows: &[String]) -> Option<AgentRawState> {
        let rows = bottom_rows(screen_rows, super::DETECTION_ROWS);
        let joined = rows.join("\n");

        // Transcript viewer: suppress all updates (idle-looking transcript
        // must not override a working state).
        if contains_ci(&joined, "showing detailed transcript")
            && contains_ci(&joined, "ctrl+o to toggle")
        {
            return None;
        }

        // Blocked: explicit approval / input-required prompts in the visible
        // screen. Highest priority — operator action needed.
        if is_blocked(&rows, &joined) {
            return Some(AgentRawState::BlockedVisible);
        }

        // Working: spinner chrome, interrupt hint, or token-stats line.
        if is_working(&rows) {
            return Some(AgentRawState::WorkingVisible);
        }

        // Idle: rounded-corner prompt box visible at the bottom.
        if is_prompt_box_visible(&rows) {
            return Some(AgentRawState::PromptVisible);
        }

        None
    }
}

/// Blocked: a current-screen approval/input dialog.
fn is_blocked(rows: &[String], joined: &str) -> bool {
    // Multi-choice approval dialog: all three affordances present.
    let has_select_dialog = contains_ci(joined, "enter to select")
        && contains_ci(joined, "esc to cancel")
        && (contains_ci(joined, "to navigate") || joined.contains("↑/↓"));
    if has_select_dialog {
        return true;
    }
    // Yes/no proceed prompt.
    if contains_ci(joined, "do you want to proceed?") {
        return true;
    }
    // Other explicit blockers, matched per-line to avoid stale scrollback.
    rows.iter().any(|line| {
        contains_ci(line, "waiting for permission")
            || contains_ci(line, "tab to amend")
            || contains_ci(line, "ctrl+e to explain")
            || contains_ci(line, "review your answers")
    })
}

/// Working: spinner + `…ing`, interrupt chrome, or `(... N tokens)` line.
fn is_working(rows: &[String]) -> bool {
    rows.iter().any(|line| {
        if contains_ci(line, "esc to interrupt") || contains_ci(line, "ctrl+c to interrupt") {
            return true;
        }
        if has_spinner_working(line) {
            return true;
        }
        if has_token_stats(line) {
            return true;
        }
        false
    })
}

/// Match `<spinner-glyph> <word>ing…` — a spinner char, a space, a word
/// ending in `ing`, and the ellipsis (U+2026).
fn has_spinner_working(line: &str) -> bool {
    let chars: Vec<char> = line.chars().collect();
    for (i, &c) in chars.iter().enumerate() {
        if SPINNER_GLYPHS.contains(&c) {
            // Look for `…` later on the line with an `ing` before it.
            let rest: String = chars[i + 1..].iter().collect();
            if rest.contains('…') && contains_ci(&rest, "ing") {
                return true;
            }
        }
    }
    false
}

/// Match a parenthesized chunk that contains a digit and the word `tokens`,
/// e.g. `(9m 21s · ↓ 13.7k tokens)`. Requires a digit so `(see tokens in
/// docs)` does not false-positive.
fn has_token_stats(line: &str) -> bool {
    let chars = line.char_indices().peekable();
    for (start, c) in chars {
        if c != '(' {
            continue;
        }
        // Scan to the matching close paren.
        if let Some(end) = line[start + 1..].find(')') {
            let inner = &line[start + 1..start + 1 + end];
            if inner.contains(|ch: char| ch.is_ascii_digit()) && contains_ci(inner, "tokens") {
                return true;
            }
        }
    }
    false
}

/// Idle: Claude renders a rounded-corner prompt box at the bottom. We
/// require at least one rounded corner char (rules out `tree` output and
/// sharp-corner status bars) plus a box-drawing density.
fn is_prompt_box_visible(rows: &[String]) -> bool {
    // ccmux approach: last non-empty line contains a rounded corner AND has
    // ≥2 distinct box-drawing/prompt chars.
    let last = rows.iter().rev().find(|l| !l.is_empty());
    let Some(last) = last else { return false };
    let has_rounded =
        last.contains('╭') || last.contains('╮') || last.contains('╰') || last.contains('╯');
    if !has_rounded {
        // Also accept a horizontal border line above the input area.
        return rows.iter().any(|l| {
            let dashes = l.chars().filter(|&c| c == '─' || c == '━').count();
            dashes > 20
        });
    }
    let distinct = ['╭', '╮', '╰', '╯', '│', '─', '>']
        .iter()
        .filter(|&&c| last.contains(c))
        .count();
    distinct >= 2
}

#[cfg(test)]
mod tests {
    use super::*;

    fn screen(bytes: &[u8]) -> Vec<String> {
        String::from_utf8_lossy(bytes)
            .replace("\r\n", "\n")
            .lines()
            .map(str::to_owned)
            .collect()
    }

    #[test]
    fn detects_working_from_spinner() {
        let s = screen("✻ Tempering… (9m 21s · ↓ 13.7k tokens)\r\n".as_bytes());
        assert_eq!(
            ClaudeDetector.detect(&s),
            Some(AgentRawState::WorkingVisible)
        );
    }

    #[test]
    fn detects_working_from_interrupt_hint() {
        let s = screen(b"Doing stuff\r\nesc to interrupt\r\n");
        assert_eq!(
            ClaudeDetector.detect(&s),
            Some(AgentRawState::WorkingVisible)
        );
    }

    #[test]
    fn detects_blocked_from_approval_dialog() {
        let s = screen(
            "Do you want to proceed?\r\n  enter to select  ↑/↓ to navigate  esc to cancel\r\n"
                .as_bytes(),
        );
        assert_eq!(
            ClaudeDetector.detect(&s),
            Some(AgentRawState::BlockedVisible)
        );
    }

    #[test]
    fn detects_idle_from_prompt_box() {
        let s = screen("╭────────────╮\r\n│ >          │\r\n╰────────────╯\r\n".as_bytes());
        assert_eq!(
            ClaudeDetector.detect(&s),
            Some(AgentRawState::PromptVisible)
        );
    }

    #[test]
    fn transcript_viewer_suppresses_state() {
        let s = screen(
            "showing detailed transcript\r\nctrl+o to toggle  ctrl+e to show all\r\n".as_bytes(),
        );
        assert_eq!(ClaudeDetector.detect(&s), None);
    }

    #[test]
    fn token_stats_requires_digit() {
        // No digit → not a working signal.
        assert!(!has_token_stats("(see tokens in docs)"));
        assert!(has_token_stats("(9m 21s · 13.7k tokens)"));
    }

    #[test]
    fn empty_screen_yields_none() {
        let s = screen(b"");
        assert_eq!(ClaudeDetector.detect(&s), None);
    }
}
