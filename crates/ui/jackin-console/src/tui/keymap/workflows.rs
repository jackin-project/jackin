// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use termrock::input::KeyCode;

use termrock::keymap::{KeyBinding, KeyChord, Keymap, Visibility};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum WorkflowsAction {
    Navigate,
    Open,
    Observe,
    TakeControl,
    Resume,
    Cancel,
    Export,
    Release,
    Confirm,
    Back,
}

pub(crate) static WORKFLOWS_KEYMAP_BINDINGS: &[KeyBinding<WorkflowsAction>] = &[
    KeyBinding::borrowed(
        &[KeyChord::plain(KeyCode::Up), KeyChord::plain(KeyCode::Down)],
        WorkflowsAction::Navigate,
        Some("select run"),
        Visibility::Shown,
        Some("↑↓"),
    ),
    KeyBinding::borrowed(
        &[KeyChord::plain(KeyCode::Enter)],
        WorkflowsAction::Open,
        Some("open run detail"),
        Visibility::Shown,
        Some("↵"),
    ),
    KeyBinding::borrowed(
        &[KeyChord::plain(KeyCode::Char('o'))],
        WorkflowsAction::Observe,
        Some("observe implementer session"),
        Visibility::Shown,
        Some("O"),
    ),
    KeyBinding::borrowed(
        &[KeyChord::plain(KeyCode::Char('t'))],
        WorkflowsAction::TakeControl,
        Some("take control"),
        Visibility::Shown,
        Some("T"),
    ),
    KeyBinding::borrowed(
        &[KeyChord::plain(KeyCode::Char('r'))],
        WorkflowsAction::Resume,
        Some("resume run"),
        Visibility::Shown,
        Some("R"),
    ),
    KeyBinding::borrowed(
        &[KeyChord::plain(KeyCode::Char('c'))],
        WorkflowsAction::Cancel,
        Some("cancel run"),
        Visibility::Shown,
        Some("C"),
    ),
    KeyBinding::borrowed(
        &[KeyChord::plain(KeyCode::Char('e'))],
        WorkflowsAction::Export,
        Some("export report"),
        Visibility::Shown,
        Some("E"),
    ),
    KeyBinding::borrowed(
        &[KeyChord::plain(KeyCode::Char('s'))],
        WorkflowsAction::Release,
        Some("release control"),
        Visibility::Shown,
        Some("S"),
    ),
    KeyBinding::borrowed(
        &[KeyChord::plain(KeyCode::Char('y'))],
        WorkflowsAction::Confirm,
        Some("confirm dialog"),
        Visibility::Shown,
        Some("Y"),
    ),
    KeyBinding::borrowed(
        &[KeyChord::plain(KeyCode::Esc)],
        WorkflowsAction::Back,
        Some("back"),
        Visibility::Shown,
        Some("Esc"),
    ),
];

pub(crate) static WORKFLOWS_KEYMAP: Keymap<WorkflowsAction> =
    Keymap::from_static(WORKFLOWS_KEYMAP_BINDINGS);
