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
/// `ConfirmState`/`CONFIRM_KEYMAP`.
/// Test-only: validates that confirm footer spans match what the keymap generates.
#[cfg(test)]
#[must_use]
pub(crate) fn yes_no_hint_spans() -> Vec<HintSpan<'static>> {
    jackin_tui::components::confirm_hint_spans()
}

// ── Workspace list ────────────────────────────────────────────────────────────

/// Actions available in the workspace-list navigation mode.
///
/// Covers the keys handled by `workspace_list_key_plan` and
/// `workspace_list_top_level_key_plan` in `jackin-console`. The `←`/`→` arrows
/// additionally act as tree collapse/expand when a tree node is selected — the
/// keymap represents their primary scroll role; the plan function gates the
/// tree behaviour on context that the keymap cannot carry.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum WorkspaceListAction {
    Navigate,
    Left,
    Right,
    Enter,
    Edit,
    NewSession,
    Delete,
    Settings,
    OpenGithub,
    EnterPreview,
    Quit,
    Exit,
}

/// Authoritative keymap for the workspace list. Drives footer-hint parity
/// tests between the footer builder and the plan-function dispatch in
/// `jackin-console` (which cannot depend on this module).
pub(crate) static WORKSPACE_LIST_KEYMAP: Keymap<WorkspaceListAction> = Keymap::new(&[
    KeyBinding {
        chords: &[KeyChord::plain(LogicalKey::Up), KeyChord::plain(LogicalKey::Down)],
        action: WorkspaceListAction::Navigate,
        hint: Some("navigate"),
        visibility: Visibility::Shown,
        glyph: Some("↑↓"),
    },
    KeyBinding {
        chords: &[
            KeyChord::plain(LogicalKey::Char('j')),
            KeyChord::plain(LogicalKey::Char('J')),
        ],
        action: WorkspaceListAction::Navigate,
        hint: None,
        visibility: Visibility::HiddenAlias,
        glyph: None,
    },
    KeyBinding {
        chords: &[
            KeyChord::plain(LogicalKey::Char('k')),
            KeyChord::plain(LogicalKey::Char('K')),
        ],
        action: WorkspaceListAction::Navigate,
        hint: None,
        visibility: Visibility::HiddenAlias,
        glyph: None,
    },
    KeyBinding {
        chords: &[KeyChord::plain(LogicalKey::Left)],
        action: WorkspaceListAction::Left,
        hint: Some("collapse / scroll"),
        visibility: Visibility::Shown,
        glyph: Some("←"),
    },
    // h/H = pure horizontal scroll; Left = tree-collapse-or-scroll.
    // Both map to the same action in the keymap (registry tracks the alias
    // relationship); exact semantics live in workspace_list_key_plan.
    KeyBinding {
        chords: &[
            KeyChord::plain(LogicalKey::Char('h')),
            KeyChord::plain(LogicalKey::Char('H')),
        ],
        action: WorkspaceListAction::Left,
        hint: None,
        visibility: Visibility::HiddenAlias,
        glyph: None,
    },
    KeyBinding {
        chords: &[KeyChord::plain(LogicalKey::Right)],
        action: WorkspaceListAction::Right,
        hint: Some("expand / scroll"),
        visibility: Visibility::Shown,
        glyph: Some("→"),
    },
    KeyBinding {
        chords: &[
            KeyChord::plain(LogicalKey::Char('l')),
            KeyChord::plain(LogicalKey::Char('L')),
        ],
        action: WorkspaceListAction::Right,
        hint: None,
        visibility: Visibility::HiddenAlias,
        glyph: None,
    },
    KeyBinding {
        chords: &[KeyChord::plain(LogicalKey::Enter)],
        action: WorkspaceListAction::Enter,
        hint: Some("launch"),
        visibility: Visibility::Shown,
        glyph: Some("↵"),
    },
    KeyBinding {
        chords: &[
            KeyChord::plain(LogicalKey::Char('e')),
            KeyChord::plain(LogicalKey::Char('E')),
        ],
        action: WorkspaceListAction::Edit,
        hint: Some("edit"),
        visibility: Visibility::Shown,
        glyph: Some("E"),
    },
    KeyBinding {
        chords: &[
            KeyChord::plain(LogicalKey::Char('n')),
            KeyChord::plain(LogicalKey::Char('N')),
        ],
        action: WorkspaceListAction::NewSession,
        hint: Some("new"),
        visibility: Visibility::Shown,
        glyph: Some("N"),
    },
    KeyBinding {
        chords: &[
            KeyChord::plain(LogicalKey::Char('d')),
            KeyChord::plain(LogicalKey::Char('D')),
        ],
        action: WorkspaceListAction::Delete,
        hint: Some("delete"),
        visibility: Visibility::Shown,
        glyph: Some("D"),
    },
    KeyBinding {
        chords: &[
            KeyChord::plain(LogicalKey::Char('s')),
            KeyChord::plain(LogicalKey::Char('S')),
        ],
        action: WorkspaceListAction::Settings,
        hint: Some("settings"),
        visibility: Visibility::Shown,
        glyph: Some("S"),
    },
    KeyBinding {
        chords: &[
            KeyChord::plain(LogicalKey::Char('o')),
            KeyChord::plain(LogicalKey::Char('O')),
        ],
        action: WorkspaceListAction::OpenGithub,
        hint: Some("open in GitHub"),
        visibility: Visibility::Shown,
        glyph: Some("O"),
    },
    // Tab enters preview-pane focus when the selected row has a snapshot.
    // Right arrow also triggers EnterPreview via workspace_list_top_level_key_plan
    // before reaching workspace_list_key_plan, so it appears above under Right.
    KeyBinding {
        chords: &[KeyChord::plain(LogicalKey::Tab)],
        action: WorkspaceListAction::EnterPreview,
        hint: Some("into preview"),
        visibility: Visibility::Shown,
        glyph: Some("⇥"),
    },
    // Ctrl-Q handled by should_open_quit_confirm upstream.
    KeyBinding {
        chords: &[KeyChord::ctrl(LogicalKey::Char('q'))],
        action: WorkspaceListAction::Quit,
        hint: None,
        visibility: Visibility::Internal,
        glyph: None,
    },
    // Esc/q/Q on the main list exits silently (no confirm dialog).
    KeyBinding {
        chords: &[
            KeyChord::plain(LogicalKey::Esc),
            KeyChord::plain(LogicalKey::Char('q')),
            KeyChord::plain(LogicalKey::Char('Q')),
        ],
        action: WorkspaceListAction::Exit,
        hint: None,
        visibility: Visibility::Internal,
        glyph: None,
    },
]);

