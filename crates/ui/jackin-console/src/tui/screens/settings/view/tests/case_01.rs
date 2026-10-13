// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

#[test]
fn general_lines_highlight_selected_setting() {
    let lines = general_lines(1, true, false, true);

    assert_eq!(lines.len(), 2);
    assert_eq!(lines[0].spans[0].content.as_ref(), "  ");
    assert_eq!(lines[0].spans[2].content.as_ref(), "enabled");
    assert_eq!(lines[1].spans[0].content.as_ref(), "\u{25b8} ");
    assert_eq!(lines[1].spans[2].content.as_ref(), "disabled");
}

#[test]
fn settings_frame_areas_match_header_tabs_body_footer_contract() {
    let areas = settings_frame_areas(Rect::new(0, 0, 80, 20), 2);

    assert_eq!(areas.header, Rect::new(0, 0, 80, 3));
    assert_eq!(areas.tabs, Rect::new(0, 3, 80, 2));
    assert_eq!(areas.body, Rect::new(0, 5, 80, 13));
    assert_eq!(areas.footer, Rect::new(0, 18, 80, 2));
}

#[test]
fn settings_header_title_is_screen_owned() {
    assert_eq!(settings_header_title(), "settings");
}

#[test]
fn settings_modal_render_plan_prioritizes_visible_modal_family() {
    assert_eq!(
        settings_modal_render_plan(true, true, true, true),
        SettingsModalRenderPlan::ErrorPopup
    );
    assert_eq!(
        settings_modal_render_plan(false, true, true, true),
        SettingsModalRenderPlan::Mounts
    );
    assert_eq!(
        settings_modal_render_plan(false, false, true, true),
        SettingsModalRenderPlan::Environments
    );
    assert_eq!(
        settings_modal_render_plan(false, false, false, true),
        SettingsModalRenderPlan::Auth
    );
    assert_eq!(
        settings_modal_render_plan(false, false, false, false),
        SettingsModalRenderPlan::None
    );
}

#[test]
fn clamp_mounts_scroll_x_for_frame_uses_settings_body_area() {
    let mut scroll = crate::tui::scroll_block::console_scroll_area_state();
    crate::tui::scroll_block::scroll_area_set_x(&mut scroll, u16::MAX);
    let area = Rect::new(0, 0, 80, 20);

    clamp_mounts_scroll_x_for_frame(area, 100, &mut scroll);

    let body = settings_frame_areas(area, 2).body;
    let expected = termrock::scroll::max_offset_u16(100, termrock::scroll::viewport_width(body));
    assert_eq!(scroll.offset_x(), expected);
}

#[test]
fn tab_content_heights_account_for_error_rows() {
    assert_eq!(mounts_content_height(4, false), 4);
    assert_eq!(mounts_content_height(4, true), 6);
    assert_eq!(env_content_height(3, true), 5);
    assert_eq!(trust_content_height(0, false), 2);
    assert_eq!(trust_content_height(3, true), 6);
}

#[test]
fn global_mount_confirm_prompts_are_settings_owned() {
    assert_eq!(
        global_mount_confirm_prompt(GlobalMountConfirm::Remove),
        "Remove selected global mount?"
    );
    assert_eq!(
        global_mount_confirm_prompt(GlobalMountConfirm::Sensitive),
        "Sensitive global mount path detected. Save anyway?"
    );
}

#[test]
fn global_mount_confirm_state_uses_settings_prompt() {
    let state = global_mount_confirm_state(GlobalMountConfirm::Discard);

    assert_eq!(state.title(), "Confirm");
    let crate::tui::components::ConfirmKind::Default { prompt } = state.kind() else {
        panic!("expected default confirm state");
    };
    assert_eq!(prompt, "Discard unsaved global mount changes?");
}

#[test]
fn settings_env_text_input_state_allows_empty_values_only() {
    let value_target = SettingsEnvTextTarget::EnvValue {
        scope: SettingsEnvScope::Global,
        key: "TOKEN".to_owned(),
    };
    let key_target = SettingsEnvTextTarget::EnvKey {
        scope: SettingsEnvScope::Global,
    };

    let value_state = settings_env_text_input_state(&value_target, "Edit TOKEN", "");
    let key_state = settings_env_text_input_state(&key_target, "New key", "");

    assert!(value_state.is_valid());
    assert!(!key_state.is_valid());
}

