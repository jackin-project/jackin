//! Tests for `update`.
use super::*;
use ratatui::layout::Rect;

#[test]
fn settings_tab_move_plan_cycles_and_sets_focus() {
    assert_eq!(
        settings_tab_move_plan(SettingsTab::Trust, 1, true),
        SettingsTabMovePlan {
            active_tab: SettingsTab::General,
            tab_bar_focused: true,
        }
    );
    assert_eq!(
        settings_tab_move_plan(SettingsTab::General, -1, false),
        SettingsTabMovePlan {
            active_tab: SettingsTab::Trust,
            tab_bar_focused: false,
        }
    );
}

#[test]
fn settings_tab_select_plan_focuses_selected_tab() {
    assert_eq!(
        settings_tab_select_plan(SettingsTab::Trust),
        SettingsTabMovePlan {
            active_tab: SettingsTab::Trust,
            tab_bar_focused: true,
        }
    );
}

#[test]
fn settings_tab_bar_focus_plan_returns_requested_focus() {
    assert!(settings_tab_bar_focus_plan(true));
    assert!(!settings_tab_bar_focus_plan(false));
}

#[test]
fn settings_tab_at_position_maps_tab_strip_cells() {
    assert_eq!(
        settings_tab_at_position(crate::tui::layout::SCREEN_HEADER_HEIGHT, 1),
        Some(SettingsTab::General)
    );
    assert_eq!(
        settings_tab_at_position(crate::tui::layout::SCREEN_HEADER_HEIGHT, 11),
        Some(SettingsTab::Mounts)
    );
    assert_eq!(
        settings_tab_at_position(crate::tui::layout::SCREEN_HEADER_HEIGHT - 1, 1),
        None
    );
}

