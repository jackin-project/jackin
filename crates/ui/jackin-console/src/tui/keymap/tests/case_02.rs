// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

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
