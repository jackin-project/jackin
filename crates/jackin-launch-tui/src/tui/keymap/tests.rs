// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin_tui::components::{KeyChord, LogicalKey};
use jackin_tui::keymap::glyph;

use super::{
    BUILD_LOG_KEYMAP, BuildLogAction, COCKPIT_KEYMAP, CONTAINER_INFO_KEYMAP, CockpitAction,
    ContainerInfoAction, FAILURE_KEYMAP, FailureAction,
};

fn assert_shown_glyphs_are_normalized<A: Copy + 'static>(
    keymap: &jackin_tui::components::Keymap<A>,
) {
    for span in keymap.hint_spans() {
        let jackin_tui::HintSpan::Key(key) = span else {
            continue;
        };
        assert_ne!(key, concat!("T", "ab"));
        assert!(!key.contains("\u{2191}/"));
        assert!(!key.contains("\u{2190}/"));
        assert!(!key.contains(concat!("PgUp", " PgDn")));
        assert!(!key.contains(concat!("Alt", "+")));
        assert!(!key.contains(concat!("Shift", "+")));
        assert!(!key.contains(concat!("Ctrl", "+")));
    }
}

#[test]
fn shown_keymap_glyphs_use_canonical_spellings() {
    assert_shown_glyphs_are_normalized(&COCKPIT_KEYMAP);
    assert_shown_glyphs_are_normalized(&BUILD_LOG_KEYMAP);
    assert_shown_glyphs_are_normalized(&FAILURE_KEYMAP);
    assert_shown_glyphs_are_normalized(&CONTAINER_INFO_KEYMAP);
}

// ── COCKPIT ──────────────────────────────────────────────────────────────────

#[test]
fn cockpit_global_keys_dispatch() {
    assert_eq!(
        COCKPIT_KEYMAP.dispatch(KeyChord::ctrl(LogicalKey::Char('q'))),
        Some(CockpitAction::OpenQuitConfirm)
    );
    // Ctrl+C is intercepted before dispatch, but is registered so its hint
    // derives from the same table as Ctrl+Q.
    assert_eq!(
        COCKPIT_KEYMAP.dispatch(KeyChord::ctrl(LogicalKey::Char('c'))),
        Some(CockpitAction::HardExit)
    );
}

