use jackin_config::AppConfig;
use jackin_config::WorkspaceConfig;
use jackin_console::tui::auth::{AuthKind, AuthMode};
use jackin_console::tui::components::file_browser::{
    FileBrowserOutcome, FileBrowserState, FolderListing,
};
use jackin_console::tui::effect::FileBrowserEffectContext;
use jackin_console::tui::effect::{ConsoleEffect, WorkspaceSaveEffect};
use jackin_console::tui::state::update::{ManagerBackgroundEvent, ManagerMessage};

use crate::console::tui::state::{
    AuthForm, AuthFormFocus, AuthFormTarget, CreatePreludeState, EditorState,
    FileBrowserTarget, ManagerEffect, ManagerStage, ManagerState, Modal, PendingRoleLoad,
    SettingsAuthModal, SettingsState,
};
use crate::console::tui::{WorkspaceSaveWriteInput, WorkspaceSaveWriteMode};

use super::{
    apply_role_load_completion, execute_manager_effect, execute_workspace_save_effect,
    execute_workspace_save_write, poll_background_messages,
};

#[tokio::test]
async fn poll_background_messages_routes_file_browser_poll_through_message() {
    let tmp = tempfile::tempdir().unwrap();
    let paths = jackin_core::JackinPaths::for_tests(tmp.path());
    let cwd = tmp.path();
    let mut config = AppConfig::default();
    let mut state = ManagerState::from_config(&config, cwd);

    let events = poll_background_messages(&mut state, &mut config, &paths);

    assert!(events.iter().any(|event| matches!(
        event,
        ManagerBackgroundEvent::Message(ManagerMessage::PollFileBrowserGitUrls)
    )));
}

#[tokio::test]
async fn execute_manager_effect_requests_instance_refresh() {
    let tmp = tempfile::tempdir().unwrap();
    let paths = jackin_core::JackinPaths::for_tests(tmp.path());
    let cwd = tmp.path();
    let mut config = AppConfig::default();
    let mut state = ManagerState::from_config(&config, cwd);

    execute_manager_effect(
        &mut state,
        &mut config,
        &paths,
        ConsoleEffect::RequestInstanceRefresh.into(),
    );

    assert!(
        state.instance_refresh_in_flight(),
        "instance refresh effect should spawn a worker"
    );
}

#[tokio::test]
async fn workspace_save_drift_check_starts_worker() {
    let tmp = tempfile::tempdir().unwrap();
    let paths = jackin_core::JackinPaths::for_tests(tmp.path());
    let cwd = tmp.path();
    let mut config = AppConfig::default();
    let editor = EditorState::new_edit("workspace".into(), WorkspaceConfig::default());
    let mut state = ManagerState::from_config(&config, cwd);
    state.stage = ManagerStage::Editor(editor);

    execute_workspace_save_effect(
        &mut state,
        &mut config,
        &paths,
        cwd,
        WorkspaceSaveEffect::StartDriftCheck {
            original_name: "workspace".into(),
            prospective_mounts: Vec::new(),
            plan: jackin_console::tui::state::PendingSaveCommit {
                effective_removals: Vec::new(),
                final_mounts: None,
                delete_isolated_acknowledged: false,
                isolated_cleanup_complete: false,
            },
            exit_on_success: false,
        },
    );

    let ManagerStage::Editor(editor) = &state.stage else {
        panic!("expected editor stage");
    };
    assert!(
        editor.pending_drift_check.is_some(),
        "workspace save drift detection should run on a worker"
    );
    assert!(matches!(editor.modal, Some(Modal::StatusPopup { .. })));
}

#[tokio::test]
async fn workspace_save_write_starts_config_save_worker() {
    let tmp = tempfile::tempdir().unwrap();
    let paths = jackin_core::JackinPaths::for_tests(tmp.path());
    let cwd = tmp.path();
    let mut config = AppConfig::default();
    let original = WorkspaceConfig::default();
    let pending = WorkspaceConfig::default();
    let editor = EditorState::new_edit("workspace".into(), original.clone());
    let mut state = ManagerState::from_config(&config, cwd);
    state.stage = ManagerStage::Editor(editor);

    execute_workspace_save_write(
        &mut state,
        &mut config,
        &paths,
        cwd,
        WorkspaceSaveWriteInput {
            mode: WorkspaceSaveWriteMode::Edit {
                original_name: "workspace".into(),
                pending_name: None,
                effective_removals: Vec::new(),
            },
            original: &original,
            pending: &pending,
        },
        false,
    );

    assert!(
        state.config_save_in_flight(),
        "workspace config save should run on a worker"
    );
}

