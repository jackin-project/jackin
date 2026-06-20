use super::{
    EDITOR_CONTENT_KEYMAP, EDITOR_GENERAL_RENAME_KEYMAP, EDITOR_GENERAL_TOGGLE_KEYMAP,
    EDITOR_GENERAL_WORKDIR_KEYMAP, EDITOR_GLOBAL_KEYMAP, EDITOR_ROLE_NEW_KEYMAP,
    EDITOR_TAB_BAR_KEYMAP, INLINE_PICKER_SHELL_KEYMAP, SETTINGS_CONTENT_SHELL_KEYMAP,
    SETTINGS_ENV_TAB_KEYMAP, SETTINGS_GENERAL_TAB_KEYMAP, SETTINGS_GENERAL_TOGGLE_KEYMAP,
    SETTINGS_GLOBAL_MOUNTS_TAB_KEYMAP, SETTINGS_TAB_BAR_KEYMAP, SETTINGS_TRUST_TAB_KEYMAP,
    SETTINGS_TRUST_TOGGLE_KEYMAP, EditorContentAction, EditorGlobalAction, EditorTabBarAction,
    InlinePickerShellAction, SettingsContentShellAction, SettingsEnvTabAction,
    SettingsGeneralTabAction, SettingsGlobalMountsTabAction, SettingsTabBarAction,
    SettingsTrustTabAction,
};
use jackin_tui::components::{KeyChord, LogicalKey};

// ── Editor global ─────────────────────────────────────────────────────────────

#[test]
fn editor_global_save_and_escape() {
    assert_eq!(
        EDITOR_GLOBAL_KEYMAP.dispatch(KeyChord::plain(LogicalKey::Char('s'))),
        Some(EditorGlobalAction::Save)
    );
    assert_eq!(
        EDITOR_GLOBAL_KEYMAP.dispatch(KeyChord::plain(LogicalKey::Char('S'))),
        Some(EditorGlobalAction::Save)
    );
    assert_eq!(
        EDITOR_GLOBAL_KEYMAP.dispatch(KeyChord::plain(LogicalKey::Esc)),
        Some(EditorGlobalAction::Escape)
    );
}

#[test]
fn editor_global_no_nav_keys() {
    assert_eq!(
        EDITOR_GLOBAL_KEYMAP.dispatch(KeyChord::plain(LogicalKey::Up)),
        None
    );
    assert_eq!(
        EDITOR_GLOBAL_KEYMAP.dispatch(KeyChord::plain(LogicalKey::Tab)),
        None
    );
}

// ── Editor tab-bar ────────────────────────────────────────────────────────────

#[test]
fn editor_tab_bar_nav() {
    assert_eq!(
        EDITOR_TAB_BAR_KEYMAP.dispatch(KeyChord::plain(LogicalKey::Left)),
        Some(EditorTabBarAction::PrevTab)
    );
    assert_eq!(
        EDITOR_TAB_BAR_KEYMAP.dispatch(KeyChord::plain(LogicalKey::BackTab)),
        Some(EditorTabBarAction::PrevTab)
    );
    assert_eq!(
        EDITOR_TAB_BAR_KEYMAP.dispatch(KeyChord::plain(LogicalKey::Right)),
        Some(EditorTabBarAction::NextTab)
    );
    assert_eq!(
        EDITOR_TAB_BAR_KEYMAP.dispatch(KeyChord::plain(LogicalKey::Tab)),
        Some(EditorTabBarAction::FocusContent)
    );
    assert_eq!(
        EDITOR_TAB_BAR_KEYMAP.dispatch(KeyChord::plain(LogicalKey::Down)),
        Some(EditorTabBarAction::FocusContent)
    );
}

#[test]
fn editor_tab_bar_vim_aliases() {
    for ch in ['j', 'J'] {
        assert_eq!(
            EDITOR_TAB_BAR_KEYMAP.dispatch(KeyChord::plain(LogicalKey::Char(ch))),
            Some(EditorTabBarAction::FocusContent),
            "'{ch}' must focus content"
        );
    }
}

// ── Editor content ────────────────────────────────────────────────────────────

#[test]
fn editor_content_move_field() {
    assert_eq!(
        EDITOR_CONTENT_KEYMAP.dispatch(KeyChord::plain(LogicalKey::Up)),
        Some(EditorContentAction::MoveUp)
    );
    assert_eq!(
        EDITOR_CONTENT_KEYMAP.dispatch(KeyChord::plain(LogicalKey::Down)),
        Some(EditorContentAction::MoveDown)
    );
}

