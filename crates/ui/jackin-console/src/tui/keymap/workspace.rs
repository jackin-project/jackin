// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! Workspace-list keymap.

use termrock::input::KeyCode;

use termrock::keymap::{KeyBinding, KeyChord, Keymap, Visibility};

/// Actions resolvable from a key on the workspace-list screen.
///
/// The keymap resolves a key to one of these; `workspace_list_key_plan` then
/// folds in runtime context the table cannot carry (list-scroll focus, the
/// selected row's type) to produce the final `WorkspaceListKeyPlan`. Footer
/// builders pull each advertised key's glyph from this same table via
/// [`crate::tui::components::Keymap::glyph_for`], so an advertised key cannot
/// drift from the dispatched key.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum WorkspaceListAction {
    NavigateUp,
    NavigateDown,
    TreeLeft,
    TreeRight,
    ScrollLeft,
    ScrollRight,
    Enter,
    Edit,
    NewSession,
    Delete,
    OpenGithub,
    Settings,
    Prewarm,
    InstanceReconnect,
    InstanceNewSession,
    InstanceShell,
    InstanceInspect,
    InstanceStop,
    ConfirmPurge,
    EnterPreview,
    Exit,
    Quit,
}

/// Authoritative keymap for the workspace list: single source for both
/// `workspace_list_key_plan` dispatch and the workspace-row / instance-row
/// footer glyphs in `components/footer_hints.rs`.
///
/// Hint labels are intentionally absent from most rows here because the same
/// glyph carries different labels per context (`↵` = "launch" on a workspace
/// row, "reconnect" on an instance row; `N` = "new" vs "new session"). Footers
/// supply the contextual label and take only the glyph from this table.
pub(crate) static WORKSPACE_LIST_KEYMAP_BINDINGS: &[KeyBinding<WorkspaceListAction>] = &[
    KeyBinding::borrowed(
        &[KeyChord::plain(KeyCode::Up)],
        WorkspaceListAction::NavigateUp,
        None,
        Visibility::Shown,
        Some("↑↓"),
    ),
    KeyBinding::borrowed(
        &[KeyChord::plain(KeyCode::Down)],
        WorkspaceListAction::NavigateDown,
        None,
        Visibility::Internal,
        None,
    ),
    KeyBinding::borrowed(
        &[
            KeyChord::plain(KeyCode::Char('k')),
            KeyChord::plain(KeyCode::Char('K')),
        ],
        WorkspaceListAction::NavigateUp,
        None,
        Visibility::HiddenAlias,
        None,
    ),
    KeyBinding::borrowed(
        &[
            KeyChord::plain(KeyCode::Char('j')),
            KeyChord::plain(KeyCode::Char('J')),
        ],
        WorkspaceListAction::NavigateDown,
        None,
        Visibility::HiddenAlias,
        None,
    ),
    KeyBinding::borrowed(
        &[KeyChord::plain(KeyCode::Left)],
        WorkspaceListAction::TreeLeft,
        None,
        Visibility::Shown,
        Some("←"),
    ),
    KeyBinding::borrowed(
        &[
            KeyChord::plain(KeyCode::Char('h')),
            KeyChord::plain(KeyCode::Char('H')),
        ],
        WorkspaceListAction::ScrollLeft,
        None,
        Visibility::HiddenAlias,
        None,
    ),
    KeyBinding::borrowed(
        &[KeyChord::plain(KeyCode::Right)],
        WorkspaceListAction::TreeRight,
        None,
        Visibility::Shown,
        Some("→"),
    ),
    KeyBinding::borrowed(
        &[
            KeyChord::plain(KeyCode::Char('l')),
            KeyChord::plain(KeyCode::Char('L')),
        ],
        WorkspaceListAction::ScrollRight,
        None,
        Visibility::HiddenAlias,
        None,
    ),
    KeyBinding::borrowed(
        &[KeyChord::plain(KeyCode::Enter)],
        WorkspaceListAction::Enter,
        None,
        Visibility::Shown,
        Some("↵"),
    ),
    KeyBinding::borrowed(
        &[
            KeyChord::plain(KeyCode::Char('e')),
            KeyChord::plain(KeyCode::Char('E')),
        ],
        WorkspaceListAction::Edit,
        Some("edit"),
        Visibility::Shown,
        Some("E"),
    ),
    KeyBinding::borrowed(
        &[
            KeyChord::plain(KeyCode::Char('n')),
            KeyChord::plain(KeyCode::Char('N')),
        ],
        WorkspaceListAction::NewSession,
        None,
        Visibility::Shown,
        Some("N"),
    ),
    KeyBinding::borrowed(
        &[
            KeyChord::plain(KeyCode::Char('d')),
            KeyChord::plain(KeyCode::Char('D')),
        ],
        WorkspaceListAction::Delete,
        Some("delete"),
        Visibility::Shown,
        Some("D"),
    ),
    KeyBinding::borrowed(
        &[
            KeyChord::plain(KeyCode::Char('w')),
            KeyChord::plain(KeyCode::Char('W')),
        ],
        WorkspaceListAction::Prewarm,
        None,
        Visibility::HiddenAlias,
        Some("W"),
    ),
    KeyBinding::borrowed(
        &[
            KeyChord::plain(KeyCode::Char('o')),
            KeyChord::plain(KeyCode::Char('O')),
        ],
        WorkspaceListAction::OpenGithub,
        Some("open in GitHub"),
        Visibility::Shown,
        Some("O"),
    ),
    KeyBinding::borrowed(
        &[
            KeyChord::plain(KeyCode::Char('s')),
            KeyChord::plain(KeyCode::Char('S')),
        ],
        WorkspaceListAction::Settings,
        Some("settings"),
        Visibility::Shown,
        Some("S"),
    ),
    // Instance-row actions. Advertised contextually (instance-row footer only),
    // so they carry no `hint` here and are HiddenAlias for the base hint bar.
    KeyBinding::borrowed(
        &[
            KeyChord::plain(KeyCode::Char('r')),
            KeyChord::plain(KeyCode::Char('R')),
        ],
        WorkspaceListAction::InstanceReconnect,
        None,
        Visibility::HiddenAlias,
        Some("R"),
    ),
    KeyBinding::borrowed(
        &[
            KeyChord::plain(KeyCode::Char('a')),
            KeyChord::plain(KeyCode::Char('A')),
        ],
        WorkspaceListAction::InstanceNewSession,
        None,
        Visibility::HiddenAlias,
        Some("A"),
    ),
    KeyBinding::borrowed(
        &[
            KeyChord::plain(KeyCode::Char('x')),
            KeyChord::plain(KeyCode::Char('X')),
        ],
        WorkspaceListAction::InstanceShell,
        None,
        Visibility::HiddenAlias,
        Some("X"),
    ),
    KeyBinding::borrowed(
        &[
            KeyChord::plain(KeyCode::Char('i')),
            KeyChord::plain(KeyCode::Char('I')),
        ],
        WorkspaceListAction::InstanceInspect,
        None,
        Visibility::HiddenAlias,
        Some("I"),
    ),
    KeyBinding::borrowed(
        &[
            KeyChord::plain(KeyCode::Char('t')),
            KeyChord::plain(KeyCode::Char('T')),
        ],
        WorkspaceListAction::InstanceStop,
        None,
        Visibility::HiddenAlias,
        Some("T"),
    ),
    KeyBinding::borrowed(
        &[
            KeyChord::plain(KeyCode::Char('p')),
            KeyChord::plain(KeyCode::Char('P')),
        ],
        WorkspaceListAction::ConfirmPurge,
        None,
        Visibility::HiddenAlias,
        Some("P"),
    ),
    KeyBinding::borrowed(
        &[KeyChord::plain(KeyCode::Tab)],
        WorkspaceListAction::EnterPreview,
        Some("into preview"),
        Visibility::Shown,
        Some("⇥"),
    ),
    KeyBinding::borrowed(
        &[
            KeyChord::plain(KeyCode::Esc),
            KeyChord::plain(KeyCode::Char('q')),
            KeyChord::plain(KeyCode::Char('Q')),
        ],
        WorkspaceListAction::Exit,
        None,
        Visibility::Internal,
        None,
    ),
    // Ctrl-Q is intercepted upstream by `should_open_quit_confirm`; it never
    // reaches the list resolver (which dispatches modifier-free chords). The
    // binding exists only so the footer can derive the `Ctrl-Q` glyph.
    KeyBinding::borrowed(
        &[KeyChord::ctrl(KeyCode::Char('q'))],
        WorkspaceListAction::Quit,
        Some("quit"),
        Visibility::Internal,
        Some("Ctrl-Q"),
    ),
];
pub(crate) static WORKSPACE_LIST_KEYMAP: Keymap<WorkspaceListAction> =
    Keymap::from_static(WORKSPACE_LIST_KEYMAP_BINDINGS);

// ── Preview pane (workspace list → preview focus) ─────────────────────────────
