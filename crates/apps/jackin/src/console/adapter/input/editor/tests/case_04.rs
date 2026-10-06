// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

#[test]
fn agents_tab_star_on_current_default_clears_it() {
    // With default = "alpha" (effectively allowed under shorthand),
    // pressing `*` on the same row clears the default. Toggle-off is
    // symmetric with the Space allow/disallow toggle.
    let mut config = config_with_agents(&["alpha", "beta"]);
    let mut ws = empty_ws();
    ws.default_role = Some("alpha".into());
    let mut state = editor_on_agents_tab(ws, 0);

    press(&mut state, &mut config, KeyCode::Char('*')).unwrap();

    let ManagerStage::Editor(e) = &state.stage else {
        panic!("editor stage expected");
    };
    assert!(
        e.pending.default_role.is_none(),
        "`*` on the current default must clear it; got {:?}",
        e.pending.default_role,
    );
}

#[test]
fn agents_tab_star_on_unallowed_agent_is_noop() {
    // Workspace in "custom" mode with only `alpha` allowed; cursor
    // on row 1 (`beta`, NOT in the allow list). `*` must not set
    // beta as default — defaults are meaningless on disallowed
    // roles and the operator should `Space` to allow first.
    let mut config = config_with_agents(&["alpha", "beta", "gamma"]);
    let mut ws = empty_ws();
    ws.allowed_roles = vec!["alpha".into()];
    let mut state = editor_on_agents_tab(ws, 1);

    press(&mut state, &mut config, KeyCode::Char('*')).unwrap();

    let ManagerStage::Editor(e) = &state.stage else {
        panic!("editor stage expected");
    };
    assert!(
        e.pending.default_role.is_none(),
        "`*` on a disallowed role must be a no-op; got {:?}",
        e.pending.default_role,
    );
    assert_eq!(
        e.pending.allowed_roles,
        vec!["alpha".to_owned()],
        "`*` must not silently extend the allow list; got {:?}",
        e.pending.allowed_roles,
    );
}

#[test]
fn agents_tab_disallow_default_clears_default() {
    // With "alpha" pinned as default (custom allow list = [alpha]),
    // pressing Space on alpha to disallow it must also clear the
    // default — defaults are only meaningful on allowed roles.
    let mut config = config_with_agents(&["alpha", "beta"]);
    let mut ws = empty_ws();
    ws.allowed_roles = vec!["alpha".into()];
    ws.default_role = Some("alpha".into());
    let mut state = editor_on_agents_tab(ws, 0);

    press(&mut state, &mut config, KeyCode::Char(' ')).unwrap();

    let ManagerStage::Editor(e) = &state.stage else {
        panic!("editor stage expected");
    };
    assert!(
        !e.pending.allowed_roles.contains(&"alpha".to_owned()),
        "alpha must be removed from allowed_roles after Space; got {:?}",
        e.pending.allowed_roles,
    );
    assert!(
        e.pending.default_role.is_none(),
        "disallowing the current default must clear default_role; got {:?}",
        e.pending.default_role,
    );
}

#[test]
fn d_key_no_longer_sets_default_agent_on_agents_tab() {
    // Regression guard: the `D` binding was removed in favour of `*`.
    // Pressing `D` on an role row must now be a no-op (no other
    // Roles-tab binding listens for `D`).
    let mut config = config_with_agents(&["alpha", "beta"]);
    let mut state = editor_on_agents_tab(empty_ws(), 1);

    press(&mut state, &mut config, KeyCode::Char('D')).unwrap();

    let ManagerStage::Editor(e) = &state.stage else {
        panic!("editor stage expected");
    };
    assert!(
        e.pending.default_role.is_none(),
        "`D` must no longer set the default role on the Roles tab",
    );
}

#[test]
fn roles_tab_enter_does_not_toggle_allowed_agent() {
    let mut config = config_with_agents(&["alpha", "beta"]);
    let mut state = editor_on_agents_tab(empty_ws(), 1);

    press(&mut state, &mut config, KeyCode::Enter).unwrap();

    assert_eq!(
        pending_allowed(&state),
        Vec::<String>::new(),
        "Enter on Roles row must not toggle allowed_roles",
    );
}

