// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Launch cockpit keymaps — single source of truth for key dispatch and hint advertisement.
//!
//! Each keymap covers one cockpit mode (main cockpit, build log overlay, failure popup,
//! container info). The dispatch in `subscriptions.rs` uses these tables instead of
//! ad-hoc `KeyCode` match arms so that handled keys and advertised keys are coupled.

use termrock::input::KeyCode;
use termrock::keymap::{KeyBinding, KeyChord, Keymap, Visibility, glyph};
use termrock::widgets::HintSpan;

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
pub static COCKPIT_KEYMAP_BINDINGS: &[KeyBinding<CockpitAction>] = &[
    KeyBinding::borrowed(
        &[KeyChord::ctrl(KeyCode::Char('c'))],
        CockpitAction::HardExit,
        Some("abort"),
        Visibility::Shown,
        Some("Ctrl-C"),
    ),
    KeyBinding::borrowed(
        &[KeyChord::ctrl(KeyCode::Char('q'))],
        CockpitAction::OpenQuitConfirm,
        Some("quit"),
        Visibility::Shown,
        Some("Ctrl-Q"),
    ),
];
pub static COCKPIT_KEYMAP: Keymap<CockpitAction> = Keymap::from_static(COCKPIT_KEYMAP_BINDINGS);

/// The `Ctrl-C abort · Ctrl-Q quit` global-key hint group, derived from
/// [`COCKPIT_KEYMAP`]. Every cockpit surface (main, failure popup, build log,
/// container info) appends this instead of hand-writing the two key spans.
#[must_use]
pub fn cockpit_global_hint_spans() -> Vec<HintSpan<'static>> {
    COCKPIT_KEYMAP.hint_spans()
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

pub static BUILD_LOG_KEYMAP_BINDINGS: &[KeyBinding<BuildLogAction>] = &[
    KeyBinding::borrowed(
        &[KeyChord::plain(KeyCode::Esc)],
        BuildLogAction::Close,
        Some("close"),
        Visibility::Shown,
        Some("Esc"),
    ),
    KeyBinding::borrowed(
        &[KeyChord::plain(KeyCode::Up)],
        BuildLogAction::ScrollUp,
        Some("scroll"),
        Visibility::Shown,
        Some("↑↓/j/k"),
    ),
    KeyBinding::borrowed(
        &[KeyChord::plain(KeyCode::Down)],
        BuildLogAction::ScrollDown,
        None,
        Visibility::HiddenAlias,
        None,
    ),
    KeyBinding::borrowed(
        &[
            KeyChord::plain(KeyCode::Char('j')),
            KeyChord::plain(KeyCode::Char('J')),
        ],
        BuildLogAction::ScrollDown,
        None,
        Visibility::HiddenAlias,
        None,
    ),
    KeyBinding::borrowed(
        &[
            KeyChord::plain(KeyCode::Char('k')),
            KeyChord::plain(KeyCode::Char('K')),
        ],
        BuildLogAction::ScrollUp,
        None,
        Visibility::HiddenAlias,
        None,
    ),
    KeyBinding::borrowed(
        &[KeyChord::plain(KeyCode::PageUp)],
        BuildLogAction::PageUp,
        Some("page"),
        Visibility::Shown,
        Some(glyph::PGUP_PGDN),
    ),
    KeyBinding::borrowed(
        &[KeyChord::plain(KeyCode::PageDown)],
        BuildLogAction::PageDown,
        None,
        Visibility::HiddenAlias,
        None,
    ),
];
pub static BUILD_LOG_KEYMAP: Keymap<BuildLogAction> =
    Keymap::from_static(BUILD_LOG_KEYMAP_BINDINGS);

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
        spans.push(HintSpan::GroupSep);
        BUILD_LOG_KEYMAP.push_spans_for(BuildLogAction::PageUp, &mut spans);
        spans.push(HintSpan::GroupSep);
    }
    BUILD_LOG_KEYMAP.push_spans_for(BuildLogAction::Close, &mut spans);
    spans.push(HintSpan::GroupSep);
    spans.extend(COCKPIT_KEYMAP.hint_spans());
    spans
}

// ── Failure popup ─────────────────────────────────────────────────────────────

/// Keys accepted by the failure popup.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FailureAction {
    Dismiss,
}

pub static FAILURE_KEYMAP_BINDINGS: &[KeyBinding<FailureAction>] = &[KeyBinding::borrowed(
    &[
        KeyChord::plain(KeyCode::Enter),
        KeyChord::plain(KeyCode::Esc),
    ],
    FailureAction::Dismiss,
    Some("dismiss"),
    Visibility::Shown,
    Some("↵/Esc"),
)];
pub static FAILURE_KEYMAP: Keymap<FailureAction> = Keymap::from_static(FAILURE_KEYMAP_BINDINGS);

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

pub static CONTAINER_INFO_KEYMAP_BINDINGS: &[KeyBinding<ContainerInfoAction>] = &[
    KeyBinding::borrowed(
        &[KeyChord::plain(KeyCode::Enter)],
        ContainerInfoAction::CopyValue,
        Some("copy value"),
        Visibility::Shown,
        Some("↵"),
    ),
    KeyBinding::borrowed(
        &[KeyChord::plain(KeyCode::Esc)],
        ContainerInfoAction::Close,
        Some("close"),
        Visibility::Shown,
        Some("Esc"),
    ),
];
pub static CONTAINER_INFO_KEYMAP: Keymap<ContainerInfoAction> =
    Keymap::from_static(CONTAINER_INFO_KEYMAP_BINDINGS);
