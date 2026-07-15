// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Launch cockpit keymaps — single source of truth for key dispatch and hint advertisement.
//!
//! Each keymap covers one cockpit mode (main cockpit, build log overlay, failure popup,
//! container info). The dispatch in `subscriptions.rs` uses these tables instead of
//! ad-hoc `KeyCode` match arms so that handled keys and advertised keys are coupled.

use jackin_tui::HintSpan;
use termrock::keymap::{KeyBinding, KeyChord, Keymap, LogicalKey, Visibility, glyph};

pub(crate) fn donor_hints(spans: Vec<termrock::HintSpan<'static>>) -> Vec<HintSpan<'static>> {
    spans
        .into_iter()
        .map(|span| match span {
            termrock::HintSpan::Key(value) => HintSpan::Key(value),
            termrock::HintSpan::DynKey(value) => HintSpan::DynKey(value),
            termrock::HintSpan::Text(value) => HintSpan::Text(value),
            termrock::HintSpan::Dyn(value) => HintSpan::Dyn(value),
            termrock::HintSpan::Sep => HintSpan::Sep,
            termrock::HintSpan::GroupSep => HintSpan::GroupSep,
        })
        .collect()
}

// ── Cockpit main ─────────────────────────────────────────────────────────────

/// Top-level cockpit actions (no dialog open).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CockpitAction {
    /// Ctrl+C: hard cancel. Intercepted before dispatch (it must win even while
    /// a dialog is open); the binding exists so its hint derives from this table.
    HardExit,
    /// Open the "Exit jackin❯?" quit confirmation (graceful cancel + cleanup).
    OpenQuitConfirm,
}

/// The two global cockpit keys, advertised on every cockpit surface. Both are
/// `Shown` so [`cockpit_global_hint_spans`] is the single source for the
/// `Ctrl-C abort · Ctrl-Q quit` group every dialog appends.
pub static COCKPIT_KEYMAP: Keymap<CockpitAction> = Keymap::new(&[
    KeyBinding {
        chords: &[KeyChord::ctrl(LogicalKey::Char('c'))],
        action: CockpitAction::HardExit,
        hint: Some("abort"),
        visibility: Visibility::Shown,
        glyph: Some("Ctrl-C"),
    },
    KeyBinding {
        chords: &[KeyChord::ctrl(LogicalKey::Char('q'))],
        action: CockpitAction::OpenQuitConfirm,
        hint: Some("quit"),
        visibility: Visibility::Shown,
        glyph: Some("Ctrl-Q"),
    },
]);

/// The `Ctrl-C abort · Ctrl-Q quit` global-key hint group, derived from
/// [`COCKPIT_KEYMAP`]. Every cockpit surface (main, failure popup, build log,
/// container info) appends this instead of hand-writing the two key spans.
#[must_use]
pub fn cockpit_global_hint_spans() -> Vec<HintSpan<'static>> {
    donor_hints(COCKPIT_KEYMAP.hint_spans())
}

// ── Build log overlay ─────────────────────────────────────────────────────────

/// Keys accepted by the build-log overlay.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BuildLogAction {
    Close,
    ScrollUp,
    ScrollDown,
    PageUp,
    PageDown,
}

pub static BUILD_LOG_KEYMAP: Keymap<BuildLogAction> = Keymap::new(&[
    KeyBinding {
        chords: &[KeyChord::plain(LogicalKey::Esc)],
        action: BuildLogAction::Close,
        hint: Some("close"),
        visibility: Visibility::Shown,
        glyph: Some("Esc"),
    },
    KeyBinding {
        chords: &[KeyChord::plain(LogicalKey::Up)],
        action: BuildLogAction::ScrollUp,
        hint: Some("scroll"),
        visibility: Visibility::Shown,
        glyph: Some("↑↓/j/k"),
    },
    KeyBinding {
        chords: &[KeyChord::plain(LogicalKey::Down)],
        action: BuildLogAction::ScrollDown,
        hint: None,
        visibility: Visibility::HiddenAlias,
        glyph: None,
    },
    KeyBinding {
        chords: &[
            KeyChord::plain(LogicalKey::Char('j')),
            KeyChord::plain(LogicalKey::Char('J')),
        ],
        action: BuildLogAction::ScrollDown,
        hint: None,
        visibility: Visibility::HiddenAlias,
        glyph: None,
    },
    KeyBinding {
        chords: &[
            KeyChord::plain(LogicalKey::Char('k')),
            KeyChord::plain(LogicalKey::Char('K')),
        ],
        action: BuildLogAction::ScrollUp,
        hint: None,
        visibility: Visibility::HiddenAlias,
        glyph: None,
    },
    KeyBinding {
        chords: &[KeyChord::plain(LogicalKey::PageUp)],
        action: BuildLogAction::PageUp,
        hint: Some("page"),
        visibility: Visibility::Shown,
        glyph: Some(glyph::PGUP_PGDN),
    },
    KeyBinding {
        chords: &[KeyChord::plain(LogicalKey::PageDown)],
        action: BuildLogAction::PageDown,
        hint: None,
        visibility: Visibility::HiddenAlias,
        glyph: None,
    },
]);

/// Build the hint spans for the build-log overlay, gating scroll/page on whether
/// the content overflows vertically (matching `build_log_scroll_axes`).
pub fn build_log_hint_spans(vertical: bool) -> Vec<HintSpan<'static>> {
    use termrock::keymap::SCROLL_HINT_KEYMAP as SCROLL_KEYMAP;
    use termrock::scroll::ScrollAxes;
    let mut spans = SCROLL_KEYMAP.hint_spans_for_axes(ScrollAxes {
        vertical,
        horizontal: false,
    });
    if vertical {
        spans.push(termrock::HintSpan::GroupSep);
        BUILD_LOG_KEYMAP.push_spans_for(BuildLogAction::PageUp, &mut spans);
        spans.push(termrock::HintSpan::GroupSep);
    }
    BUILD_LOG_KEYMAP.push_spans_for(BuildLogAction::Close, &mut spans);
    spans.push(termrock::HintSpan::GroupSep);
    spans.extend(COCKPIT_KEYMAP.hint_spans());
    donor_hints(spans)
}

// ── Failure popup ─────────────────────────────────────────────────────────────

/// Keys accepted by the failure popup.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FailureAction {
    Dismiss,
}

pub static FAILURE_KEYMAP: Keymap<FailureAction> = Keymap::new(&[KeyBinding {
    chords: &[
        KeyChord::plain(LogicalKey::Enter),
        KeyChord::plain(LogicalKey::Esc),
    ],
    action: FailureAction::Dismiss,
    hint: Some("dismiss"),
    visibility: Visibility::Shown,
    glyph: Some("↵/Esc"),
}]);

// ── Container info overlay ────────────────────────────────────────────────────

/// Keys accepted by the container info overlay (excluding scroll, which is
/// handled by `handle_key_for_axes`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ContainerInfoAction {
    CopyValue,
    Close,
}

#[cfg(test)]
mod tests;

pub static CONTAINER_INFO_KEYMAP: Keymap<ContainerInfoAction> = Keymap::new(&[
    KeyBinding {
        chords: &[KeyChord::plain(LogicalKey::Enter)],
        action: ContainerInfoAction::CopyValue,
        hint: Some("copy value"),
        visibility: Visibility::Shown,
        glyph: Some("↵"),
    },
    KeyBinding {
        chords: &[KeyChord::plain(LogicalKey::Esc)],
        action: ContainerInfoAction::Close,
        hint: Some("close"),
        visibility: Visibility::Shown,
        glyph: Some("Esc"),
    },
]);