#[test]
fn editor_tab_bar_follows_aria_key_pattern() {
    let mut config = config_with_agents(&["alpha", "beta"]);
    let mut state = ManagerState::from_config(&config, std::path::Path::new("/"));
    state.stage = ManagerStage::Editor(EditorState::new_edit(
        "ws".into(),
        WorkspaceConfig::default(),
    ));

    press(&mut state, &mut config, KeyCode::Right).unwrap();
    assert!(
        matches!(&state.stage, ManagerStage::Editor(editor) if editor.tab_bar_focused() && editor.active_tab == EditorTab::Mounts)
    );

    press(&mut state, &mut config, KeyCode::Left).unwrap();
    assert!(
        matches!(&state.stage, ManagerStage::Editor(editor) if editor.tab_bar_focused() && editor.active_tab == EditorTab::General)
    );

    press(&mut state, &mut config, KeyCode::Down).unwrap();
    assert!(
        matches!(&state.stage, ManagerStage::Editor(editor) if !editor.tab_bar_focused()),
        "Down from focused tab bar must enter content",
    );

    press(&mut state, &mut config, KeyCode::BackTab).unwrap();
    assert!(
        matches!(&state.stage, ManagerStage::Editor(editor) if editor.tab_bar_focused()),
        "ShiftTab from content must return to tab bar",
    );

    press(&mut state, &mut config, KeyCode::Down).unwrap();
    press(&mut state, &mut config, KeyCode::Esc).unwrap();
    assert!(
        matches!(&state.stage, ManagerStage::Editor(editor) if editor.tab_bar_focused()),
        "Esc from content must return to tab bar",
    );
}

#[test]
fn tab_switch_via_tab_clears_content_scroll_focus() {
    // When content owns focus and the operator presses Tab to cycle tabs,
    // tab_bar_focused must become true and the stale content scroll focus
    // must be cleared so no green border appears on the new tab's content.
    let mut config = config_with_agents(&["alpha"]);
    let mut state = ManagerState::from_config(&config, std::path::Path::new("/"));
    state.stage = ManagerStage::Editor(EditorState::new_edit(
        "ws".into(),
        WorkspaceConfig::default(),
    ));

    // Enter content from tab bar.
    press(&mut state, &mut config, KeyCode::Down).unwrap();
    let tab_bar_cleared = matches!(&state.stage, ManagerStage::Editor(e) if !e.tab_bar_focused());
    assert!(tab_bar_cleared, "Down must enter content");

    // Tab while content is focused cycles to the next tab AND returns focus to tab bar.
    press(&mut state, &mut config, KeyCode::Tab).unwrap();
    assert!(
        matches!(&state.stage, ManagerStage::Editor(e)
            if e.tab_bar_focused()
                && !e.tab_content_scroll_focused()
                && !e.workspace_mounts_scroll_focused()),
        "Tab from content must return focus to tab bar and clear content scroll focus"
    );
}

#[test]
fn toggle_in_all_mode_demotes_to_custom_without_this_agent() {
    // Starting state: "all" mode (empty list), three roles. Pressing
    // Space on row 1 (`beta`) must produce a custom list containing
    // every other role — i.e. `[alpha, gamma]` — so that `beta`
    // flips from `[x]` to `[ ]` and the status line reads
    // `custom (2 of 3 allowed)`.
    let mut config = config_with_agents(&["alpha", "beta", "gamma"]);
    let mut state = editor_on_agents_tab(empty_ws(), 1);

    press(&mut state, &mut config, KeyCode::Char(' ')).unwrap();

    let list = pending_allowed(&state);
    assert_eq!(
        list,
        vec!["alpha".to_owned(), "gamma".to_owned()],
        "list must be populated with every other role when demoting from 'all'"
    );
}

#[test]
fn toggle_custom_last_item_clears_to_empty() {
    // Starting state: "custom" mode with a single allowed role.
    // Toggling that role off must leave the list empty (reverting
    // to the "all" shorthand) — NOT pinning it at a phantom
    // `custom (0 of N allowed)` state.
    let mut config = config_with_agents(&["alpha", "beta"]);
    let mut ws = empty_ws();
    ws.allowed_roles = vec!["alpha".into()];
    let mut state = editor_on_agents_tab(ws, 0);

    press(&mut state, &mut config, KeyCode::Char(' ')).unwrap();

    assert_eq!(
        pending_allowed(&state),
        Vec::<String>::new(),
        "removing the last custom entry must leave the list empty (= all allowed)",
    );
}

#[test]
fn toggle_adds_back_to_custom() {
    // Starting state: "custom" mode with `[alpha]` (so `beta` reads
    // `[ ]`). Pressing Space on `beta` (row 1) must add it, producing
    // `[alpha, beta]` — and since that still doesn't cover every
    // role (`gamma` is missing), the list must stay non-empty.
    let mut config = config_with_agents(&["alpha", "beta", "gamma"]);
    let mut ws = empty_ws();
    ws.allowed_roles = vec!["alpha".into()];
    let mut state = editor_on_agents_tab(ws, 1);

    press(&mut state, &mut config, KeyCode::Char(' ')).unwrap();

    let mut list = pending_allowed(&state);
    list.sort();
    assert_eq!(
        list,
        vec!["alpha".to_owned(), "beta".to_owned()],
        "adding `beta` with `gamma` still missing must produce a 2-of-3 custom list",
    );
}