#[test]
fn editor_content_vim_nav_aliases() {
    for ch in ['k', 'K'] {
        assert_eq!(
            EDITOR_CONTENT_KEYMAP.dispatch(KeyChord::plain(LogicalKey::Char(ch))),
            Some(EditorContentAction::MoveUp),
            "'{ch}' must move up"
        );
    }
    for ch in ['j', 'J'] {
        assert_eq!(
            EDITOR_CONTENT_KEYMAP.dispatch(KeyChord::plain(LogicalKey::Char(ch))),
            Some(EditorContentAction::MoveDown),
            "'{ch}' must move down"
        );
    }
}

#[test]
fn editor_content_vim_scroll_aliases() {
    for ch in ['h', 'H'] {
        assert_eq!(
            EDITOR_CONTENT_KEYMAP.dispatch(KeyChord::plain(LogicalKey::Char(ch))),
            Some(EditorContentAction::ScrollLeft),
            "'{ch}' must scroll left"
        );
    }
    for ch in ['l', 'L'] {
        assert_eq!(
            EDITOR_CONTENT_KEYMAP.dispatch(KeyChord::plain(LogicalKey::Char(ch))),
            Some(EditorContentAction::ScrollRight),
            "'{ch}' must scroll right"
        );
    }
}

#[test]
fn editor_content_header_arrows() {
    assert_eq!(
        EDITOR_CONTENT_KEYMAP.dispatch(KeyChord::plain(LogicalKey::Left)),
        Some(EditorContentAction::CollapseHeader)
    );
    assert_eq!(
        EDITOR_CONTENT_KEYMAP.dispatch(KeyChord::plain(LogicalKey::Right)),
        Some(EditorContentAction::ExpandHeader)
    );
}

#[test]
fn editor_content_tab_and_enter() {
    assert_eq!(
        EDITOR_CONTENT_KEYMAP.dispatch(KeyChord::plain(LogicalKey::Tab)),
        Some(EditorContentAction::NextTab)
    );
    assert_eq!(
        EDITOR_CONTENT_KEYMAP.dispatch(KeyChord::plain(LogicalKey::BackTab)),
        Some(EditorContentAction::FocusTabBar)
    );
    assert_eq!(
        EDITOR_CONTENT_KEYMAP.dispatch(KeyChord::plain(LogicalKey::Enter)),
        Some(EditorContentAction::CheckImmediate)
    );
}

// ── Settings tab-bar ──────────────────────────────────────────────────────────

#[test]
fn settings_tab_bar_nav() {
    assert_eq!(
        SETTINGS_TAB_BAR_KEYMAP.dispatch(KeyChord::plain(LogicalKey::Left)),
        Some(SettingsTabBarAction::PrevTab)
    );
    assert_eq!(
        SETTINGS_TAB_BAR_KEYMAP.dispatch(KeyChord::plain(LogicalKey::Right)),
        Some(SettingsTabBarAction::NextTab)
    );
    assert_eq!(
        SETTINGS_TAB_BAR_KEYMAP.dispatch(KeyChord::plain(LogicalKey::Tab)),
        Some(SettingsTabBarAction::FocusContent)
    );
    assert_eq!(
        SETTINGS_TAB_BAR_KEYMAP.dispatch(KeyChord::plain(LogicalKey::Down)),
        Some(SettingsTabBarAction::FocusContent)
    );
}

#[test]
fn settings_tab_bar_vim_aliases() {
    for ch in ['j', 'J'] {
        assert_eq!(
            SETTINGS_TAB_BAR_KEYMAP.dispatch(KeyChord::plain(LogicalKey::Char(ch))),
            Some(SettingsTabBarAction::FocusContent),
            "'{ch}' must focus content"
        );
    }
}

// ── Settings content shell ────────────────────────────────────────────────────

#[test]
fn settings_content_shell_keys() {
    assert_eq!(
        SETTINGS_CONTENT_SHELL_KEYMAP.dispatch(KeyChord::plain(LogicalKey::Tab)),
        Some(SettingsContentShellAction::NextTab)
    );
    assert_eq!(
        SETTINGS_CONTENT_SHELL_KEYMAP.dispatch(KeyChord::plain(LogicalKey::BackTab)),
        Some(SettingsContentShellAction::FocusTabBar)
    );
    assert_eq!(
        SETTINGS_CONTENT_SHELL_KEYMAP.dispatch(KeyChord::plain(LogicalKey::Esc)),
        Some(SettingsContentShellAction::FocusTabBarOrClearAuth)
    );
}

// ── Settings General tab ──────────────────────────────────────────────────────