#[tokio::test]
async fn settings_save_starts_config_save_worker() {
    let tmp = tempfile::tempdir().unwrap();
    let paths = jackin_core::JackinPaths::for_tests(tmp.path());
    let cwd = tmp.path();
    let mut config = AppConfig::default();
    let settings = SettingsState::from_config(&config);
    let mut state = ManagerState::from_config(&config, cwd);
    state.stage = ManagerStage::Settings(settings);

    execute_manager_effect(
        &mut state,
        &mut config,
        &paths,
        ConsoleEffect::SaveSettings.into(),
    );

    assert!(
        state.config_save_in_flight(),
        "settings config save should run on a worker"
    );
}

#[tokio::test]
async fn remove_workspace_starts_config_save_worker() {
    let tmp = tempfile::tempdir().unwrap();
    let paths = jackin_core::JackinPaths::for_tests(tmp.path());
    let cwd = tmp.path();
    let mut config = AppConfig::default();
    let mut state = ManagerState::from_config(&config, cwd);

    super::execute_remove_workspace(&mut state, &mut config, &paths, cwd, "workspace");

    assert!(
        state.config_save_in_flight(),
        "workspace delete should run on a worker"
    );
}

#[tokio::test]
async fn trusted_role_source_persist_starts_config_save_worker() {
    let tmp = tempfile::tempdir().unwrap();
    let paths = jackin_core::JackinPaths::for_tests(tmp.path());
    let cwd = tmp.path();
    let mut config = AppConfig::default();
    let editor = EditorState::new_edit("workspace".into(), WorkspaceConfig::default());
    let mut state = ManagerState::from_config(&config, cwd);
    state.stage = ManagerStage::Editor(editor);

    execute_manager_effect(
        &mut state,
        &mut config,
        &paths,
        ManagerEffect::PersistTrustedRoleSource {
            key: "agent-smith".into(),
            source: jackin_config::RoleSource::default(),
        },
    );

    assert!(
        state.config_save_in_flight(),
        "trusted role source persistence should run on a worker"
    );
}

#[tokio::test]
async fn role_load_completion_starts_role_source_persist_worker() {
    let tmp = tempfile::tempdir().unwrap();
    let paths = jackin_core::JackinPaths::for_tests(tmp.path());
    let cwd = tmp.path();
    let mut config = AppConfig::default();
    let editor = EditorState::new_edit("workspace".into(), WorkspaceConfig::default());
    let mut state = ManagerState::from_config(&config, cwd);
    state.stage = ManagerStage::Editor(editor);
    let (_tx, rx) = tokio::sync::oneshot::channel();

    apply_role_load_completion(
        &mut state,
        &mut config,
        &paths,
        PendingRoleLoad {
            raw: "agent-smith".into(),
            key: "agent-smith".into(),
            source: jackin_config::RoleSource::default(),
            rx,
        },
        Ok(()),
    );

    assert!(
        state.config_save_in_flight(),
        "loaded role source persistence should run on a worker"
    );
}

#[tokio::test]
async fn create_prelude_file_browser_open_starts_listing_worker() {
    let tmp = tempfile::tempdir().unwrap();
    let paths = jackin_core::JackinPaths::for_tests(tmp.path());
    let cwd = tmp.path();
    let mut config = AppConfig::default();
    let mut state = ManagerState::from_config(&config, cwd);

    execute_manager_effect(
        &mut state,
        &mut config,
        &paths,
        ManagerEffect::OpenCreatePreludeFileBrowser,
    );

    assert!(
        state.file_browser_listing_in_flight(),
        "file browser open should scan directories on a worker"
    );
    assert!(
        matches!(state.stage, ManagerStage::List),
        "the modal should open only after the worker returns"
    );
}

#[tokio::test]
async fn file_browser_navigation_starts_listing_worker() {
    let tmp = tempfile::tempdir().unwrap();
    let paths = jackin_core::JackinPaths::for_tests(tmp.path());
    let cwd = tmp.path();
    let mut config = AppConfig::default();
    let mut state = ManagerState::from_config(&config, cwd);
    let listing = FolderListing {
        root: cwd.to_path_buf(),
        cwd: cwd.to_path_buf(),
        entries: Vec::new(),
    };
    let mut prelude = CreatePreludeState::new();
    prelude.modal = Some(Modal::FileBrowser {
        target: FileBrowserTarget::CreateFirstMountSrc,
        state: FileBrowserState::from_listing(listing),
    });
    state.stage = ManagerStage::CreatePrelude(prelude);

    execute_manager_effect(
        &mut state,
        &mut config,
        &paths,
        ManagerEffect::ApplyFileBrowserOutcome {
            context: FileBrowserEffectContext::Prelude { browser_cwd: None },
            outcome: FileBrowserOutcome::NavigateTo(cwd.join("child")),
        },
    );

    assert!(
        state.file_browser_listing_in_flight(),
        "file browser navigation should scan directories on a worker"
    );
}

