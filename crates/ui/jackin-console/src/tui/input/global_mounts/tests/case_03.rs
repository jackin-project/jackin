// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

#[test]
fn after_settings_event_promotes_subtab_errors_to_error_popup() {
    fn set_mounts_error(settings: &mut SettingsState<'_>) {
        settings.mounts.error = Some("mounts detail".into());
    }
    fn set_env_error(settings: &mut SettingsState<'_>) {
        settings.env.error = Some("env detail".into());
    }
    fn set_auth_error(settings: &mut SettingsState<'_>) {
        settings.auth.error = Some("auth detail".into());
    }
    fn set_trust_error(settings: &mut SettingsState<'_>) {
        settings.trust.error = Some("trust detail".into());
    }

    type SettingsErrorSetter<'a> = fn(&mut SettingsState<'a>);
    let cases: [(&str, SettingsErrorSetter<'_>); 4] = [
        ("mounts", set_mounts_error),
        ("env", set_env_error),
        ("auth", set_auth_error),
        ("trust", set_trust_error),
    ];

    for (name, set_error) in cases {
        let tmp = tempfile::tempdir().unwrap();
        let paths = JackinPaths::for_tests(tmp.path());
        paths.ensure_base_dirs().unwrap();
        let config = AppConfig::default();
        let mut state = ManagerState::from_config(&config, tmp.path());
        let mut settings = SettingsState::from_config(&config);
        set_error(&mut settings);
        state.stage = ManagerStage::Settings(settings);

        after_settings_event(&mut state);

        let ManagerStage::Settings(settings) = &state.stage else {
            panic!("must stay in Settings stage");
        };
        let popup = settings
            .error_popup
            .as_ref()
            .unwrap_or_else(|| panic!("{name} error must promote to ErrorPopup"));
        assert_eq!(popup.title, "Settings error");
        assert!(
            popup.message.contains(name),
            "{name} error detail must survive promotion: {:?}",
            popup.message,
        );
        assert!(settings.mounts.error.is_none());
        assert!(settings.env.error.is_none());
        assert!(settings.auth.error.is_none());
        assert!(settings.trust.error.is_none());
    }
}

#[test]
fn after_settings_event_exit_requested_pops_to_list() {
    let tmp = tempfile::tempdir().unwrap();
    let paths = JackinPaths::for_tests(tmp.path());
    paths.ensure_base_dirs().unwrap();
    let config = AppConfig::default();
    let mut state = ManagerState::from_config(&config, tmp.path());
    let mut settings = SettingsState::from_config(&config);
    settings.mounts.exit_requested = true;
    state.stage = ManagerStage::Settings(settings);

    after_settings_event(&mut state);

    assert!(
        matches!(state.stage, ManagerStage::List),
        "exit_requested must pop to List; got {:?}",
        state.stage,
    );
}
