// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::{
    EDITOR_CONTENT_KEYMAP, EDITOR_GENERAL_RENAME_KEYMAP, EDITOR_GENERAL_TOGGLE_KEYMAP,
    EDITOR_GENERAL_WORKDIR_KEYMAP, EDITOR_GLOBAL_KEYMAP, EDITOR_ROLE_NEW_KEYMAP,
    EDITOR_TAB_BAR_KEYMAP, EditorContentAction, EditorGlobalAction, EditorTabBarAction,
    INLINE_PICKER_SHELL_KEYMAP, InlinePickerShellAction, PREVIEW_PANE_KEYMAP, PreviewPaneAction,
    SETTINGS_CONTENT_SHELL_KEYMAP, SETTINGS_ENV_TAB_KEYMAP, SETTINGS_GENERAL_TAB_KEYMAP,
    SETTINGS_GENERAL_TOGGLE_KEYMAP, SETTINGS_GLOBAL_MOUNTS_TAB_KEYMAP, SETTINGS_TAB_BAR_KEYMAP,
    SETTINGS_TRUST_TAB_KEYMAP, SETTINGS_TRUST_TOGGLE_KEYMAP, SettingsContentShellAction,
    SettingsEnvTabAction, SettingsGeneralTabAction, SettingsGlobalMountsTabAction,
    SettingsTabBarAction, SettingsTrustTabAction, WORKSPACE_LIST_KEYMAP, WorkspaceListAction,
};
use termrock::input::KeyCode;
use termrock::keymap::KeyChord;

// ── Workspace list ────────────────────────────────────────────────────────────

#[test]
fn workspace_list_keymap_nav_and_vim_aliases() {
    use WorkspaceListAction::*;
    assert_eq!(
        WORKSPACE_LIST_KEYMAP.dispatch(KeyChord::plain(KeyCode::Up)),
        Some(NavigateUp)
    );
    assert_eq!(
        WORKSPACE_LIST_KEYMAP.dispatch(KeyChord::plain(KeyCode::Down)),
        Some(NavigateDown)
    );
    for ch in ['k', 'K'] {
        assert_eq!(
            WORKSPACE_LIST_KEYMAP.dispatch(KeyChord::plain(KeyCode::Char(ch))),
            Some(NavigateUp),
            "vim '{ch}' must move up"
        );
    }
    for ch in ['j', 'J'] {
        assert_eq!(
            WORKSPACE_LIST_KEYMAP.dispatch(KeyChord::plain(KeyCode::Char(ch))),
            Some(NavigateDown),
            "vim '{ch}' must move down"
        );
    }
    for ch in ['h', 'H'] {
        assert_eq!(
            WORKSPACE_LIST_KEYMAP.dispatch(KeyChord::plain(KeyCode::Char(ch))),
            Some(ScrollLeft),
        );
    }
    for ch in ['l', 'L'] {
        assert_eq!(
            WORKSPACE_LIST_KEYMAP.dispatch(KeyChord::plain(KeyCode::Char(ch))),
            Some(ScrollRight),
        );
    }
}

#[test]
fn workspace_list_keymap_action_and_instance_keys() {
    use WorkspaceListAction::*;
    let cases: &[(KeyCode, WorkspaceListAction)] = &[
        (KeyCode::Left, TreeLeft),
        (KeyCode::Right, TreeRight),
        (KeyCode::Enter, Enter),
        (KeyCode::Char('e'), Edit),
        (KeyCode::Char('n'), NewSession),
        (KeyCode::Char('d'), Delete),
        (KeyCode::Char('o'), OpenGithub),
        (KeyCode::Char('s'), Settings),
        (KeyCode::Char('r'), InstanceReconnect),
        (KeyCode::Char('a'), InstanceNewSession),
        (KeyCode::Char('x'), InstanceShell),
        (KeyCode::Char('i'), InstanceInspect),
        (KeyCode::Char('t'), InstanceStop),
        (KeyCode::Char('p'), ConfirmPurge),
        (KeyCode::Tab, EnterPreview),
        (KeyCode::Esc, Exit),
        (KeyCode::Char('q'), Exit),
        (KeyCode::Char('Q'), Exit),
    ];
    for (key, expected) in cases {
        assert_eq!(
            WORKSPACE_LIST_KEYMAP.dispatch(KeyChord::plain(*key)),
            Some(*expected),
            "key {key:?} must map to {expected:?}"
        );
    }
}

