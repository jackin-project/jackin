#[cfg(test)]
use std::collections::BTreeMap;

use jackin_config::{
    AgentAuthConfig, AppConfig, AuthForwardMode, EnvValue, GlobalMountRow, MountConfig,
    RoleSource,
};
use jackin_tui::components::{ErrorPopupState, FocusOwner};

use crate::tui::components::footer_hints::{
    ModalAuthFormFooterState, ModalConfirmSaveFooterState, ModalFileBrowserFooterState,
    ModalFooterMode, ModalOpPickerFooterState,
};
use crate::tui::components::modal_rects::{
    ModalAuthFormState, ModalConfirmSaveState, ModalConfirmState, ModalOpPickerState,
    ModalRectMode, ModalRolePickerState,
};

use super::{
    GlobalMountsState, SettingsAfterEventOutcome, SettingsAuthRow, SettingsAuthState,
    SettingsEnvConfig, SettingsEnvRow, SettingsEnvScope, SettingsEnvState,
    SettingsGeneralSaveRefs, SettingsGeneralState, SettingsState, SettingsTrustRow,
    SettingsTrustState, settings_env_config_from_app_config,
    settings_trust_rows_from_app_config,
};

struct TestRolePicker(usize);

impl ModalRolePickerState for TestRolePicker {
    fn filtered_len(&self) -> usize {
        self.0
    }
}

struct TestConfirm;

fn minimal_settings_state()
-> SettingsState<(), (), SettingsAuthState<String, &'static str, ()>, (), ErrorPopupState, ()>
{
    SettingsState {
        active_tab: super::SettingsTab::General,
        focus_owner: FocusOwner::TabBar,
        hover_target: None,
        general: SettingsGeneralState::from_values(false, false),
        mounts: (),
        env: (),
        auth: SettingsAuthState::from_rows_and_github_env(Vec::new(), BTreeMap::new()),
        trust: (),
        error_popup: None,
        pending_token_generate: None,
        cached_footer_h: 1,
    }
}

#[test]
fn settings_error_popup_open_and_dismiss_live_on_state() {
    let mut state = minimal_settings_state();

    state.open_error_popup("Settings error", "bad value");
    assert!(state.error_popup.is_some());

    state.auth.modal = Some("child");
    state.auth.modal_parents.push("parent");
    state.dismiss_error_popup();

    assert!(state.error_popup.is_none());
    assert_eq!(state.auth.modal, Some("parent"));
}

impl ModalConfirmState for TestConfirm {
    fn width_pct(&self) -> u16 {
        42
    }

    fn required_height(&self) -> u16 {
        9
    }
}

struct TestConfirmSave;

impl ModalConfirmSaveState for TestConfirmSave {
    fn required_height(&self) -> u16 {
        12
    }
}

impl ModalConfirmSaveFooterState for TestConfirmSave {
    fn footer_mode(&self) -> ModalFooterMode {
        ModalFooterMode::ConfirmSave {
            scroll_axes: jackin_tui::components::ScrollAxes::none(),
        }
    }
}

struct TestOpPicker(bool);

impl ModalOpPickerState for TestOpPicker {
    fn has_naming_stage_input(&self) -> bool {
        self.0
    }
}

impl ModalOpPickerFooterState for TestOpPicker {
    fn footer_mode(&self, include_refresh: bool) -> ModalFooterMode {
        ModalFooterMode::FilteredPicker {
            include_refresh,
            include_collapse: false,
        }
    }
}

struct TestAuthForm;

impl ModalAuthFormState for TestAuthForm {
    fn required_height(&self) -> u16 {
        13
    }
}

impl ModalAuthFormFooterState<()> for TestAuthForm {
    fn footer_mode(&self, _focus: (), can_generate_token: bool) -> ModalFooterMode {
        ModalFooterMode::AuthForm {
            focus: super::AuthFormFocus::Mode,
            shows_source_folder: false,
            shows_credential_block: false,
            can_generate_token,
        }
    }
}

struct TestFileBrowser;

impl ModalFileBrowserFooterState for TestFileBrowser {
    fn footer_items(&self) -> Vec<jackin_tui::HintSpan<'static>> {
        vec![jackin_tui::HintSpan::Text("file")]
    }
}

fn empty_env_config<V>() -> SettingsEnvConfig<V> {
    SettingsEnvConfig {
        env: BTreeMap::new(),
        roles: BTreeMap::new(),
    }
}

#[test]
fn settings_env_config_from_app_config_copies_global_and_role_env() {
    let mut config = AppConfig::default();
    config
        .env
        .insert("GLOBAL".into(), EnvValue::Plain("1".into()));
    config.roles.insert(
        "alpha".into(),
        RoleSource {
            git: "https://example.invalid/alpha.git".into(),
            trusted: true,
            env: BTreeMap::from([("ROLE".into(), EnvValue::Plain("2".into()))]),
        },
    );

    let out = settings_env_config_from_app_config(&config);

    assert_eq!(out.env.get("GLOBAL"), Some(&EnvValue::Plain("1".into())));
    assert_eq!(
        out.roles.get("alpha").and_then(|role| role.get("ROLE")),
        Some(&EnvValue::Plain("2".into()))
    );
}

#[test]
fn settings_env_state_from_config_sets_original_and_pending() {
    let mut config = AppConfig::default();
    config.env.insert("KEY".into(), EnvValue::Plain("1".into()));

    let state = SettingsEnvState::<EnvValue, ()>::from_config(&config);

    assert_eq!(
        state.pending.env.get("KEY"),
        Some(&EnvValue::Plain("1".into()))
    );
    assert_eq!(state.original, state.pending);
    assert!(state.modal.is_none());
    assert_eq!(state.selected, 0);
}