#[tokio::test]
async fn file_browser_commit_starts_validation_worker() {
    let tmp = tempfile::tempdir().unwrap();
    let paths = jackin_core::JackinPaths::for_tests(tmp.path());
    let cwd = tmp.path();
    let child = cwd.join("child");
    std::fs::create_dir(&child).unwrap();
    let mut config = AppConfig::default();
    let mut state = ManagerState::from_config(&config, cwd);
    let listing = FolderListing {
        root: cwd.to_path_buf(),
        cwd: cwd.to_path_buf(),
        entries: Vec::new(),
    };
    let mut prelude = CreatePreludeState::new();
    prelude.modal = Some(Modal::FileBrowser {
        target: FileBrowserTarget::CreateFirstMountSrc,
        state: FileBrowserState::from_listing(listing),
    });
    state.stage = ManagerStage::CreatePrelude(prelude);

    execute_manager_effect(
        &mut state,
        &mut config,
        &paths,
        ManagerEffect::ApplyFileBrowserOutcome {
            context: FileBrowserEffectContext::Prelude { browser_cwd: None },
            outcome: FileBrowserOutcome::RequestCommit(child),
        },
    );

    assert!(
        state.file_browser_commit_in_flight(),
        "file browser commit validation should run on a worker"
    );
}

#[tokio::test]
async fn editor_auth_source_folder_open_starts_listing_worker() {
    let tmp = tempfile::tempdir().unwrap();
    let paths = jackin_core::JackinPaths::for_tests(tmp.path());
    let cwd = tmp.path();
    let mut config = AppConfig::default();
    let mut state = ManagerState::from_config(&config, cwd);
    let mut editor = EditorState::new_edit("workspace".into(), WorkspaceConfig::default());
    let mut form = AuthForm::new(AuthKind::Claude);
    form.set_mode(AuthMode::Sync);
    form.set_source_folder(cwd.to_path_buf());
    editor.modal = Some(Modal::AuthForm {
        target: AuthFormTarget::Workspace {
            kind: AuthKind::Claude,
        },
        state: Box::new(form),
        focus: AuthFormFocus::SourceFolder,
        literal_buffer: String::new(),
    });
    state.stage = ManagerStage::Editor(editor);

    execute_manager_effect(
        &mut state,
        &mut config,
        &paths,
        ManagerEffect::OpenEditorAuthSourceFolderBrowser,
    );

    assert!(state.file_browser_listing_in_flight());
    let ManagerStage::Editor(editor) = &state.stage else {
        panic!("expected editor stage");
    };
    assert!(matches!(editor.modal, Some(Modal::AuthForm { .. })));
}

#[tokio::test]
async fn settings_auth_source_folder_open_starts_listing_worker() {
    let tmp = tempfile::tempdir().unwrap();
    let paths = jackin_core::JackinPaths::for_tests(tmp.path());
    let cwd = tmp.path();
    let mut config = AppConfig::default();
    let mut state = ManagerState::from_config(&config, cwd);
    let mut settings = SettingsState::from_config(&config);
    let mut form = AuthForm::new(AuthKind::Claude);
    form.set_mode(AuthMode::Sync);
    form.set_source_folder(cwd.to_path_buf());
    settings.auth.modal = Some(SettingsAuthModal::AuthForm {
        target: AuthFormTarget::Workspace {
            kind: AuthKind::Claude,
        },
        state: Box::new(form),
        focus: AuthFormFocus::SourceFolder,
        literal_buffer: String::new(),
    });
    state.stage = ManagerStage::Settings(settings);

    execute_manager_effect(
        &mut state,
        &mut config,
        &paths,
        ManagerEffect::OpenSettingsAuthSourceFolderBrowser,
    );

    assert!(state.file_browser_listing_in_flight());
    let ManagerStage::Settings(settings) = &state.stage else {
        panic!("expected settings stage");
    };
    assert!(matches!(
        settings.auth.modal,
        Some(SettingsAuthModal::AuthForm { .. })
    ));
}
