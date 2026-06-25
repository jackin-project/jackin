//! Tests for `auth`.
use super::*;
use crate::tui::auth::AuthKind;
use crate::tui::components::auth_panel::CredentialInput;
use crate::tui::state::AuthRow;
use crate::tui::state::{AuthFormTarget, EditorState, FieldFocus, ManagerStage, ManagerState};
use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyEventState, KeyModifiers};
use jackin_config::{
    AgentAuthConfig, AppConfig, AuthForwardMode, GithubAuthConfig, GithubAuthMode,
};
use jackin_config::{MountConfig, WorkspaceConfig, WorkspaceRoleOverride};
use jackin_core::{OpRef, env_model};
use jackin_env::OpRunner;

fn key(code: KeyCode) -> KeyEvent {
    KeyEvent {
        code,
        modifiers: KeyModifiers::NONE,
        kind: KeyEventKind::Press,
        state: KeyEventState::NONE,
    }
}

/// Per-test op-cache (no shared state between test cases).
fn fresh_op_cache() -> std::rc::Rc<std::cell::RefCell<OpCache>> {
    std::rc::Rc::new(std::cell::RefCell::new(OpCache::default()))
}

/// Test wrapper around `handle_auth_form_key` with
/// `op_available = true`.
fn drive_key(editor: &mut EditorState<'_>, k: KeyEvent) -> bool {
    handle_auth_form_key(editor, k, true).is_dirty()
}

fn complete_source_folder_browser_open(editor: &mut EditorState<'_>) {
    let outcome = handle_auth_form_key(editor, key(KeyCode::Enter), true);
    assert_eq!(
        outcome,
        AuthFormKeyOutcome::OpenSourceFolderBrowser,
        "source-folder Enter should request async browser open"
    );
    let listing = crate::tui::components::file_browser::FolderListing {
        root: PathBuf::from("/host"),
        cwd: PathBuf::from("/host"),
        entries: Vec::new(),
    };
    assert!(open_auth_source_folder_browser_from_form_with_state(
        editor,
        crate::tui::components::file_browser::FileBrowserState::from_listing(listing),
    ));
}

/// Return the flat-row index for `WorkspaceMode { Claude }`.
/// Panics if the row doesn't exist (which would indicate a broken
/// fixture, not a test-under-test failure).
fn workspace_claude_row_idx(editor: &EditorState<'_>, config: &AppConfig) -> usize {
    editor
        .auth_flat_rows(config)
        .iter()
        .position(|r| {
            matches!(
                r,
                AuthRow::WorkspaceMode {
                    kind: AuthKind::Claude,
                }
            )
        })
        .expect("WorkspaceMode × Claude row must exist in")
}

/// Return the flat-row index for `WorkspaceMode { Github }`.
fn workspace_github_row_idx(editor: &EditorState<'_>, config: &AppConfig) -> usize {
    editor
        .auth_flat_rows(config)
        .iter()
        .position(|r| {
            matches!(
                r,
                AuthRow::WorkspaceMode {
                    kind: AuthKind::Github,
                }
            )
        })
        .expect("WorkspaceMode × Github row must exist in")
}

fn build_state() -> (AppConfig, ManagerState<'static>) {
    let mut cfg = AppConfig::default();
    let mut ws = WorkspaceConfig {
        workdir: "/code/proj".into(),
        mounts: vec![MountConfig {
            src: "/code/proj".into(),
            dst: "/code/proj".into(),
            readonly: false,
            isolation: jackin_config::MountIsolation::Shared,
        }],
        allowed_roles: vec!["smith".into()],
        ..Default::default()
    };
    ws.allowed_roles.sort();
    cfg.workspaces.insert("proj".into(), ws);
    cfg.roles.insert(
        "smith".into(),
        jackin_config::RoleSource {
            git: "https://example.com/jackin-smith.git".into(),
            trusted: true,
            env: std::collections::BTreeMap::default(),
        },
    );

    let cwd = PathBuf::from("/tmp");
    let mut state = ManagerState::from_config(&cfg, &cwd);
    let ws = cfg.workspaces.get("proj").unwrap().clone();
    let mut editor = EditorState::new_edit("proj".into(), ws);
    editor.active_tab = crate::tui::state::EditorTab::Auth;
    editor.auth_selected_kind = Some(AuthKind::Claude);
    let ws_claude_idx = workspace_claude_row_idx(&editor, &cfg);
    editor.active_field = FieldFocus::Row(ws_claude_idx);
    state.stage = ManagerStage::Editor(editor);
    (cfg, state)
}

/// Build state focused on the GitHub kind for the github-tab tests.
fn build_github_state() -> (AppConfig, ManagerState<'static>) {
    let (cfg, mut state) = build_state();
    let ManagerStage::Editor(editor) = &mut state.stage else {
        panic!()
    };
    editor.auth_selected_kind = Some(AuthKind::Github);
    let ws_github_idx = workspace_github_row_idx(editor, &cfg);
    editor.active_field = FieldFocus::Row(ws_github_idx);
    (cfg, state)
}

#[test]
fn auth_form_saving_workspace_zai_ignore_removes_key() {
    let mut editor = EditorState::new_edit("proj".into(), WorkspaceConfig::default());
    editor
        .pending
        .env
        .insert("ZAI_API_KEY".into(), EnvValue::Plain("secret".into()));
    let mut form = AuthForm::new(AuthKind::Zai);
    form.set_mode(AuthMode::Ignore);

    editor.persist_auth_form(
        &AuthFormTarget::Workspace {
            kind: AuthKind::Zai,
        },
        &form,
    );

    assert!(!editor.pending.env.contains_key("ZAI_API_KEY"));
}

#[test]
fn auth_form_saving_role_zai_ignore_removes_key() {
    let mut editor = EditorState::new_edit("proj".into(), WorkspaceConfig::default());
    let mut role = WorkspaceRoleOverride::default();
    role.env
        .insert("ZAI_API_KEY".into(), EnvValue::Plain("secret".into()));
    editor.pending.roles.insert("smith".into(), role);
    let mut form = AuthForm::new(AuthKind::Zai);
    form.set_mode(AuthMode::Ignore);

    editor.persist_auth_form(
        &AuthFormTarget::WorkspaceRole {
            role: "smith".into(),
            kind: AuthKind::Zai,
        },
        &form,
    );

    assert!(
        !editor
            .pending
            .roles
            .get("smith")
            .is_some_and(|role| role.env.contains_key("ZAI_API_KEY"))
    );
}

