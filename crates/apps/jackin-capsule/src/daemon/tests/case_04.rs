// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use crate::tui::components::dialog::{Dialog, DialogAction};
use crate::tui::message::Action;
use jackin_protocol::attach::SpawnRequest;

#[test]
fn spawn_request_rejects_agent_outside_allowlist_before_pty_spawn() {
    let dir = tempfile::tempdir().unwrap();
    let mut config = super::test_config(dir.path());
    config.instances = vec!["web".to_owned()];
    config.agents = [("web".to_owned(), "some-agent".to_owned())]
        .into_iter()
        .collect();
    let mut mux = super::super::Multiplexer::new(24, 80, config).unwrap();
    let err = mux
        .spawn_request(SpawnRequest::Instance("evil".to_owned()), &[])
        .unwrap_err();
    assert!(err.to_string().contains("allowlist"), "{err:?}");
    assert!(mux.session_supervisor.sessions.is_empty());
    assert!(mux.session_supervisor.tabs.is_empty());
}

#[test]
fn spawn_failure_popup_stays_open_until_dismissed() {
    let dir = tempfile::tempdir().unwrap();
    let mut mux = super::test_mux(dir.path());
    mux.open_spawn_failure_dialog("boom".to_owned());
    assert!(matches!(mux.dialog_top(), Some(Dialog::SpawnFailure(_))));
    mux.apply_action(Action::RefreshUsage);
    assert!(matches!(mux.dialog_top(), Some(Dialog::SpawnFailure(_))));
    mux.apply_dialog_action(DialogAction::Dismiss);
    assert!(mux.dialog_top().is_none());
}