#[test]
fn settings_env_pop_modal_chain_clears_pending_key_only_when_closed() {
    let mut state = SettingsEnvState::<String, i32>::from_pending(empty_env_config());
    state.modal = Some(2);
    state.modal_parents.push(1);
    state.pending_env_key = Some((SettingsEnvScope::Global, "KEY".into()));
    state.pending_picker_value = Some("value".into());

    state.pop_modal_chain_and_clear_pending_env_key_if_closed();

    assert_eq!(state.modal, Some(1));
    assert!(state.pending_env_key.is_some());
    assert!(state.pending_picker_value.is_some());

    state.pop_modal_chain_and_clear_pending_env_key_if_closed();

    assert!(state.modal.is_none());
    assert!(state.pending_env_key.is_none());
    assert!(state.pending_picker_value.is_none());
}

#[test]
fn settings_env_pop_modal_chain_can_clear_pending_key_immediately() {
    let mut state = SettingsEnvState::<String, i32>::from_pending(empty_env_config());
    state.modal = Some(2);
    state.modal_parents.push(1);
    state.pending_env_key = Some((SettingsEnvScope::Global, "KEY".into()));
    state.pending_picker_value = Some("value".into());

    state.pop_modal_chain_and_clear_pending_env_key();

    assert_eq!(state.modal, Some(1));
    assert!(state.pending_env_key.is_none());
    assert!(state.pending_picker_value.is_none());
}

#[test]
fn settings_env_pop_modal_chain_can_clear_picker_target_immediately() {
    let mut state = SettingsEnvState::<String, i32>::from_pending(empty_env_config());
    state.modal = Some(2);
    state.modal_parents.push(1);
    state.pending_picker_target = Some((SettingsEnvScope::Global, Some("KEY".into())));
    state.pending_picker_value = Some("value".into());

    state.pop_modal_chain_and_clear_picker_target();

    assert_eq!(state.modal, Some(1));
    assert!(state.pending_picker_target.is_none());
    assert!(state.pending_picker_value.is_none());
}

#[test]
fn settings_env_set_pending_picker_target_stores_scope_and_key() {
    let mut state = SettingsEnvState::<String, i32>::from_pending(empty_env_config());

    state.set_pending_picker_target((SettingsEnvScope::Global, Some("KEY".into())));

    assert_eq!(
        state.pending_picker_target,
        Some((SettingsEnvScope::Global, Some(String::from("KEY"))))
    );
}

#[test]
fn settings_env_set_pending_env_key_stores_scope_and_key() {
    let mut state = SettingsEnvState::<String, i32>::from_pending(empty_env_config());

    state.set_pending_env_key(SettingsEnvScope::Global, "KEY".into());

    assert_eq!(
        state.pending_env_key,
        Some((SettingsEnvScope::Global, String::from("KEY")))
    );
}

#[test]
fn settings_env_clear_pending_env_key_removes_scope_and_key() {
    let mut state = SettingsEnvState::<String, i32>::from_pending(empty_env_config());
    state.set_pending_env_key(SettingsEnvScope::Global, "KEY".into());

    state.clear_pending_env_key();

    assert!(state.pending_env_key.is_none());
}

#[test]
fn settings_env_clear_pending_picker_target_removes_scope_and_key() {
    let mut state = SettingsEnvState::<String, i32>::from_pending(empty_env_config());
    state.set_pending_picker_target((SettingsEnvScope::Global, Some("KEY".into())));

    state.clear_pending_picker_target();

    assert!(state.pending_picker_target.is_none());
}

#[test]
fn settings_env_stash_pending_picker_value_stores_value() {
    let mut state = SettingsEnvState::<String, i32>::from_pending(empty_env_config());

    state.stash_pending_picker_value("value".into());

    assert_eq!(state.pending_picker_value, Some(String::from("value")));
}

#[test]
fn settings_env_take_pending_picker_value_moves_value() {
    let mut state = SettingsEnvState::<String, i32>::from_pending(empty_env_config());
    state.stash_pending_picker_value("value".into());

    assert!(state.has_pending_picker_value());
    assert_eq!(
        state.take_pending_picker_value(),
        Some(String::from("value"))
    );
    assert!(!state.has_pending_picker_value());
}

#[test]
fn settings_env_set_value_updates_global_env() {
    let mut state = SettingsEnvState::<String, i32>::from_pending(empty_env_config());

    state.set_value(&SettingsEnvScope::Global, "KEY", "value".into());

    assert_eq!(state.pending.env.get("KEY"), Some(&String::from("value")));
}

#[test]
fn settings_env_expand_role_tracks_role() {
    let mut state = SettingsEnvState::<String, i32>::from_pending(empty_env_config());

    state.expand_role("default".into());

    assert!(state.expanded.contains("default"));
}

#[test]
fn settings_env_set_and_take_error_moves_error_message() {
    let mut state = SettingsEnvState::<String, i32>::from_pending(empty_env_config());

    state.set_error("missing role");

    assert_eq!(state.take_error(), Some(String::from("missing role")));
    assert!(state.take_error().is_none());
}

#[test]
fn settings_env_remove_selected_row_deletes_key_and_clamps_selection() {
    let mut config = SettingsEnvConfig {
        env: BTreeMap::from([
            ("A".into(), String::from("1")),
            ("B".into(), String::from("2")),
        ]),
        roles: BTreeMap::new(),
    };
    let mut state = SettingsEnvState::<String, i32>::from_pending(config.clone());
    let rows =
        crate::tui::screens::settings::update::settings_env_flat_rows(&config, &state.expanded);
    state.selected = rows
        .iter()
        .position(|row| {
            matches!(
                row,
                SettingsEnvRow::Key {
                    scope: SettingsEnvScope::Global,
                    key,
                } if key == "B"
            )
        })
        .expect("B row should exist");

    assert!(state.remove_selected_row());

    config.env.remove("B");
    assert_eq!(state.pending, config);
    assert!(state.selected < state.pending.env.len() + 1);
}

#[test]
fn settings_trust_rows_from_app_config_copies_role_trust_facts() {
    let mut config = AppConfig::default();
    config.roles.insert(
        "alpha".into(),
        RoleSource {
            git: "https://example.invalid/alpha.git".into(),
            trusted: true,
            env: BTreeMap::new(),
        },
    );

    let rows = settings_trust_rows_from_app_config(&config);

    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].role, "alpha");
    assert_eq!(rows[0].git, "https://example.invalid/alpha.git");
    assert!(rows[0].trusted);
}