/// Walking from the workspace × Claude row through the form:
/// Enter opens form, Space cycles mode to `api_key`, Tab moves to
/// credential, Enter picks source, type literal, Enter confirms,
/// Enter saves. The
/// in-memory `pending.claude` and `pending.env` reflect the change.
#[test]
fn auth_form_save_persists_workspace_layer_into_pending() {
    let (cfg, mut state) = build_state();
    // Open form (Enter) on row 0 → workspace × Claude.
    let ManagerStage::Editor(editor) = &mut state.stage else {
        panic!()
    };
    open_auth_form_modal(editor, &cfg);
    assert!(matches!(editor.modal, Some(Modal::AuthForm { .. })));

    // Cycle mode: None → first available (sync) → ApiKey is two cycles.
    drive_key(editor, key(KeyCode::Char(' ')));
    drive_key(editor, key(KeyCode::Char(' ')));
    // Tab advances to credential row, then Enter opens the source picker.
    drive_key(editor, key(KeyCode::Tab));
    drive_key(editor, key(KeyCode::Enter));
    assert!(matches!(editor.modal, Some(Modal::AuthSourcePicker { .. })));
    apply_plain_source_picker_to_auth_form(editor);
    assert!(matches!(editor.modal, Some(Modal::TextInput { .. })));
    apply_plain_text_to_auth_form(editor, "secret");
    // Enter → save.
    let closed = drive_key(editor, key(KeyCode::Enter));
    assert!(closed, "save must close the modal");
    assert!(editor.modal.is_none(), "modal should be gone");

    // pending.claude reflects ApiKey.
    let claude_cfg = editor
        .pending
        .claude
        .as_ref()
        .expect("workspace claude block must be set");
    assert_eq!(claude_cfg.auth_forward, AuthForwardMode::ApiKey);
    // pending.env carries the credential.
    let value = editor
        .pending
        .env
        .get(env_model::ANTHROPIC_API_KEY_ENV_NAME)
        .expect("credential env var must be set");
    match value {
        EnvValue::Plain(s) => assert_eq!(s, "secret"),
        EnvValue::Extended(value) => assert_eq!(value.value, "secret"),
        EnvValue::OpRef(_) => panic!("expected plain literal credential"),
    }
}

/// Reset action clears the layer's mode without touching any
/// credential env var. Confirms that the Reset button on the form
/// produces the "drop down to inherited" behavior.
#[test]
fn auth_form_reset_clears_workspace_layer_mode() {
    let (cfg, mut state) = build_state();
    let ManagerStage::Editor(editor) = &mut state.stage else {
        panic!()
    };
    editor.pending.claude = Some(AgentAuthConfig {
        auth_forward: AuthForwardMode::ApiKey,
        ..Default::default()
    });
    open_auth_form_modal(editor, &cfg);
    // Tab through to Reset and Enter.
    // From Mode → Down → Cred → Tab → Save → Tab → Cancel → Tab → Reset.
    drive_key(editor, key(KeyCode::Down)); // Mode → CredentialSource (intra-area)
    drive_key(editor, key(KeyCode::Tab)); // Cred → Save (Tab crosses to button area)
    drive_key(editor, key(KeyCode::Tab)); // → Cancel
    drive_key(editor, key(KeyCode::Tab)); // → Reset
    let closed = drive_key(editor, key(KeyCode::Enter));
    assert!(closed, "reset must close the modal");
    assert!(
        editor.pending.claude.is_none(),
        "reset must clear workspace claude block"
    );
}

/// Cancel doesn't persist anything to pending: the workspace layer
/// stays untouched.
#[test]
fn auth_form_cancel_does_not_mutate_pending() {
    let (cfg, mut state) = build_state();
    let ManagerStage::Editor(editor) = &mut state.stage else {
        panic!()
    };
    open_auth_form_modal(editor, &cfg);
    drive_key(editor, key(KeyCode::Char(' '))); // cycle to sync
    // Esc cancels at any focus.
    let closed = drive_key(editor, key(KeyCode::Esc));
    assert!(closed);
    assert!(
        editor.pending.claude.is_none(),
        "cancel must not write to pending"
    );
}

#[test]
fn auth_form_enter_on_mode_does_not_navigate_tab_does() {
    let (cfg, mut state) = build_state();
    let ManagerStage::Editor(editor) = &mut state.stage else {
        panic!()
    };
    open_auth_form_modal(editor, &cfg);
    drive_key(editor, key(KeyCode::Char(' ')));
    drive_key(editor, key(KeyCode::Char(' ')));

    drive_key(editor, key(KeyCode::Enter));
    let Some(Modal::AuthForm { focus, .. }) = &editor.modal else {
        panic!("auth form must still be open")
    };
    assert_eq!(
        *focus,
        AuthFormFocus::Mode,
        "Enter on mode must not move to the next actionable row"
    );

    drive_key(editor, key(KeyCode::Tab));
    let Some(Modal::AuthForm { focus, .. }) = &editor.modal else {
        panic!("auth form must still be open")
    };
    assert_eq!(
        *focus,
        AuthFormFocus::CredentialSource,
        "Tab on mode must move to the credential row"
    );
}

/// Tab from the last focusable control wraps back to the first.
#[test]
fn auth_form_tab_wraps_around_at_reset() {
    let (cfg, mut state) = build_state();
    let ManagerStage::Editor(editor) = &mut state.stage else {
        panic!()
    };
    open_auth_form_modal(editor, &cfg);
    // Walk to Reset (last focusable):
    // Mode → Tab → Save → Tab → Cancel → Tab → Reset.
    drive_key(editor, key(KeyCode::Tab));
    drive_key(editor, key(KeyCode::Tab));
    drive_key(editor, key(KeyCode::Tab));
    let Some(Modal::AuthForm { focus, .. }) = &editor.modal else {
        panic!("auth form must still be open")
    };
    assert_eq!(*focus, AuthFormFocus::Reset);

    // Tab from Reset wraps to Mode.
    drive_key(editor, key(KeyCode::Tab));
    let Some(Modal::AuthForm { focus, .. }) = &editor.modal else {
        panic!("auth form must still be open")
    };
    assert_eq!(
        *focus,
        AuthFormFocus::Mode,
        "Tab on Reset must wrap to Mode"
    );

    // BackTab from Mode wraps to Reset (last).
    drive_key(editor, key(KeyCode::BackTab));
    let Some(Modal::AuthForm { focus, .. }) = &editor.modal else {
        panic!("auth form must still be open")
    };
    assert_eq!(
        *focus,
        AuthFormFocus::Reset,
        "BackTab on Mode must wrap to Reset"
    );
}

#[test]
fn auth_form_right_on_reset_stays_on_reset() {
    let (cfg, mut state) = build_state();
    let ManagerStage::Editor(editor) = &mut state.stage else {
        panic!()
    };
    open_auth_form_modal(editor, &cfg);
    // Walk to Reset: Mode → Tab → Save → Tab → Cancel → Tab → Reset.
    drive_key(editor, key(KeyCode::Tab));
    drive_key(editor, key(KeyCode::Tab));
    drive_key(editor, key(KeyCode::Tab));

    // Right must not leave the button row.
    drive_key(editor, key(KeyCode::Right));
    let Some(Modal::AuthForm { focus, .. }) = &editor.modal else {
        panic!("auth form must still be open")
    };
    assert_eq!(
        *focus,
        AuthFormFocus::Reset,
        "Right on Reset must not move focus off the button row"
    );
}