#[test]
fn settings_env_value_text_label_names_key() {
    assert_eq!(settings_env_value_text_label("TOKEN"), "Edit TOKEN");
    assert_eq!(settings_env_value_current_text(Some("value")), "value");
    assert_eq!(settings_env_value_current_text(None), "");
}

#[test]
fn settings_env_value_edit_text_plan_owns_lookup_and_labels() {
    let pending = SettingsEnvConfig {
        env: BTreeMap::from([(
            "TOKEN".to_owned(),
            jackin_core::EnvValue::Plain("abc".to_owned()),
        )]),
        roles: BTreeMap::from([(
            "ops".to_owned(),
            BTreeMap::from([(
                "ROLE_TOKEN".to_owned(),
                jackin_core::EnvValue::Plain("def".to_owned()),
            )]),
        )]),
    };

    assert_eq!(
        settings_env_value_edit_text_plan(&pending, SettingsEnvScope::Global, "TOKEN".to_owned()),
        SettingsEnvValueEditTextPlan {
            target: SettingsEnvTextTarget::EnvValue {
                scope: SettingsEnvScope::Global,
                key: "TOKEN".to_owned(),
            },
            label: "Edit TOKEN".to_owned(),
            current: "abc".to_owned(),
        }
    );
    assert_eq!(
        settings_env_value_edit_text_plan(
            &pending,
            SettingsEnvScope::Role("ops".to_owned()),
            "ROLE_TOKEN".to_owned()
        ),
        SettingsEnvValueEditTextPlan {
            target: SettingsEnvTextTarget::EnvValue {
                scope: SettingsEnvScope::Role("ops".to_owned()),
                key: "ROLE_TOKEN".to_owned(),
            },
            label: "Edit ROLE_TOKEN".to_owned(),
            current: "def".to_owned(),
        }
    );
}

#[test]
fn settings_env_plain_value_text_plan_owns_empty_value_modal() {
    assert_eq!(
        settings_env_plain_value_text_plan(SettingsEnvScope::Global, "TOKEN".to_owned()),
        SettingsEnvValueEditTextPlan {
            target: SettingsEnvTextTarget::EnvValue {
                scope: SettingsEnvScope::Global,
                key: "TOKEN".to_owned(),
            },
            label: "Edit TOKEN".to_owned(),
            current: String::new(),
        }
    );
}

#[test]
fn settings_env_source_picker_state_names_key() {
    let state = settings_env_source_picker_state("TOKEN");

    assert_eq!(state.key, "TOKEN");
    assert!(state.op_available);
}

#[test]
fn settings_env_delete_confirm_state_uses_key_prompt() {
    let state = settings_env_delete_confirm_state("TOKEN");

    let crate::tui::components::ConfirmKind::Default { prompt } = state.kind() else {
        panic!("expected default confirm state");
    };
    assert_eq!(prompt, "Delete environment variable TOKEN?");
}

#[test]
fn global_mount_text_input_state_names_label() {
    let state = global_mount_text_input_state("Destination", "/workspace");

    assert_eq!(state.label, "Destination");
    assert_eq!(state.value(), "/workspace");
}

#[test]
fn global_mount_scope_text_value_uses_empty_global_fallback() {
    assert_eq!(global_mount_scope_text_value(Some("ops")), "ops");
    assert_eq!(global_mount_scope_text_value(None), "");
}