#[test]
fn settings_trust_state_from_config_sets_original_and_pending() {
    let mut config = AppConfig::default();
    config.roles.insert(
        "alpha".into(),
        RoleSource {
            git: "https://example.invalid/alpha.git".into(),
            trusted: true,
            env: BTreeMap::new(),
        },
    );

    let state = SettingsTrustState::from_config(&config);

    assert_eq!(state.pending, state.original);
    assert_eq!(state.pending[0].role, "alpha");
    assert!(state.error.is_none());
}

#[test]
fn settings_trust_set_and_take_error_moves_error_message() {
    let mut state = SettingsTrustState::from_rows(Vec::new());

    state.set_error("trust failed");

    assert_eq!(state.take_error(), Some(String::from("trust failed")));
    assert!(state.take_error().is_none());
}

#[test]
fn global_mounts_state_from_rows_sets_original_and_pending() {
    let state = GlobalMountsState::<String, ()>::from_rows(vec!["one".into()]);

    assert_eq!(state.selected, 0);
    assert_eq!(state.pending, vec![String::from("one")]);
    assert_eq!(state.original, vec![String::from("one")]);
    assert!(state.modal.is_none());
    assert!(!state.exit_requested);
}

#[test]
fn global_mounts_start_add_draft_resets_draft_and_parent_chain() {
    let mut state = GlobalMountsState::<String, i32>::from_rows(Vec::new());
    state.modal = Some(2);
    state.modal_parents.push(1);
    state.add_draft = None;

    state.start_add_draft();

    assert!(state.add_draft.is_some());
    assert!(state.modal_parents.is_empty());
    assert_eq!(state.modal, Some(2));
}

#[test]
fn global_mounts_remove_row_and_select_updates_pending_and_selection() {
    let mut state =
        GlobalMountsState::<String, i32>::from_rows(vec!["one".into(), "two".into()]);

    state.remove_row_and_select(0, 0);

    assert_eq!(state.pending, vec![String::from("two")]);
    assert_eq!(state.selected, 0);
}

#[test]
fn global_mounts_set_and_take_error_moves_error_message() {
    let mut state = GlobalMountsState::<String, i32>::from_rows(Vec::new());

    state.set_error("missing mount");

    assert_eq!(state.take_error(), Some(String::from("missing mount")));
    assert!(state.take_error().is_none());
}

#[test]
fn global_mounts_request_and_take_exit_flag() {
    let mut state = GlobalMountsState::<String, i32>::from_rows(Vec::new());

    state.request_exit();

    assert!(state.take_exit_requested());
    assert!(!state.take_exit_requested());
}

#[test]
fn global_mounts_add_row_and_close_updates_pending_selection_and_modal() {
    let mut state = GlobalMountsState::<GlobalMountRow, i32>::from_rows(Vec::new());
    state.modal = Some(1);
    state.modal_parents.push(0);
    let row = GlobalMountRow {
        scope: None,
        name: "cache".into(),
        mount: MountConfig {
            src: "/tmp/cache".into(),
            dst: "/home/agent/.cache".into(),
            readonly: false,
            isolation: jackin_core::isolation::MountIsolation::Shared,
        },
    };

    state.add_row_and_close(row, 0);

    assert_eq!(state.pending.len(), 1);
    assert_eq!(state.selected, 0);
    assert!(state.modal.is_none());
    assert!(state.modal_parents.is_empty());
}

#[test]
fn global_mounts_toggle_selected_readonly_updates_selected_row() {
    let mut state = GlobalMountsState::<GlobalMountRow, i32>::from_rows(vec![
        GlobalMountRow {
            scope: None,
            name: "cache".into(),
            mount: MountConfig {
                src: "/tmp/cache".into(),
                dst: "/home/agent/.cache".into(),
                readonly: false,
                isolation: jackin_core::isolation::MountIsolation::Shared,
            },
        },
        GlobalMountRow {
            scope: None,
            name: "logs".into(),
            mount: MountConfig {
                src: "/tmp/logs".into(),
                dst: "/home/agent/logs".into(),
                readonly: true,
                isolation: jackin_core::isolation::MountIsolation::Shared,
            },
        },
    ]);
    state.selected = 1;

    state.toggle_selected_readonly();

    assert!(!state.pending[1].mount.readonly);
    assert!(!state.pending[0].mount.readonly);
}

#[test]
fn global_mounts_pop_modal_chain_preserves_add_draft_when_parent_remains() {
    let mut state = GlobalMountsState::<String, i32>::from_rows(Vec::new());
    state.modal = Some(2);
    state.modal_parents.push(1);
    state.add_draft = Some(super::GlobalMountDraft::default());

    state.pop_modal_chain_and_clear_add_draft_if_closed();

    assert_eq!(state.modal, Some(1));
    assert!(state.add_draft.is_some());
}

#[test]
fn global_mounts_pop_modal_chain_clears_add_draft_when_closed() {
    let mut state = GlobalMountsState::<String, i32>::from_rows(Vec::new());
    state.modal = Some(1);
    state.add_draft = Some(super::GlobalMountDraft::default());

    state.pop_modal_chain_and_clear_add_draft_if_closed();

    assert_eq!(state.modal, None);
    assert!(state.add_draft.is_none());
}

#[test]
fn global_mount_modal_reports_debug_kind() {
    type TestModal = super::GlobalMountModal<(), (), (), (), (), (), ()>;

    let modal = TestModal::Confirm {
        action: super::GlobalMountConfirm::Sensitive,
        state: (),
    };

    assert_eq!(
        modal.debug_kind(),
        crate::tui::debug::SettingsMountModalDebugKind::ConfirmSensitive
    );
}