#[test]
fn auth_form_source_folder_browse_stages_and_save_persists_workspace_layer() {
    let (cfg, mut state) = build_state();
    let ManagerStage::Editor(editor) = &mut state.stage else {
        panic!()
    };
    open_auth_form_modal(editor, &cfg);
    drive_key(editor, key(KeyCode::Char(' '))); // unset → sync

    drive_key(editor, key(KeyCode::Tab));
    let Some(Modal::AuthForm { focus, .. }) = &editor.modal else {
        panic!("auth form must still be open")
    };
    assert_eq!(*focus, AuthFormFocus::SourceFolder);

    complete_source_folder_browser_open(editor);
    assert!(matches!(
        editor.modal,
        Some(Modal::FileBrowser {
            target: FileBrowserTarget::AuthFormSourceFolder,
            ..
        })
    ));
    apply_source_folder_to_auth_form(editor, PathBuf::from("/host/claude"));

    let Some(Modal::AuthForm { state, focus, .. }) = &editor.modal else {
        panic!("folder commit must restore auth form")
    };
    assert_eq!(*focus, AuthFormFocus::Save);
    assert_eq!(state.source_folder, Some(PathBuf::from("/host/claude")));

    let closed = drive_key(editor, key(KeyCode::Enter));
    assert!(closed, "save must close the modal");
    assert_eq!(
        editor
            .pending
            .claude
            .as_ref()
            .and_then(|auth| auth.sync_source_dir.clone()),
        Some(PathBuf::from("/host/claude"))
    );
}

#[test]
fn auth_form_source_folder_save_persists_role_layer() {
    let (cfg, mut state) = build_state();
    let ManagerStage::Editor(editor) = &mut state.stage else {
        panic!()
    };
    editor.pending.roles.insert(
        "smith".into(),
        WorkspaceRoleOverride {
            claude: Some(AgentAuthConfig {
                auth_forward: AuthForwardMode::Sync,
                ..Default::default()
            }),
            ..Default::default()
        },
    );
    editor.auth_expanded.insert("smith".into());
    let smith_claude_idx = editor
        .auth_flat_rows(&cfg)
        .iter()
        .position(|r| {
            matches!(
                r,
                AuthRow::RoleMode {
                    role,
                    kind: AuthKind::Claude,
                } if role == "smith"
            )
        })
        .expect("RoleMode smith × Claude must exist after override insertion");
    editor.active_field = FieldFocus::Row(smith_claude_idx);
    open_auth_form_modal(editor, &cfg);

    drive_key(editor, key(KeyCode::Tab));
    let Some(Modal::AuthForm { focus, .. }) = &editor.modal else {
        panic!("auth form must still be open")
    };
    assert_eq!(*focus, AuthFormFocus::SourceFolder);

    complete_source_folder_browser_open(editor);
    apply_source_folder_to_auth_form(editor, PathBuf::from("/host/role-claude"));
    let closed = drive_key(editor, key(KeyCode::Enter));
    assert!(closed, "save must close the modal");

    let role = editor.pending.roles.get("smith").unwrap();
    assert_eq!(
        role.claude
            .as_ref()
            .and_then(|auth| auth.sync_source_dir.clone()),
        Some(PathBuf::from("/host/role-claude"))
    );
    assert!(
        editor
            .pending
            .claude
            .as_ref()
            .and_then(|auth| auth.sync_source_dir.clone())
            .is_none(),
        "role save must not write the workspace layer"
    );
}

#[test]
fn auth_form_source_folder_browser_cancel_keeps_pending_untouched() {
    let (cfg, mut state) = build_state();
    let ManagerStage::Editor(editor) = &mut state.stage else {
        panic!()
    };
    let pending_before = editor.pending.clone();
    open_auth_form_modal(editor, &cfg);
    drive_key(editor, key(KeyCode::Char(' '))); // unset → sync
    drive_key(editor, key(KeyCode::Tab));
    complete_source_folder_browser_open(editor);

    editor.pop_modal_chain();
    assert!(matches!(editor.modal, Some(Modal::AuthForm { .. })));
    assert_eq!(editor.pending, pending_before);
}

#[test]
fn auth_form_reset_clears_workspace_layer_mode_and_source_folder() {
    let (cfg, mut state) = build_state();
    let ManagerStage::Editor(editor) = &mut state.stage else {
        panic!()
    };
    editor.pending.claude = Some(AgentAuthConfig {
        auth_forward: AuthForwardMode::Sync,
        sync_source_dir: Some(PathBuf::from("/host/claude")),
    });
    open_auth_form_modal(editor, &cfg);

    drive_key(editor, key(KeyCode::Tab)); // Source folder
    drive_key(editor, key(KeyCode::Tab)); // Save
    drive_key(editor, key(KeyCode::Tab)); // Cancel
    drive_key(editor, key(KeyCode::Tab)); // Reset
    let closed = drive_key(editor, key(KeyCode::Enter));
    assert!(closed, "reset must close the modal");
    assert!(
        editor.pending.claude.is_none(),
        "reset must clear mode and source folder"
    );
}

#[test]
fn auth_form_typing_on_credential_row_does_not_set_plain_text() {
    let (cfg, mut state) = build_state();
    let ManagerStage::Editor(editor) = &mut state.stage else {
        panic!()
    };
    open_auth_form_modal(editor, &cfg);
    drive_key(editor, key(KeyCode::Char(' ')));
    drive_key(editor, key(KeyCode::Char(' ')));
    drive_key(editor, key(KeyCode::Tab));
    drive_key(editor, key(KeyCode::Char('x')));

    let Some(Modal::AuthForm { state, focus, .. }) = &editor.modal else {
        panic!("auth form must still be open")
    };
    assert_eq!(*focus, AuthFormFocus::CredentialSource);
    assert_eq!(
        state.credential,
        CredentialInput::None,
        "typing on credential row must not bypass the source picker"
    );
}

