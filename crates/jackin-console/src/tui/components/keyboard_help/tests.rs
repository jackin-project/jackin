// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;
use crate::tui::components::ConfirmState;
use crate::tui::state::{EditorState, SettingsState};
use termrock::input::{KeyCode, KeyEvent, KeyModifiers};
use termrock::keymap::{KeyChord, Visibility};
use termrock::widgets::{HelpEntrySource, KeyboardHelpOutcome};

fn state() -> ManagerState<'static> {
    let config = jackin_config::AppConfig::default();
    let cwd = std::path::PathBuf::from("/tmp/jackin-test");
    ManagerState::from_config(&config, &cwd)
}

#[test]
fn every_route_produces_keymap_sourced_entries() {
    let system = DesignSystem::default();

    let mut list = state();
    let mut stages: Vec<ManagerStage<'_>> = Vec::new();
    stages.push(ManagerStage::List);
    stages.push(ManagerStage::Editor(EditorState::new_create()));
    stages.push(ManagerStage::Settings(SettingsState::from_config(
        &jackin_config::AppConfig::default(),
    )));
    stages.push(ManagerStage::CreatePrelude(
        crate::tui::state::CreatePreludeState::default(),
    ));
    stages.push(ManagerStage::ConfirmDelete {
        name: "alpha".to_owned(),
        state: ConfirmState::new("Delete workspace?"),
    });
    stages.push(ManagerStage::ConfirmInstancePurge {
        container: "abc".to_owned(),
        label: "ws".to_owned(),
        state: ConfirmState::new("Purge instance?"),
    });
    for stage in stages {
        list.stage = stage;
        let entries = console_help_entries(&list, &system);
        assert!(!entries.is_empty());
        assert!(entries.iter().all(|e| e.source == HelpEntrySource::Keymap));
    }
}

#[test]
fn remap_changes_advertised_chord() {
    // Chord text must come from the live binding, never a hardcoded literal:
    // the same action bound to a different chord yields a different entry.
    let system = DesignSystem::default();
    let static_entries = console_help_entries(&state(), &system);
    let help_entry = static_entries
        .iter()
        .find(|e| e.id == "Global:help")
        .unwrap_or_else(|| panic!("global help entry present"));
    assert_eq!(help_entry.chord, "?");

    use crate::tui::keymap::ConsoleGlobalAction;
    static REMAPPED_BINDINGS: &[KeyBinding<ConsoleGlobalAction>] = &[KeyBinding::borrowed(
        &[KeyChord::plain(KeyCode::Char('h'))],
        ConsoleGlobalAction::OpenKeyboardHelp,
        Some("help"),
        Visibility::Shown,
        None,
    )];
    static REMAPPED: Keymap<ConsoleGlobalAction> = Keymap::from_static(REMAPPED_BINDINGS);
    let entries = entries_from(&REMAPPED, &system, "Global");
    let entry = entries
        .iter()
        .find(|e| e.id == "Global:help")
        .unwrap_or_else(|| panic!("remapped help entry present"));
    assert_eq!(entry.chord, "H");
}

#[test]
fn no_entry_advertises_an_absent_chord() {
    let system = DesignSystem::default();
    let entries = console_help_entries(&state(), &system);
    assert!(entries.iter().all(|e| !e.chord.is_empty()));
}

#[test]
fn help_state_opens_modal_and_closes_on_esc() {
    let system = DesignSystem::default();
    let entries = console_help_entries(&state(), &system);
    let mut help = termrock::widgets::KeyboardHelpState::modal();
    assert!(help.is_open());

    let outcome = help.handle_key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE), &entries);
    assert_eq!(outcome, KeyboardHelpOutcome::Closed);
}