#[test]
fn toggle_refills_custom_to_all_when_last_agent_added_makes_it_complete() {
    // Starting state: "custom" mode with all-but-one role present.
    // Adding the missing one would yield `custom (N of N allowed)` —
    // semantically identical to "all allowed". The toggle must
    // collapse back to the empty-list shorthand so the status badge
    // reads `all`, not `custom (3 of 3 allowed)`.
    let mut config = config_with_agents(&["alpha", "beta", "gamma"]);
    let mut ws = empty_ws();
    ws.allowed_roles = vec!["alpha".into(), "beta".into()];
    // Cursor on row 2 (role `gamma`, the missing one).
    let mut state = editor_on_agents_tab(ws, 2);

    press(&mut state, &mut config, KeyCode::Char(' ')).unwrap();

    assert_eq!(
        pending_allowed(&state),
        Vec::<String>::new(),
        "filling the custom list must collapse it to empty (= all allowed)",
    );
}

#[test]
fn r_key_toggles_readonly_on_current_mount_row() {
    // Start rw → one R press should flip to ro and register as a change.
    let mut config = AppConfig::default();
    let mut state = editor_on_mounts_tab(ws_with_one_mount(false), 0);

    press(&mut state, &mut config, KeyCode::Char('R')).unwrap();

    let ManagerStage::Editor(e) = &state.stage else {
        panic!("editor stage expected");
    };
    assert!(
        e.pending.mounts[0].readonly,
        "R on rw mount must flip to ro",
    );
    assert!(
        e.change_count() > 0,
        "flipping readonly must surface as a change; got change_count={}",
        e.change_count()
    );
}

#[test]
fn r_key_lowercase_also_toggles_readonly() {
    // Operators often hit `r` without holding shift; both cases must work.
    let mut config = AppConfig::default();
    let mut state = editor_on_mounts_tab(ws_with_one_mount(false), 0);

    press(&mut state, &mut config, KeyCode::Char('r')).unwrap();

    let ManagerStage::Editor(e) = &state.stage else {
        panic!("editor stage expected");
    };
    assert!(e.pending.mounts[0].readonly);
}

#[test]
fn rows_beyond_workspace_mounts_are_noop_in_workspace_editor() {
    let tmp = tempfile::tempdir().unwrap();
    let src = tmp.path().join("cache");
    std::fs::create_dir_all(&src).unwrap();
    let mut config = AppConfig::default();
    config
        .roles
        .insert("agent-smith".into(), jackin_config::RoleSource::default());
    config.add_mount(
        "cache",
        MountConfig {
            src: src.display().to_string(),
            dst: "/cache".into(),
            readonly: false,
            isolation: jackin_config::MountIsolation::Shared,
        },
        None,
    );
    let mut ws = ws_with_one_mount(false);
    ws.allowed_roles = vec!["agent-smith".into()];
    // Global mounts are intentionally not rendered/editable here; row 2
    // simulates stale input beyond the workspace mount + add sentinel.
    let mut state = editor_on_mounts_tab(ws, 2);

    press(&mut state, &mut config, KeyCode::Char('R')).unwrap();
    press(&mut state, &mut config, KeyCode::Char('D')).unwrap();
    press(&mut state, &mut config, KeyCode::Char('I')).unwrap();

    let ManagerStage::Editor(e) = &state.stage else {
        panic!("editor stage expected");
    };
    assert_eq!(e.pending.mounts.len(), 1);
    assert!(!e.pending.mounts[0].readonly);
    assert_eq!(
        e.pending.mounts[0].isolation,
        jackin_config::MountIsolation::Shared
    );
}

#[test]
fn r_key_on_sentinel_is_noop() {
    // Cursor on the `+ Add mount` sentinel (row == mounts.len()) — R must
    // not mutate mounts or trigger a change.
    let mut config = AppConfig::default();
    let ws = ws_with_one_mount(false);
    let before = ws.mounts.clone();
    let mut state = editor_on_mounts_tab(ws, 1); // sentinel row

    press(&mut state, &mut config, KeyCode::Char('R')).unwrap();

    let ManagerStage::Editor(e) = &state.stage else {
        panic!("editor stage expected");
    };
    assert_eq!(
        e.pending.mounts, before,
        "R on sentinel must leave mounts untouched"
    );
    assert_eq!(
        e.change_count(),
        0,
        "R on sentinel must not mark editor dirty"
    );
}

#[test]
fn r_key_twice_restores_original() {
    // Flipping twice must bring `readonly` back to the starting value AND
    // net out to zero changes — the diff-based change_count treats
    // identical mounts as unchanged.
    let mut config = AppConfig::default();
    let mut state = editor_on_mounts_tab(ws_with_one_mount(false), 0);

    press(&mut state, &mut config, KeyCode::Char('R')).unwrap();
    press(&mut state, &mut config, KeyCode::Char('R')).unwrap();

    let ManagerStage::Editor(e) = &state.stage else {
        panic!("editor stage expected");
    };
    assert!(
        !e.pending.mounts[0].readonly,
        "two R presses must restore original rw state"
    );
    assert_eq!(
        e.change_count(),
        0,
        "two R presses must net zero changes; got {}",
        e.change_count()
    );
}