/// Picking the role × kind row mounts the form against the
/// override layer. Save persists the mode under
/// `pending.roles[role].claude` and the env var under
/// `pending.roles[role].env`.
#[test]
fn auth_form_save_persists_role_layer_into_pending() {
    let (cfg, mut state) = build_state();
    let ManagerStage::Editor(editor) = &mut state.stage else {
        panic!()
    };
    // Insert a Claude override entry for "smith" so it materialises
    // in the focused Claude auth view.
    editor.pending.roles.insert(
        "smith".into(),
        WorkspaceRoleOverride {
            claude: Some(AgentAuthConfig {
                auth_forward: AuthForwardMode::Sync,
                ..Default::default()
            }),
            ..Default::default()
        },
    );
    // Expand the role so the RoleMode child is emitted.
    editor.auth_expanded.insert("smith".into());
    // Dynamically locate the smith × Claude kind row.
    let smith_claude_idx = editor
        .auth_flat_rows(&cfg)
        .iter()
        .position(|r| {
            matches!(
                r,
                AuthRow::RoleMode {
                    role,
                    kind: AuthKind::Claude,
                } if role == "smith"
            )
        })
        .expect("RoleMode smith × Claude must exist after override insertion");
    editor.active_field = FieldFocus::Row(smith_claude_idx);
    open_auth_form_modal(editor, &cfg);
    let Some(Modal::AuthForm { target, .. }) = &editor.modal else {
        panic!("form must be open");
    };
    assert_eq!(
        target,
        &AuthFormTarget::WorkspaceRole {
            role: "smith".into(),
            kind: AuthKind::Claude,
        }
    );

    // Cycle sync to api_key, choose plain credential, type, tab to save, enter.
    drive_key(editor, key(KeyCode::Char(' ')));
    drive_key(editor, key(KeyCode::Tab));
    drive_key(editor, key(KeyCode::Enter));
    assert!(matches!(editor.modal, Some(Modal::AuthSourcePicker { .. })));
    apply_plain_source_picker_to_auth_form(editor);
    assert!(matches!(editor.modal, Some(Modal::TextInput { .. })));
    apply_plain_text_to_auth_form(editor, "abc");
    let closed = drive_key(editor, key(KeyCode::Enter));
    assert!(closed);

    let role_entry = editor
        .pending
        .roles
        .get("smith")
        .expect("role override must exist");
    let cfg = role_entry
        .claude
        .as_ref()
        .expect("role override claude must be set");
    assert_eq!(cfg.auth_forward, AuthForwardMode::ApiKey);
    let env_val = role_entry
        .env
        .get(env_model::ANTHROPIC_API_KEY_ENV_NAME)
        .expect("role env credential must be set");
    match env_val {
        EnvValue::Plain(s) => assert_eq!(s, "abc"),
        EnvValue::Extended(value) => assert_eq!(value.value, "abc"),
        EnvValue::OpRef(_) => panic!("expected plain literal"),
    }
}

/// Choosing 1Password from the credential source picker swaps the
/// auth-form modal for an `OpPicker` and stashes the form context in
/// `pending_auth_form_return`. Confirms the open path of the picker
/// round-trip wiring.
#[test]
fn auth_form_op_ref_picker_invocation_opens_op_picker_modal() {
    let (cfg, mut state) = build_state();
    let ManagerStage::Editor(editor) = &mut state.stage else {
        panic!()
    };
    open_auth_form_modal(editor, &cfg);
    // Mode → ApiKey (two cycles past `None → sync`).
    drive_key(editor, key(KeyCode::Char(' ')));
    drive_key(editor, key(KeyCode::Char(' ')));
    // Tab advances to the credential row, then Enter opens the source picker.
    drive_key(editor, key(KeyCode::Tab));
    let closed = drive_key(editor, key(KeyCode::Enter));
    assert!(closed, "opening source picker must close auth form");
    assert!(matches!(editor.modal, Some(Modal::AuthSourcePicker { .. })));
    open_op_picker_from_auth_source(editor, fresh_op_cache());
    assert!(
        matches!(editor.modal, Some(Modal::OpPicker { .. })),
        "auth form must hand off to OpPicker from the source picker"
    );
    assert!(
        !editor.modal_parents.is_empty(),
        "auth-form context must be stashed for the picker to return to"
    );
}

/// `g` on the workspace × Claude `oauth_token` form opens the
/// shared source picker (plain vs. 1Password) and arms
/// `generating_token_target`, driving the token-generate (mint)
/// path. The storage-target choice (op vs. plain) happens at the
/// source picker, not before it.
#[test]
fn auth_form_generate_opens_source_picker_and_arms_target() {
    let (cfg, mut state) = build_state();
    let ManagerStage::Editor(editor) = &mut state.stage else {
        panic!()
    };
    open_auth_form_modal(editor, &cfg);
    // Drive the mode to OAuthToken so the generate gate holds.
    let Some(Modal::AuthForm { state: form, .. }) = editor.modal.as_mut() else {
        panic!("auth form must be open")
    };
    form.set_mode(AuthMode::OAuthToken);
    assert!(editor.auth_form_can_generate_token());

    let closed = drive_key(editor, key(KeyCode::Char('g')));
    assert!(closed, "generate must consume the keystroke");
    assert!(
        matches!(editor.modal, Some(Modal::AuthSourcePicker { .. })),
        "generate must open the source picker as the first step"
    );
    assert!(
        !editor.modal_parents.is_empty(),
        "generate must stash the form so the post-mint re-mount can return to it; \
             generate vs. provide is disambiguated by the generate marker, not the stash"
    );
    assert!(
        matches!(
            editor.generating_token_target,
            Some(AuthFormTarget::Workspace {
                kind: AuthKind::Claude
            })
        ),
        "generate must arm the workspace × Claude target"
    );
}

/// After the `g`/`G` generate trigger stashes the form, the mint
/// completion re-mounts the Edit-auth dialog with the minted op
/// credential applied and focus on Save — the same shape the
/// `run_console` loop drives by calling `apply_op_picker_to_auth_form`
/// with the wired `OpRef`. The form is NOT persisted here; Save does
/// that. Uses an injected stub `OpRunner` so no real `op` binary runs.
#[test]
fn auth_form_generate_op_mint_remounts_form_focus_save() {
    struct StubRunner;
    impl OpRunner for StubRunner {
        fn read(&self, _r: &str) -> anyhow::Result<String> {
            Ok("sk-ant-oat01-MINTED".into())
        }
    }

    let (cfg, mut state) = build_state();
    let ManagerStage::Editor(editor) = &mut state.stage else {
        panic!()
    };
    let pending_before = editor.pending.clone();

    // Open the form on workspace × Claude, drive mode to OAuthToken,
    // then press `g` to start generate (stashes the form).
    open_auth_form_modal(editor, &cfg);
    let Some(Modal::AuthForm { state: form, .. }) = editor.modal.as_mut() else {
        panic!("auth form must be open")
    };
    form.set_mode(AuthMode::OAuthToken);
    let closed = drive_key(editor, key(KeyCode::Char('g')));
    assert!(closed, "generate must consume the keystroke");
    assert!(matches!(editor.modal, Some(Modal::AuthSourcePicker { .. })));
    assert!(
        !editor.modal_parents.is_empty(),
        "generate must stash the form for the post-mint re-mount"
    );

    // Simulate the loop's post-mint re-mount with the wired OpRef.
    let minted = OpRef {
        op: "op://uuid/claude-vault".into(),
        path: "Personal/Claude/oauth-token".into(),
        account: None,
        on_demand: false,
    };
    apply_op_picker_to_auth_form_with_runner(editor, minted.clone(), &StubRunner);

    // Form is back, focus Save, credential carries the minted ref.
    let Some(Modal::AuthForm { state, focus, .. }) = &editor.modal else {
        panic!("mint completion must re-mount the auth form");
    };
    assert_eq!(
        *focus,
        AuthFormFocus::Save,
        "post-mint re-mount drops the cursor onto Save"
    );
    match &state.credential {
        CredentialInput::OpRef(r) => assert_eq!(r, &minted),
        other => panic!("expected OpRef credential after mint; got {other:?}"),
    }
    assert!(
        state.can_save(),
        "form must be commitable once the minted ref is applied"
    );
    assert!(
        editor.modal_parents.is_empty(),
        "stash must be drained on the re-mount"
    );
    // Persistence is deferred to Save: pending stays untouched.
    assert_eq!(
        editor.pending, pending_before,
        "mint must not persist; only the operator's Save writes pending"
    );
}

