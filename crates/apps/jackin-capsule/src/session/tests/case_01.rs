// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

#[test]
fn effective_state_flaps_count_once_per_rolling_window_episode() {
    let mut session = test_session_with_policy(OscPolicy::default());
    let started = std::time::Instant::now();
    assert!(!session.record_status_transition(started));
    assert!(!session.record_status_transition(started + std::time::Duration::from_secs(10)));
    assert!(session.record_status_transition(started + std::time::Duration::from_secs(20)));
    assert!(!session.record_status_transition(started + std::time::Duration::from_secs(25)));

    assert!(!session.record_status_transition(started + std::time::Duration::from_secs(61)));
    assert!(!session.record_status_transition(started + std::time::Duration::from_secs(70)));
    assert!(session.record_status_transition(started + std::time::Duration::from_secs(80)));
}

#[test]
fn advance_status_publishes_screen_blocked_through_full_tick() {
    let now = std::time::Instant::now();
    let mut session = test_session_with_policy(OscPolicy::default());
    session.child_pid = Some(42);
    session.feed_pty(b"approve?");
    let registry = status_test_registry();
    let mut sampler = StaticProcessSampler::foreground_agent(42, Agent::Codex);
    let rows = session.visible_screen_rows();
    assert!(rows.iter().any(|row| row.contains("approve?")));
    assert!(registry.evaluate(session.agent.as_deref(), &rows).is_some());

    let tick = session.advance_status_with_process_sampler(Some(&registry), &mut sampler, now);

    assert_eq!(session.state, AgentState::Blocked);
    assert_eq!(
        tick.transition
            .as_ref()
            .map(|transition| transition.effective),
        Some(AgentState::Blocked)
    );
    let report = session.status.report(session.agent.clone());
    assert_eq!(report.raw_state, RawAgentState::Blocked);
    assert_eq!(report.source, AgentStatusSource::VisibleScreen);
    assert!(report.visible_blocker);
}

#[test]
fn process_sampler_double_drives_process_evidence_on_any_host() {
    let mut session = test_session_with_policy(OscPolicy::default());
    session.child_pid = Some(42);
    let mut sampler = StaticProcessSampler::foreground_agent(42, Agent::Codex);
    sampler.descendants = 2;
    sampler.cpu_delta = 7;

    let evidence = session.sample_process_evidence_with(&mut sampler, std::time::Instant::now());

    assert!(evidence.physics_sampled);
    assert!(evidence.child_alive);
    assert!(evidence.root_is_agent);
    assert!(evidence.foreground_is_agent);
    assert_eq!(evidence.foreground_pgid, Some(42));
    assert_eq!(evidence.child_process_count, 2);
    assert_eq!(evidence.cpu_jiffies_delta, 7);
}

#[test]
fn advance_status_publishes_screen_idle_as_done_when_unseen() {
    let now = std::time::Instant::now();
    let mut session = test_session_with_policy(OscPolicy::default());
    session.child_pid = Some(42);
    session.state = AgentState::Working;
    session.status.effective = AgentState::Working;
    session.status.raw = RawAgentState::Working;
    session.status.confidence = AgentStatusConfidence::Strong;
    session.status.seen = false;
    session.feed_pty(b"ready");
    let registry = status_test_registry();
    let mut sampler = StaticProcessSampler::foreground_agent(42, Agent::Codex);
    let rows = session.visible_screen_rows();
    assert!(rows.iter().any(|row| row.contains("ready")));
    assert!(registry.evaluate(session.agent.as_deref(), &rows).is_some());

    let tick = session.advance_status_with_process_sampler(Some(&registry), &mut sampler, now);

    assert_eq!(session.state, AgentState::Done);
    assert_eq!(
        tick.transition
            .as_ref()
            .map(|transition| transition.effective),
        Some(AgentState::Done)
    );
    let report = session.status.report(session.agent.clone());
    assert_eq!(report.raw_state, RawAgentState::Idle);
    assert_eq!(report.source, AgentStatusSource::VisibleScreen);
    assert!(report.visible_idle);
}

#[test]
fn advance_status_publishes_fresh_authority_with_injected_foreground_identity() {
    let now = std::time::Instant::now();
    let mut session = test_session_with_policy(OscPolicy::default());
    session.child_pid = Some(42);
    session.authority = Some(AuthorityEvidence {
        source_id: "opencode-plugin".to_owned(),
        grade: AuthorityGrade::Complete,
        mapped_state: RawAgentState::Working,
        pending_permission: false,
        last_event: now,
        notes: Vec::new(),
    });
    let mut sampler = StaticProcessSampler::foreground_agent(42, Agent::Codex);

    let tick = session.advance_status_with_process_sampler(None, &mut sampler, now);

    assert_eq!(session.state, AgentState::Working);
    assert_eq!(
        tick.transition
            .as_ref()
            .map(|transition| transition.effective),
        Some(AgentState::Working)
    );
    let report = session.status.report(session.agent.clone());
    assert_eq!(report.raw_state, RawAgentState::Working);
    assert_eq!(
        report.source,
        AgentStatusSource::Reported {
            source_id: "opencode-plugin".to_owned()
        }
    );
}

#[test]
fn advance_status_does_not_publish_working_from_helper_physics_alone() {
    let now = std::time::Instant::now();
    let mut session = test_session_with_policy(OscPolicy::default());
    session.child_pid = Some(42);
    let mut sampler = StaticProcessSampler::foreground_agent(42, Agent::Codex);
    sampler.descendants = 1;
    sampler.cpu_delta = 1;

    let tick = session.advance_status_with_process_sampler(None, &mut sampler, now);

    assert_eq!(session.state, AgentState::Unknown);
    assert!(tick.transition.is_none());
    let report = session.status.report(session.agent.clone());
    assert_eq!(report.raw_state, RawAgentState::Unknown);
    assert_eq!(report.source, AgentStatusSource::None);
}

#[test]
fn advance_status_publishes_unknown_when_no_evidence_matches() {
    let now = std::time::Instant::now();
    let mut session = test_session_with_policy(OscPolicy::default());
    session.child_pid = Some(42);
    session.state = AgentState::Working;
    session.status.effective = AgentState::Working;
    session.status.raw = RawAgentState::Working;
    session.status.confidence = AgentStatusConfidence::Strong;
    let mut sampler = StaticProcessSampler::foreground_agent(42, Agent::Codex);

    let tick = session.advance_status_with_process_sampler(None, &mut sampler, now);

    assert_eq!(session.state, AgentState::Unknown);
    assert_eq!(
        tick.transition
            .as_ref()
            .map(|transition| transition.effective),
        Some(AgentState::Unknown)
    );
    let report = session.status.report(session.agent.clone());
    assert_eq!(report.raw_state, RawAgentState::Unknown);
    assert_eq!(report.source, AgentStatusSource::None);
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
    let mut probe = termpane::DamageGrid::new(24, 80, 100);
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

#[test]
fn osc_52_clipboard_write_is_re_emitted_when_policy_allows() {
    let drained = drained_with_policy(b"\x1b]52;c;SGVsbG8=\x07", OscPolicy::for_test_allow_all());
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