#[test]
fn settings_env_modal_reports_scroll_target() {
    type TestModal =
        super::SettingsEnvModal<(), (), TestOpPicker, TestRolePicker, (), TestConfirm>;

    assert_eq!(
        TestModal::OpPicker {
            state: Box::new(TestOpPicker(false)),
        }
        .scroll_target(),
        crate::tui::update::SettingsEnvModalScrollTarget::OpPicker
    );
    assert_eq!(
        TestModal::RolePicker {
            state: TestRolePicker(7),
        }
        .scroll_target(),
        crate::tui::update::SettingsEnvModalScrollTarget::RolePicker
    );
    assert_eq!(
        TestModal::SourcePicker { state: () }.scroll_target(),
        crate::tui::update::SettingsEnvModalScrollTarget::None
    );
}

#[test]
fn global_mount_modal_reports_scroll_target() {
    type TestModal =
        super::GlobalMountModal<(), (), (), (), TestRolePicker, TestConfirm, TestConfirmSave>;

    assert_eq!(
        TestModal::RolePicker {
            state: TestRolePicker(7),
        }
        .scroll_target(),
        crate::tui::update::GlobalMountModalScrollTarget::RolePicker
    );
    assert_eq!(
        TestModal::ScopePicker { state: () }.scroll_target(),
        crate::tui::update::GlobalMountModalScrollTarget::None
    );
}

#[test]
fn global_mount_modal_reports_letter_input_kind() {
    type TestModal =
        super::GlobalMountModal<(), (), (), (), TestRolePicker, TestConfirm, TestConfirmSave>;

    assert_eq!(
        TestModal::Text {
            target: super::GlobalMountTextTarget::AddName,
            state: Box::new(()),
        }
        .letter_input_kind(),
        Some(crate::tui::run::LetterInputModalKind::TextInput)
    );
    assert_eq!(
        TestModal::RolePicker {
            state: TestRolePicker(7),
        }
        .letter_input_kind(),
        Some(crate::tui::run::LetterInputModalKind::Other)
    );
}

#[test]
fn settings_auth_modal_reports_scroll_target() {
    type TestModal = super::SettingsAuthModal<(), (), TestOpPicker, (), (), TestAuthForm, ()>;

    assert_eq!(
        TestModal::OpPicker {
            state: Box::new(TestOpPicker(false)),
        }
        .scroll_target(),
        crate::tui::update::SettingsAuthModalScrollTarget::OpPicker
    );
    assert_eq!(
        TestModal::SourcePicker { state: () }.scroll_target(),
        crate::tui::update::SettingsAuthModalScrollTarget::None
    );
}

#[test]
fn settings_env_modal_reports_rect_mode() {
    type TestModal =
        super::SettingsEnvModal<(), (), TestOpPicker, TestRolePicker, (), TestConfirm>;

    let modal = TestModal::RolePicker {
        state: TestRolePicker(7),
    };

    assert_eq!(
        modal.rect_mode(),
        ModalRectMode::RolePicker { filtered_len: 7 }
    );
}

#[test]
fn settings_env_op_naming_modal_uses_text_input_rect_mode() {
    type TestModal =
        super::SettingsEnvModal<(), (), TestOpPicker, TestRolePicker, (), TestConfirm>;

    let modal = TestModal::OpPicker {
        state: Box::new(TestOpPicker(true)),
    };

    assert_eq!(modal.rect_mode(), ModalRectMode::TextInput);
}

#[test]
fn global_mount_modal_reports_rect_mode() {
    type TestModal =
        super::GlobalMountModal<(), (), (), (), TestRolePicker, TestConfirm, TestConfirmSave>;

    let modal = TestModal::PreviewSave {
        state: TestConfirmSave,
    };

    assert_eq!(
        modal.rect_mode(),
        ModalRectMode::ConfirmSave {
            required_height: 12
        }
    );
}

#[test]
fn settings_auth_modal_reports_rect_mode() {
    type TestModal = super::SettingsAuthModal<(), (), TestOpPicker, (), (), TestAuthForm, ()>;

    let modal = TestModal::AuthForm {
        target: (),
        state: Box::new(TestAuthForm),
        focus: (),
        literal_buffer: String::new(),
    };

    assert_eq!(
        modal.rect_mode(),
        ModalRectMode::AuthForm {
            required_height: 13
        }
    );
}

#[test]
fn settings_modals_report_footer_items() {
    type EnvModal =
        super::SettingsEnvModal<(), (), TestOpPicker, TestRolePicker, (), TestConfirm>;
    type MountModal = super::GlobalMountModal<
        (),
        TestFileBrowser,
        (),
        (),
        TestRolePicker,
        TestConfirm,
        TestConfirmSave,
    >;
    type AuthModal =
        super::SettingsAuthModal<(), (), TestOpPicker, TestFileBrowser, (), TestAuthForm, ()>;

    assert!(
        EnvModal::RolePicker {
            state: TestRolePicker(3),
        }
        .footer_items()
        .contains(&jackin_tui::HintSpan::Text("filter"))
    );

    assert!(
        MountModal::PreviewSave {
            state: TestConfirmSave,
        }
        .footer_items()
        .contains(&jackin_tui::HintSpan::Text("save"))
    );

    assert!(
        AuthModal::AuthForm {
            target: (),
            state: Box::new(TestAuthForm),
            focus: (),
            literal_buffer: String::new(),
        }
        .footer_items(true)
        .contains(&jackin_tui::HintSpan::Text("generate"))
    );
}

#[test]
fn settings_auth_state_from_rows_and_github_env_sets_originals() {
    let rows = vec![SettingsAuthRow {
        kind: crate::tui::auth::AuthKind::Github,
        mode: crate::tui::auth::AuthMode::Token,
        sync_source_dir: None,
    }];
    let github_env = BTreeMap::from([("GH_TOKEN".into(), EnvValue::Plain("token".into()))]);

    let state = SettingsAuthState::<EnvValue, (), ()>::from_rows_and_github_env(
        rows.clone(),
        github_env.clone(),
    );

    assert_eq!(state.selected, 0);
    assert_eq!(state.pending, rows);
    let rows = state.pending.clone();
    assert_eq!(state.original, rows);
    assert_eq!(state.github_env, github_env);
    assert_eq!(state.original_github_env, github_env);
    assert!(state.modal.is_none());
}