#[test]
fn settings_general_tab_nav() {
    assert_eq!(
        SETTINGS_GENERAL_TAB_KEYMAP.dispatch(KeyChord::plain(LogicalKey::Up)),
        Some(SettingsGeneralTabAction::MoveUp)
    );
    assert_eq!(
        SETTINGS_GENERAL_TAB_KEYMAP.dispatch(KeyChord::plain(LogicalKey::Down)),
        Some(SettingsGeneralTabAction::MoveDown)
    );
}

#[test]
fn settings_general_tab_vim_aliases() {
    for ch in ['k', 'K'] {
        assert_eq!(
            SETTINGS_GENERAL_TAB_KEYMAP.dispatch(KeyChord::plain(LogicalKey::Char(ch))),
            Some(SettingsGeneralTabAction::MoveUp),
            "'{ch}' must move up"
        );
    }
    for ch in ['j', 'J'] {
        assert_eq!(
            SETTINGS_GENERAL_TAB_KEYMAP.dispatch(KeyChord::plain(LogicalKey::Char(ch))),
            Some(SettingsGeneralTabAction::MoveDown),
            "'{ch}' must move down"
        );
    }
}

#[test]
fn settings_general_tab_actions() {
    assert_eq!(
        SETTINGS_GENERAL_TAB_KEYMAP.dispatch(KeyChord::plain(LogicalKey::Char(' '))),
        Some(SettingsGeneralTabAction::Toggle)
    );
    assert_eq!(
        SETTINGS_GENERAL_TAB_KEYMAP.dispatch(KeyChord::plain(LogicalKey::Char('s'))),
        Some(SettingsGeneralTabAction::Save)
    );
    assert_eq!(
        SETTINGS_GENERAL_TAB_KEYMAP.dispatch(KeyChord::plain(LogicalKey::Char('S'))),
        Some(SettingsGeneralTabAction::Save)
    );
    assert_eq!(
        SETTINGS_GENERAL_TAB_KEYMAP.dispatch(KeyChord::plain(LogicalKey::Char('q'))),
        Some(SettingsGeneralTabAction::Back)
    );
    assert_eq!(
        SETTINGS_GENERAL_TAB_KEYMAP.dispatch(KeyChord::plain(LogicalKey::Char('Q'))),
        Some(SettingsGeneralTabAction::Back)
    );
    assert_eq!(
        SETTINGS_GENERAL_TAB_KEYMAP.dispatch(KeyChord::plain(LogicalKey::Esc)),
        Some(SettingsGeneralTabAction::Back)
    );
}

// ── Settings Env tab ──────────────────────────────────────────────────────────

#[test]
fn settings_env_tab_nav_and_actions() {
    assert_eq!(
        SETTINGS_ENV_TAB_KEYMAP.dispatch(KeyChord::plain(LogicalKey::Up)),
        Some(SettingsEnvTabAction::MoveUp)
    );
    assert_eq!(
        SETTINGS_ENV_TAB_KEYMAP.dispatch(KeyChord::plain(LogicalKey::Down)),
        Some(SettingsEnvTabAction::MoveDown)
    );
    assert_eq!(
        SETTINGS_ENV_TAB_KEYMAP.dispatch(KeyChord::plain(LogicalKey::Char('a'))),
        Some(SettingsEnvTabAction::Add)
    );
    assert_eq!(
        SETTINGS_ENV_TAB_KEYMAP.dispatch(KeyChord::plain(LogicalKey::Char('s'))),
        Some(SettingsEnvTabAction::Save)
    );
    assert_eq!(
        SETTINGS_ENV_TAB_KEYMAP.dispatch(KeyChord::plain(LogicalKey::Char('d'))),
        Some(SettingsEnvTabAction::Delete)
    );
    assert_eq!(
        SETTINGS_ENV_TAB_KEYMAP.dispatch(KeyChord::plain(LogicalKey::Char('m'))),
        Some(SettingsEnvTabAction::ToggleMask)
    );
    assert_eq!(
        SETTINGS_ENV_TAB_KEYMAP.dispatch(KeyChord::plain(LogicalKey::Char('p'))),
        Some(SettingsEnvTabAction::OpenPicker)
    );
    assert_eq!(
        SETTINGS_ENV_TAB_KEYMAP.dispatch(KeyChord::plain(LogicalKey::Enter)),
        Some(SettingsEnvTabAction::Enter)
    );
    assert_eq!(
        SETTINGS_ENV_TAB_KEYMAP.dispatch(KeyChord::plain(LogicalKey::Char('q'))),
        Some(SettingsEnvTabAction::Back)
    );
}

