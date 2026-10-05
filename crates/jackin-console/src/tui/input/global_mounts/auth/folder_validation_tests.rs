// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Renderable Settings fixture and interaction oracle for invalid folder recovery.
//! The worker's explicit completion is injected; no host credentials or filesystem
//! listing are read. Apple credential validation and interactive terminal execution
//! remain separate execution gates.

use super::*;
use crate::tui::components::file_browser::{
    FileBrowserOutcome, FileBrowserState, FolderEntry, FolderListing,
};
use crate::tui::effect::FileBrowserEffectContext;
use crate::tui::file_browser::{FileBrowserCommitResult, apply_file_browser_commit_result};
use crate::tui::state::{ManagerEffect, SettingsState, SettingsTab};
use jackin_config::{AccountConfig, AccountCredential, AiProvider, AppConfig};
use jackin_core::{Agent, JackinPaths};
use ratatui::{Terminal, backend::TestBackend};
use std::path::PathBuf;

const ROOT: &str = "/fixture/accounts";
const REJECTION: &str = "Selected folder has no Codex credentials";
const SECRET: &str = "synthetic-unsaved-credential";

fn fixture() -> (ManagerState<'static>, AppConfig) {
    let mut config = AppConfig::default();
    config.accounts.insert(
        "work".into(),
        AccountConfig {
            enabled: true,
            name: "Work account".into(),
            provider: AiProvider::OpenAi,
            credential: AccountCredential::Profile {
                agent: Agent::Codex,
                directory: PathBuf::from("/fixture/original-codex"),
                xdg_roots: None,
                source_selector: None,
            },
        },
    );
    let mut state = ManagerState::from_config(&config, std::path::Path::new(ROOT));
    let mut settings = SettingsState::from_config(&config);
    settings.active_tab = SettingsTab::Auth;
    settings.set_focus_owner(crate::tui::focus::ConsoleFocusTarget::Content(SettingsTab::Auth));
    settings.auth.selected = 0;
    open_settings_auth_form(&mut settings.auth, &settings.env);
    let Some(SettingsModal::AuthForm {
        state: form,
        focus,
        literal_buffer,
        ..
    }) = settings.auth.modal_mut()
    else {
        panic!("fixture account form");
    };
    form.set_literal(SECRET.into());
    *focus = AuthFormFocus::SourceFolder;
    *literal_buffer = SECRET.into();
    let mut browser = FileBrowserState::from_listing(FolderListing {
        root: ROOT.into(),
        cwd: ROOT.into(),
        entries: (0..32)
            .map(|index| FolderEntry {
                name: format!("profile-{index:02}"),
                path: PathBuf::from(ROOT).join(format!("profile-{index:02}")),
                is_parent: false,
                is_git: false,
            })
            .collect(),
    });
    browser.show_hidden = true;
    browser.list_state.select(Some(20));
    settings.auth.push_auth_modal(SettingsModal::AuthSourceFolderPicker { state: browser });
    state.stage = ManagerStage::Settings(settings);
    (state, config)
}

fn settings<'state, 'config>(state: &'state ManagerState<'config>) -> &'state SettingsState<'config> {
    let ManagerStage::Settings(settings) = &state.stage else {
        panic!("fixture remains in Settings");
    };
    settings
}

fn key(state: &mut ManagerState<'_>, config: &mut AppConfig, code: KeyCode) {
    crate::tui::input::dispatch::handle_key(
        state,
        config,
        &JackinPaths::for_tests(std::path::Path::new(ROOT)),
        std::path::Path::new(ROOT),
        KeyEvent::new(code, crossterm::event::KeyModifiers::NONE),
        &|_, _| Ok(()),
    )
    .unwrap();
}