/// The plain-text mint completion re-mounts the form with the minted
/// literal applied and focus Save (the `EnvValue::Plain` arm of the
/// loop, via `apply_plain_text_to_auth_form`). No persistence until
/// Save.
#[test]
fn auth_form_generate_plain_mint_remounts_form_focus_save() {
    let (cfg, mut state) = build_state();
    let ManagerStage::Editor(editor) = &mut state.stage else {
        panic!()
    };
    open_auth_form_modal(editor, &cfg);
    let Some(Modal::AuthForm { state: form, .. }) = editor.modal.as_mut() else {
        panic!("auth form must be open")
    };
    form.set_mode(AuthMode::OAuthToken);
    drive_key(editor, key(KeyCode::Char('g')));
    assert!(!editor.modal_parents.is_empty());

    // Simulate the loop's post-mint re-mount with the minted literal.
    apply_plain_text_to_auth_form(editor, "sk-ant-oat01-PLAIN");

    let Some(Modal::AuthForm { state, focus, .. }) = &editor.modal else {
        panic!("plain mint completion must re-mount the auth form");
    };
    assert_eq!(*focus, AuthFormFocus::Save);
    match &state.credential {
        CredentialInput::Literal(s) => assert_eq!(s, "sk-ant-oat01-PLAIN"),
        other => panic!("expected literal credential after plain mint; got {other:?}"),
    }
    assert!(editor.modal_parents.is_empty());
}

/// `g` is a no-op when the mode is not `oauth_token` (here ApiKey):
/// the auth form stays open and no target is armed.
#[test]
fn auth_form_generate_is_noop_for_non_oauth_token_mode() {
    let (cfg, mut state) = build_state();
    let ManagerStage::Editor(editor) = &mut state.stage else {
        panic!()
    };
    open_auth_form_modal(editor, &cfg);
    let Some(Modal::AuthForm { state: form, .. }) = editor.modal.as_mut() else {
        panic!("auth form must be open")
    };
    form.set_mode(AuthMode::ApiKey);
    assert!(!editor.auth_form_can_generate_token());

    let closed = drive_key(editor, key(KeyCode::Char('g')));
    assert!(!closed, "g must be a no-op when not oauth_token");
    assert!(matches!(editor.modal, Some(Modal::AuthForm { .. })));
    assert!(editor.generating_token_target.is_none());
}

/// Simulating a successful `OpPicker` commit re-mounts the auth
/// form with the picked `OpRef` applied. `can_save` flips to true
/// because the form now carries a valid `OpRef` and a committed
/// mode. Uses an injected fake `OpRunner` so the test never
/// shells out to the real `op` binary.
#[test]
fn auth_form_op_ref_picker_commit_applies_to_form() {
    struct StubRunner;
    impl OpRunner for StubRunner {
        fn read(&self, _r: &str) -> anyhow::Result<String> {
            Ok("sk-ant-from-vault".into())
        }
    }

    let (cfg, mut state) = build_state();
    let ManagerStage::Editor(editor) = &mut state.stage else {
        panic!()
    };
    // Open the auth form on workspace × Claude and choose
    // 1Password from the source picker.
    open_auth_form_modal(editor, &cfg);
    drive_key(editor, key(KeyCode::Char(' ')));
    drive_key(editor, key(KeyCode::Char(' ')));
    drive_key(editor, key(KeyCode::Tab));
    drive_key(editor, key(KeyCode::Enter));
    open_op_picker_from_auth_source(editor, fresh_op_cache());
    assert!(matches!(editor.modal, Some(Modal::OpPicker { .. })));

    // Simulate the picker committing a valid OpRef. Bypass the
    // production `OpCli` by calling the runner-injecting helper
    // directly — same code path the editor.rs handler invokes,
    // just with a stub runner.
    let picked = OpRef {
        op: "op://uuid/anthropic-vault".into(),
        path: "Work/Anthropic/api-key".into(),
        account: None,
        on_demand: false,
    };
    apply_op_picker_to_auth_form_with_runner(editor, picked.clone(), &StubRunner);

    // Form is back; the credential carries the picked OpRef and
    // can_save must be true (mode + non-empty OpRef both set).
    let Some(Modal::AuthForm { state, focus, .. }) = &editor.modal else {
        panic!("auth form must be re-mounted after picker commit");
    };
    assert_eq!(
        *focus,
        AuthFormFocus::Save,
        "successful picker commit drops cursor onto Save"
    );
    match &state.credential {
        CredentialInput::OpRef(r) => assert_eq!(r, &picked),
        other => panic!("expected OpRef credential after picker commit; got {other:?}"),
    }
    assert!(
        state.can_save(),
        "form must be commitable after picker supplies a non-empty OpRef"
    );
    assert!(
        editor.modal_parents.is_empty(),
        "stash must be drained on commit"
    );
}