#[test]
fn settings_auth_state_reports_selected_detail_focusability() {
    let rows = vec![SettingsAuthRow {
        kind: crate::tui::auth::AuthKind::Github,
        mode: crate::tui::auth::AuthMode::Token,
        sync_source_dir: None,
    }];
    let mut state =
        SettingsAuthState::<EnvValue, (), ()>::from_rows_and_github_env(rows, BTreeMap::new());

    assert!(state.selected_detail_row_is_focusable());

    state.selected_kind = Some(crate::tui::auth::AuthKind::Github);
    state.selected = 1;

    assert!(!state.selected_detail_row_is_focusable());
}

#[test]
fn settings_auth_set_and_take_error_moves_error_message() {
    let mut state = SettingsAuthState::<EnvValue, (), ()>::from_rows_and_github_env(
        Vec::new(),
        BTreeMap::new(),
    );

    state.set_error("auth failed");

    assert_eq!(state.take_error(), Some(String::from("auth failed")));
    assert!(state.take_error().is_none());
}

#[test]
fn settings_auth_token_generation_flag_can_start_and_finish() {
    let mut state = SettingsAuthState::<EnvValue, (), ()>::from_rows_and_github_env(
        Vec::new(),
        BTreeMap::new(),
    );

    assert!(!state.is_generating_token());

    state.start_generating_token();
    assert!(state.is_generating_token());

    state.finish_generating_token();
    assert!(!state.is_generating_token());
}

#[test]
fn settings_auth_clamp_selected_row_uses_current_row_count() {
    let rows = vec![SettingsAuthRow {
        kind: crate::tui::auth::AuthKind::Github,
        mode: crate::tui::auth::AuthMode::Token,
        sync_source_dir: None,
    }];
    let mut state =
        SettingsAuthState::<EnvValue, (), ()>::from_rows_and_github_env(rows, BTreeMap::new());
    state.selected = 10;

    state.clamp_selected_row();

    assert_eq!(state.selected, 0);
}

#[test]
fn settings_auth_child_modal_stack_round_trips_parent() {
    let mut state = SettingsAuthState::<EnvValue, i32, ()>::from_rows_and_github_env(
        Vec::new(),
        BTreeMap::new(),
    );

    state.open_child_modal(1, 2);

    assert_eq!(state.modal, Some(2));
    assert_eq!(state.pop_parent_modal(), Some(1));
    assert_eq!(state.pop_parent_modal(), None);
}

#[test]
fn settings_auth_modal_slot_methods_round_trip() {
    let mut state = SettingsAuthState::<EnvValue, i32, ()>::from_rows_and_github_env(
        Vec::new(),
        BTreeMap::new(),
    );

    assert!(!state.has_modal());

    state.set_modal(7);

    assert!(state.has_modal());
    assert_eq!(state.modal_ref(), Some(&7));
    assert_eq!(state.modal_mut().map(|modal| *modal), Some(7));
    assert_eq!(state.take_modal(), Some(7));
    assert!(!state.has_modal());

    state.set_modal(9);
    state.clear_modal();

    assert!(!state.has_modal());
}

#[test]
fn settings_auth_enter_and_clear_selected_kind_update_selection() {
    let rows = vec![SettingsAuthRow {
        kind: crate::tui::auth::AuthKind::Github,
        mode: crate::tui::auth::AuthMode::Sync,
        sync_source_dir: None,
    }];
    let mut state =
        SettingsAuthState::<EnvValue, (), ()>::from_rows_and_github_env(rows, BTreeMap::new());

    state.enter_selected_kind();

    assert_eq!(
        state.selected_kind,
        Some(crate::tui::auth::AuthKind::Github)
    );
    assert_eq!(state.selected, 0);

    state.clear_selected_kind();

    assert_eq!(state.selected_kind, None);
    assert_eq!(state.selected, 0);
}

#[test]
fn settings_auth_selected_kind_and_scroll_accessors_reflect_state() {
    let rows = vec![SettingsAuthRow {
        kind: crate::tui::auth::AuthKind::Github,
        mode: crate::tui::auth::AuthMode::Sync,
        sync_source_dir: None,
    }];
    let mut state =
        SettingsAuthState::<EnvValue, (), ()>::from_rows_and_github_env(rows, BTreeMap::new());

    assert_eq!(state.selected_kind(), None);
    assert!(!state.has_selected_kind());

    state.enter_selected_kind();
    *state.scroll_y_mut() = 3;

    assert_eq!(
        state.selected_kind(),
        Some(crate::tui::auth::AuthKind::Github)
    );
    assert!(state.has_selected_kind());
    assert_eq!(state.scroll_y, 3);
}

#[test]
fn settings_auth_save_refs_expose_persisted_inputs() {
    let rows = vec![SettingsAuthRow {
        kind: crate::tui::auth::AuthKind::Github,
        mode: crate::tui::auth::AuthMode::Sync,
        sync_source_dir: None,
    }];
    let state = SettingsAuthState::<EnvValue, (), ()>::from_rows_and_github_env(
        rows,
        BTreeMap::from([(String::from("GH_TOKEN"), "token".into())]),
    );

    let refs = state.save_refs();

    assert_eq!(refs.pending.len(), 1);
    assert_eq!(
        refs.original_github_env.get("GH_TOKEN"),
        Some(&EnvValue::from("token"))
    );
    assert_eq!(
        refs.github_env.get("GH_TOKEN"),
        Some(&EnvValue::from("token"))
    );
}