#[test]
fn settings_env_tab_vim_aliases() {
    for ch in ['k', 'K'] {
        assert_eq!(
            SETTINGS_ENV_TAB_KEYMAP.dispatch(KeyChord::plain(LogicalKey::Char(ch))),
            Some(SettingsEnvTabAction::MoveUp)
        );
    }
    for ch in ['j', 'J'] {
        assert_eq!(
            SETTINGS_ENV_TAB_KEYMAP.dispatch(KeyChord::plain(LogicalKey::Char(ch))),
            Some(SettingsEnvTabAction::MoveDown)
        );
    }
}

// ── Settings Trust tab ────────────────────────────────────────────────────────

#[test]
fn settings_trust_tab_scroll_aliases() {
    for ch in ['h', 'H'] {
        assert_eq!(
            SETTINGS_TRUST_TAB_KEYMAP.dispatch(KeyChord::plain(LogicalKey::Char(ch))),
            Some(SettingsTrustTabAction::ScrollLeft),
            "'{ch}' must scroll left"
        );
    }
    for ch in ['l', 'L'] {
        assert_eq!(
            SETTINGS_TRUST_TAB_KEYMAP.dispatch(KeyChord::plain(LogicalKey::Char(ch))),
            Some(SettingsTrustTabAction::ScrollRight),
            "'{ch}' must scroll right"
        );
    }
}

#[test]
fn settings_trust_tab_actions() {
    assert_eq!(
        SETTINGS_TRUST_TAB_KEYMAP.dispatch(KeyChord::plain(LogicalKey::Char(' '))),
        Some(SettingsTrustTabAction::Toggle)
    );
    assert_eq!(
        SETTINGS_TRUST_TAB_KEYMAP.dispatch(KeyChord::plain(LogicalKey::Char('s'))),
        Some(SettingsTrustTabAction::Save)
    );
    assert_eq!(
        SETTINGS_TRUST_TAB_KEYMAP.dispatch(KeyChord::plain(LogicalKey::Char('q'))),
        Some(SettingsTrustTabAction::Back)
    );
}

// ── Settings Global Mounts tab ────────────────────────────────────────────────

#[test]
fn settings_global_mounts_nav_and_scroll() {
    assert_eq!(
        SETTINGS_GLOBAL_MOUNTS_TAB_KEYMAP.dispatch(KeyChord::plain(LogicalKey::Up)),
        Some(SettingsGlobalMountsTabAction::MoveUp)
    );
    assert_eq!(
        SETTINGS_GLOBAL_MOUNTS_TAB_KEYMAP.dispatch(KeyChord::plain(LogicalKey::Down)),
        Some(SettingsGlobalMountsTabAction::MoveDown)
    );
    for ch in ['h', 'H'] {
        assert_eq!(
            SETTINGS_GLOBAL_MOUNTS_TAB_KEYMAP.dispatch(KeyChord::plain(LogicalKey::Char(ch))),
            Some(SettingsGlobalMountsTabAction::ScrollLeft)
        );
    }
    for ch in ['l', 'L'] {
        assert_eq!(
            SETTINGS_GLOBAL_MOUNTS_TAB_KEYMAP.dispatch(KeyChord::plain(LogicalKey::Char(ch))),
            Some(SettingsGlobalMountsTabAction::ScrollRight)
        );
    }
}

#[test]
fn settings_global_mounts_vim_nav() {
    for ch in ['k', 'K'] {
        assert_eq!(
            SETTINGS_GLOBAL_MOUNTS_TAB_KEYMAP.dispatch(KeyChord::plain(LogicalKey::Char(ch))),
            Some(SettingsGlobalMountsTabAction::MoveUp)
        );
    }
    for ch in ['j', 'J'] {
        assert_eq!(
            SETTINGS_GLOBAL_MOUNTS_TAB_KEYMAP.dispatch(KeyChord::plain(LogicalKey::Char(ch))),
            Some(SettingsGlobalMountsTabAction::MoveDown)
        );
    }
}