/// A failed vault read (e.g. biometric timeout) must NOT corrupt
/// the form's credential. The form is re-stashed into
/// `pending_auth_form_return` and `Modal::ErrorPopup` is mounted;
/// dismissing the popup must restore the form with the prior
/// credential intact.
#[test]
fn auth_form_op_ref_picker_failed_read_does_not_apply_op_ref() {
    struct FailRunner;
    impl OpRunner for FailRunner {
        fn read(&self, _r: &str) -> anyhow::Result<String> {
            Err(anyhow::anyhow!("biometric prompt timed out"))
        }
    }

    let (cfg, mut state) = build_state();
    let ManagerStage::Editor(editor) = &mut state.stage else {
        panic!()
    };
    open_auth_form_modal(editor, &cfg);
    drive_key(editor, key(KeyCode::Char(' ')));
    drive_key(editor, key(KeyCode::Char(' ')));
    drive_key(editor, key(KeyCode::Tab));
    drive_key(editor, key(KeyCode::Enter));
    open_op_picker_from_auth_source(editor, fresh_op_cache());

    let picked = OpRef {
        op: "op://uuid/missing".into(),
        path: "Vault/Missing/field".into(),
        account: None,
        on_demand: false,
    };
    apply_op_picker_to_auth_form_with_runner(editor, picked, &FailRunner);

    // ErrorPopup mounted; form re-stashed for the popup dismissal
    // path to re-open via restore_auth_form_after_op_picker_cancel.
    assert!(
        matches!(editor.modal, Some(Modal::ErrorPopup { .. })),
        "failed vault read must surface an error popup"
    );
    assert!(
        !editor.modal_parents.is_empty(),
        "form must be re-stashed so popup dismiss can restore it"
    );

    // Simulate ErrorPopup dismiss → form restored.
    restore_auth_form_after_op_picker_cancel(editor);
    let Some(Modal::AuthForm { state, .. }) = &editor.modal else {
        panic!("popup dismiss must restore the auth form");
    };
    assert!(
        !matches!(state.credential, CredentialInput::OpRef(ref r) if r.path == "Vault/Missing/field"),
        "failed OpRef must not land in form credential"
    );
}

/// Esc on an open auth form must drain `pending_auth_form_return`
/// alongside dismissing the modal — leaving it set would let a
/// later `OpPicker` open from the Secrets tab silently inherit a
/// stale auth-form context. Defensive cleanup against future
/// picker flows.
#[test]
fn auth_form_esc_clears_pending_auth_form_return() {
    let (cfg, mut state) = build_state();
    let ManagerStage::Editor(editor) = &mut state.stage else {
        panic!()
    };
    // Stash a return path manually as if a picker handoff was in
    // flight. (Reaching this state via the public API is hard
    // because the picker swap takes the modal — the defensive
    // cleanup is for reentrancy / partial-flow bugs we don't want
    // to leak through Esc.)
    editor.modal_parents.push(Modal::AuthForm {
        target: AuthFormTarget::Workspace {
            kind: AuthKind::Claude,
        },
        state: Box::new(AuthForm::new(AuthKind::Claude)),
        focus: AuthFormFocus::Mode,
        literal_buffer: String::new(),
    });
    // Open the auth form modal so handle_auth_form_key can be
    // entered.
    open_auth_form_modal(editor, &cfg);
    assert!(matches!(editor.modal, Some(Modal::AuthForm { .. })));

    let closed = drive_key(editor, key(KeyCode::Esc));
    assert!(closed, "Esc must close the auth form");
    assert!(editor.modal.is_none(), "modal must be dropped");
    assert!(
        editor.modal_parents.is_empty(),
        "Esc must drain pending_auth_form_return so future picker flows \
             don't inherit stale stash state"
    );
}

/// `Enter` on the Save focus when `can_save` is false (e.g. an
/// `OpRef` credential with empty `op` and `path`) must NOT dismiss
/// the modal NOR mutate `editor.pending`. `can_save` rejects empty
/// `OpRef`s; this test pins that the input layer honours the guard
/// rather than ignoring it.
#[test]
fn auth_form_save_disabled_blocks_enter() {
    let (cfg, mut state) = build_state();
    let ManagerStage::Editor(editor) = &mut state.stage else {
        panic!()
    };
    let pending_before = editor.pending.clone();
    open_auth_form_modal(editor, &cfg);
    // Build a form state with mode = ApiKey and credential = empty
    // OpRef so `can_save` returns false.
    drive_key(editor, key(KeyCode::Char(' ')));
    drive_key(editor, key(KeyCode::Char(' ')));
    if let Some(Modal::AuthForm { state, .. }) = editor.modal.as_mut() {
        state.credential = CredentialInput::OpRef(OpRef {
            op: String::new(),
            path: String::new(),
            account: None,
            on_demand: false,
        });
    } else {
        panic!("auth form must still be open");
    }

    // Confirm the form's credential is the empty OpRef and can_save
    // is false.
    let Some(Modal::AuthForm { state, .. }) = &editor.modal else {
        panic!("auth form must still be open");
    };
    match &state.credential {
        CredentialInput::OpRef(r) => {
            assert!(
                r.op.is_empty() && r.path.is_empty(),
                "expected empty OpRef as setup; got {r:?}"
            );
        }
        other => panic!("expected OpRef credential after toggle; got {other:?}"),
    }
    assert!(
        !state.can_save(),
        "form must NOT be save-able with mode set + empty OpRef"
    );

    // Move focus directly to Save and press Enter. The handler
    // must short-circuit on `!can_save()` and leave the modal
    // open + pending untouched.
    if let Some(Modal::AuthForm { focus, .. }) = editor.modal.as_mut() {
        *focus = AuthFormFocus::Save;
    } else {
        panic!("auth form must still be open");
    }
    let closed = drive_key(editor, key(KeyCode::Enter));
    assert!(
        !closed,
        "Enter on Save with !can_save must NOT close the modal"
    );
    assert!(
        matches!(editor.modal, Some(Modal::AuthForm { .. })),
        "modal must remain on AuthForm; got {:?}",
        editor.modal
    );
    assert_eq!(
        editor.pending, pending_before,
        "Enter on Save with !can_save must NOT mutate editor.pending"
    );
}

/// Saving the GitHub form on the workspace layer with `token` mode
/// plus a literal `GH_TOKEN` writes the workspace `[github]` block
/// AND lands the credential under `[workspaces.<ws>.github.env]`
/// (NOT the regular `[workspaces.<ws>.env]` block — that would
/// leak `GH_TOKEN` into the operator-env layer launch resolves
/// through, while the github-specific layer is what
/// `build_github_env_layers` reads).
#[test]
fn github_form_save_persists_workspace_layer_into_pending() {
    let (cfg, mut state) = build_github_state();
    let ManagerStage::Editor(editor) = &mut state.stage else {
        panic!()
    };
    open_auth_form_modal(editor, &cfg);
    assert!(matches!(editor.modal, Some(Modal::AuthForm { .. })));
    // available_modes for Github is [sync, token, ignore].
    // Cycle: None → sync → token (two presses).
    drive_key(editor, key(KeyCode::Char(' ')));
    drive_key(editor, key(KeyCode::Char(' ')));
    // Token requires GH_TOKEN — Tab to credential, Enter, then
    // pick literal source and type a token.
    drive_key(editor, key(KeyCode::Tab));
    drive_key(editor, key(KeyCode::Enter));
    assert!(matches!(editor.modal, Some(Modal::AuthSourcePicker { .. })));
    apply_plain_source_picker_to_auth_form(editor);
    apply_plain_text_to_auth_form(editor, "ghp_xxx");
    let closed = drive_key(editor, key(KeyCode::Enter));
    assert!(closed, "save must close the modal");
    let github_block = editor
        .pending
        .github
        .as_ref()
        .expect("workspace github block must be set");
    assert_eq!(github_block.auth_forward, GithubAuthMode::Token);
    let value = github_block
        .env
        .get("GH_TOKEN")
        .expect("GH_TOKEN must land on the github env block, not the regular env block");
    match value {
        EnvValue::Plain(s) => assert_eq!(s, "ghp_xxx"),
        EnvValue::Extended(value) => assert_eq!(value.value, "ghp_xxx"),
        EnvValue::OpRef(_) => panic!("expected plain literal credential"),
    }
    // GH_TOKEN must NOT have leaked into the regular workspace env
    // map — that would shadow the kind-scoped value at launch
    // resolution and bypass `build_github_env_layers`.
    assert!(
        !editor.pending.env.contains_key("GH_TOKEN"),
        "GH_TOKEN must not land in the regular workspace env map"
    );
}

