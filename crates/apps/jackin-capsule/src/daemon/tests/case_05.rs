// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use crate::tui::layout::{PaneTree, SplitPosition, Tab};

fn two_tabs() -> Vec<Tab> {
    vec![
        Tab::new_single("a", 1, "codename-a"),
        Tab::new_single("b", 2, "codename-b"),
    ]
}

#[test]
fn zoom_state_is_independent_per_tab() {
    let dir = tempfile::tempdir().unwrap();
    let mut mux = super::test_mux(dir.path());
    mux.session_supervisor.tabs = two_tabs();
    mux.toggle_zoom();
    assert_eq!(mux.session_supervisor.tabs[0].zoomed, Some(1));
    assert_eq!(mux.session_supervisor.tabs[1].zoomed, None);
    mux.session_supervisor.active_tab = 1;
    mux.toggle_zoom();
    assert_eq!(mux.session_supervisor.tabs[0].zoomed, Some(1));
    assert_eq!(mux.session_supervisor.tabs[1].zoomed, Some(2));
    mux.toggle_zoom();
    assert_eq!(mux.session_supervisor.tabs[0].zoomed, Some(1));
    assert_eq!(mux.session_supervisor.tabs[1].zoomed, None);
}

#[test]
fn killing_zoomed_pane_clears_only_owning_tab_zoom() {
    let dir = tempfile::tempdir().unwrap();
    let mut mux = super::test_mux(dir.path());
    let mut split = PaneTree::Leaf(2);
    assert!(split.split_v(2, 3, SplitPosition::After));
    let mut zoomed_tab = Tab::new_single("b", 2, "codename-b");
    zoomed_tab.tree = split;
    zoomed_tab.focused_id = 2;
    zoomed_tab.zoomed = Some(2);
    let mut plain_tab = Tab::new_single("a", 1, "codename-a");
    plain_tab.zoomed = Some(1);
    mux.session_supervisor.tabs = vec![plain_tab, zoomed_tab];
    mux.session_supervisor.active_tab = 1;
    mux.remove_exited_session(2);
    assert_eq!(mux.session_supervisor.tabs[0].zoomed, Some(1));
    assert_eq!(mux.session_supervisor.tabs[1].zoomed, None);
    assert_eq!(mux.session_supervisor.tabs[1].tree.all_ids(), vec![3]);
}

#[test]
fn remove_exited_session_retires_codename_inv_d8() {
    let dir = tempfile::tempdir().unwrap();
    let mut mux = super::test_mux(dir.path());
    mux.session_supervisor.tabs = two_tabs();
    mux.session_supervisor
        .codename_live
        .extend(["codename-a".to_owned(), "codename-b".to_owned()]);
    mux.session_supervisor
        .agent_history
        .push(super::super::AgentRecord {
            session_id: 1,
            codename: "codename-a".to_owned(),
            agent: None,
            account_id: None,
            provider: None,
            started_at: chrono::Utc::now(),
            exited_at: None,
        });
    let survivor = mux.session_supervisor.tabs[1].label_owned();
    mux.remove_exited_session(1);
    assert_eq!(mux.session_supervisor.tabs.len(), 1);
    assert_eq!(mux.session_supervisor.tabs[0].label_owned(), survivor);
    assert!(!mux.session_supervisor.codename_live.contains("codename-a"));
    assert!(
        mux.session_supervisor
            .codename_retired
            .contains("codename-a")
    );
    assert!(mux.session_supervisor.codename_live.contains("codename-b"));
    let record = mux
        .session_supervisor
        .agent_history
        .iter()
        .find(|record| record.session_id == 1)
        .expect("history kept");
    assert!(record.exited_at.is_some());
}