#[test]
fn settings_global_mounts_action_keys() {
    use SettingsGlobalMountsTabAction::*;
    let cases: &[(LogicalKey, SettingsGlobalMountsTabAction)] = &[
        (LogicalKey::Char('s'), Save),
        (LogicalKey::Char('S'), Save),
        (LogicalKey::Char('r'), ToggleReadonly),
        (LogicalKey::Char('R'), ToggleReadonly),
        (LogicalKey::Char('a'), Add),
        (LogicalKey::Char('A'), Add),
        (LogicalKey::Char('d'), Delete),
        (LogicalKey::Char('D'), Delete),
        (LogicalKey::Char('o'), OpenGithub),
        (LogicalKey::Char('O'), OpenGithub),
        (LogicalKey::Char('n'), EditRename),
        (LogicalKey::Char('N'), EditRename),
        (LogicalKey::Char('1'), EditSource),
        (LogicalKey::Char('2'), EditDest),
        (LogicalKey::Char('3'), EditScope),
        (LogicalKey::Enter, Enter),
        (LogicalKey::Esc, Back),
        (LogicalKey::Char('q'), Back),
        (LogicalKey::Char('Q'), Back),
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
        INLINE_PICKER_SHELL_KEYMAP.dispatch(KeyChord::plain(LogicalKey::Left)),
        Some(InlinePickerShellAction::ScrollLeft)
    );
    assert_eq!(
        INLINE_PICKER_SHELL_KEYMAP.dispatch(KeyChord::plain(LogicalKey::Right)),
        Some(InlinePickerShellAction::ScrollRight)
    );
}

#[test]
fn inline_picker_shell_vim_scroll_aliases() {
    for ch in ['h', 'H'] {
        assert_eq!(
            INLINE_PICKER_SHELL_KEYMAP.dispatch(KeyChord::plain(LogicalKey::Char(ch))),
            Some(InlinePickerShellAction::ScrollLeft),
            "'{ch}' must scroll left"
        );
    }
    for ch in ['l', 'L'] {
        assert_eq!(
            INLINE_PICKER_SHELL_KEYMAP.dispatch(KeyChord::plain(LogicalKey::Char(ch))),
            Some(InlinePickerShellAction::ScrollRight),
            "'{ch}' must scroll right"
        );
    }
}

#[test]
fn inline_picker_shell_q_not_exit() {
    // q/Q must NOT be captured — they filter in the SelectList, not exit.
    assert_eq!(
        INLINE_PICKER_SHELL_KEYMAP.dispatch(KeyChord::plain(LogicalKey::Char('q'))),
        None
    );
    assert_eq!(
        INLINE_PICKER_SHELL_KEYMAP.dispatch(KeyChord::plain(LogicalKey::Char('Q'))),
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
            jackin_tui::HintSpan::Key(k) | jackin_tui::HintSpan::Text(k) => Some(*k),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join(" ");
    assert!(text.contains("↵"), "rename keymap must advertise ↵: {text}");
    assert!(text.contains("rename"), "rename keymap must say rename: {text}");
}

#[test]
fn editor_general_workdir_hint() {
    let spans = EDITOR_GENERAL_WORKDIR_KEYMAP.hint_spans();
    let text: String = spans
        .iter()
        .filter_map(|s| match s {
            jackin_tui::HintSpan::Key(k) | jackin_tui::HintSpan::Text(k) => Some(*k),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join(" ");
    assert!(text.contains("working directory"), "workdir keymap must say working directory: {text}");
}

#[test]
fn editor_general_toggle_hint() {
    let spans = EDITOR_GENERAL_TOGGLE_KEYMAP.hint_spans();
    let text: String = spans
        .iter()
        .filter_map(|s| match s {
            jackin_tui::HintSpan::Key(k) | jackin_tui::HintSpan::Text(k) => Some(*k),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join(" ");
    assert!(text.contains("toggle"), "toggle keymap must say toggle: {text}");
}

#[test]
fn editor_role_new_hint() {
    let spans = EDITOR_ROLE_NEW_KEYMAP.hint_spans();
    let text: String = spans
        .iter()
        .filter_map(|s| match s {
            jackin_tui::HintSpan::Key(k) | jackin_tui::HintSpan::Text(k) => Some(*k),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join(" ");
    assert!(text.contains("↵/A"), "role new keymap must advertise ↵/A: {text}");
    assert!(text.contains("load role"), "role new keymap must say load role: {text}");
}

#[test]
fn settings_general_toggle_hint() {
    let spans = SETTINGS_GENERAL_TOGGLE_KEYMAP.hint_spans();
    let text: String = spans
        .iter()
        .filter_map(|s| match s {
            jackin_tui::HintSpan::Key(k) | jackin_tui::HintSpan::Text(k) => Some(*k),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join(" ");
    assert!(text.contains("toggle"), "settings general toggle keymap: {text}");
}

#[test]
fn settings_trust_toggle_hint() {
    let spans = SETTINGS_TRUST_TOGGLE_KEYMAP.hint_spans();
    let text: String = spans
        .iter()
        .filter_map(|s| match s {
            jackin_tui::HintSpan::Key(k) | jackin_tui::HintSpan::Text(k) => Some(*k),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join(" ");
    assert!(text.contains("trust"), "trust toggle keymap: {text}");
}