#[test]
fn settings_auth_detail_row_count_adds_source_row_only_when_needed() {
    assert_eq!(
        settings_auth_detail_row_count(AuthKind::Github, AuthMode::Token),
        3
    );
    assert_eq!(
        settings_auth_detail_row_count(AuthKind::Github, AuthMode::Sync),
        2
    );
    assert_eq!(
        settings_auth_detail_row_count(AuthKind::Claude, AuthMode::Sync),
        3
    );
    assert_eq!(
        settings_auth_detail_row_count(AuthKind::Claude, AuthMode::ApiKey),
        3
    );
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TestAuthKind {
    Claude,
}

#[test]
fn settings_auth_kind_entry_plan_selects_kind_and_resets_row() {
    assert_eq!(
        enter_settings_auth_kind_plan(Some(TestAuthKind::Claude)),
        Some(SettingsAuthKindPlan {
            selected_kind: Some(TestAuthKind::Claude),
            selected: 0,
        })
    );
    assert_eq!(enter_settings_auth_kind_plan::<TestAuthKind>(None), None);
}

#[test]
fn settings_auth_kind_clear_plan_clears_kind_and_resets_row() {
    assert_eq!(
        clear_settings_auth_kind_plan::<TestAuthKind>(),
        SettingsAuthKindPlan {
            selected_kind: None,
            selected: 0,
        }
    );
}

#[test]
fn settings_auth_selection_plan_clamps_to_rows() {
    let rows = [
        SettingsAuthDetailRow::Mode,
        SettingsAuthDetailRow::Source,
        SettingsAuthDetailRow::SourceFolder,
        SettingsAuthDetailRow::Spacer,
    ];
    assert_eq!(settings_auth_selection_plan(0, &rows, 99), 0);
    assert_eq!(settings_auth_selection_plan(2, &rows, -99), 0);
}

#[test]
fn settings_auth_row_is_focusable_keeps_preview_rows_visible_but_skipped() {
    assert!(settings_auth_row_is_focusable(SettingsAuthDetailRow::Mode));
    assert!(!settings_auth_row_is_focusable(
        SettingsAuthDetailRow::Source
    ));
    assert!(!settings_auth_row_is_focusable(
        SettingsAuthDetailRow::SourceFolder
    ));
    assert!(!settings_auth_row_is_focusable(
        SettingsAuthDetailRow::Spacer
    ));
}

#[test]
fn settings_trust_selection_plan_clamps_and_updates_scroll() {
    let plan = settings_trust_selection_plan(0, 4, 99, 0, 8, 0);
    assert_eq!(plan.selected, 3);
    assert!(plan.scroll_y > 0);
}

#[test]
fn settings_trust_row_select_plan_bounds_checks_and_focuses_content() {
    assert_eq!(
        settings_trust_row_select_plan(1, 3),
        SettingsTrustRowSelectPlan {
            selected: Some(1),
            content_focused: true,
        }
    );
    assert_eq!(
        settings_trust_row_select_plan(3, 3),
        SettingsTrustRowSelectPlan {
            selected: None,
            content_focused: true,
        }
    );
}

#[test]
fn settings_trust_row_at_position_skips_header_and_applies_scroll() {
    let area = Rect::new(0, 5, 80, 10);

    assert_eq!(settings_trust_row_at_position(area, 1, 7, 0, 3), Some(0));
    assert_eq!(settings_trust_row_at_position(area, 1, 8, 2, 5), Some(3));
    assert_eq!(settings_trust_row_at_position(area, 1, 6, 0, 3), None);
    assert_eq!(settings_trust_row_at_position(area, 1, 10, 0, 3), None);
    assert_eq!(settings_trust_row_at_position(area, 80, 6, 0, 3), None);
}

#[test]
fn settings_scroll_focus_plan_routes_by_tab_and_modal() {
    assert_eq!(
        settings_scroll_focus_plan(SettingsTab::Mounts, false, true),
        SettingsScrollFocusPlan {
            mounts: true,
            env: false,
            auth: false,
            trust: false,
        }
    );
    assert_eq!(
        settings_scroll_focus_plan(SettingsTab::Auth, false, true),
        SettingsScrollFocusPlan {
            mounts: false,
            env: false,
            auth: true,
            trust: false,
        }
    );
    assert_eq!(
        settings_scroll_focus_plan(SettingsTab::Trust, true, true),
        SettingsScrollFocusPlan {
            mounts: false,
            env: false,
            auth: false,
            trust: false,
        }
    );
}

#[test]
fn settings_modal_open_reports_any_modal_surface() {
    assert!(!settings_modal_open(false, false, false, false));
    assert!(settings_modal_open(true, false, false, false));
    assert!(settings_modal_open(false, true, false, false));
    assert!(settings_modal_open(false, false, true, false));
    assert!(settings_modal_open(false, false, false, true));
}

#[test]
fn settings_horizontal_scroll_plan_updates_and_clamps_offset() {
    assert_eq!(settings_horizontal_scroll_plan(0, 8, 10, 40), 8);
    assert_eq!(settings_horizontal_scroll_plan(8, -99, 10, 40), 0);
}

#[test]
fn settings_env_selection_plan_skips_spacers_and_updates_scroll() {
    let rows = [
        SettingsEnvRow::Key {
            scope: SettingsEnvScope::Global,
            key: "ALPHA".to_owned(),
        },
        SettingsEnvRow::SectionSpacer,
        SettingsEnvRow::GlobalAddSentinel,
    ];
    let plan = settings_env_selection_plan(0, &rows, 1, 0, 8, 0);
    assert_eq!(plan.selected, 2);
    assert!(plan.scroll_y > 0);
}

#[test]
fn settings_global_mounts_selection_plan_clamps_to_add_row() {
    let plan = settings_global_mounts_selection_plan(0, 2, 99, 0, 8, 0);
    assert_eq!(plan.selected, 2);
    assert!(plan.scroll_y > 0);
}

fn env_config() -> SettingsEnvConfig<&'static str> {
    SettingsEnvConfig {
        env: BTreeMap::from([("GLOBAL".to_owned(), "x")]),
        roles: BTreeMap::from([
            (
                "alpha".to_owned(),
                BTreeMap::from([("ROLE_A".to_owned(), "x"), ("ROLE_B".to_owned(), "x")]),
            ),
            ("empty".to_owned(), BTreeMap::new()),
        ]),
    }
}