#[test]
fn workspace_list_keymap_glyphs_match_footer_literals() {
    // Footer builders pull glyphs from this table; assert the glyphs are the
    // exact strings the footers expect, so dispatch and advertisement agree.
    use WorkspaceListAction::*;
    assert_eq!(WORKSPACE_LIST_KEYMAP.glyph_for(NavigateUp), "↑↓");
    assert_eq!(WORKSPACE_LIST_KEYMAP.glyph_for(Enter), "↵");
    assert_eq!(WORKSPACE_LIST_KEYMAP.glyph_for(Edit), "E");
    assert_eq!(WORKSPACE_LIST_KEYMAP.glyph_for(NewSession), "N");
    assert_eq!(WORKSPACE_LIST_KEYMAP.glyph_for(Delete), "D");
    assert_eq!(WORKSPACE_LIST_KEYMAP.glyph_for(Settings), "S");
    assert_eq!(WORKSPACE_LIST_KEYMAP.glyph_for(OpenGithub), "O");
    assert_eq!(WORKSPACE_LIST_KEYMAP.glyph_for(InstanceShell), "X");
    assert_eq!(WORKSPACE_LIST_KEYMAP.glyph_for(InstanceStop), "T");
    assert_eq!(WORKSPACE_LIST_KEYMAP.glyph_for(InstanceInspect), "I");
    assert_eq!(WORKSPACE_LIST_KEYMAP.glyph_for(ConfirmPurge), "P");
    assert_eq!(WORKSPACE_LIST_KEYMAP.glyph_for(EnterPreview), "⇥");
    assert_eq!(WORKSPACE_LIST_KEYMAP.glyph_for(TreeLeft), "←");
    assert_eq!(WORKSPACE_LIST_KEYMAP.glyph_for(TreeRight), "→");
    assert_eq!(WORKSPACE_LIST_KEYMAP.glyph_for(Quit), "Ctrl-Q");
}

// ── Preview pane ──────────────────────────────────────────────────────────────

#[test]
fn preview_pane_keymap_dispatch_and_aliases() {
    use PreviewPaneAction::*;
    assert_eq!(
        PREVIEW_PANE_KEYMAP.dispatch(KeyChord::plain(KeyCode::Up)),
        Some(NavigateUp)
    );
    assert_eq!(
        PREVIEW_PANE_KEYMAP.dispatch(KeyChord::plain(KeyCode::Down)),
        Some(NavigateDown)
    );
    assert_eq!(
        PREVIEW_PANE_KEYMAP.dispatch(KeyChord::plain(KeyCode::Char('k'))),
        Some(NavigateUp)
    );
    assert_eq!(
        PREVIEW_PANE_KEYMAP.dispatch(KeyChord::plain(KeyCode::Char('j'))),
        Some(NavigateDown)
    );
    assert_eq!(
        PREVIEW_PANE_KEYMAP.dispatch(KeyChord::plain(KeyCode::Enter)),
        Some(Attach)
    );
    assert_eq!(
        PREVIEW_PANE_KEYMAP.dispatch(KeyChord::plain(KeyCode::Esc)),
        Some(Back)
    );
    assert_eq!(
        PREVIEW_PANE_KEYMAP.dispatch(KeyChord::plain(KeyCode::Left)),
        Some(Back)
    );
    assert_eq!(
        PREVIEW_PANE_KEYMAP.dispatch(KeyChord::plain(KeyCode::BackTab)),
        Some(Back)
    );
}