#[test]
fn settings_subpanel_save_refs_expose_persisted_inputs() {
    let mounts = GlobalMountsState::<i32, ()>::from_rows(vec![1, 2]);
    let env = SettingsEnvState::<EnvValue, ()>::from_pending(SettingsEnvConfig {
        env: BTreeMap::from([(String::from("KEY"), "value".into())]),
        roles: BTreeMap::new(),
    });
    let trust = SettingsTrustState::from_rows(vec![SettingsTrustRow {
        role: String::from("role"),
        git: String::from("https://example.test/repo.git"),
        trusted: true,
    }]);
    let general = SettingsGeneralState::from_values(true, false);

    let mount_refs = mounts.save_refs();
    let env_refs = env.save_refs();
    let trust_refs = trust.save_refs();
    let general_refs = general.save_refs();

    assert_eq!(mount_refs.original, &[1, 2]);
    assert_eq!(mount_refs.pending, &[1, 2]);
    assert_eq!(
        env_refs.original.env.get("KEY"),
        Some(&EnvValue::from("value"))
    );
    assert_eq!(
        env_refs.pending.env.get("KEY"),
        Some(&EnvValue::from("value"))
    );
    assert_eq!(trust_refs.pending[0].role, "role");
    assert_eq!(
        general_refs,
        SettingsGeneralSaveRefs {
            git_coauthor_trailer: true,
            git_dco: false,
        }
    );
}

#[test]
fn settings_state_applies_tab_move_plan() {
    type TestState = SettingsState<
        GlobalMountsState<GlobalMountRow, ()>,
        SettingsEnvState<EnvValue, ()>,
        SettingsAuthState<EnvValue, (), ()>,
        SettingsTrustState,
        (),
        (),
    >;
    let mut state = TestState::from_config(&AppConfig::default());

    state.apply_tab_move_plan(crate::tui::screens::settings::update::SettingsTabMovePlan {
        active_tab: super::SettingsTab::Trust,
        tab_bar_focused: false,
    });

    assert_eq!(state.active_tab, super::SettingsTab::Trust);
    assert!(!state.tab_bar_focused());

    state.apply_tab_bar_focus_plan(true);
    assert!(state.tab_bar_focused());
}

#[test]
fn settings_state_applies_scroll_focus_plan() {
    type TestState = SettingsState<
        GlobalMountsState<GlobalMountRow, ()>,
        SettingsEnvState<EnvValue, ()>,
        SettingsAuthState<EnvValue, (), ()>,
        SettingsTrustState,
        (),
        (),
    >;
    let mut state = TestState::from_config(&AppConfig::default());

    state.apply_scroll_focus_plan(
        crate::tui::screens::settings::update::SettingsScrollFocusPlan {
            mounts: false,
            env: true,
            auth: false,
            trust: false,
        },
    );

    assert!(state.content_focused(super::SettingsTab::Environments));
}

#[test]
fn settings_state_applies_trust_row_select_plan_and_focus() {
    type TestState = SettingsState<
        GlobalMountsState<GlobalMountRow, ()>,
        SettingsEnvState<EnvValue, ()>,
        SettingsAuthState<EnvValue, (), ()>,
        SettingsTrustState,
        (),
        (),
    >;
    let mut state = TestState::from_config(&AppConfig::default());
    state.set_tab_bar_focused(true);

    state.apply_trust_row_select_plan(
        crate::tui::screens::settings::update::SettingsTrustRowSelectPlan {
            selected: Some(2),
            content_focused: true,
        },
    );

    assert_eq!(state.trust.selected, 2);
    assert!(state.content_focused(super::SettingsTab::Trust));

    state.set_hover_target(Some(super::SettingsHoverTarget::TrustRow(1)));
    assert_eq!(state.hovered_trust_row(), Some(1));
}

#[test]
fn settings_general_state_moves_and_toggles_selection() {
    let mut state = SettingsGeneralState::from_values(false, false);

    state.move_selection(1);
    state.toggle_selected();

    assert_eq!(state.selected, 1);
    assert!(state.pending_dco);

    state.move_selection(-1);
    state.toggle_selected();

    assert_eq!(state.selected, 0);
    assert!(state.pending_coauthor_trailer);
}

#[test]
fn settings_env_state_updates_role_expansion() {
    let mut state = SettingsEnvState::<EnvValue, ()>::from_pending(empty_env_config());

    state.set_role_expanded(String::from("alpha"), true);
    assert!(state.expanded.contains("alpha"));

    state.set_role_expanded(String::from("alpha"), false);
    assert!(!state.expanded.contains("alpha"));
}

#[test]
fn settings_subpanels_apply_selection_plans() {
    let mut mounts = GlobalMountsState::<String, ()>::from_rows(Vec::new());
    let mut env = SettingsEnvState::<EnvValue, ()>::from_pending(empty_env_config());
    let mut trust = SettingsTrustState::from_rows(Vec::new());

    mounts.apply_selection_plan(
        crate::tui::screens::settings::update::SettingsSelectionScrollPlan {
            selected: 2,
            scroll_y: 3,
        },
    );
    env.apply_selection_plan(
        crate::tui::screens::settings::update::SettingsSelectionScrollPlan {
            selected: 4,
            scroll_y: 5,
        },
    );
    trust.apply_selection_plan(
        crate::tui::screens::settings::update::SettingsSelectionScrollPlan {
            selected: 6,
            scroll_y: 7,
        },
    );

    assert_eq!((mounts.selected, mounts.scroll_y), (2, 3));
    assert_eq!((env.selected, env.scroll_y), (4, 5));
    assert_eq!((trust.selected, trust.scroll_y), (6, 7));
}

#[test]
fn settings_subpanels_apply_scroll_and_trust_row_plans() {
    let mut mounts = GlobalMountsState::<String, ()>::from_rows(Vec::new());
    let mut trust = SettingsTrustState::from_rows(Vec::new());

    mounts.apply_horizontal_scroll(8);
    trust.apply_horizontal_scroll(13);
    let content_focused = trust.apply_row_select_plan(
        crate::tui::screens::settings::update::SettingsTrustRowSelectPlan {
            selected: Some(3),
            content_focused: true,
        },
    );

    assert_eq!(mounts.scroll_x, 8);
    assert_eq!(trust.scroll_x, 13);
    assert_eq!(trust.selected, 3);
    assert!(content_focused);
}