/// `D` on a Github `RoleHeader` clears the role's
/// `[workspaces.<ws>.roles.<role>.github]` override.
#[test]
fn d_on_github_role_header_clears_role_override() {
    let (cfg, mut state) = build_github_state();
    let ManagerStage::Editor(editor) = &mut state.stage else {
        panic!()
    };
    // Seed a role override on smith × Github.
    editor.pending.roles.insert(
        "smith".into(),
        WorkspaceRoleOverride {
            github: Some(GithubAuthConfig {
                auth_forward: GithubAuthMode::Ignore,
                ..Default::default()
            }),
            ..Default::default()
        },
    );
    // Locate the RoleHeader and put the cursor on it.
    let header_idx = editor
        .auth_flat_rows(&cfg)
        .iter()
        .position(|r| matches!(r, AuthRow::RoleHeader { role, .. } if role == "smith"))
        .expect("smith RoleHeader must exist after override insertion");
    editor.active_field = FieldFocus::Row(header_idx);
    handle_d_on_auth_row(editor, &cfg);
    let smith = editor
        .pending
        .roles
        .get("smith")
        .expect("override entry must remain");
    assert!(
        smith.github.is_none(),
        "D on github RoleHeader must clear the role's github override"
    );
}

/// `D` on a Github workspace mode row clears
/// `[workspaces.<ws>.github]` so resolution falls back to the
/// global `[github]` default.
#[test]
fn d_on_github_workspace_mode_row_clears_workspace_block() {
    let (cfg, mut state) = build_github_state();
    let ManagerStage::Editor(editor) = &mut state.stage else {
        panic!()
    };
    editor.pending.github = Some(GithubAuthConfig {
        auth_forward: GithubAuthMode::Token,
        ..Default::default()
    });
    let ws_github_idx = workspace_github_row_idx(editor, &cfg);
    editor.active_field = FieldFocus::Row(ws_github_idx);
    handle_d_on_auth_row(editor, &cfg);
    assert!(
        editor.pending.github.is_none(),
        "D on github WorkspaceMode must clear [workspaces.<ws>.github]"
    );
}

/// `D` on a Z.AI workspace mode row removes `ZAI_API_KEY` from the
/// workspace `[env]` — the env-only kind has no typed block to null,
/// so the reset path must reach into the env map.
#[test]
fn d_on_zai_workspace_mode_row_clears_env_key() {
    let (cfg, mut state) = build_state();
    let ManagerStage::Editor(editor) = &mut state.stage else {
        panic!()
    };
    editor.pending.env.insert(
        env_model::ZAI_API_KEY_ENV_NAME.to_owned(),
        EnvValue::Plain("zai-key".into()),
    );
    // Detail rows render for the selected kind; focus the Z.AI section.
    editor.auth_selected_kind = Some(AuthKind::Zai);
    let idx = editor
        .auth_flat_rows(&cfg)
        .iter()
        .position(|r| {
            matches!(
                r,
                AuthRow::WorkspaceMode {
                    kind: AuthKind::Zai
                }
            )
        })
        .expect("Z.AI WorkspaceMode row must exist with a key configured");
    editor.active_field = FieldFocus::Row(idx);
    handle_d_on_auth_row(editor, &cfg);
    assert!(
        !editor
            .pending
            .env
            .contains_key(env_model::ZAI_API_KEY_ENV_NAME),
        "D on Z.AI WorkspaceMode must remove ZAI_API_KEY from the workspace env"
    );
}

#[test]
fn d_on_workspace_preview_rows_is_noop() {
    let (cfg, mut state) = build_state();
    let ManagerStage::Editor(editor) = &mut state.stage else {
        panic!()
    };
    editor.pending.claude = Some(AgentAuthConfig {
        auth_forward: AuthForwardMode::ApiKey,
        sync_source_dir: Some(PathBuf::from("/host/claude")),
    });
    let source_idx = editor
        .auth_flat_rows(&cfg)
        .iter()
        .position(|r| {
            matches!(
                r,
                AuthRow::WorkspaceSource {
                    kind: AuthKind::Claude
                }
            )
        })
        .expect("Claude API key mode must render a workspace source preview row");
    editor.active_field = FieldFocus::Row(source_idx);
    handle_d_on_auth_row(editor, &cfg);
    assert_eq!(
        editor.pending.claude.as_ref().map(|auth| auth.auth_forward),
        Some(AuthForwardMode::ApiKey)
    );

    editor.pending.claude = Some(AgentAuthConfig {
        auth_forward: AuthForwardMode::Sync,
        sync_source_dir: Some(PathBuf::from("/host/claude")),
    });
    let source_folder_idx = editor
        .auth_flat_rows(&cfg)
        .iter()
        .position(|r| {
            matches!(
                r,
                AuthRow::WorkspaceSourceFolder {
                    kind: AuthKind::Claude
                }
            )
        })
        .expect("Claude sync mode must render a workspace source-folder preview row");
    editor.active_field = FieldFocus::Row(source_folder_idx);
    handle_d_on_auth_row(editor, &cfg);
    assert_eq!(
        editor
            .pending
            .claude
            .as_ref()
            .and_then(|auth| auth.sync_source_dir.clone()),
        Some(PathBuf::from("/host/claude"))
    );
}