#[test]
fn preview_pane_hint_spans_advertise_shown_keys_only() {
    let text: String = PREVIEW_PANE_KEYMAP
        .hint_spans()
        .iter()
        .filter_map(|s| match s {
            termrock::widgets::HintSpan::Key(k) | termrock::widgets::HintSpan::Text(k) => Some(*k),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join(" ");
    assert!(text.contains("↑↓"), "{text}");
    assert!(text.contains("navigate panes"), "{text}");
    assert!(text.contains("↵"), "{text}");
    assert!(text.contains("Esc/←"), "{text}");
}

// ── Editor global ─────────────────────────────────────────────────────────────

#[test]
fn editor_global_save_and_escape() {
    assert_eq!(
        EDITOR_GLOBAL_KEYMAP.dispatch(KeyChord::plain(KeyCode::Char('s'))),
        Some(EditorGlobalAction::Save)
    );
    assert_eq!(
        EDITOR_GLOBAL_KEYMAP.dispatch(KeyChord::plain(KeyCode::Char('S'))),
        Some(EditorGlobalAction::Save)
    );
    assert_eq!(
        EDITOR_GLOBAL_KEYMAP.dispatch(KeyChord::plain(KeyCode::Esc)),
        Some(EditorGlobalAction::Escape)
    );
}

#[test]
fn editor_global_no_nav_keys() {
    assert_eq!(
        EDITOR_GLOBAL_KEYMAP.dispatch(KeyChord::plain(KeyCode::Up)),
        None
    );
    assert_eq!(
        EDITOR_GLOBAL_KEYMAP.dispatch(KeyChord::plain(KeyCode::Tab)),
        None
    );
}

// ── Editor tab-bar ────────────────────────────────────────────────────────────

#[test]
fn editor_tab_bar_nav() {
    assert_eq!(
        EDITOR_TAB_BAR_KEYMAP.dispatch(KeyChord::plain(KeyCode::Left)),
        Some(EditorTabBarAction::PrevTab)
    );
    assert_eq!(
        EDITOR_TAB_BAR_KEYMAP.dispatch(KeyChord::plain(KeyCode::BackTab)),
        Some(EditorTabBarAction::PrevTab)
    );
    assert_eq!(
        EDITOR_TAB_BAR_KEYMAP.dispatch(KeyChord::plain(KeyCode::Right)),
        Some(EditorTabBarAction::NextTab)
    );
    assert_eq!(
        EDITOR_TAB_BAR_KEYMAP.dispatch(KeyChord::plain(KeyCode::Tab)),
        Some(EditorTabBarAction::FocusContent)
    );
    assert_eq!(
        EDITOR_TAB_BAR_KEYMAP.dispatch(KeyChord::plain(KeyCode::Down)),
        Some(EditorTabBarAction::FocusContent)
    );
}

#[test]
fn editor_tab_bar_vim_aliases() {
    for ch in ['j', 'J'] {
        assert_eq!(
            EDITOR_TAB_BAR_KEYMAP.dispatch(KeyChord::plain(KeyCode::Char(ch))),
            Some(EditorTabBarAction::FocusContent),
            "'{ch}' must focus content"
        );
    }
}

// ── Editor content ────────────────────────────────────────────────────────────

#[test]
fn editor_content_move_field() {
    assert_eq!(
        EDITOR_CONTENT_KEYMAP.dispatch(KeyChord::plain(KeyCode::Up)),
        Some(EditorContentAction::MoveUp)
    );
    assert_eq!(
        EDITOR_CONTENT_KEYMAP.dispatch(KeyChord::plain(KeyCode::Down)),
        Some(EditorContentAction::MoveDown)
    );
}

#[test]
fn editor_content_vim_nav_aliases() {
    for ch in ['k', 'K'] {
        assert_eq!(
            EDITOR_CONTENT_KEYMAP.dispatch(KeyChord::plain(KeyCode::Char(ch))),
            Some(EditorContentAction::MoveUp),
            "'{ch}' must move up"
        );
    }
    for ch in ['j', 'J'] {
        assert_eq!(
            EDITOR_CONTENT_KEYMAP.dispatch(KeyChord::plain(KeyCode::Char(ch))),
            Some(EditorContentAction::MoveDown),
            "'{ch}' must move down"
        );
    }
}

#[test]
fn editor_content_vim_scroll_aliases() {
    for ch in ['h', 'H'] {
        assert_eq!(
            EDITOR_CONTENT_KEYMAP.dispatch(KeyChord::plain(KeyCode::Char(ch))),
            Some(EditorContentAction::ScrollLeft),
            "'{ch}' must scroll left"
        );
    }
    for ch in ['l', 'L'] {
        assert_eq!(
            EDITOR_CONTENT_KEYMAP.dispatch(KeyChord::plain(KeyCode::Char(ch))),
            Some(EditorContentAction::ScrollRight),
            "'{ch}' must scroll right"
        );
    }
}

#[test]
fn editor_content_header_arrows() {
    assert_eq!(
        EDITOR_CONTENT_KEYMAP.dispatch(KeyChord::plain(KeyCode::Left)),
        Some(EditorContentAction::CollapseHeader)
    );
    assert_eq!(
        EDITOR_CONTENT_KEYMAP.dispatch(KeyChord::plain(KeyCode::Right)),
        Some(EditorContentAction::ExpandHeader)
    );
}

#[test]
fn editor_content_tab_and_enter() {
    assert_eq!(
        EDITOR_CONTENT_KEYMAP.dispatch(KeyChord::plain(KeyCode::Tab)),
        Some(EditorContentAction::NextTab)
    );
    assert_eq!(
        EDITOR_CONTENT_KEYMAP.dispatch(KeyChord::plain(KeyCode::BackTab)),
        Some(EditorContentAction::FocusTabBar)
    );
    assert_eq!(
        EDITOR_CONTENT_KEYMAP.dispatch(KeyChord::plain(KeyCode::Enter)),
        Some(EditorContentAction::CheckImmediate)
    );
}

// ── Settings tab-bar ──────────────────────────────────────────────────────────

#[test]
fn settings_tab_bar_nav() {
    assert_eq!(
        SETTINGS_TAB_BAR_KEYMAP.dispatch(KeyChord::plain(KeyCode::Left)),
        Some(SettingsTabBarAction::PrevTab)
    );
    assert_eq!(
        SETTINGS_TAB_BAR_KEYMAP.dispatch(KeyChord::plain(KeyCode::Right)),
        Some(SettingsTabBarAction::NextTab)
    );
    assert_eq!(
        SETTINGS_TAB_BAR_KEYMAP.dispatch(KeyChord::plain(KeyCode::Tab)),
        Some(SettingsTabBarAction::FocusContent)
    );
    assert_eq!(
        SETTINGS_TAB_BAR_KEYMAP.dispatch(KeyChord::plain(KeyCode::Down)),
        Some(SettingsTabBarAction::FocusContent)
    );
}

#[test]
fn settings_tab_bar_vim_aliases() {
    for ch in ['j', 'J'] {
        assert_eq!(
            SETTINGS_TAB_BAR_KEYMAP.dispatch(KeyChord::plain(KeyCode::Char(ch))),
            Some(SettingsTabBarAction::FocusContent),
            "'{ch}' must focus content"
        );
    }
}

// ── Settings content shell ────────────────────────────────────────────────────

#[test]
fn settings_content_shell_keys() {
    assert_eq!(
        SETTINGS_CONTENT_SHELL_KEYMAP.dispatch(KeyChord::plain(KeyCode::Tab)),
        Some(SettingsContentShellAction::NextTab)
    );
    assert_eq!(
        SETTINGS_CONTENT_SHELL_KEYMAP.dispatch(KeyChord::plain(KeyCode::BackTab)),
        Some(SettingsContentShellAction::FocusTabBar)
    );
    assert_eq!(
        SETTINGS_CONTENT_SHELL_KEYMAP.dispatch(KeyChord::plain(KeyCode::Esc)),
        Some(SettingsContentShellAction::FocusTabBarOrClearAuth)
    );
}

// ── Settings General tab ──────────────────────────────────────────────────────

#[test]
fn settings_general_tab_nav() {
    assert_eq!(
        SETTINGS_GENERAL_TAB_KEYMAP.dispatch(KeyChord::plain(KeyCode::Up)),
        Some(SettingsGeneralTabAction::MoveUp)
    );
    assert_eq!(
        SETTINGS_GENERAL_TAB_KEYMAP.dispatch(KeyChord::plain(KeyCode::Down)),
        Some(SettingsGeneralTabAction::MoveDown)
    );
}

#[test]
fn settings_general_tab_vim_aliases() {
    for ch in ['k', 'K'] {
        assert_eq!(
            SETTINGS_GENERAL_TAB_KEYMAP.dispatch(KeyChord::plain(KeyCode::Char(ch))),
            Some(SettingsGeneralTabAction::MoveUp),
            "'{ch}' must move up"
        );
    }
    for ch in ['j', 'J'] {
        assert_eq!(
            SETTINGS_GENERAL_TAB_KEYMAP.dispatch(KeyChord::plain(KeyCode::Char(ch))),
            Some(SettingsGeneralTabAction::MoveDown),
            "'{ch}' must move down"
        );
    }
}

#[test]
fn settings_general_tab_actions() {
    assert_eq!(
        SETTINGS_GENERAL_TAB_KEYMAP.dispatch(KeyChord::plain(KeyCode::Char(' '))),
        Some(SettingsGeneralTabAction::Toggle)
    );
    assert_eq!(
        SETTINGS_GENERAL_TAB_KEYMAP.dispatch(KeyChord::plain(KeyCode::Char('s'))),
        Some(SettingsGeneralTabAction::Save)
    );
    assert_eq!(
        SETTINGS_GENERAL_TAB_KEYMAP.dispatch(KeyChord::plain(KeyCode::Char('S'))),
        Some(SettingsGeneralTabAction::Save)
    );
    assert_eq!(
        SETTINGS_GENERAL_TAB_KEYMAP.dispatch(KeyChord::plain(KeyCode::Char('q'))),
        Some(SettingsGeneralTabAction::Back)
    );
    assert_eq!(
        SETTINGS_GENERAL_TAB_KEYMAP.dispatch(KeyChord::plain(KeyCode::Char('Q'))),
        Some(SettingsGeneralTabAction::Back)
    );
    assert_eq!(
        SETTINGS_GENERAL_TAB_KEYMAP.dispatch(KeyChord::plain(KeyCode::Esc)),
        Some(SettingsGeneralTabAction::Back)
    );
}

// ── Settings Env tab ──────────────────────────────────────────────────────────

#[test]
fn settings_env_tab_nav_and_actions() {
    assert_eq!(
        SETTINGS_ENV_TAB_KEYMAP.dispatch(KeyChord::plain(KeyCode::Up)),
        Some(SettingsEnvTabAction::MoveUp)
    );
    assert_eq!(
        SETTINGS_ENV_TAB_KEYMAP.dispatch(KeyChord::plain(KeyCode::Down)),
        Some(SettingsEnvTabAction::MoveDown)
    );
    assert_eq!(
        SETTINGS_ENV_TAB_KEYMAP.dispatch(KeyChord::plain(KeyCode::Char('a'))),
        Some(SettingsEnvTabAction::Add)
    );
    assert_eq!(
        SETTINGS_ENV_TAB_KEYMAP.dispatch(KeyChord::plain(KeyCode::Char('s'))),
        Some(SettingsEnvTabAction::Save)
    );
    assert_eq!(
        SETTINGS_ENV_TAB_KEYMAP.dispatch(KeyChord::plain(KeyCode::Char('d'))),
        Some(SettingsEnvTabAction::Delete)
    );
    assert_eq!(
        SETTINGS_ENV_TAB_KEYMAP.dispatch(KeyChord::plain(KeyCode::Char('m'))),
        Some(SettingsEnvTabAction::ToggleMask)
    );
    assert_eq!(
        SETTINGS_ENV_TAB_KEYMAP.dispatch(KeyChord::plain(KeyCode::Char('p'))),
        Some(SettingsEnvTabAction::OpenPicker)
    );
    assert_eq!(
        SETTINGS_ENV_TAB_KEYMAP.dispatch(KeyChord::plain(KeyCode::Enter)),
        Some(SettingsEnvTabAction::Enter)
    );
    assert_eq!(
        SETTINGS_ENV_TAB_KEYMAP.dispatch(KeyChord::plain(KeyCode::Char('q'))),
        Some(SettingsEnvTabAction::Back)
    );
}

#[test]
fn settings_env_tab_vim_aliases() {
    for ch in ['k', 'K'] {
        assert_eq!(
            SETTINGS_ENV_TAB_KEYMAP.dispatch(KeyChord::plain(KeyCode::Char(ch))),
            Some(SettingsEnvTabAction::MoveUp)
        );
    }
    for ch in ['j', 'J'] {
        assert_eq!(
            SETTINGS_ENV_TAB_KEYMAP.dispatch(KeyChord::plain(KeyCode::Char(ch))),
            Some(SettingsEnvTabAction::MoveDown)
        );
    }
}

// ── Settings Trust tab ────────────────────────────────────────────────────────

#[test]
fn settings_trust_tab_scroll_aliases() {
    for ch in ['h', 'H'] {
        assert_eq!(
            SETTINGS_TRUST_TAB_KEYMAP.dispatch(KeyChord::plain(KeyCode::Char(ch))),
            Some(SettingsTrustTabAction::ScrollLeft),
            "'{ch}' must scroll left"
        );
    }
    for ch in ['l', 'L'] {
        assert_eq!(
            SETTINGS_TRUST_TAB_KEYMAP.dispatch(KeyChord::plain(KeyCode::Char(ch))),
            Some(SettingsTrustTabAction::ScrollRight),
            "'{ch}' must scroll right"
        );
    }
}

#[test]
fn settings_trust_tab_actions() {
    assert_eq!(
        SETTINGS_TRUST_TAB_KEYMAP.dispatch(KeyChord::plain(KeyCode::Char(' '))),
        Some(SettingsTrustTabAction::Toggle)
    );
    assert_eq!(
        SETTINGS_TRUST_TAB_KEYMAP.dispatch(KeyChord::plain(KeyCode::Char('s'))),
        Some(SettingsTrustTabAction::Save)
    );
    assert_eq!(
        SETTINGS_TRUST_TAB_KEYMAP.dispatch(KeyChord::plain(KeyCode::Char('q'))),
        Some(SettingsTrustTabAction::Back)
    );
}

// ── Settings Global Mounts tab ────────────────────────────────────────────────

#[test]
fn settings_global_mounts_nav_and_scroll() {
    assert_eq!(
        SETTINGS_GLOBAL_MOUNTS_TAB_KEYMAP.dispatch(KeyChord::plain(KeyCode::Up)),
        Some(SettingsGlobalMountsTabAction::MoveUp)
    );
    assert_eq!(
        SETTINGS_GLOBAL_MOUNTS_TAB_KEYMAP.dispatch(KeyChord::plain(KeyCode::Down)),
        Some(SettingsGlobalMountsTabAction::MoveDown)
    );
    for ch in ['h', 'H'] {
        assert_eq!(
            SETTINGS_GLOBAL_MOUNTS_TAB_KEYMAP.dispatch(KeyChord::plain(KeyCode::Char(ch))),
            Some(SettingsGlobalMountsTabAction::ScrollLeft)
        );
    }
    for ch in ['l', 'L'] {
        assert_eq!(
            SETTINGS_GLOBAL_MOUNTS_TAB_KEYMAP.dispatch(KeyChord::plain(KeyCode::Char(ch))),
            Some(SettingsGlobalMountsTabAction::ScrollRight)
        );
    }
}

#[test]
fn settings_global_mounts_vim_nav() {
    for ch in ['k', 'K'] {
        assert_eq!(
            SETTINGS_GLOBAL_MOUNTS_TAB_KEYMAP.dispatch(KeyChord::plain(KeyCode::Char(ch))),
            Some(SettingsGlobalMountsTabAction::MoveUp)
        );
    }
    for ch in ['j', 'J'] {
        assert_eq!(
            SETTINGS_GLOBAL_MOUNTS_TAB_KEYMAP.dispatch(KeyChord::plain(KeyCode::Char(ch))),
            Some(SettingsGlobalMountsTabAction::MoveDown)
        );
    }
}

#[test]
fn settings_global_mounts_action_keys() {
    use SettingsGlobalMountsTabAction::*;
    let cases: &[(KeyCode, SettingsGlobalMountsTabAction)] = &[
        (KeyCode::Char('s'), Save),
        (KeyCode::Char('S'), Save),
        (KeyCode::Char('r'), ToggleReadonly),
        (KeyCode::Char('R'), ToggleReadonly),
        (KeyCode::Char('a'), Add),
        (KeyCode::Char('A'), Add),
        (KeyCode::Char('d'), Delete),
        (KeyCode::Char('D'), Delete),
        (KeyCode::Char('o'), OpenGithub),
        (KeyCode::Char('O'), OpenGithub),
        (KeyCode::Char('n'), EditRename),
        (KeyCode::Char('N'), EditRename),
        (KeyCode::Char('1'), EditSource),
        (KeyCode::Char('2'), EditDest),
        (KeyCode::Char('3'), EditScope),
        (KeyCode::Enter, Enter),
        (KeyCode::Esc, Back),
        (KeyCode::Char('q'), Back),
        (KeyCode::Char('Q'), Back),
    ];
    for (key, expected) in cases {
        assert_eq!(
            SETTINGS_GLOBAL_MOUNTS_TAB_KEYMAP.dispatch(KeyChord::plain(*key)),
            Some(*expected),
            "{key:?} must map to {expected:?}"
        );
    }
}

// ── Inline picker shell ───────────────────────────────────────────────────────

#[test]
fn inline_picker_shell_scroll() {
    assert_eq!(
        INLINE_PICKER_SHELL_KEYMAP.dispatch(KeyChord::plain(KeyCode::Left)),
        Some(InlinePickerShellAction::ScrollLeft)
    );
    assert_eq!(
        INLINE_PICKER_SHELL_KEYMAP.dispatch(KeyChord::plain(KeyCode::Right)),
        Some(InlinePickerShellAction::ScrollRight)
    );
}

#[test]
fn inline_picker_shell_vim_scroll_aliases() {
    for ch in ['h', 'H'] {
        assert_eq!(
            INLINE_PICKER_SHELL_KEYMAP.dispatch(KeyChord::plain(KeyCode::Char(ch))),
            Some(InlinePickerShellAction::ScrollLeft),
            "'{ch}' must scroll left"
        );
    }
    for ch in ['l', 'L'] {
        assert_eq!(
            INLINE_PICKER_SHELL_KEYMAP.dispatch(KeyChord::plain(KeyCode::Char(ch))),
            Some(InlinePickerShellAction::ScrollRight),
            "'{ch}' must scroll right"
        );
    }
}

#[test]
fn inline_picker_shell_q_not_exit() {
    // q/Q must NOT be captured — they filter in the SelectList, not exit.
    assert_eq!(
        INLINE_PICKER_SHELL_KEYMAP.dispatch(KeyChord::plain(KeyCode::Char('q'))),
        None
    );
    assert_eq!(
        INLINE_PICKER_SHELL_KEYMAP.dispatch(KeyChord::plain(KeyCode::Char('Q'))),
        None
    );
}

// ── Row-level hint keymaps ────────────────────────────────────────────────────

#[test]
fn editor_general_rename_hint() {
    let spans = EDITOR_GENERAL_RENAME_KEYMAP.hint_spans();
    let text: String = spans
        .iter()
        .filter_map(|s| match s {
            termrock::widgets::HintSpan::Key(k) | termrock::widgets::HintSpan::Text(k) => Some(*k),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join(" ");
    assert!(text.contains("↵"), "rename keymap must advertise ↵: {text}");
    assert!(
        text.contains("rename"),
        "rename keymap must say rename: {text}"
    );
}

#[test]
fn editor_general_workdir_hint() {
    let spans = EDITOR_GENERAL_WORKDIR_KEYMAP.hint_spans();
    let text: String = spans
        .iter()
        .filter_map(|s| match s {
            termrock::widgets::HintSpan::Key(k) | termrock::widgets::HintSpan::Text(k) => Some(*k),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join(" ");
    assert!(
        text.contains("working directory"),
        "workdir keymap must say working directory: {text}"
    );
}

#[test]
fn editor_general_toggle_hint() {
    let spans = EDITOR_GENERAL_TOGGLE_KEYMAP.hint_spans();
    let text: String = spans
        .iter()
        .filter_map(|s| match s {
            termrock::widgets::HintSpan::Key(k) | termrock::widgets::HintSpan::Text(k) => Some(*k),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join(" ");
    assert!(
        text.contains("toggle"),
        "toggle keymap must say toggle: {text}"
    );
}

#[test]
fn editor_role_new_hint() {
    let spans = EDITOR_ROLE_NEW_KEYMAP.hint_spans();
    let text: String = spans
        .iter()
        .filter_map(|s| match s {
            termrock::widgets::HintSpan::Key(k) | termrock::widgets::HintSpan::Text(k) => Some(*k),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join(" ");
    assert!(
        text.contains("↵/A"),
        "role new keymap must advertise ↵/A: {text}"
    );
    assert!(
        text.contains("load role"),
        "role new keymap must say load role: {text}"
    );
}

#[test]
fn settings_general_toggle_hint() {
    let spans = SETTINGS_GENERAL_TOGGLE_KEYMAP.hint_spans();
    let text: String = spans
        .iter()
        .filter_map(|s| match s {
            termrock::widgets::HintSpan::Key(k) | termrock::widgets::HintSpan::Text(k) => Some(*k),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join(" ");
    assert!(
        text.contains("toggle"),
        "settings general toggle keymap: {text}"
    );
}

#[test]
fn settings_trust_toggle_hint() {
    let spans = SETTINGS_TRUST_TOGGLE_KEYMAP.hint_spans();
    let text: String = spans
        .iter()
        .filter_map(|s| match s {
            termrock::widgets::HintSpan::Key(k) | termrock::widgets::HintSpan::Text(k) => Some(*k),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join(" ");
    assert!(text.contains("trust"), "trust toggle keymap: {text}");
}

// ── Bridged dispatch (keymap_bridge cutover) ───────────────────────────────
//
// Step 4 of plan 011 routes every production dispatch site through
// `dispatch_keymap_action`. These tests pin the equivalence the cutover must
// preserve: bridged dispatch resolves the same action as direct
// `Keymap::dispatch` for every bound chord (and the same miss for unbound
// ones), and `Visibility` keeps driving hint advertisement exactly as before.

use super::bridged_keymap_action;
use termrock::input::{KeyEvent, KeyModifiers};
use termrock::keymap::Keymap as TermrockKeymap;

fn assert_bridged_matches_direct<A>(name: &str, map: &TermrockKeymap<A>, extra_chords: &[KeyChord])
where
    A: Clone + Copy + PartialEq + std::fmt::Debug + 'static,
{
    let chords: Vec<KeyChord> = map
        .bindings()
        .iter()
        .flat_map(|binding| binding.chords().iter().copied())
        .chain(extra_chords.iter().copied())
        .collect();
    for chord in chords {
        let event = KeyEvent::new(chord.key, chord.mods);
        assert_eq!(
            bridged_keymap_action(map, event),
            map.dispatch(chord),
            "{name}: bridged dispatch must match direct dispatch for {chord:?}"
        );
    }
}

#[test]
fn bridged_dispatch_matches_direct_editor_keymaps() {
    let misses = [
        KeyChord::plain(KeyCode::Up),
        KeyChord::ctrl(KeyCode::Char('s')),
        KeyChord::plain(KeyCode::Char('z')),
    ];
    assert_bridged_matches_direct("editor global", &EDITOR_GLOBAL_KEYMAP, &misses);
    assert_bridged_matches_direct("editor tab bar", &EDITOR_TAB_BAR_KEYMAP, &misses);
    assert_bridged_matches_direct("editor content", &EDITOR_CONTENT_KEYMAP, &misses);
}

#[test]
fn bridged_dispatch_matches_direct_settings_keymaps() {
    let misses = [
        KeyChord::plain(KeyCode::Home),
        KeyChord::ctrl(KeyCode::Char('d')),
    ];
    assert_bridged_matches_direct("settings tab bar", &SETTINGS_TAB_BAR_KEYMAP, &misses);
    assert_bridged_matches_direct(
        "settings content shell",
        &SETTINGS_CONTENT_SHELL_KEYMAP,
        &misses,
    );
    assert_bridged_matches_direct("settings general", &SETTINGS_GENERAL_TAB_KEYMAP, &misses);
    assert_bridged_matches_direct("settings env", &SETTINGS_ENV_TAB_KEYMAP, &misses);
    assert_bridged_matches_direct("settings trust", &SETTINGS_TRUST_TAB_KEYMAP, &misses);
    assert_bridged_matches_direct(
        "settings global mounts",
        &SETTINGS_GLOBAL_MOUNTS_TAB_KEYMAP,
        &misses,
    );
}

#[test]
fn bridged_dispatch_matches_direct_list_keymaps() {
    // Ctrl-Q is bound (Quit); a bare modified arrow is not.
    let extras = [
        KeyChord::ctrl(KeyCode::Char('q')),
        KeyChord::ctrl(KeyCode::Up),
        KeyChord::plain(KeyCode::PageDown),
    ];
    assert_bridged_matches_direct("workspace list", &WORKSPACE_LIST_KEYMAP, &extras);
    assert_bridged_matches_direct("preview pane", &PREVIEW_PANE_KEYMAP, &extras);
    assert_bridged_matches_direct("inline picker", &INLINE_PICKER_SHELL_KEYMAP, &extras);
}

#[test]
fn bridged_dispatch_ignores_release_events() {
    let mut release = KeyEvent::new(KeyCode::Char('s'), KeyModifiers::NONE);
    release.kind = termrock::input::KeyEventKind::Release;
    assert_eq!(bridged_keymap_action(&EDITOR_GLOBAL_KEYMAP, release), None);
}

fn hint_keys(spans: Vec<termrock::widgets::HintSpan<'static>>) -> Vec<String> {
    spans
        .iter()
        .filter_map(|span| match span {
            termrock::widgets::HintSpan::Key(key) => Some((*key).to_owned()),
            termrock::widgets::HintSpan::DynKey(key) => Some(key.clone()),
            _ => None,
        })
        .collect()
}

#[test]
fn visibility_shown_bindings_advertise_hidden_aliases_do_not() {
    let keys = hint_keys(WORKSPACE_LIST_KEYMAP.hint_spans());
    // Shown bindings advertise their glyph…
    for glyph in ["↑↓", "↵", "E", "D", "O", "S", "⇥", "←", "→"] {
        assert!(
            keys.iter().any(|key| key == glyph),
            "shown glyph {glyph}: {keys:?}"
        );
    }
    // …HiddenAlias bindings carry dispatch-only chords: never advertised.
    for glyph in ["W", "R", "A", "X", "I", "T", "P"] {
        assert!(
            !keys.iter().any(|key| key == glyph),
            "hidden-alias glyph {glyph} must stay out of hints: {keys:?}"
        );
    }
    // Visibility::Internal (Ctrl-Q quit) is derived contextually via
    // `glyph_for`, never through the uncontextual hint list.
    assert!(
        !keys.iter().any(|key| key == "Ctrl-Q"),
        "internal binding must not self-advertise: {keys:?}"
    );
    assert_eq!(
        WORKSPACE_LIST_KEYMAP.glyph_for(WorkspaceListAction::Quit),
        "Ctrl-Q"
    );
}

#[test]
fn visibility_editor_tab_bar_alias_stays_hidden() {
    let keys = hint_keys(EDITOR_TAB_BAR_KEYMAP.hint_spans());
    assert!(keys.iter().any(|key| key == "⇥/↓"), "{keys:?}");
    // j/J FocusContent alias is HiddenAlias: no standalone glyph appears.
    assert!(!keys.iter().any(|key| key == "J"), "{keys:?}");
}
