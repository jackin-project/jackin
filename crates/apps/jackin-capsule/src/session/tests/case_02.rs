// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

#[test]
fn agent_synchronized_output_toggles_are_absorbed() {
    // The capsule's own frame brackets supersede the agent's BSU/ESU; a
    // forwarded `?2026h` whose matching `l` is dropped froze the outer
    // terminal (D6), so the grid absorbs both toggles.
    for toggle in [&b"\x1b[?2026h"[..], &b"\x1b[?2026l"[..]] {
        let drained = drained(toggle);
        assert!(
            drained.is_empty(),
            "agent ?2026 toggles must never reach the outer terminal: {drained:?}"
        );
    }
}

#[test]
fn known_csi_does_not_double_emit() {
    // Cursor positioning `\x1b[5;3H` is handled by the grid; it must not be
    // re-emitted as passthrough (which would duplicate the cursor move).
    let drained = drained(b"\x1b[5;3H");
    assert!(
        drained.iter().all(|f| !f.ends_with(b"H")),
        "grid-handled CSI leaked through: {drained:?}"
    );
}

#[test]
fn drain_returns_empty_when_no_passthrough_emitted() {
    let drained = drained(b"plain text without any escape sequences");
    assert!(drained.is_empty());
}

#[test]
fn osc_52_clipboard_dropped_when_policy_denies() {
    let drained = drained_with_policy(b"\x1b]52;c;SGVsbG8=\x07", OscPolicy::for_test_deny_all());
    assert!(
        drained.is_empty(),
        "OSC 52 leaked under deny policy: {drained:?}"
    );
}

#[test]
fn osc_9_notification_dropped_when_policy_denies() {
    let drained = drained_with_policy(b"\x1b]9;build finished\x07", OscPolicy::for_test_deny_all());
    assert!(
        drained.is_empty(),
        "OSC 9 leaked under deny policy: {drained:?}"
    );
}

#[test]
fn osc_2_title_dropped_when_policy_denies() {
    let drained = drained_with_policy(b"\x1b]2;rogue title\x07", OscPolicy::for_test_deny_all());
    assert!(
        drained.is_empty(),
        "OSC 2 leaked under deny policy: {drained:?}"
    );
}

#[test]
fn osc_8_hyperlink_dropped_when_policy_denies() {
    let drained = drained_with_policy(
        b"\x1b]8;;https://example/\x07text\x1b]8;;\x07",
        OscPolicy::for_test_deny_all(),
    );
    assert!(
        drained.is_empty(),
        "OSC 8 leaked under deny policy: {drained:?}"
    );
}

#[test]
fn osc_8_unsafe_scheme_dropped_even_when_policy_allows() {
    // A `javascript:` URI must never reach the host terminal regardless of
    // the operator's hyperlink policy.
    let drained = drained(b"\x1b]8;;javascript:alert(1)\x07");
    assert!(
        drained
            .iter()
            .all(|f| !f.windows(b"javascript".len()).any(|w| w == b"javascript")),
        "unsafe OSC 8 scheme leaked: {drained:?}"
    );
}

#[test]
fn drain_clears_pending_between_calls() {
    let mut session = test_session_with_policy(OscPolicy::for_test_allow_all());
    session.feed_pty(b"\x1b]52;c;AAAA\x07");
    let first = session.drain_passthrough();
    assert_eq!(first.len(), 1);
    let second = session.drain_passthrough();
    assert!(
        second.is_empty(),
        "drain must clear pending; got {second:?}"
    );
}

#[test]
fn build_agent_command_overrides_stale_agent_env() {
    let env = vec![("JACKIN_AGENT".to_owned(), "claude".to_owned())];
    let cmd = build_agent_command(&spawn_spec("codex", "codex-work", None, &env));

    assert_eq!(
        cmd.get_env("JACKIN_AGENT").and_then(|value| value.to_str()),
        Some("codex")
    );
}

