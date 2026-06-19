//! Console keymaps — single source of truth coupling key dispatch and hint bar advertisement.
//!
//! Production keymaps here drive both `input/*.rs` dispatch (via `Keymap::dispatch()`)
//! and `jackin-console` hint builders (re-produce matching spans since that crate cannot
//! depend on this one). Phase 2 migration: preview-pane done; remaining console input
//! surfaces (editor, settings, list navigation) deferred to Phase 3.

#[cfg(test)]
use jackin_tui::HintSpan;
use jackin_tui::components::{KeyBinding, KeyChord, Keymap, LogicalKey, Visibility};

// ── Preview pane (workspace list → preview focus) ─────────────────────────────

/// Actions available in the workspace-list preview-pane focus mode.
///
/// `NavigatePane` covers both Up and Down; the dispatch site checks the chord to
/// determine direction (up = delta −1, down = delta +1).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PreviewPaneAction {
    NavigatePane,
    Attach,
    Back,
    Quit,
}

/// Authoritative keymap for preview-pane focus. Drives both
/// `handle_preview_focused_key` dispatch and the footer-hint builder.
pub(crate) static PREVIEW_PANE_KEYMAP: Keymap<PreviewPaneAction> = Keymap::new(&[
    KeyBinding {
        chords: &[KeyChord::plain(LogicalKey::Up), KeyChord::plain(LogicalKey::Down)],
        action: PreviewPaneAction::NavigatePane,
        hint: Some("navigate panes"),
        visibility: Visibility::Shown,
        glyph: Some("↑↓"),
    },
    KeyBinding {
        chords: &[
            KeyChord::plain(LogicalKey::Char('j')),
            KeyChord::plain(LogicalKey::Char('J')),
        ],
        action: PreviewPaneAction::NavigatePane,
        hint: None,
        visibility: Visibility::HiddenAlias,
        glyph: None,
    },
    KeyBinding {
        chords: &[
            KeyChord::plain(LogicalKey::Char('k')),
            KeyChord::plain(LogicalKey::Char('K')),
        ],
        action: PreviewPaneAction::NavigatePane,
        hint: None,
        visibility: Visibility::HiddenAlias,
        glyph: None,
    },
    KeyBinding {
        chords: &[KeyChord::plain(LogicalKey::Enter)],
        action: PreviewPaneAction::Attach,
        hint: Some("attach focused pane"),
        visibility: Visibility::Shown,
        glyph: Some("↵"),
    },
    KeyBinding {
        chords: &[KeyChord::plain(LogicalKey::Esc), KeyChord::plain(LogicalKey::Left)],
        action: PreviewPaneAction::Back,
        hint: Some("back"),
        visibility: Visibility::Shown,
        glyph: Some("Esc/←"),
    },
    KeyBinding {
        chords: &[KeyChord::plain(LogicalKey::BackTab)],
        action: PreviewPaneAction::Back,
        hint: None,
        visibility: Visibility::HiddenAlias,
        glyph: None,
    },
    // Ctrl-Q is handled by the outer should_open_quit_confirm before reaching
    // handle_preview_focused_key, but is registered Internal so it appears in
    // the dispatched key set for coverage tests without adding a hint span.
    KeyBinding {
        chords: &[KeyChord::ctrl(LogicalKey::Char('q'))],
        action: PreviewPaneAction::Quit,
        hint: None,
        visibility: Visibility::Internal,
        glyph: None,
    },
]);

/// Hint spans for the preview-pane focus derived from [`PREVIEW_PANE_KEYMAP`].
///
/// These spans are re-produced verbatim in `jackin-console` `footer_hints.rs`
/// (`PreviewPane` arm) because `jackin-console` cannot depend on this module.
/// Test-only: validates the re-produced spans match what the keymap generates.
#[cfg(test)]
#[must_use]
pub(crate) fn preview_pane_hint_spans() -> Vec<HintSpan<'static>> {
    let mut spans = PREVIEW_PANE_KEYMAP.hint_spans();
    if spans.len() >= 4 {
        spans.insert(3, HintSpan::Sep);
    }
    spans
}

// ── Console yes/no modal ─────────────────────────────────────────────────────

/// Hint spans for console yes/no confirm modals.
///
/// Delegates to [`jackin_tui::components::confirm_hint_spans`] so hints agree with
/// `ConfirmState`/`CONFIRM_KEYMAP`. Previously `yes_no_footer_items` omitted `↵ confirm`.
/// Test-only: validates that `yes_no_footer_items` produces the same spans.
#[cfg(test)]
#[must_use]
pub(crate) fn yes_no_hint_spans() -> Vec<HintSpan<'static>> {
    jackin_tui::components::confirm_hint_spans()
}

#[cfg(test)]
mod tests;