#[test]
fn d_on_role_preview_rows_is_noop() {
    let (cfg, mut state) = build_state();
    let ManagerStage::Editor(editor) = &mut state.stage else {
        panic!()
    };
    editor.auth_expanded.insert("smith".into());
    editor.pending.roles.insert(
        "smith".into(),
        WorkspaceRoleOverride {
            claude: Some(AgentAuthConfig {
                auth_forward: AuthForwardMode::ApiKey,
                sync_source_dir: Some(PathBuf::from("/host/role-claude")),
            }),
            ..Default::default()
        },
    );
    let source_idx = editor
        .auth_flat_rows(&cfg)
        .iter()
        .position(|r| {
            matches!(
                r,
                AuthRow::RoleSource {
                    role,
                    kind: AuthKind::Claude
                } if role == "smith"
            )
        })
        .expect("Claude API key mode must render a role source preview row");
    editor.active_field = FieldFocus::Row(source_idx);
    handle_d_on_auth_row(editor, &cfg);
    assert_eq!(
        editor
            .pending
            .roles
            .get("smith")
            .and_then(|role| role.claude.as_ref())
            .map(|auth| auth.auth_forward),
        Some(AuthForwardMode::ApiKey)
    );

    editor
        .pending
        .roles
        .get_mut("smith")
        .and_then(|role| role.claude.as_mut())
        .expect("smith claude block must remain")
        .auth_forward = AuthForwardMode::Sync;
    let source_folder_idx = editor
        .auth_flat_rows(&cfg)
        .iter()
        .position(|r| {
            matches!(
                r,
                AuthRow::RoleSourceFolder {
                    role,
                    kind: AuthKind::Claude
                } if role == "smith"
            )
        })
        .expect("Claude sync mode must render a role source-folder preview row");
    editor.active_field = FieldFocus::Row(source_folder_idx);
    handle_d_on_auth_row(editor, &cfg);
    assert_eq!(
        editor
            .pending
            .roles
            .get("smith")
            .and_then(|role| role.claude.as_ref())
            .and_then(|auth| auth.sync_source_dir.clone()),
        Some(PathBuf::from("/host/role-claude"))
    );
}

/// Round-trip: save a workspace `[github]` block with `token`
/// plus `GH_TOKEN`, build a fresh editor over the resulting
/// `WorkspaceConfig`, and confirm `EditorState::auth_flat_rows` re-renders the
/// saved values (mode → token, `GH_TOKEN` visible) without any
/// extra operator interaction.
#[test]
fn github_form_save_round_trip_renders_persisted_values() {
    let (cfg, mut state) = build_github_state();
    let ManagerStage::Editor(editor) = &mut state.stage else {
        panic!()
    };
    open_auth_form_modal(editor, &cfg);
    drive_key(editor, key(KeyCode::Char(' '))); // None → sync
    drive_key(editor, key(KeyCode::Char(' '))); // sync → token
    drive_key(editor, key(KeyCode::Tab));
    drive_key(editor, key(KeyCode::Enter));
    apply_plain_source_picker_to_auth_form(editor);
    apply_plain_text_to_auth_form(editor, "ghp_round_trip");
    drive_key(editor, key(KeyCode::Enter));

    // Pull the persisted workspace and remount the editor from it
    // — this is the same shape `from_config` materialises after a
    // disk reload.
    let saved_ws = editor.pending.clone();
    let mut reloaded = EditorState::new_edit("proj".into(), saved_ws);
    reloaded.active_tab = crate::tui::state::EditorTab::Auth;
    reloaded.auth_selected_kind = Some(AuthKind::Github);
    let rows = reloaded.auth_flat_rows(&cfg);
    // WorkspaceMode + WorkspaceSource (token requires GH_TOKEN).
    assert!(
        rows.iter().any(|r| matches!(
            r,
            AuthRow::WorkspaceMode {
                kind: AuthKind::Github
            }
        )),
        "reload must surface WorkspaceMode for Github; got {rows:?}"
    );
    assert!(
        rows.iter().any(|r| matches!(
            r,
            AuthRow::WorkspaceSource {
                kind: AuthKind::Github
            }
        )),
        "reload must surface WorkspaceSource for Github (token mode requires GH_TOKEN); got {rows:?}"
    );
    let github_block = reloaded.pending.github.expect("github block must persist");
    assert_eq!(github_block.auth_forward, GithubAuthMode::Token);
    match github_block
        .env
        .get("GH_TOKEN")
        .expect("GH_TOKEN must persist on the github env block")
    {
        EnvValue::Plain(s) => assert_eq!(s, "ghp_round_trip"),
        EnvValue::Extended(value) => assert_eq!(value.value, "ghp_round_trip"),
        EnvValue::OpRef(_) => panic!("expected plain literal"),
    }
}

/// The role-override picker filters out any role that already has
/// a `[workspaces.<ws>.roles.<role>.github]` override — same "no
/// duplicate override" rule the Claude / Codex picker applies for
/// their respective kinds.
#[test]
fn github_role_override_picker_filters_already_overridden_roles() {
    let mut cfg = AppConfig::default();
    let mut ws = WorkspaceConfig {
        workdir: "/code/proj".into(),
        mounts: vec![MountConfig {
            src: "/code/proj".into(),
            dst: "/code/proj".into(),
            readonly: false,
            isolation: jackin_config::MountIsolation::Shared,
        }],
        allowed_roles: vec!["smith".into(), "brown".into()],
        ..Default::default()
    };
    ws.allowed_roles.sort();
    // Pre-seed an override on "brown" × Github so the picker
    // should filter it out and only offer "smith".
    ws.roles.insert(
        "brown".into(),
        WorkspaceRoleOverride {
            github: Some(GithubAuthConfig {
                auth_forward: GithubAuthMode::Ignore,
                ..Default::default()
            }),
            ..Default::default()
        },
    );
    cfg.workspaces.insert("proj".into(), ws.clone());
    for r in ["smith", "brown"] {
        cfg.roles.insert(
            r.into(),
            jackin_config::RoleSource {
                git: format!("https://example.com/{r}.git"),
                trusted: true,
                env: std::collections::BTreeMap::default(),
            },
        );
    }
    let cwd = PathBuf::from("/tmp");
    let mut state = ManagerState::from_config(&cfg, &cwd);
    let mut editor = EditorState::new_edit("proj".into(), ws);
    editor.active_tab = crate::tui::state::EditorTab::Auth;
    editor.auth_selected_kind = Some(AuthKind::Github);
    state.stage = ManagerStage::Editor(editor);
    let ManagerStage::Editor(editor) = &mut state.stage else {
        panic!()
    };
    open_auth_role_picker(editor, &cfg);
    let Some(Modal::AuthRolePicker { state: picker }) = editor.modal.as_ref() else {
        panic!("AuthRolePicker must be open; got {:?}", editor.modal);
    };
    // The picker exposes its candidate list as the `roles` field —
    // pull the keys and assert "brown" was filtered out before the
    // picker was even seeded.
    let labels: Vec<String> = picker
        .roles
        .iter()
        .map(jackin_core::RoleSelector::key)
        .collect();
    assert!(
        labels.iter().any(|s| s == "smith"),
        "smith must remain a candidate; got {labels:?}"
    );
    assert!(
        !labels.iter().any(|s| s == "brown"),
        "brown already has a github override and must be filtered out; got {labels:?}"
    );
}