#[test]
fn settings_env_flat_rows_include_expanded_role_entries() {
    let expanded = BTreeSet::from(["alpha".to_owned()]);
    let rows = settings_env_flat_rows(&env_config(), &expanded);
    assert!(matches!(rows[0], SettingsEnvRow::Key { .. }));
    assert!(matches!(rows[1], SettingsEnvRow::SectionSpacer));
    assert!(matches!(rows[2], SettingsEnvRow::GlobalAddSentinel));
    assert!(rows.iter().any(
        |row| matches!(row, SettingsEnvRow::RoleHeader { role, expanded: true } if role == "alpha")
    ));
    assert!(
        rows.iter()
            .any(|row| matches!(row, SettingsEnvRow::RoleAddSentinel(role) if role == "alpha"))
    );
    assert!(
        !rows
            .iter()
            .any(|row| matches!(row, SettingsEnvRow::RoleHeader { role, .. } if role == "empty"))
    );
}

#[test]
fn settings_env_flat_rows_collapse_role_entries() {
    let rows = settings_env_flat_rows(&env_config(), &BTreeSet::new());
    assert!(rows.iter().any(
        |row| matches!(row, SettingsEnvRow::RoleHeader { role, expanded: false } if role == "alpha")
    ));
    assert!(
        !rows
            .iter()
            .any(|row| matches!(row, SettingsEnvRow::RoleAddSentinel(role) if role == "alpha"))
    );
}

#[test]
fn settings_env_value_and_forbidden_keys_follow_scope() {
    let pending = env_config();

    assert_eq!(
        settings_env_value(&pending, &SettingsEnvScope::Global, "GLOBAL"),
        Some(&"x")
    );
    assert_eq!(
        settings_env_value(&pending, &SettingsEnvScope::Role("alpha".into()), "ROLE_A"),
        Some(&"x")
    );
    assert_eq!(
        forbidden_settings_env_keys(&pending, &SettingsEnvScope::Role("alpha".into())),
        vec!["ROLE_A".to_owned(), "ROLE_B".to_owned()]
    );
}

#[test]
fn set_settings_env_value_expands_role_scope() {
    let mut pending = SettingsEnvConfig {
        env: BTreeMap::new(),
        roles: BTreeMap::new(),
    };
    let mut expanded = BTreeSet::new();

    set_settings_env_value(
        &mut pending,
        &mut expanded,
        &SettingsEnvScope::Role("alpha".into()),
        "TOKEN",
        "secret",
    );

    assert_eq!(
        settings_env_value(&pending, &SettingsEnvScope::Role("alpha".into()), "TOKEN"),
        Some(&"secret")
    );
    assert!(expanded.contains("alpha"));
}

#[test]
fn toggle_settings_env_mask_for_row_skips_unmaskable_values() {
    let pending = env_config();
    let mut unmasked = BTreeSet::new();
    let row = SettingsEnvRow::Key {
        scope: SettingsEnvScope::Global,
        key: "GLOBAL".to_owned(),
    };

    assert!(!toggle_settings_env_mask_for_row(
        &mut unmasked,
        &pending,
        Some(&row),
        |_| false
    ));
    assert!(unmasked.is_empty());

    assert!(toggle_settings_env_mask_for_row(
        &mut unmasked,
        &pending,
        Some(&row),
        |_| true
    ));
    assert!(unmasked.contains(&(SettingsEnvScope::Global, "GLOBAL".to_owned())));
}

#[test]
fn remove_settings_env_row_deletes_key_and_clamps_selection() {
    let mut pending = env_config();
    let expanded = BTreeSet::from(["alpha".to_owned()]);
    let mut selected = 99;
    let row = SettingsEnvRow::Key {
        scope: SettingsEnvScope::Role("alpha".to_owned()),
        key: "ROLE_B".to_owned(),
    };

    assert!(remove_settings_env_row(
        &mut pending,
        &expanded,
        &mut selected,
        Some(&row),
    ));

    assert!(!pending.roles["alpha"].contains_key("ROLE_B"));
    assert_eq!(
        selected,
        settings_env_flat_row_count(&pending, &expanded) - 1
    );
}

