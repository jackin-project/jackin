// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use termrock::input::KeyCode;
use termrock::keymap::{KeyBinding, KeyChord, Keymap, Visibility, glyph};
use termrock::widgets::HintSpan;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CockpitAction {
    HardExit,

    OpenQuitConfirm,
}

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

#[must_use]
pub fn cockpit_global_hint_spans() -> Vec<HintSpan<'static>> {
    COCKPIT_KEYMAP.hint_spans()
}

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