#[test]
fn amp_command_exports_xdg_data_home_as_durable_parent() {
    let empty: Vec<(String, String)> = Vec::new();
    let spec = AgentSpawnSpec {
        agent: "amp",
        instance: "amp",
        home_dir: "/home/agent/.local/share",
        forwarded_dir: "/jackin/amp",
        model: None,
        effort: None,
        auth_mode: Some("sync"),
        env_passthrough: &empty,
        cwd: Path::new("/workspace"),
        codename: "test",
        identity: jackin_protocol::SessionIdentity {
            uid: 2_000,
            gid: 2_000,
        },
    };
    let command = build_agent_command(&spec);

    assert_eq!(
        command
            .get_env("XDG_DATA_HOME")
            .and_then(|value| value.to_str()),
        Some("/home/agent/.local/share")
    );
}

#[test]
fn agent_and_shell_children_require_explicit_github_capability() {
    let inherited = EXPLICIT_CAPABILITY_ENV_NAMES
        .iter()
        .map(|name| ((*name).to_owned(), "ambient-secret".to_owned()))
        .collect::<Vec<_>>();
    let agent = build_agent_command(&spawn_spec("codex", "codex-work", None, &inherited));
    let shell = build_shell_command(
        &inherited,
        Path::new("/workspace"),
        "test",
        jackin_protocol::SessionIdentity {
            uid: 2_000,
            gid: 2_000,
        },
    );

    for name in EXPLICIT_CAPABILITY_ENV_NAMES {
        assert!(agent.get_env(name).is_none(), "agent inherited {name}");
        assert!(shell.get_env(name).is_none(), "shell inherited {name}");
        assert!(
            !SESSION_ENV_PASSTHROUGH.contains(name),
            "ambient capability {name} is still in the session allowlist"
        );
    }
}

#[test]
fn isolated_wrapper_carries_instance_identity_into_session_exec() {
    let args = isolated_wrapper_args(
        jackin_protocol::SessionIdentity {
            uid: 2_017,
            gid: 2_017,
        },
        Some("claude-personal"),
        "/jackin/runtime/entrypoint.sh",
    );
    let args = args
        .iter()
        .map(|arg| arg.to_string_lossy().into_owned())
        .collect::<Vec<_>>();
    assert_eq!(
        args,
        vec![
            "__isolated-exec",
            "claude-personal",
            "2017",
            "2017",
            "/jackin/runtime/entrypoint.sh",
        ]
    );
}

#[test]
fn build_agent_command_injects_only_bounded_auth_mode() {
    let env = vec![(
        jackin_protocol::AUTH_MODE_ENV.to_owned(),
        "private-stale-mode".to_owned(),
    )];
    let cmd = build_agent_command(&spawn_spec("codex", "codex-work", Some("api_key"), &env));

    assert_eq!(
        cmd.get_env(jackin_protocol::AUTH_MODE_ENV)
            .and_then(|value| value.to_str()),
        Some("api_key")
    );
}

#[test]
fn build_agent_command_uses_stable_pane_term() {
    let env = vec![("TERM".to_owned(), "xterm-ghostty".to_owned())];
    let cmd = build_agent_command(&spawn_spec("codex", "codex-work", None, &env));

    assert_eq!(
        cmd.get_env("TERM").and_then(|value| value.to_str()),
        Some("xterm-256color")
    );
}

#[test]
fn build_agent_command_advertises_truecolor() {
    let env = vec![("COLORTERM".to_owned(), "24bit".to_owned())];
    let cmd = build_agent_command(&spawn_spec("claude", "claude-work", None, &env));

    assert_eq!(
        cmd.get_env("COLORTERM").and_then(|value| value.to_str()),
        Some("truecolor")
    );
}

#[test]
fn build_shell_command_advertises_truecolor() {
    let env = vec![("COLORTERM".to_owned(), "false".to_owned())];
    let cmd = build_shell_command(
        &env,
        Path::new("/workspace"),
        "test",
        jackin_protocol::SessionIdentity {
            uid: 2_000,
            gid: 2_000,
        },
    );

    assert_eq!(
        cmd.get_env("COLORTERM").and_then(|value| value.to_str()),
        Some("truecolor")
    );
}

