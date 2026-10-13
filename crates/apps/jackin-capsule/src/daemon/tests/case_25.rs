// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

#[test]
fn session_launch_derives_state_root_for_second_live_pane() {
    let mut mux = two_codex_mux();
    let first = mux
        .session_launch(Some("codex-work"), None, &[], "test")
        .expect("first pane launches");
    assert_eq!(
        first.cmd.get_env("CODEX_HOME").and_then(|v| v.to_str()),
        Some("/home/agent/.codex")
    );
    let (session, _session_rx) = test_session_with_agent(24, 80, Some("codex-work".to_owned()));
    mux.session_supervisor.sessions.insert(1, session);

    let second = mux
        .session_launch(Some("codex-work"), None, &[], "test")
        .expect("second pane launches");
    assert_eq!(
        second.cmd.get_env("CODEX_HOME").and_then(|v| v.to_str()),
        Some("/home/agent/.codex/panes/0")
    );
    let third = mux
        .session_launch(Some("codex-work"), None, &[], "test")
        .expect("third pane launches");
    assert_eq!(
        third.cmd.get_env("CODEX_HOME").and_then(|v| v.to_str()),
        Some("/home/agent/.codex/panes/1")
    );
    // A sibling instance without live panes keeps its own base home.
    let sibling = mux
        .session_launch(Some("codex-personal"), None, &[], "test")
        .expect("sibling instance launches");
    assert_eq!(
        sibling.cmd.get_env("CODEX_HOME").and_then(|v| v.to_str()),
        Some("/home/agent/.codex-codex-personal")
    );
}

#[test]
fn session_launch_renders_instance_labels_for_same_agent_instances() {
    let mut mux = two_claude_mux();
    let work = mux
        .session_launch(Some("claude-work"), None, &[], "test")
        .expect("known instance launches");
    let personal = mux
        .session_launch(Some("claude-personal"), None, &[], "test")
        .expect("known instance launches");
    // Two same-agent instances are distinguishable in tab/pane chrome.
    assert_eq!(work.label, "Claude · Work");
    assert_eq!(personal.label, "Personal Claude");
}

#[test]
fn session_launch_falls_back_to_slug_title_without_instance_label() {
    let mut mux = two_claude_mux();
    mux.launch_env.launch_config.labels.clear();
    let launch = mux
        .session_launch(Some("claude-work"), None, &[], "test")
        .expect("known instance launches");
    assert_eq!(launch.label, "Claude");
    let launch = mux
        .session_launch(Some("claude-work"), Some("Z.AI"), &[], "test")
        .expect("known instance launches");
    assert_eq!(launch.label, "Claude (Z.AI)");
    let shell = mux
        .session_launch(None, None, &[], "test")
        .expect("shell launches");
    assert_eq!(shell.label, "Shell");
}

#[test]
fn record_agent_history_stamps_account_from_launch_config() {
    let mut mux = two_claude_mux();
    // `claude-work` is a sync instance with no credential-envelope entry;
    // the account still resolves from the launch config map.
    mux.record_agent_history(1, "badger".into(), Some("claude-work".into()), None);
    mux.record_agent_history(2, "wombat".into(), None, None);
    mux.record_agent_history(3, "quokka".into(), Some("ghost".into()), None);
    let history = &mux.session_supervisor.agent_history;
    assert_eq!(history[0].agent.as_deref(), Some("claude-work"));
    assert_eq!(history[0].account_id.as_deref(), Some("work"));
    // Default-provider inference resolves the slug through the instance map.
    assert_eq!(history[0].provider.as_deref(), Some("anthropic"));
    assert_eq!(history[1].agent, None);
    assert_eq!(history[1].account_id, None);
    assert_eq!(history[2].account_id, None);
}

#[test]
fn spawn_session_gate_rejects_unknown_and_ambiguous_targets() {
    let mut mux = two_claude_mux();
    let err = mux
        .spawn_session(Some("codex-work".to_owned()), &[], None)
        .expect_err("unknown instance must error at the spawn gate");
    assert!(
        err.to_string().contains("rejected spawn target"),
        "unexpected error: {err}"
    );
    // The shared slug never silently substitutes one of the two instances.
    mux.spawn_session(Some("claude".to_owned()), &[], None)
        .expect_err("ambiguous slug must error at the spawn gate");
    // ... while an exact config ID passes the gate: whatever the PTY spawn
    // itself does in this bed, the failure (if any) is never a rejection.
    if let Err(err) = mux.spawn_session(Some("claude-work".to_owned()), &[], None) {
        assert!(
            !err.to_string().contains("rejected spawn target"),
            "resolution must succeed for an exact ID: {err}"
        );
    }
}

#[test]
fn spawn_session_shell_leaves_identity_empty() {
    // `Session::spawn` parks PTY output via `spawn_blocking`: enter a
    // runtime like the other shell-spawn tests.
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("test runtime");
    let _guard = runtime.enter();
    let mut mux = two_claude_mux();
    let workdir = tempfile::tempdir().expect("test workdir");
    mux.launch_env.workdir = workdir.path().to_path_buf();
    let id = mux
        .spawn_session(None, &[], None)
        .expect("shell spawns in test bed");
    let session = mux
        .session_supervisor
        .sessions
        .get(id)
        .expect("session is registered");
    assert_eq!(session.label, "Shell");
    assert_eq!(session.agent, None);
    assert_eq!(session.account_id, None);
    let tab = mux
        .session_supervisor
        .tabs
        .last()
        .expect("spawn opens a tab");
    assert_eq!(tab.instance, None);
    assert_eq!(tab.account_id, None);
    let record = mux
        .session_supervisor
        .agent_history
        .last()
        .expect("spawn records history");
    assert_eq!(record.session_id, id);
    assert_eq!(record.agent, None);
    assert_eq!(record.account_id, None);
}