// ── Editor top-level (content-focused mode) ───────────────────────────────────

/// Actions available at the editor top level when content (not the tab bar)
/// has focus. Covers keys handled by `editor_top_level_key_plan` in
/// `jackin-console`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum EditorTopLevelAction {
    MoveField,
    ScrollLeft,
    ScrollRight,
    NextTab,
    FocusTabBar,
    Save,
    Escape,
    Quit,
}

/// Authoritative keymap for the editor surface (content-focused mode). Drives
/// footer-hint parity tests. Production dispatch uses `editor_top_level_key_plan`
/// in `jackin-console` which cannot depend on this module.
pub(crate) static EDITOR_TOP_LEVEL_KEYMAP: Keymap<EditorTopLevelAction> = Keymap::new(&[
    KeyBinding {
        chords: &[KeyChord::plain(LogicalKey::Up), KeyChord::plain(LogicalKey::Down)],
        action: EditorTopLevelAction::MoveField,
        hint: Some("move field"),
        visibility: Visibility::Shown,
        glyph: Some("↑↓"),
    },
    KeyBinding {
        chords: &[
            KeyChord::plain(LogicalKey::Char('k')),
            KeyChord::plain(LogicalKey::Char('K')),
        ],
        action: EditorTopLevelAction::MoveField,
        hint: None,
        visibility: Visibility::HiddenAlias,
        glyph: None,
    },
    KeyBinding {
        chords: &[
            KeyChord::plain(LogicalKey::Char('j')),
            KeyChord::plain(LogicalKey::Char('J')),
        ],
        action: EditorTopLevelAction::MoveField,
        hint: None,
        visibility: Visibility::HiddenAlias,
        glyph: None,
    },
    KeyBinding {
        chords: &[
            KeyChord::plain(LogicalKey::Char('h')),
            KeyChord::plain(LogicalKey::Char('H')),
        ],
        action: EditorTopLevelAction::ScrollLeft,
        hint: None,
        visibility: Visibility::HiddenAlias,
        glyph: None,
    },
    KeyBinding {
        chords: &[
            KeyChord::plain(LogicalKey::Char('l')),
            KeyChord::plain(LogicalKey::Char('L')),
        ],
        action: EditorTopLevelAction::ScrollRight,
        hint: None,
        visibility: Visibility::HiddenAlias,
        glyph: None,
    },
    KeyBinding {
        chords: &[KeyChord::plain(LogicalKey::Tab)],
        action: EditorTopLevelAction::NextTab,
        hint: Some("next tab"),
        visibility: Visibility::Shown,
        glyph: Some("⇥"),
    },
    KeyBinding {
        chords: &[KeyChord::plain(LogicalKey::BackTab)],
        action: EditorTopLevelAction::FocusTabBar,
        hint: Some("tab bar"),
        visibility: Visibility::Shown,
        glyph: Some("⇤"),
    },
    KeyBinding {
        chords: &[
            KeyChord::plain(LogicalKey::Char('s')),
            KeyChord::plain(LogicalKey::Char('S')),
        ],
        action: EditorTopLevelAction::Save,
        hint: Some("save"),
        visibility: Visibility::Shown,
        glyph: Some("S"),
    },
    KeyBinding {
        chords: &[KeyChord::plain(LogicalKey::Esc)],
        action: EditorTopLevelAction::Escape,
        hint: Some("back / discard"),
        visibility: Visibility::Shown,
        glyph: Some("Esc"),
    },
    KeyBinding {
        chords: &[KeyChord::ctrl(LogicalKey::Char('q'))],
        action: EditorTopLevelAction::Quit,
        hint: None,
        visibility: Visibility::Internal,
        glyph: None,
    },
]);

#[cfg(test)]
mod tests;
