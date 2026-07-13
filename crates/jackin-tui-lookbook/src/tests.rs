// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin_tui::components::{KeyChord, LogicalKey};
use jackin_tui::keymap::glyph;

use crate::{PREVIEW_KEYMAP, PreviewAction, SIDEBAR_KEYMAP, SidebarAction};

// ── SIDEBAR ───────────────────────────────────────────────────────────────────

#[test]
fn sidebar_down_up_dispatch_navigate() {
    assert_eq!(
        SIDEBAR_KEYMAP.dispatch(KeyChord::plain(LogicalKey::Down)),
        Some(SidebarAction::Navigate)
    );
    assert_eq!(
        SIDEBAR_KEYMAP.dispatch(KeyChord::plain(LogicalKey::Up)),
        Some(SidebarAction::Navigate)
    );
}

#[test]
fn sidebar_vim_aliases_dispatch_navigate() {
    assert_eq!(
        SIDEBAR_KEYMAP.dispatch(KeyChord::plain(LogicalKey::Char('j'))),
        Some(SidebarAction::Navigate)
    );
    assert_eq!(
        SIDEBAR_KEYMAP.dispatch(KeyChord::plain(LogicalKey::Char('k'))),
        Some(SidebarAction::Navigate)
    );
}

#[test]
fn sidebar_home_end_dispatch_go_to_edge() {
    assert_eq!(
        SIDEBAR_KEYMAP.dispatch(KeyChord::plain(LogicalKey::Home)),
        Some(SidebarAction::GoToEdge)
    );
    assert_eq!(
        SIDEBAR_KEYMAP.dispatch(KeyChord::plain(LogicalKey::End)),
        Some(SidebarAction::GoToEdge)
    );
}

#[test]
fn sidebar_tab_dispatches_focus_preview() {
    assert_eq!(
        SIDEBAR_KEYMAP.dispatch(KeyChord::plain(LogicalKey::Tab)),
        Some(SidebarAction::FocusPreview)
    );
}

#[test]
fn sidebar_q_esc_dispatch_quit() {
    assert_eq!(
        SIDEBAR_KEYMAP.dispatch(KeyChord::plain(LogicalKey::Char('q'))),
        Some(SidebarAction::Quit)
    );
    assert_eq!(
        SIDEBAR_KEYMAP.dispatch(KeyChord::plain(LogicalKey::Esc)),
        Some(SidebarAction::Quit)
    );
}

#[test]
fn sidebar_non_registered_keys_return_none() {
    for chord in [
        KeyChord::plain(LogicalKey::Enter),
        KeyChord::plain(LogicalKey::Char('a')),
        KeyChord::plain(LogicalKey::Char('Q')),
        KeyChord::ctrl(LogicalKey::Char('c')),
        KeyChord::plain(LogicalKey::PageUp),
        KeyChord::plain(LogicalKey::PageDown),
        KeyChord::plain(LogicalKey::BackTab),
    ] {
        assert_eq!(
            SIDEBAR_KEYMAP.dispatch(chord),
            None,
            "sidebar must not dispatch {chord:?}"
        );
    }
}

#[test]
fn sidebar_hints_advertise_navigate_and_quit() {
    let spans = SIDEBAR_KEYMAP.hint_spans();
    let text: String = spans
        .iter()
        .filter_map(|s| match s {
            jackin_tui::HintSpan::Key(k) | jackin_tui::HintSpan::Text(k) => Some(*k),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join(" ");
    assert!(text.contains("↑↓"), "must advertise ↑↓ navigate: {text}");
    assert!(text.contains("navigate"), "must label navigate: {text}");
    assert!(text.contains("⇥"), "must advertise Tab: {text}");
    assert!(text.contains("quit"), "must advertise quit: {text}");
    assert!(text.contains("Home"), "must advertise Home: {text}");
    // HiddenAlias — j/k must NOT appear in hint bar
    assert!(
        !text.contains(" j ") && !text.contains("j/k"),
        "j/k alias must not appear in hint: {text}"
    );
}

// ── PREVIEW ───────────────────────────────────────────────────────────────────

#[test]
fn preview_esc_tab_backtab_dispatch_back_to_list() {
    for chord in [
        KeyChord::plain(LogicalKey::Esc),
        KeyChord::plain(LogicalKey::Tab),
        KeyChord::plain(LogicalKey::BackTab),
    ] {
        assert_eq!(
            PREVIEW_KEYMAP.dispatch(chord),
            Some(PreviewAction::BackToList),
            "preview back-to-list must dispatch {chord:?}"
        );
    }
}

#[test]
fn preview_arrows_dispatch_forward() {
    for chord in [
        KeyChord::plain(LogicalKey::Up),
        KeyChord::plain(LogicalKey::Down),
        KeyChord::plain(LogicalKey::Left),
        KeyChord::plain(LogicalKey::Right),
    ] {
        assert_eq!(
            PREVIEW_KEYMAP.dispatch(chord),
            Some(PreviewAction::Forward),
            "preview must forward {chord:?}"
        );
    }
}

#[test]
fn preview_page_keys_dispatch() {
    assert_eq!(
        PREVIEW_KEYMAP.dispatch(KeyChord::plain(LogicalKey::PageDown)),
        Some(PreviewAction::PageDown)
    );
    assert_eq!(
        PREVIEW_KEYMAP.dispatch(KeyChord::plain(LogicalKey::PageUp)),
        Some(PreviewAction::PageUp)
    );
}

#[test]
fn preview_shift_j_k_dispatch_move_preview() {
    assert_eq!(
        PREVIEW_KEYMAP.dispatch(KeyChord::plain(LogicalKey::Char('J'))),
        Some(PreviewAction::MovePreviewDown)
    );
    assert_eq!(
        PREVIEW_KEYMAP.dispatch(KeyChord::plain(LogicalKey::Char('K'))),
        Some(PreviewAction::MovePreviewUp)
    );
}

#[test]
fn preview_non_registered_keys_return_none() {
    for chord in [
        KeyChord::plain(LogicalKey::Enter),
        KeyChord::plain(LogicalKey::Char('q')),
        KeyChord::plain(LogicalKey::Char('j')),
        KeyChord::plain(LogicalKey::Char('k')),
        KeyChord::ctrl(LogicalKey::Char('q')),
    ] {
        assert_eq!(
            PREVIEW_KEYMAP.dispatch(chord),
            None,
            "preview must not dispatch {chord:?}"
        );
    }
}

#[test]
fn preview_hints_advertise_back_and_interact() {
    let spans = PREVIEW_KEYMAP.hint_spans();
    let text: String = spans
        .iter()
        .filter_map(|s| match s {
            jackin_tui::HintSpan::Key(k) | jackin_tui::HintSpan::Text(k) => Some(*k),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join(" ");
    assert!(text.contains("Esc"), "must advertise Esc back: {text}");
    assert!(text.contains("back to list"), "must label back: {text}");
    assert!(text.contains("↑↓"), "must advertise arrow interact: {text}");
    assert!(
        text.contains("J/K"),
        "must advertise J/K move preview: {text}"
    );
    assert!(
        text.contains(glyph::PGUP_PGDN),
        "must advertise page: {text}"
    );
    // BackTab is HiddenAlias — must NOT appear in hint bar separately
    assert!(
        !text.contains("BackTab"),
        "BackTab alias must not appear in hint: {text}"
    );
}