#[test]
fn settings_trust_state_toggles_selected_row() {
    let mut state = SettingsTrustState::from_rows(vec![SettingsTrustRow {
        role: String::from("role"),
        git: String::from("https://example.test/repo.git"),
        trusted: false,
    }]);

    state.toggle_selected();

    assert!(state.pending[0].trusted);
}

#[test]
fn settings_auth_move_selection_uses_current_rows() {
    let rows = vec![
        SettingsAuthRow {
            kind: crate::tui::auth::AuthKind::Github,
            mode: crate::tui::auth::AuthMode::Sync,
            sync_source_dir: None,
        },
        SettingsAuthRow {
            kind: crate::tui::auth::AuthKind::Claude,
            mode: crate::tui::auth::AuthMode::Sync,
            sync_source_dir: None,
        },
    ];
    let mut state =
        SettingsAuthState::<EnvValue, (), ()>::from_rows_and_github_env(rows, BTreeMap::new());

    state.move_selection(1);

    assert_eq!(state.selected, 1);
}

#[test]
fn settings_auth_pending_op_commit_round_trips() {
    let mut state = SettingsAuthState::<EnvValue, (), i32>::from_rows_and_github_env(
        Vec::new(),
        BTreeMap::new(),
    );

    state.set_pending_op_commit(7);

    assert_eq!(state.pending_op_commit_mut().copied(), Some(7));
    assert_eq!(state.take_pending_op_commit(), Some(7));
    assert_eq!(state.take_pending_op_commit(), None);
}

#[test]
fn settings_auth_open_selected_modal_supplies_row_and_credential() {
    let rows = vec![SettingsAuthRow {
        kind: crate::tui::auth::AuthKind::Claude,
        mode: crate::tui::auth::AuthMode::ApiKey,
        sync_source_dir: None,
    }];
    let mut state =
        SettingsAuthState::<EnvValue, (), ()>::from_rows_and_github_env(rows, BTreeMap::new());
    state.selected_kind = Some(crate::tui::auth::AuthKind::Claude);
    let agent_env = BTreeMap::from([(
        String::from(jackin_core::env_model::ANTHROPIC_API_KEY_ENV_NAME),
        "secret".into(),
    )]);

    state.open_selected_auth_modal(&agent_env, |kind, row, existing| {
        assert_eq!(kind, crate::tui::auth::AuthKind::Claude);
        assert_eq!(row.mode, crate::tui::auth::AuthMode::ApiKey);
        assert_eq!(existing, Some(EnvValue::from("secret")));
    });

    assert_eq!(state.modal, Some(()));
}

#[test]
fn settings_auth_apply_outcome_updates_row_and_env() {
    let rows = vec![SettingsAuthRow {
        kind: crate::tui::auth::AuthKind::Claude,
        mode: crate::tui::auth::AuthMode::Sync,
        sync_source_dir: None,
    }];
    let mut state =
        SettingsAuthState::<EnvValue, (), ()>::from_rows_and_github_env(rows, BTreeMap::new());
    let mut agent_env = BTreeMap::new();

    state.apply_auth_outcome(
        crate::tui::auth::AuthKind::Claude,
        crate::tui::components::auth_panel::AuthFormOutcome {
            mode: crate::tui::auth::AuthMode::ApiKey,
            env_var_name: Some(jackin_core::env_model::ANTHROPIC_API_KEY_ENV_NAME),
            env_value: Some("secret".into()),
            source_folder: None,
        },
        &mut agent_env,
    );

    assert_eq!(state.pending[0].mode, crate::tui::auth::AuthMode::ApiKey);
    assert_eq!(
        agent_env.get(jackin_core::env_model::ANTHROPIC_API_KEY_ENV_NAME),
        Some(&EnvValue::from("secret"))
    );
}

#[test]
fn settings_auth_clear_kind_resets_row_and_env() {
    let rows = vec![SettingsAuthRow {
        kind: crate::tui::auth::AuthKind::Claude,
        mode: crate::tui::auth::AuthMode::ApiKey,
        sync_source_dir: Some(std::path::PathBuf::from("/tmp/auth")),
    }];
    let mut state =
        SettingsAuthState::<EnvValue, (), ()>::from_rows_and_github_env(rows, BTreeMap::new());
    let mut agent_env = BTreeMap::from([(
        String::from(jackin_core::env_model::ANTHROPIC_API_KEY_ENV_NAME),
        "secret".into(),
    )]);

    state.clear_auth_kind(crate::tui::auth::AuthKind::Claude, &mut agent_env);

    assert_eq!(state.pending[0].mode, crate::tui::auth::AuthMode::Sync);
    assert_eq!(state.pending[0].sync_source_dir, None);
    assert!(!agent_env.contains_key(jackin_core::env_model::ANTHROPIC_API_KEY_ENV_NAME));
}

#[test]
fn settings_auth_state_from_config_sets_rows_and_originals() {
    let config = AppConfig {
        claude: Some(AgentAuthConfig {
            auth_forward: AuthForwardMode::ApiKey,
            ..Default::default()
        }),
        ..Default::default()
    };

    let state = SettingsAuthState::<EnvValue, (), ()>::from_config(&config);

    assert_eq!(state.pending, state.original);
    assert_eq!(state.github_env, state.original_github_env);
    assert!(
        state
            .pending
            .iter()
            .any(|row| row.kind == crate::tui::auth::AuthKind::Claude
                && row.mode == crate::tui::auth::AuthMode::ApiKey)
    );
}