#[test]
fn global_mount_edit_text_initial_routes_edit_targets() {
    let row = jackin_config::GlobalMountRow {
        scope: Some("ops".to_owned()),
        name: "cache".to_owned(),
        mount: jackin_config::MountConfig {
            src: "/host/cache".to_owned(),
            dst: "/jackin/cache".to_owned(),
            readonly: true,
            isolation: jackin_config::MountIsolation::Shared,
        },
    };

    assert_eq!(
        global_mount_edit_text_initial(&row, &GlobalMountTextTarget::Rename),
        Some("cache".to_owned())
    );
    assert_eq!(
        global_mount_edit_text_initial(&row, &GlobalMountTextTarget::Source),
        Some("/host/cache".to_owned())
    );
    assert_eq!(
        global_mount_edit_text_initial(&row, &GlobalMountTextTarget::Destination),
        Some("/jackin/cache".to_owned())
    );
    assert_eq!(
        global_mount_edit_text_initial(&row, &GlobalMountTextTarget::Scope),
        Some("ops".to_owned())
    );
    assert_eq!(
        global_mount_edit_text_initial(&row, &GlobalMountTextTarget::AddSource),
        None
    );
}

#[test]
fn global_mount_selected_edit_text_plan_routes_selected_row() {
    let rows = vec![
        jackin_config::GlobalMountRow {
            scope: None,
            name: "logs".to_owned(),
            mount: jackin_config::MountConfig {
                src: "/host/logs".to_owned(),
                dst: "/jackin/logs".to_owned(),
                readonly: false,
                isolation: jackin_config::MountIsolation::Shared,
            },
        },
        jackin_config::GlobalMountRow {
            scope: Some("ops".to_owned()),
            name: "cache".to_owned(),
            mount: jackin_config::MountConfig {
                src: "/host/cache".to_owned(),
                dst: "/jackin/cache".to_owned(),
                readonly: true,
                isolation: jackin_config::MountIsolation::Shared,
            },
        },
    ];

    assert_eq!(
        global_mount_selected_edit_text_plan(&rows, 1, GlobalMountTextTarget::Rename),
        Some(GlobalMountEditTextPlan {
            target: GlobalMountTextTarget::Rename,
            label: "Rename mount",
            initial: "cache".to_owned(),
        })
    );
    assert_eq!(
        global_mount_selected_edit_text_plan(&rows, 1, GlobalMountTextTarget::Scope),
        Some(GlobalMountEditTextPlan {
            target: GlobalMountTextTarget::Scope,
            label: "Scope (empty = global)",
            initial: "ops".to_owned(),
        })
    );
    assert_eq!(
        global_mount_selected_edit_text_plan(&rows, 3, GlobalMountTextTarget::Rename),
        None
    );
    assert_eq!(
        global_mount_selected_edit_text_plan(&rows, 1, GlobalMountTextTarget::AddSource),
        None
    );
}

#[test]
fn global_mount_text_target_labels_are_settings_owned() {
    assert_eq!(
        global_mount_text_target_label(&GlobalMountTextTarget::Rename),
        Some("Rename mount")
    );
    assert_eq!(
        global_mount_text_target_label(&GlobalMountTextTarget::AddScope),
        Some("Scope (empty = global)")
    );
    assert_eq!(
        global_mount_text_target_label(&GlobalMountTextTarget::AddDestination),
        Some("Destination")
    );
}

#[test]
fn settings_env_delete_confirm_prompt_names_key() {
    assert_eq!(
        settings_env_delete_confirm_prompt("TOKEN"),
        "Delete environment variable TOKEN?"
    );
}

#[test]
fn settings_env_key_input_state_marks_scope_duplicates() {
    let mut pending = SettingsEnvConfig {
        env: BTreeMap::new(),
        roles: BTreeMap::new(),
    };
    pending.env.insert("GLOBAL".to_owned(), "1".to_owned());
    pending
        .roles
        .entry("alpha".to_owned())
        .or_default()
        .insert("ROLE_TOKEN".to_owned(), "2".to_owned());

    let state = settings_env_key_input_state(
        &pending,
        &SettingsEnvScope::Role("alpha".to_owned()),
        "New alpha environment key",
        "",
    );

    assert_eq!(state.label, "New alpha environment key");
    assert_eq!(state.forbidden_label, "role alpha");
    assert!(!state.is_duplicate());

    let duplicate = settings_env_key_input_state(
        &pending,
        &SettingsEnvScope::Role("alpha".to_owned()),
        "New alpha environment key",
        "ROLE_TOKEN",
    );
    assert!(duplicate.is_duplicate());
}