fn render(state: &mut ManagerState<'_>, config: &AppConfig) -> String {
    let area = ratatui::layout::Rect::new(0, 0, 100, 30);
    crate::tui::view::prepare_for_render(state, config, std::path::Path::new(ROOT), area);
    let mut terminal = Terminal::new(TestBackend::new(area.width, area.height)).unwrap();
    terminal
        .draw(|frame| {
            crate::tui::view::render(frame, area, state, config, std::path::Path::new(ROOT));
        })
        .unwrap();
    let buffer = terminal.backend().buffer();
    (0..area.height)
        .map(|y| {
            (0..area.width)
                .map(|x| buffer[(x, y)].symbol())
                .collect::<String>()
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn picker_snapshot(state: &ManagerState<'_>) -> Result<String, &'static str> {
    let Some(SettingsModal::AuthSourceFolderPicker { state: browser }) =
        settings(state).auth.modal_ref()
    else {
        return Err("active profile folder picker missing");
    };
    // Includes selection/scroll state, entries, cwd, root and hidden-folder flag.
    Ok(format!("{browser:?}"))
}

fn parent_snapshot(state: &ManagerState<'_>) -> String {
    let auth = &settings(state).auth;
    assert_eq!(auth.modals.parents().len(), 1);
    assert_eq!(auth.editing_account.as_deref(), Some("work"));
    assert_eq!(auth.selected_kind(), Some(crate::tui::auth::AuthKind::Codex));
    format!("{:?}", auth.modals.parents())
}

fn request_folder(state: &mut ManagerState<'_>, config: &mut AppConfig, index: usize) -> PathBuf {
    key(state, config, KeyCode::Char('s'));
    let effects = state.drain_effects();
    let [ManagerEffect::ApplyFileBrowserOutcome {
        context: FileBrowserEffectContext::SettingsAuth,
        outcome: FileBrowserOutcome::RequestCommit(path),
    }] = effects.as_slice()
    else {
        panic!("Settings dispatch must request folder validation: {effects:?}");
    };
    assert_eq!(*path, PathBuf::from(ROOT).join(format!("profile-{index:02}")));
    path.clone()
}

fn reject_folder(state: &mut ManagerState<'_>, config: &mut AppConfig) {
    request_folder(state, config, 20);
    assert!(apply_file_browser_commit_result(
        state,
        FileBrowserCommitResult::Rejected {
            context: FileBrowserEffectContext::SettingsAuth,
            reason: REJECTION.into(),
        },
    ));
    crate::tui::input::global_mounts::after_settings_event(state);
    assert!(settings(state).error_popup.is_some());
    assert!(render(state, config).contains(REJECTION));
}

#[test]
fn invalid_folder_dismissal_preserves_picker_then_accepts_another_folder() {
    for dismiss in [KeyCode::Esc, KeyCode::Enter] {
        let (mut state, mut config) = fixture();
        let before_render = render(&mut state, &config);
        assert!(before_render.contains("profile-20"));
        assert!(!before_render.contains(SECRET));
        let before_picker = picker_snapshot(&state).unwrap();
        let before_parent = parent_snapshot(&state);
        let before_pending = settings(&state).auth.pending.clone();
        reject_folder(&mut state, &mut config);
        key(&mut state, &mut config, dismiss);
        assert!(settings(&state).error_popup.is_none());
        assert_eq!(picker_snapshot(&state).unwrap(), before_picker);
        assert_eq!(parent_snapshot(&state), before_parent);
        assert_eq!(render(&mut state, &config), before_render);
        assert_eq!(settings(&state).auth.pending, before_pending);

        key(&mut state, &mut config, KeyCode::Down);
        let accepted = request_folder(&mut state, &mut config, 21);
        assert!(apply_file_browser_commit_result(
            &mut state,
            FileBrowserCommitResult::Accepted {
                context: FileBrowserEffectContext::SettingsAuth,
                path: accepted.clone(),
            },
        ));
        let auth = &settings(&state).auth;
        assert!(auth.modals.parents().is_empty());
        let Some(SettingsModal::AuthForm {
            target,
            state: form,
            focus,
            literal_buffer,
        }) = auth.modal_ref()
        else {
            panic!("successful selection restores account form");
        };
        assert!(matches!(
            target,
            AuthFormTarget::Workspace {
                kind: crate::tui::auth::AuthKind::Codex
            }
        ));
        assert_eq!(*focus, AuthFormFocus::Save);
        assert_eq!(form.source_folder.as_ref(), Some(&accepted));
        assert_eq!(form.literal_buffer(), SECRET);
        assert_eq!(literal_buffer, SECRET);
        assert_eq!(auth.pending, before_pending, "selection stages; Save commits");
    }
}

#[test]
fn invalid_folder_dismissal_then_cancel_restores_exact_unsaved_form() {
    let (mut state, mut config) = fixture();
    let before_parent = parent_snapshot(&state);
    let before_pending = settings(&state).auth.pending.clone();
    reject_folder(&mut state, &mut config);
    key(&mut state, &mut config, KeyCode::Enter);
    key(&mut state, &mut config, KeyCode::Esc);
    let auth = &settings(&state).auth;
    assert!(auth.modals.parents().is_empty());
    assert_eq!(format!("{:?}", [auth.modal_ref().unwrap()]), before_parent);
    assert_eq!(auth.pending, before_pending);
    assert!(matches!(
        auth.modal_ref(),
        Some(SettingsModal::AuthForm {
            focus: AuthFormFocus::SourceFolder,
            ..
        })
    ));
}

#[test]
fn invalid_folder_oracle_rejects_old_unconditional_pop_mutant() {
    let (mut state, mut config) = fixture();
    reject_folder(&mut state, &mut config);
    let ManagerStage::Settings(current) = &mut state.stage else {
        unreachable!();
    };
    // Controlled old implementation: popup dismissal incorrectly closes child.
    current.error_popup = None;
    current.auth.restore_pending_auth_form();
    assert_eq!(
        picker_snapshot(&state),
        Err("active profile folder picker missing")
    );
    assert!(matches!(
        settings(&state).auth.modal_ref(),
        Some(SettingsModal::AuthForm { .. })
    ));
}
