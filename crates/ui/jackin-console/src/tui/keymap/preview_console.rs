// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! Preview-pane and console-global keymaps.

use termrock::input::KeyCode;

use termrock::keymap::{KeyBinding, KeyChord, Keymap, Visibility};

/// Actions in the workspace-list preview-pane focus mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PreviewPaneAction {
    NavigateUp,
    NavigateDown,
    Attach,
    Back,
}

/// Authoritative keymap for preview-pane focus: drives both
/// `preview_pane_key_plan` dispatch and the `PreviewPane` footer (which is
/// `PREVIEW_PANE_KEYMAP.hint_spans()` verbatim — no context branches).
pub(crate) static PREVIEW_PANE_KEYMAP_BINDINGS: &[KeyBinding<PreviewPaneAction>] = &[
    KeyBinding::borrowed(
        &[KeyChord::plain(KeyCode::Up)],
        PreviewPaneAction::NavigateUp,
        Some("navigate panes"),
        Visibility::Shown,
        Some("↑↓"),
    ),
    KeyBinding::borrowed(
        &[KeyChord::plain(KeyCode::Down)],
        PreviewPaneAction::NavigateDown,
        None,
        Visibility::Internal,
        None,
    ),
    KeyBinding::borrowed(
        &[
            KeyChord::plain(KeyCode::Char('k')),
            KeyChord::plain(KeyCode::Char('K')),
        ],
        PreviewPaneAction::NavigateUp,
        None,
        Visibility::HiddenAlias,
        None,
    ),
    KeyBinding::borrowed(
        &[
            KeyChord::plain(KeyCode::Char('j')),
            KeyChord::plain(KeyCode::Char('J')),
        ],
        PreviewPaneAction::NavigateDown,
        None,
        Visibility::HiddenAlias,
        None,
    ),
    KeyBinding::borrowed(
        &[KeyChord::plain(KeyCode::Enter)],
        PreviewPaneAction::Attach,
        Some("attach focused pane"),
        Visibility::Shown,
        Some("↵"),
    ),
    KeyBinding::borrowed(
        &[
            KeyChord::plain(KeyCode::Esc),
            KeyChord::plain(KeyCode::Left),
        ],
        PreviewPaneAction::Back,
        Some("back"),
        Visibility::Shown,
        Some("Esc/←"),
    ),
    KeyBinding::borrowed(
        &[KeyChord::plain(KeyCode::BackTab)],
        PreviewPaneAction::Back,
        None,
        Visibility::HiddenAlias,
        None,
    ),
];
pub(crate) static PREVIEW_PANE_KEYMAP: Keymap<PreviewPaneAction> =
    Keymap::from_static(PREVIEW_PANE_KEYMAP_BINDINGS);

// ── Console-global (intercepted centrally before per-screen planners) ────────

/// Console-global actions. `?` is intercepted by `should_open_keyboard_help`
/// inside the dispatch `Stage` arm — per-screen planners never see the key
/// (same central-interception pattern as Ctrl+Q, but modal-safe: the consult
/// point guarantees no modal owns input). The binding exists so the
/// keyboard-help overlay and the footer hints derive the `?` glyph from live
/// keymap data.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ConsoleGlobalAction {
    OpenKeyboardHelp,
}

pub(crate) static CONSOLE_GLOBAL_KEYMAP_BINDINGS: &[KeyBinding<ConsoleGlobalAction>] =
    &[KeyBinding::borrowed(
        &[KeyChord::plain(KeyCode::Char('?'))],
        ConsoleGlobalAction::OpenKeyboardHelp,
        Some("help"),
        Visibility::Shown,
        Some("?"),
    )];
pub(crate) static CONSOLE_GLOBAL_KEYMAP: Keymap<ConsoleGlobalAction> =
    Keymap::from_static(CONSOLE_GLOBAL_KEYMAP_BINDINGS);