#[test]
fn agent_model_args_match_cli_contracts() {
    assert_eq!(
        agent_model_args("claude", Some("sonnet")),
        vec!["--model", "sonnet"]
    );
    assert_eq!(
        agent_model_args("codex", Some("gpt-5")),
        vec!["-m", "gpt-5"]
    );
    assert_eq!(
        agent_model_args("kimi", Some("kimi-k2")),
        vec!["--model", "kimi-k2"]
    );
    assert_eq!(
        agent_model_args("omp", Some("openrouter/sonnet")),
        vec!["--model", "openrouter/sonnet"]
    );
    assert_eq!(
        agent_model_args("hermes", Some("openrouter/sonnet")),
        vec!["--model", "openrouter/sonnet"]
    );
    assert_eq!(
        agent_model_args("opencode", Some("zai/glm")),
        vec!["-m", "zai/glm"]
    );
    assert_eq!(
        agent_model_args("grok", Some("grok-build-0.1")),
        vec!["-m", "grok-build-0.1"]
    );
    assert!(agent_model_args("amp", None).is_empty());
    assert!(agent_model_args("amp", Some("ignored")).is_empty());
}

#[test]
fn build_shell_command_removes_stale_agent_env() {
    let env = vec![("JACKIN_AGENT".to_owned(), "claude".to_owned())];
    let cmd = build_shell_command(
        &env,
        Path::new("/workspace"),
        "test",
        jackin_protocol::SessionIdentity {
            uid: 2_000,
            gid: 2_000,
        },
    );

    assert!(cmd.get_env("JACKIN_AGENT").is_none());
}

#[test]
fn build_shell_command_restores_container_home_and_rejects_foreign_home() {
    let env = vec![("HOME".to_owned(), "/foreign-home".to_owned())];
    let cmd = build_shell_command(
        &env,
        Path::new("/workspace"),
        "test",
        jackin_protocol::SessionIdentity {
            uid: 2_000,
            gid: 2_000,
        },
    );

    let daemon_home = std::env::var("HOME").ok();
    assert_eq!(
        cmd.get_env("HOME").and_then(|value| value.to_str()),
        daemon_home.as_deref(),
        "shell keeps the daemon's container HOME, never a passthrough value"
    );
}

#[test]
fn pty_output_does_not_change_state() {
    // The old flap engine flipped state on every PTY byte (Idle→Working) and
    // could not hold a blocked dialog through its own repaint. After Phase 2,
    // PTY output updates recency only and never authors state.
    let mut session = test_session_with_policy(OscPolicy::default());
    session.state = AgentState::Blocked;
    let before = session.last_output_at;
    session.feed_pty(b"\x1b[2K some redrawn dialog frame\r\n");
    assert_eq!(
        session.state,
        AgentState::Blocked,
        "PTY output must not author state"
    );
    assert!(
        session.last_output_at >= before,
        "PTY output still updates recency evidence"
    );
}

#[test]
fn operator_input_does_not_change_state() {
    // A keystroke inside a blocked dialog used to flip Blocked→Working and
    // re-notify. After Phase 2 it updates the input timestamp and reports
    // whether it cleared a latched blocker, but never authors state.
    let mut session = test_session_with_policy(OscPolicy::default());

    session.state = AgentState::Blocked;
    assert!(session.mark_operator_input(), "reports it was blocked");
    assert_eq!(
        session.state,
        AgentState::Blocked,
        "operator input must not author state"
    );

    session.state = AgentState::Done;
    assert!(!session.mark_operator_input());
    assert_eq!(
        session.state,
        AgentState::Done,
        "operator input must not author state"
    );
}

#[test]
fn redraw_soak_produces_zero_state_transitions() {
    // The flap engine produced a Blocked↔Working flip on every redraw frame.
    // Replaying a permission-dialog repaint many times must now yield zero
    // state changes at the session level. (The real single Blocked transition
    // arrives with the Phase 3/8 evidence pipeline; this guards that redraws
    // alone never author state — the regression that motivated this work.)
    let mut session = test_session_with_policy(OscPolicy::default());
    let start = session.state;
    let frame =
        b"\x1b[2K\x1b[1;1H Do you want to proceed?\r\n  1. Yes\r\n  2. No\r\n  esc to cancel\r\n";
    let mut transitions = 0;
    let mut prev = start;
    for _ in 0..150 {
        session.feed_pty(frame);
        if session.state != prev {
            transitions += 1;
            prev = session.state;
        }
    }
    assert_eq!(
        transitions, 0,
        "redraws must not author any state transition"
    );
    assert_eq!(session.state, start);
}
