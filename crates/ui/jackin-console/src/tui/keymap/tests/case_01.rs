// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

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