#[test]
fn settings_env_add_target_follows_row_scope() {
    let global = SettingsEnvRow::GlobalAddSentinel;
    let role = SettingsEnvRow::Key {
        scope: SettingsEnvScope::Role("alpha".to_owned()),
        key: "TOKEN".to_owned(),
    };

    assert_eq!(
        settings_env_add_target_for_row(Some(&global)),
        Some(SettingsEnvScope::Global)
    );
    assert_eq!(
        settings_env_add_target_for_row(Some(&role)),
        Some(SettingsEnvScope::Role("alpha".to_owned()))
    );
}

#[test]
fn settings_env_picker_target_skips_headers_and_spacers() {
    let key = SettingsEnvRow::Key {
        scope: SettingsEnvScope::Role("alpha".to_owned()),
        key: "TOKEN".to_owned(),
    };
    let header = SettingsEnvRow::RoleHeader {
        role: "alpha".to_owned(),
        expanded: true,
    };

    assert_eq!(
        settings_env_picker_target_for_row(Some(&key)),
        Some((
            SettingsEnvScope::Role("alpha".to_owned()),
            Some("TOKEN".to_owned())
        ))
    );
    assert_eq!(settings_env_picker_target_for_row(Some(&header)), None);
    assert_eq!(
        settings_env_picker_target_for_row(Some(&SettingsEnvRow::GlobalAddSentinel)),
        Some((SettingsEnvScope::Global, None))
    );
}

#[test]
fn settings_env_enter_plan_handles_value_scope_and_headers() {
    let pending = env_config();
    let key = SettingsEnvRow::Key {
        scope: SettingsEnvScope::Global,
        key: "GLOBAL".to_owned(),
    };
    let collapsed = SettingsEnvRow::RoleHeader {
        role: "alpha".to_owned(),
        expanded: false,
    };
    let expanded = SettingsEnvRow::RoleHeader {
        role: "alpha".to_owned(),
        expanded: true,
    };

    assert_eq!(
        settings_env_enter_plan_for_row(&pending, Some(&key), |value| value.is_some()),
        SettingsEnvEnterPlan::EditValue {
            scope: SettingsEnvScope::Global,
            key: "GLOBAL".to_owned()
        }
    );
    assert_eq!(
        settings_env_enter_plan_for_row(&pending, Some(&key), |_| false),
        SettingsEnvEnterPlan::Noop
    );
    assert_eq!(
        settings_env_enter_plan_for_row(&pending, Some(&collapsed), |_| true),
        SettingsEnvEnterPlan::ExpandRole("alpha".to_owned())
    );
    assert_eq!(
        settings_env_enter_plan_for_row(&pending, Some(&expanded), |_| true),
        SettingsEnvEnterPlan::Noop
    );
}

#[test]
fn settings_env_enter_plan_handles_add_rows() {
    let pending = env_config();

    assert_eq!(
        settings_env_enter_plan_for_row(&pending, Some(&SettingsEnvRow::GlobalAddSentinel), |_| {
            true
        }),
        SettingsEnvEnterPlan::OpenScopePicker
    );
    assert_eq!(
        settings_env_enter_plan_for_row(
            &pending,
            Some(&SettingsEnvRow::RoleAddSentinel("alpha".to_owned())),
            |_| true
        ),
        SettingsEnvEnterPlan::AddRoleKey {
            scope: SettingsEnvScope::Role("alpha".to_owned()),
        }
    );
}

#[test]
fn settings_confirm_plan_routes_confirm_cancel_and_continue() {
    assert_eq!(
        settings_confirm_plan(GlobalMountConfirm::Save, ModalOutcome::Commit(true)),
        SettingsConfirmPlan::Commit
    );
    assert_eq!(
        settings_confirm_plan(GlobalMountConfirm::Save, ModalOutcome::Commit(false)),
        SettingsConfirmPlan::Cancel {
            abort_sensitive: false
        }
    );
    assert_eq!(
        settings_confirm_plan(GlobalMountConfirm::Sensitive, ModalOutcome::Cancel),
        SettingsConfirmPlan::Cancel {
            abort_sensitive: true
        }
    );
    assert_eq!(
        settings_confirm_plan(GlobalMountConfirm::Remove, ModalOutcome::Continue),
        SettingsConfirmPlan::Continue
    );
}