#[test]
fn settings_state_clears_ignored_env_only_auth_keys() {
    let env: SettingsEnvState<EnvValue, ()> = SettingsEnvState {
        selected: 0,
        pending: SettingsEnvConfig {
            env: BTreeMap::from([(
                jackin_core::env_model::ZAI_API_KEY_ENV_NAME.to_owned(),
                EnvValue::Plain("zai".into()),
            )]),
            roles: BTreeMap::new(),
        },
        original: SettingsEnvConfig {
            env: BTreeMap::new(),
            roles: BTreeMap::new(),
        },
        modal: None,
        modal_parents: Vec::new(),
        pending_env_key: None,
        pending_picker_target: None,
        pending_picker_value: None,
        unmasked_rows: std::collections::BTreeSet::default(),
        expanded: std::collections::BTreeSet::default(),
        error: None,
        scroll_y: 0,
    };
    let mut state = SettingsState {
        active_tab: super::SettingsTab::General,
        focus_owner: FocusOwner::TabBar,
        hover_target: None,
        general: SettingsGeneralState::from_values(false, false),
        mounts: GlobalMountsState::<String, ()>::from_rows(Vec::new()),
        env,
        auth: SettingsAuthState::<EnvValue, (), ()>::from_rows_and_github_env(
            vec![SettingsAuthRow {
                kind: crate::tui::auth::AuthKind::Zai,
                mode: crate::tui::auth::AuthMode::Ignore,
                sync_source_dir: None,
            }],
            BTreeMap::new(),
        ),
        trust: SettingsTrustState::from_rows(Vec::new()),
        error_popup: None::<()>,
        pending_token_generate: None::<()>,
        cached_footer_h: 1,
    };

    state.clear_ignored_env_only_auth_keys();

    assert!(
        !state
            .env
            .pending
            .env
            .contains_key(jackin_core::env_model::ZAI_API_KEY_ENV_NAME)
    );
}

#[test]
fn settings_state_from_config_builds_all_panels_clean() {
    let mut config = AppConfig::default();
    config.git.dco = true;
    config.env.insert("KEY".into(), EnvValue::Plain("1".into()));

    type TestState = SettingsState<
        GlobalMountsState<GlobalMountRow, ()>,
        SettingsEnvState<EnvValue, ()>,
        SettingsAuthState<EnvValue, (), ()>,
        SettingsTrustState,
        (),
        (),
    >;

    let state = TestState::from_config(&config);

    assert!(state.general.pending_dco);
    assert_eq!(
        state.env.pending.env.get("KEY"),
        Some(&EnvValue::Plain("1".into()))
    );
    assert!(!state.is_dirty());
    assert_eq!(state.change_count(), 0);
}

#[test]
fn settings_state_env_flat_rows_reads_pending_env() {
    type TestState = SettingsState<
        GlobalMountsState<GlobalMountRow, ()>,
        SettingsEnvState<EnvValue, ()>,
        SettingsAuthState<EnvValue, (), ()>,
        SettingsTrustState,
        (),
        (),
    >;
    let mut state = TestState::from_config(&AppConfig::default());
    state
        .env
        .pending
        .env
        .insert("KEY".into(), EnvValue::Plain("1".into()));

    assert!(state.env_flat_rows().iter().any(|row| matches!(
        row,
        SettingsEnvRow::Key {
            scope: SettingsEnvScope::Global,
            key
        } if key == "KEY"
    )));
}

#[test]
fn settings_state_after_event_outcome_drains_error_and_exit() {
    type TestState = SettingsState<
        GlobalMountsState<GlobalMountRow, ()>,
        SettingsEnvState<EnvValue, ()>,
        SettingsAuthState<EnvValue, (), ()>,
        SettingsTrustState,
        (),
        (),
    >;
    let mut state = TestState::from_config(&AppConfig::default());
    state.mounts.set_error("bad mount");
    state.env.set_error("bad env");
    state.auth.set_error("bad auth");
    state.trust.set_error("bad trust");
    state.mounts.request_exit();

    assert_eq!(
        state.take_after_event_outcome(),
        SettingsAfterEventOutcome {
            exit_requested: true,
            error: Some("bad mount".into()),
        }
    );
    assert_eq!(
        state.take_after_event_outcome(),
        SettingsAfterEventOutcome {
            exit_requested: false,
            error: Some("bad env".into()),
        }
    );
    assert_eq!(
        state.take_after_event_outcome(),
        SettingsAfterEventOutcome {
            exit_requested: false,
            error: Some("bad auth".into()),
        }
    );
    assert_eq!(
        state.take_after_event_outcome(),
        SettingsAfterEventOutcome {
            exit_requested: false,
            error: Some("bad trust".into()),
        }
    );
    assert_eq!(
        state.take_after_event_outcome(),
        SettingsAfterEventOutcome {
            exit_requested: false,
            error: None,
        }
    );
}

#[test]
fn settings_state_owns_settings_geometry_facts() {
    type TestState = SettingsState<
        GlobalMountsState<GlobalMountRow, ()>,
        SettingsEnvState<EnvValue, ()>,
        SettingsAuthState<EnvValue, (), ()>,
        SettingsTrustState,
        (),
        (),
    >;
    let mut state = TestState::from_config(&AppConfig::default());
    state.mounts.pending.push(GlobalMountRow {
        scope: None,
        name: "cache".into(),
        mount: MountConfig {
            src: "/tmp/cache".into(),
            dst: "/home/agent/.cache".into(),
            readonly: false,
            isolation: jackin_core::isolation::MountIsolation::Shared,
        },
    });
    state
        .env
        .pending
        .env
        .insert("KEY".into(), EnvValue::Plain("1".into()));
    state.trust.pending = vec![SettingsTrustRow {
        role: "smith".into(),
        git: "https://example.invalid/smith.git".into(),
        trusted: false,
    }];
    state.mounts.error = Some("bad mount".into());
    state.env.error = Some("bad env".into());
    state.auth.error = Some("bad auth".into());
    state.trust.error = Some("bad trust".into());
    state.mounts.scroll_x = 1000;

    assert!(state.mounts_content_height() >= 2);
    assert!(state.env_content_height() >= 3);
    assert!(state.auth_content_height() >= 2);
    assert!(state.trust_content_height() >= 3);

    state.clamp_mounts_scroll_for_frame(ratatui::layout::Rect::new(0, 0, 120, 30));

    assert!(state.mounts.scroll_x < 1000);
}
}