#[test]
fn cockpit_global_hint_spans_advertise_both_keys() {
    let text: String = super::cockpit_global_hint_spans()
        .iter()
        .filter_map(|s| match s {
            jackin_tui::HintSpan::Key(k) | jackin_tui::HintSpan::Text(k) => Some(*k),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join(" ");
    assert_eq!(text, "Ctrl-C abort Ctrl-Q quit", "{text}");
}

#[test]
fn cockpit_non_registered_keys_return_none() {
    for chord in [
        KeyChord::plain(LogicalKey::Char('q')),
        KeyChord::plain(LogicalKey::Esc),
        KeyChord::plain(LogicalKey::Enter),
    ] {
        assert_eq!(
            COCKPIT_KEYMAP.dispatch(chord),
            None,
            "cockpit must not dispatch {chord:?}"
        );
    }
}

// ── BUILD LOG ─────────────────────────────────────────────────────────────────

#[test]
fn build_log_dispatches_all_advertised_keys() {
    use BuildLogAction::*;
    let cases = [
        (KeyChord::plain(LogicalKey::Esc), Close),
        (KeyChord::plain(LogicalKey::Up), ScrollUp),
        (KeyChord::plain(LogicalKey::Down), ScrollDown),
        (KeyChord::plain(LogicalKey::Char('j')), ScrollDown),
        (KeyChord::plain(LogicalKey::Char('J')), ScrollDown),
        (KeyChord::plain(LogicalKey::Char('k')), ScrollUp),
        (KeyChord::plain(LogicalKey::Char('K')), ScrollUp),
        (KeyChord::plain(LogicalKey::PageUp), PageUp),
        (KeyChord::plain(LogicalKey::PageDown), PageDown),
    ];
    for (chord, expected) in cases {
        assert_eq!(
            BUILD_LOG_KEYMAP.dispatch(chord),
            Some(expected),
            "build_log must dispatch {chord:?}"
        );
    }
}

#[test]
fn build_log_non_registered_keys_return_none() {
    for chord in [
        KeyChord::plain(LogicalKey::Enter),
        KeyChord::plain(LogicalKey::Tab),
        KeyChord::plain(LogicalKey::Char('q')),
        KeyChord::ctrl(LogicalKey::Char('q')),
    ] {
        assert_eq!(
            BUILD_LOG_KEYMAP.dispatch(chord),
            None,
            "build_log must not dispatch {chord:?}"
        );
    }
}

#[test]
fn build_log_hints_advertise_esc_and_scroll() {
    let spans = super::build_log_hint_spans(true);
    let text: String = spans
        .iter()
        .filter_map(|s| match s {
            jackin_tui::HintSpan::Key(k) | jackin_tui::HintSpan::Text(k) => Some(*k),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join(" ");
    assert!(text.contains("Esc"), "must advertise Esc close: {text}");
    assert!(text.contains("↑↓"), "must advertise scroll: {text}");
    assert!(
        text.contains(glyph::PGUP_PGDN),
        "must advertise page: {text}"
    );
}

// ── FAILURE ───────────────────────────────────────────────────────────────────

#[test]
fn failure_dispatches_enter_and_esc() {
    assert_eq!(
        FAILURE_KEYMAP.dispatch(KeyChord::plain(LogicalKey::Enter)),
        Some(FailureAction::Dismiss)
    );
    assert_eq!(
        FAILURE_KEYMAP.dispatch(KeyChord::plain(LogicalKey::Esc)),
        Some(FailureAction::Dismiss)
    );
}

#[test]
fn failure_non_registered_keys_return_none() {
    for chord in [
        KeyChord::plain(LogicalKey::Char('y')),
        KeyChord::plain(LogicalKey::Char('q')),
        KeyChord::plain(LogicalKey::Tab),
        KeyChord::ctrl(LogicalKey::Char('q')),
    ] {
        assert_eq!(
            FAILURE_KEYMAP.dispatch(chord),
            None,
            "failure must not dispatch {chord:?}"
        );
    }
}

#[test]
fn failure_hints_advertise_dismiss() {
    let spans = FAILURE_KEYMAP.hint_spans();
    let text: String = spans
        .iter()
        .filter_map(|s| match s {
            jackin_tui::HintSpan::Key(k) | jackin_tui::HintSpan::Text(k) => Some(*k),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join(" ");
    assert!(text.contains("dismiss"), "must advertise dismiss: {text}");
    assert!(
        text.contains("↵") || text.contains("Esc"),
        "must show key: {text}"
    );
}

// ── CONTAINER INFO ────────────────────────────────────────────────────────────

#[test]
fn container_info_dispatches_enter_copy_and_esc_close() {
    assert_eq!(
        CONTAINER_INFO_KEYMAP.dispatch(KeyChord::plain(LogicalKey::Enter)),
        Some(ContainerInfoAction::CopyValue)
    );
    assert_eq!(
        CONTAINER_INFO_KEYMAP.dispatch(KeyChord::plain(LogicalKey::Esc)),
        Some(ContainerInfoAction::Close)
    );
}

#[test]
fn container_info_non_registered_keys_return_none() {
    for chord in [
        KeyChord::plain(LogicalKey::Tab),
        KeyChord::plain(LogicalKey::Char('q')),
        KeyChord::plain(LogicalKey::Up),
        KeyChord::ctrl(LogicalKey::Char('q')),
    ] {
        assert_eq!(
            CONTAINER_INFO_KEYMAP.dispatch(chord),
            None,
            "container_info must not dispatch {chord:?}"
        );
    }
}

#[test]
fn container_info_hints_advertise_copy_and_close() {
    let spans = CONTAINER_INFO_KEYMAP.hint_spans();
    let text: String = spans
        .iter()
        .filter_map(|s| match s {
            jackin_tui::HintSpan::Key(k) | jackin_tui::HintSpan::Text(k) => Some(*k),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join(" ");
    assert!(text.contains("copy value"), "must advertise copy: {text}");
    assert!(text.contains("close"), "must advertise close: {text}");
}
