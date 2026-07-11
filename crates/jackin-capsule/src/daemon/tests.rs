//! Unit tests for `jackin-capsule` daemon: input dispatch, session management,
//! tab lifecycle, git context, status-bar rendering, and PTY session behavior.
use super::*;
use std::io;
use std::sync::{Arc, Mutex};

use crate::pr_context::{command_output_or_lookup_error, command_stdout_trimmed};
use crate::protocol::attach::read_server_frame;
use crate::tui::components::dialog::PullRequestStatus;
use portable_pty::{ChildKiller, MasterPty, PtySize};
use tokio::io::AsyncReadExt;

#[derive(Debug)]
struct NullChildKiller;

impl ChildKiller for NullChildKiller {
    fn kill(&mut self) -> io::Result<()> {
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

    fn try_clone_reader(&self) -> Result<Box<dyn io::Read + Send>> {
        Ok(Box::new(io::empty()))
    }

    fn take_writer(&self) -> Result<Box<dyn io::Write + Send>> {
        Ok(Box::new(io::sink()))
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
    fn tty_name(&self) -> Option<PathBuf> {
        None
    }
}

#[test]
fn status_tick_select_arm_stays_above_pty_output() {
    let source = include_str!("../daemon.rs");
    let tick_arm = source
        .find("_ = state_ticker.tick()")
        .unwrap_or_else(|| panic!("state ticker select arm missing"));
    let output_arm = source
        .find("Some(event) = mux.event_rx.recv()")
        .unwrap_or_else(|| panic!("PTY event select arm missing"));
    assert!(
        tick_arm < output_arm,
        "state ticker must stay above PTY output in the biased select"
    );
}

#[tokio::test(start_paused = true)]
async fn ready_status_tick_wins_over_ready_pty_output() {
    let (tx, mut rx) = mpsc::unbounded_channel();
    let mut state_ticker = interval(STATE_TICK_INTERVAL);
    state_ticker.tick().await;
    tokio::time::advance(STATE_TICK_INTERVAL).await;
    tx.send(SessionEvent::Output {
        session_id: 1,
        data: b"busy".to_vec(),
    })
    .unwrap_or_else(|_| panic!("test receiver must be open"));

    let selected = tokio::select! {
        biased;
        _ = state_ticker.tick() => "tick",
        Some(_) = rx.recv() => "output",
    };

    assert_eq!(
        selected, "tick",
        "ready status tick must beat ready PTY output under biased select"
    );
}

#[test]
fn spawn_failure_popup_stays_open_until_dismissed() {
    let contains = |frame: &[u8], needle: &[u8]| frame.windows(needle.len()).any(|w| w == needle);
    let mut mux = single_pane_tab_mux();
    let (session, rx) = test_session(20, 78);
    drop(rx);
    mux.sessions.insert(1, session);
    mux.open_spawn_failure_dialog("boom: agent slug rejected".to_owned());
    let frame = compose_after(&mut mux, FullRedrawReason::DialogChange);
    assert!(
        contains(&frame, b"boom: agent slug rejected"),
        "spawn failure popup must ride the composed frame: {:?}",
        String::from_utf8_lossy(&frame)
    );
    assert!(matches!(mux.dialog_top(), Some(Dialog::SpawnFailure(_))));

    drop(handle_input_frame(
        &mut mux,
        InputEvent::Data(b"x".to_vec()),
    ));
    assert!(
        matches!(mux.dialog_top(), Some(Dialog::SpawnFailure(_))),
        "printable input must not dismiss the failure popup"
    );

    drop(handle_input_frame(
        &mut mux,
        InputEvent::Data(b"\x1b".to_vec()),
    ));
    assert!(mux.dialog_top().is_none(), "Esc must dismiss the popup");
}

#[test]
fn screen_detection_disabled_message_is_operator_visible() {
    let err = anyhow::anyhow!("bad embedded pack");
    let message = screen_detection_disabled_message(&err);

    assert!(
        message.contains("Agent status screen detection is off"),
        "message must name the disabled feature: {message}"
    );
    assert!(
        message.contains("bad embedded pack"),
        "message must carry the load failure: {message}"
    );
}

fn test_mux(rows: u16, cols: u16) -> Multiplexer {
    Multiplexer::new(
        rows,
        cols,
        CapsuleConfig {
            role: "test-role".to_owned(),
            workdir: "/workspace".to_owned(),
            agents: Vec::new(),
            models: BTreeMap::new(),
            provider_models: BTreeMap::new(),
            initial_provider: None,
            claude_marketplaces: Vec::new(),
            claude_plugins: Vec::new(),
            exec_bindings: Vec::new(),
            dirty_exit_policy: None,
            isolated_worktrees: Vec::new(),
        },
    )
    .unwrap_or_else(|error| panic!("test multiplexer construction failed: {error}"))
}

#[test]
fn begin_exec_picker_supersedes_pending_reply_and_dialog() {
    let mut mux = test_mux(40, 20);
    let (tx1, mut rx1) = tokio::sync::oneshot::channel();
    mux.begin_exec_picker("cmd1".to_owned(), vec![], tx1);

    // A second jackin-exec request arrives while the first picker is pending.
    let (tx2, _rx2) = tokio::sync::oneshot::channel();
    mux.begin_exec_picker("cmd2".to_owned(), vec![], tx2);

    // The prior client must get a structured denial, not a hung/closed socket.
    match rx1.try_recv() {
        Ok(ServerMsg::ExecDenied { reason }) => {
            assert!(reason.contains("superseded"), "unexpected reason: {reason}");
        }
        other => panic!("expected ExecDenied for the superseded request, got {other:?}"),
    }

    // Exactly one ExecPicker remains, and it is for the newer command — so a
    // later confirm can't resolve credentials for the stale one.
    match mux.dialog_top() {
        Some(Dialog::ExecPicker(state)) => assert_eq!(state.command, "cmd2"),
        other => panic!("expected a single ExecPicker(cmd2) on top, got {other:?}"),
    }
}

/// Compose the frame an invalidation with `reason` produces — the
/// derived-rendering equivalent of the old per-tier compose calls.
fn compose_after(mux: &mut Multiplexer, reason: FullRedrawReason) -> Vec<u8> {
    mux.invalidate(reason);
    mux.compose_pending_frame()
}

/// Drive `handle_input` then compose, mirroring one daemon loop pass.
/// `None` when the event did not invalidate anything.
fn handle_input_frame(mux: &mut Multiplexer, event: InputEvent) -> Option<Vec<u8>> {
    mux.handle_input(event);
    let frame = mux.compose_pending_frame();
    (!frame.is_empty()).then_some(frame)
}

/// Drive `apply_action` then compose; `None` when nothing invalidated.
fn apply_action_frame(mux: &mut Multiplexer, action: Action) -> Option<Vec<u8>> {
    mux.apply_action(action);
    let frame = mux.compose_pending_frame();
    (!frame.is_empty()).then_some(frame)
}

fn seed_usage_dialog_for_refresh_test(mux: &mut Multiplexer) {
    let (mut session, _session_rx) = test_session_with_agent(24, 80, Some("codex".to_owned()));
    session.provider = Some(crate::session::SessionProvider {
        label: "OpenAI".to_owned(),
        env_overrides: Vec::new(),
    });
    mux.sessions.insert(1, session);
    mux.tabs[0] = Tab::new_single("Codex", 1, "test");
    let mut stale = jackin_protocol::control::FocusedUsageView::unavailable("seed", 1);
    stale.updated_label = "seed".to_owned();
    stale.status_bar_label = "seed".to_owned();
    mux.dialog_push(Dialog::new_usage(stale));
}

/// Drive `handle_palette_command` then compose; `None` when empty.
fn palette_command_frame(mux: &mut Multiplexer, cmd: PaletteCommand) -> Option<Vec<u8>> {
    mux.handle_palette_command(cmd);
    let frame = mux.compose_pending_frame();
    (!frame.is_empty()).then_some(frame)
}

/// Drive `handle_prefix_command` then compose; `None` when empty.
fn prefix_command_frame(mux: &mut Multiplexer, cmd: PrefixCommand) -> Option<Vec<u8>> {
    mux.handle_prefix_command(cmd);
    let frame = mux.compose_pending_frame();
    (!frame.is_empty()).then_some(frame)
}

pub(super) fn single_pane_tab_mux() -> Multiplexer {
    single_pane_tab_mux_with_size(24, 80)
}

fn single_pane_tab_mux_with_size(rows: u16, cols: u16) -> Multiplexer {
    let mut mux = test_mux(24, 80);
    mux.resize(rows, cols);
    mux.tabs.push(Tab::new_single("Shell", 1, "test"));
    // Drain the construction-time Resize invalidation the way the real
    // attach burst does, so tests observe only their own state changes.
    drop(mux.compose_pending_frame());
    mux
}

fn frame_contains_screen_erase(frame: &[u8]) -> bool {
    frame.windows(b"\x1b[2J".len()).any(|w| w == b"\x1b[2J")
}

#[test]
fn control_reply_for_request_shapes_usage_variants() {
    let mut mux = single_pane_tab_mux();
    let (mut session, _session_rx) = test_session_with_agent(24, 80, Some("codex".to_owned()));
    session.provider = Some(crate::session::SessionProvider {
        label: "OpenAI".to_owned(),
        env_overrides: Vec::new(),
    });
    mux.sessions.insert(1, session);
    mux.tabs[0] = Tab::new_single("Codex", 1, "test");
    let focused = control_reply_for_request(&mut mux, ClientMsg::UsageFocused);
    assert!(matches!(focused, ServerMsg::UsageFocused { .. }));

    let refreshed = control_reply_for_request(&mut mux, ClientMsg::UsageRefreshFocused);
    assert!(matches!(refreshed, ServerMsg::UsageFocused { .. }));
    assert!(
        mux.pending_usage_refresh.is_some(),
        "refresh request should queue provider work instead of probing inline"
    );

    let accounts = control_reply_for_request(&mut mux, ClientMsg::UsageAccountList);
    assert!(matches!(accounts, ServerMsg::UsageAccounts { .. }));
}

#[test]
fn control_reply_report_runtime_event_applies_to_session_and_acks() {
    let mut mux = single_pane_tab_mux();
    let (session, _session_rx) = test_session_with_agent(24, 80, Some("opencode".to_owned()));
    mux.sessions.insert(1, session);

    let reply = control_reply_for_request(
        &mut mux,
        ClientMsg::ReportRuntimeEvent {
            session_id: 1,
            source_id: "hook-opencode-1".to_owned(),
            runtime: "opencode".to_owned(),
            event: "permission.asked".to_owned(),
            payload: None,
        },
    );

    assert!(matches!(reply, ServerMsg::Ack));
    let authority = mux.sessions[&1]
        .authority
        .as_ref()
        .expect("event applied to the addressed session's authority");
    assert_eq!(authority.source_id, "hook-opencode-1");
    assert!(authority.pending_permission);
}

#[test]
fn control_reply_runtime_event_and_capture_for_unknown_session_still_ack() {
    // The hook must never be blocked or failed by a stale/wrong session id: both
    // control messages Ack (and do not panic) when the session is absent.
    let mut mux = single_pane_tab_mux();

    let event_reply = control_reply_for_request(
        &mut mux,
        ClientMsg::ReportRuntimeEvent {
            session_id: 999,
            source_id: "hook-opencode-1".to_owned(),
            runtime: "opencode".to_owned(),
            event: "permission.asked".to_owned(),
            payload: None,
        },
    );
    assert!(matches!(event_reply, ServerMsg::Ack));

    let capture_reply =
        control_reply_for_request(&mut mux, ClientMsg::StatusCapture { session_id: 999 });
    assert!(matches!(capture_reply, ServerMsg::Ack));
}

#[test]
fn control_usage_account_list_uses_in_memory_cache() {
    let mut mux = single_pane_tab_mux();
    let mut view = jackin_protocol::control::FocusedUsageView::unavailable("seed", 123);
    view.focused_agent = Some("codex".to_owned());
    view.focused_provider = Some("OpenAI".to_owned());
    view.account = jackin_protocol::control::FocusedAccountHeader {
        provider_label: "OpenAI / Codex".to_owned(),
        account_label: "codex@example.com".to_owned(),
        username: None,
        plan_label: Some("Pro 20x".to_owned()),
        credential_origin: None,
    };
    view.status = jackin_protocol::control::UsageSnapshotStatus::Fresh;
    view.source = jackin_protocol::control::UsageSource::ProviderApi;
    view.confidence = jackin_protocol::control::UsageConfidence::Authoritative;
    view.buckets = vec![jackin_protocol::control::QuotaBucketView {
        used_money: None,
        limit_money: None,
        severity: jackin_protocol::control::UsageSeverity::default(),
        label: "Session".to_owned(),
        used_label: Some("63% used".to_owned()),
        limit_label: Some("100%".to_owned()),
        remaining_percent: Some(37),
        reset_label: Some("Resets in 2h".to_owned()),
        resets_at: None,
        status_slot: None,
        pace_label: None,
        status: jackin_protocol::control::UsageSnapshotStatus::Fresh,
    }];
    mux.usage_cache
        .insert_snapshot_for_test("codex", Some("OpenAI"), view);

    let accounts = control_reply_for_request(&mut mux, ClientMsg::UsageAccountList);

    let ServerMsg::UsageAccounts { accounts } = accounts else {
        panic!("usage accounts response expected");
    };
    assert_eq!(accounts.len(), 1);
    assert_eq!(accounts[0].provider, "OpenAI / Codex");
    assert_eq!(accounts[0].account_label, "codex@example.com");
    assert_eq!(accounts[0].used_amount, Some(63));
}

#[test]
fn apply_dialog_action_refresh_usage_queues_refresh_without_replacing_dialog() {
    let mut mux = single_pane_tab_mux();
    seed_usage_dialog_for_refresh_test(&mut mux);

    mux.apply_dialog_action(DialogAction::RefreshUsage);

    let Dialog::Usage { view, .. } = mux.dialog_top().expect("usage dialog still open") else {
        panic!("refresh usage action must keep usage dialog open");
    };
    // Bug 1: the action only QUEUES the refresh (pending_usage_refresh set
    // below); the "refreshing" marker is applied by the dialog tick only when a
    // refresh task is genuinely in flight. No task is spawned here, so the marker
    // must NOT appear — it is no longer driven by the scheduling flag.
    assert!(
        !view.updated_label.contains("refreshing"),
        "{:?}",
        view.updated_label
    );
    assert_eq!(view.status_bar_label, "seed");
    assert_eq!(
        mux.pending_usage_refresh,
        Some(crate::usage::UsageRefreshTarget {
            agent: "codex".to_owned(),
            provider: Some("OpenAI".to_owned())
        })
    );
}

#[test]
fn apply_action_refresh_usage_queues_refresh_without_replacing_dialog() {
    let mut mux = single_pane_tab_mux();
    seed_usage_dialog_for_refresh_test(&mut mux);

    mux.apply_action(Action::RefreshUsage);

    let Dialog::Usage { view, .. } = mux.dialog_top().expect("usage dialog still open") else {
        panic!("refresh usage action must keep usage dialog open");
    };
    // Bug 1: the action only QUEUES the refresh (pending_usage_refresh set
    // below); the "refreshing" marker is applied by the dialog tick only when a
    // refresh task is genuinely in flight. No task is spawned here, so the marker
    // must NOT appear — it is no longer driven by the scheduling flag.
    assert!(
        !view.updated_label.contains("refreshing"),
        "{:?}",
        view.updated_label
    );
    assert_eq!(view.status_bar_label, "seed");
    assert_eq!(
        mux.pending_usage_refresh,
        Some(crate::usage::UsageRefreshTarget {
            agent: "codex".to_owned(),
            provider: Some("OpenAI".to_owned())
        })
    );
}

#[test]
fn apply_dialog_action_switch_usage_provider_updates_focused_provider() {
    let mut mux = single_pane_tab_mux();
    let (session, _session_rx) = test_session_with_agent(24, 80, Some("codex".to_owned()));
    mux.sessions.insert(1, session);
    mux.tabs[0] = Tab::new_single("Codex", 1, "test");
    mux.dialog_push(Dialog::new_usage(
        jackin_protocol::control::FocusedUsageView {
            focused_provider: Some("MiniMax".to_owned()),
            account: jackin_protocol::control::FocusedAccountHeader {
                provider_label: "Usage".to_owned(),
                account_label: "seed".to_owned(),
                username: None,
                plan_label: None,
                credential_origin: None,
            },
            ..jackin_protocol::control::FocusedUsageView::unavailable("seed", 1)
        },
    ));

    mux.apply_dialog_action(DialogAction::SwitchUsageProvider {
        provider_label: "Claude".to_owned(),
    });

    let Dialog::Usage { view, .. } = mux.dialog_top().expect("usage dialog still open") else {
        panic!("switch usage provider action must keep usage dialog open");
    };
    assert_eq!(view.focused_provider.as_deref(), Some("Claude"));
    assert_eq!(view.account.provider_label, "Anthropic / Claude");
    assert_eq!(
        mux.pending_usage_refresh,
        Some(crate::usage::UsageRefreshTarget {
            agent: "codex".to_owned(),
            provider: Some("Claude".to_owned())
        })
    );
}

#[test]
fn apply_action_open_usage_queues_focused_provider_refresh() {
    let mut mux = single_pane_tab_mux();
    let (mut session, _session_rx) = test_session_with_agent(24, 80, Some("codex".to_owned()));
    session.provider = Some(crate::session::SessionProvider {
        label: "OpenAI".to_owned(),
        env_overrides: Vec::new(),
    });
    mux.sessions.insert(1, session);
    mux.tabs[0] = Tab::new_single("Codex", 1, "test");

    mux.apply_action(Action::OpenUsage);

    assert!(matches!(mux.dialog_top(), Some(Dialog::Usage { .. })));
    let Dialog::Usage { view, .. } = mux.dialog_top().expect("usage dialog open") else {
        panic!("usage dialog expected");
    };
    // Bug 1: the action only QUEUES the refresh (pending_usage_refresh set
    // below); the "refreshing" marker is applied by the dialog tick only when a
    // refresh task is genuinely in flight. No task is spawned here, so the marker
    // must NOT appear — it is no longer driven by the scheduling flag.
    assert!(
        !view.updated_label.contains("refreshing"),
        "{:?}",
        view.updated_label
    );
    assert_eq!(
        mux.pending_usage_refresh,
        Some(crate::usage::UsageRefreshTarget {
            agent: "codex".to_owned(),
            provider: Some("OpenAI".to_owned())
        })
    );
}

#[test]
fn open_usage_dialog_refreshes_visible_relative_timestamp_from_cache() {
    let mut mux = single_pane_tab_mux();
    let (mut session, _session_rx) = test_session_with_agent(24, 80, Some("codex".to_owned()));
    session.provider = Some(crate::session::SessionProvider {
        label: "OpenAI".to_owned(),
        env_overrides: Vec::new(),
    });
    mux.sessions.insert(1, session);
    mux.tabs[0] = Tab::new_single("Codex", 1, "test");
    let now_epoch = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("system time after epoch")
        .as_secs() as i64;
    let cached = jackin_protocol::control::FocusedUsageView {
        focused_agent: Some("codex".to_owned()),
        focused_provider: Some("OpenAI".to_owned()),
        account: jackin_protocol::control::FocusedAccountHeader {
            provider_label: "Codex".to_owned(),
            account_label: "alexey@example.com".to_owned(),
            username: None,
            plan_label: Some("Pro 20x".to_owned()),
            credential_origin: None,
        },
        buckets: vec![jackin_protocol::control::QuotaBucketView {
            used_money: None,
            limit_money: None,
            severity: jackin_protocol::control::UsageSeverity::default(),
            label: "Session".to_owned(),
            used_label: Some("63% used".to_owned()),
            limit_label: Some("100%".to_owned()),
            remaining_percent: Some(37),
            reset_label: Some("Resets at 15:00 UTC".to_owned()),
            resets_at: None,
            status_slot: None,
            pace_label: None,
            status: jackin_protocol::control::UsageSnapshotStatus::Fresh,
        }],
        status: jackin_protocol::control::UsageSnapshotStatus::Fresh,
        source: jackin_protocol::control::UsageSource::Cli,
        confidence: jackin_protocol::control::UsageConfidence::Authoritative,
        fetched_at_epoch: now_epoch - 120,
        updated_label: "Updated just now".to_owned(),
        status_bar_label: "Codex Session: 63% used · 37% left".to_owned(),
        tabs: Vec::new(),
        last_error: None,
    };
    mux.usage_cache
        .insert_snapshot_for_test("codex", Some("OpenAI"), cached);
    let mut view = jackin_protocol::control::FocusedUsageView::unavailable("seed", 1);
    view.updated_label = "Updated just now".to_owned();
    mux.dialog_push(Dialog::new_usage(view));

    assert!(mux.refresh_open_usage_dialog_from_cache());

    let Dialog::Usage { view, .. } = mux.dialog_top().expect("usage dialog open") else {
        panic!("usage dialog expected");
    };
    assert_eq!(view.updated_label, "Updated 2m ago");
}

fn pull_request_fixture(number: u64) -> PullRequestInfo {
    PullRequestInfo {
        number,
        title: "Surface PR context in Capsule".to_owned(),
        url: format!("https://github.com/jackin-project/jackin/pull/{number}"),
        is_draft: false,
        checks: None,
    }
}

/// Build a 40-char SHA-1-shaped OID from a single hex nibble
/// repeated 40 times. Tests want distinguishable OIDs ("H1", "H2",
/// "H3") without the eye-strain of typing 40 hex digits inline.
fn oid(nibble: char) -> Oid {
    assert!(nibble.is_ascii_hexdigit(), "nibble must be 0-9/a-f");
    Oid::parse(&nibble.to_string().repeat(40)).expect("40 hex chars is a valid Oid")
}

#[test]
fn clipboard_image_error_reason_uses_stable_compact_codes() {
    let cases = [
        (
            anyhow::anyhow!("clipboard image transfer is empty"),
            "empty",
        ),
        (
            anyhow::anyhow!("clipboard image transfer 67108865 bytes exceeds cap 67108864"),
            "oversize",
        ),
        (
            anyhow::anyhow!("clipboard image magic bytes do not match Png"),
            "signature-mismatch",
        ),
        (
            anyhow::anyhow!("clipboard image transfer 7 SHA-256 mismatch"),
            "digest-mismatch",
        ),
        (
            anyhow::anyhow!("clipboard image transfer 7 offset 4 did not match expected 8"),
            "offset-mismatch",
        ),
        (
            anyhow::anyhow!("clipboard image transfer 7 has no active start"),
            "missing-transfer",
        ),
        (
            anyhow::anyhow!("Error: Can't open display: (null)"),
            "backend-unavailable",
        ),
        (
            anyhow::anyhow!("writing /jackin/run/clipboard/clipboard-1.png"),
            "staging-io",
        ),
        (anyhow::anyhow!("unexpected payload"), "invalid-payload"),
    ];

    for (err, expected) in cases {
        assert_eq!(clipboard_image_error_reason(&err), expected);
    }
}

fn branch(name: &str) -> BranchName {
    BranchName::parse(name).expect("test branch names must parse")
}

/// Lay out a fake worktree under `temp` and return the
/// (`workdir`, `common_git_dir`) paths the test can then write
/// into. The `workdir/.git` pointer file is written so
/// `read_context_from_git_metadata` discovers the per-worktree
/// gitdir; the caller is responsible for writing HEAD + any
/// `commondir` / `refs/heads/*` ref files specific to the
/// scenario under test.
fn make_worktree_layout(temp: &Path, worktree_name: &str) -> (PathBuf, PathBuf) {
    let workdir = temp.join("workdir");
    let common_git = temp.join("repo/.git");
    let wt_git = common_git.join(format!("worktrees/{worktree_name}"));
    std::fs::create_dir_all(&workdir).unwrap();
    std::fs::create_dir_all(&wt_git).unwrap();
    std::fs::write(
        workdir.join(".git"),
        format!("gitdir: {}\n", wt_git.display()),
    )
    .unwrap();
    (workdir, common_git)
}

/// Construct the state production would land in after
/// `maybe_spawn_pull_request_context_lookup` actually spawned a
/// worker for `branch` (without shelling out to `gh`):
/// `request_id` is the id the worker carries, `in_flight = true`
/// gates the next spawn, `pull_request_context_branch` is the
/// branch the worker was started for, and a `GitHubContext`
/// dialog is open so apply-path redraw decisions exercise the
/// dialog-open code path.
fn arm_pending_pr_lookup(mux: &mut Multiplexer, branch_name: &str, request_id: u64) {
    mux.pull_request_lookup.request_id = request_id;
    mux.pull_request_lookup.in_flight = true;
    mux.pull_request_context_branch = Some(branch(branch_name));
    mux.open_github_context_dialog(Instant::now());
}

#[test]
fn outer_terminal_title_uses_workspace_and_pr_title() {
    let title = compose_outer_terminal_title(
        Path::new("/Users/operator/Projects/jackin"),
        Some("feat/capsule-pr-context-bar"),
        Some(&pull_request_fixture(436)),
    );

    assert_eq!(title, "jackin · PR #436 · Surface PR context in Capsule");
}

#[test]
fn outer_terminal_title_falls_back_to_branch_without_pr() {
    let title = compose_outer_terminal_title(
        Path::new("/Users/operator/Projects/jackin"),
        Some("feat/capsule-pr-context-bar"),
        None,
    );

    assert_eq!(title, "jackin · feat/capsule-pr-context-bar");
}

#[test]
fn outer_terminal_title_sanitizes_control_bytes() {
    let pull_request = PullRequestInfo {
        number: 436,
        title: "bad\x1b]2;owned\x07title".to_owned(),
        url: "https://github.com/jackin-project/jackin/pull/436".to_owned(),
        is_draft: false,
        checks: None,
    };
    let title =
        compose_outer_terminal_title(Path::new("/workspace/jackin"), None, Some(&pull_request));

    assert_eq!(title, "jackin · PR #436 · bad ]2;owned title");
}

#[test]
fn display_title_falls_back_when_shell_sets_empty_title() {
    let (mut session, _rx) = test_shell_session(20, 80);
    session.feed_pty(b"\x1b]2;\x07");

    assert_eq!(session_display_title(&session), "Test");
}

#[test]
fn display_title_uses_shell_title_without_repeating_shell_label() {
    let (mut session, _rx) = test_shell_session(20, 80);
    session.feed_pty(b"\x1b]2;prompt title\x07");

    assert_eq!(session_display_title(&session), "prompt title");
}

#[test]
fn display_title_uses_shell_cwd_without_repeating_shell_label() {
    let (mut session, _rx) = test_shell_session(20, 80);
    session.feed_pty(b"\x1b]7;file:///workspace/project\x07");

    assert_eq!(session_display_title(&session), "/workspace/project");
}

#[test]
fn full_frame_emits_outer_terminal_title_once_until_context_changes() {
    let mut mux = single_pane_tab_mux();
    mux.workdir = PathBuf::from("/workspace/jackin");
    mux.pull_request_context_branch = Some(branch("feat/capsule-pr-context-bar"));

    let first = String::from_utf8_lossy(&compose_after(&mut mux, FullRedrawReason::ExplicitRedraw))
        .to_string();
    assert!(
        first.contains("\x1b]2;jackin · feat/capsule-pr-context-bar\x1b\\"),
        "first frame should set branch title: {first:?}"
    );

    let second =
        String::from_utf8_lossy(&compose_after(&mut mux, FullRedrawReason::ExplicitRedraw))
            .to_string();
    assert!(
        !second.contains("\x1b]2;jackin · feat/capsule-pr-context-bar\x1b\\"),
        "unchanged full frame should not spam title: {second:?}"
    );

    mux.pull_request_context = Some(Arc::new(pull_request_fixture(436)));
    let updated =
        String::from_utf8_lossy(&compose_after(&mut mux, FullRedrawReason::ExplicitRedraw))
            .to_string();
    assert!(
        updated.contains("\x1b]2;jackin · PR #436 · Surface PR context in Capsule\x1b\\"),
        "PR context change should refresh title: {updated:?}"
    );
}

#[test]
fn full_frame_updates_outer_terminal_title_on_branch_switch() {
    let mut mux = single_pane_tab_mux();
    mux.workdir = PathBuf::from("/workspace/jackin");
    mux.workdir_context.default_branch = Some("main".to_owned());
    mux.pull_request_context_branch = Some(branch("feat/a"));

    let first = String::from_utf8_lossy(&compose_after(&mut mux, FullRedrawReason::ExplicitRedraw))
        .to_string();
    assert!(
        first.contains("\x1b]2;jackin · feat/a\x1b\\"),
        "first non-default branch should set title: {first:?}"
    );

    mux.pull_request_context_branch = Some(branch("feat/b"));
    let switched =
        String::from_utf8_lossy(&compose_after(&mut mux, FullRedrawReason::ExplicitRedraw))
            .to_string();
    assert!(
        switched.contains("\x1b]2;jackin · feat/b\x1b\\"),
        "branch switch should refresh title: {switched:?}"
    );

    mux.pull_request_context_branch = Some(branch("main"));
    let default_branch =
        String::from_utf8_lossy(&compose_after(&mut mux, FullRedrawReason::ExplicitRedraw))
            .to_string();
    assert!(
        default_branch.contains("\x1b]2;jackin\x1b\\"),
        "default branch should fall back to workspace-only title: {default_branch:?}"
    );
    assert!(
        !default_branch.contains("jackin · main"),
        "default branch name should not be propagated into title: {default_branch:?}"
    );
}

pub(super) fn test_session(rows: u16, cols: u16) -> (Session, mpsc::UnboundedReceiver<Vec<u8>>) {
    test_session_with_agent(rows, cols, Some("codex".to_owned()))
}

fn test_shell_session(rows: u16, cols: u16) -> (Session, mpsc::UnboundedReceiver<Vec<u8>>) {
    test_session_with_agent(rows, cols, None)
}

fn pane_kind_cases() -> [(Option<&'static str>, &'static str); 2] {
    [(Some("codex"), "agent"), (None, "shell")]
}

fn test_pane_session(
    rows: u16,
    cols: u16,
    agent: Option<&str>,
) -> (Session, mpsc::UnboundedReceiver<Vec<u8>>) {
    test_session_with_agent(rows, cols, agent.map(str::to_owned))
}

fn assert_focused_scroll_chrome(frame: &[u8], context: &str) {
    let rendered = String::from_utf8_lossy(frame);
    let thumb_fg = format!(
        "{}{}",
        jackin_tui::ansi::RESET,
        jackin_tui::ansi::rgb_fg(jackin_tui::DIALOG_SCROLL_THUMB)
    );
    assert!(
        rendered.contains(&thumb_fg),
        "focused {context} should use the shared scrollbar thumb color"
    );
    assert!(
        rendered.contains(jackin_tui::components::ScrollbarStyle::Line.vertical_thumb()),
        "focused {context} should draw the shared scrollbar thumb"
    );
    assert!(
        rendered.contains(jackin_tui::components::SCROLLBAR_TRACK),
        "focused {context} should draw the shared scrollbar track"
    );
}

fn assert_no_scroll_thumb(frame: &[u8], context: &str) {
    let rendered = String::from_utf8_lossy(frame);
    assert!(
        !rendered.contains(jackin_tui::components::ScrollbarStyle::Line.vertical_thumb())
            && !rendered.contains('█'),
        "{context} should not draw fake scrollback chrome"
    );
}

fn assert_frame_stays_within_geometry(frame: &[u8], rows: u16, cols: u16, context: &str) {
    let metrics = scan_emitted_frame(frame);
    assert!(
        metrics.full_screen_erases > 0,
        "{context} resize repaint must clear the old geometry"
    );
    assert!(
        metrics.cursor_moves > 0,
        "{context} resize repaint must draw cells"
    );
    assert!(
        metrics.max_row_addressed <= rows && metrics.max_col_addressed <= cols,
        "{context} resize repaint moved outside {rows}x{cols}: max {}x{}",
        metrics.max_row_addressed,
        metrics.max_col_addressed
    );
}

fn assert_wheel_cursor_fallback_sent(
    input_rx: &mut mpsc::UnboundedReceiver<Vec<u8>>,
    expected_bytes: &[u8],
) {
    assert_eq!(
        input_rx
            .try_recv()
            .expect("wheel fallback should reach PTY"),
        expected_bytes,
    );
    input_rx.try_recv().expect_err("wheel should not produce extra PTY input");
}

fn feed_top_anchored_inline_history(session: &mut Session, region_bottom: u16, lines: usize) {
    session.feed_pty(format!("\x1b[1;{region_bottom}r\x1b[{region_bottom};1H").as_bytes());
    for i in 0..lines {
        session.feed_pty(format!("\r\n\x1b[2Khistory {i}").as_bytes());
    }
    session.feed_pty(b"\x1b[r");
}

pub(super) fn test_session_with_agent(
    rows: u16,
    cols: u16,
    agent: Option<String>,
) -> (Session, mpsc::UnboundedReceiver<Vec<u8>>) {
    let (input_tx, input_rx) = mpsc::unbounded_channel();
    (
        Session::new_for_test(
            "Test".to_owned(),
            agent,
            None,
            (rows, cols),
            100,
            input_tx,
            Arc::new(Mutex::new(Box::new(NullMasterPty))),
            Arc::new(Mutex::new(Box::new(NullChildKiller))),
        ),
        input_rx,
    )
}

fn test_provider_session(
    provider: jackin_protocol::Provider,
) -> (Session, mpsc::UnboundedReceiver<Vec<u8>>) {
    let (mut session, input_rx) = test_session_with_agent(24, 80, Some("claude".to_owned()));
    session.provider = Some(crate::session::SessionProvider {
        label: provider.label().to_owned(),
        env_overrides: provider.env_overrides(Some("zai-test-token")),
    });
    (session, input_rx)
}

#[test]
fn refresh_tab_labels_preserves_provider_suffix() {
    let mut mux = test_mux(24, 80);
    let (session, _rx) = test_provider_session(jackin_protocol::Provider::Zai);
    mux.sessions.insert(1, session);
    mux.tabs.push(Tab::new_single("Claude", 1, "test"));

    mux.refresh_tab_labels();

    assert_eq!(mux.tabs[0].label(), "Claude (Z.AI)");
}

#[test]
fn split_metadata_inherits_focused_provider() {
    let mut mux = test_mux(24, 80);
    let (session, _rx) = test_provider_session(jackin_protocol::Provider::Zai);
    let expected_env = session
        .provider
        .as_ref()
        .map(|p| p.env_overrides.clone())
        .unwrap_or_default();
    mux.sessions.insert(1, session);
    mux.tabs.push(Tab::new_single("Claude (Z.AI)", 1, "test"));

    let (agent, env, provider) = mux.focused_spawn_metadata();

    assert_eq!(agent.as_deref(), Some("claude"));
    assert_eq!(provider.as_deref(), Some("Z.AI"));
    assert_eq!(env, expected_env);
}

#[test]
fn zoom_state_is_independent_per_tab() {
    let mut mux = split_tab_mux();
    mux.toggle_zoom();
    assert_eq!(mux.active_zoomed_id(), Some(1));

    let mut tab_b = Tab::new_single("Shell", 3, "test-b");
    assert!(tab_b.tree.split_h(3, 4, SplitPosition::After));
    tab_b.focused_id = 4;
    mux.tabs.push(tab_b);
    mux.active_tab = 1;
    mux.toggle_zoom();

    assert_eq!(mux.active_zoomed_id(), Some(4));
    assert_eq!(mux.tabs[0].zoomed, Some(1));
    assert_eq!(mux.tabs[1].zoomed, Some(4));

    mux.active_tab = 0;
    assert_eq!(mux.active_zoomed_id(), Some(1));
}

#[test]
fn unzooming_active_tab_does_not_clear_other_tab_zoom() {
    let mut mux = split_tab_mux();
    mux.toggle_zoom();
    let mut tab_b = Tab::new_single("Shell", 3, "test-b");
    assert!(tab_b.tree.split_h(3, 4, SplitPosition::After));
    mux.tabs.push(tab_b);
    mux.active_tab = 1;
    mux.toggle_zoom();

    mux.toggle_zoom();

    assert_eq!(mux.tabs[1].zoomed, None);
    assert_eq!(mux.tabs[0].zoomed, Some(1));
    mux.active_tab = 0;
    assert_eq!(mux.active_zoomed_id(), Some(1));
}

#[test]
fn killing_zoomed_pane_clears_only_owning_tab_zoom() {
    let mut mux = split_tab_mux();
    mux.toggle_zoom();
    let mut tab_b = Tab::new_single("Shell", 3, "test-b");
    assert!(tab_b.tree.split_h(3, 4, SplitPosition::After));
    mux.tabs.push(tab_b);
    mux.active_tab = 1;
    mux.toggle_zoom();

    mux.close_focused_pane();

    assert_eq!(mux.tabs[0].zoomed, Some(1));
    assert_eq!(mux.tabs[1].zoomed, None);
    assert_eq!(mux.tabs[1].focused_id, 4);
    mux.active_tab = 0;
    assert_eq!(mux.active_zoomed_id(), Some(1));
}

pub(super) fn split_tab_mux() -> Multiplexer {
    let mut mux = test_mux(24, 80);
    let mut tab = Tab::new_single("Shell", 1, "test");
    assert!(tab.tree.split_h(1, 2, SplitPosition::After));
    mux.tabs.push(tab);
    drop(mux.compose_pending_frame());
    mux
}

#[test]
fn resize_zero_zero_normalizes_to_default_dimensions() {
    // A client sending Resize { rows: 0, cols: 0 } is asking for
    // "use the defaults"; the daemon must floor through
    // `normalize_size` and never store 0 in `term_rows`/`term_cols`,
    // because zero-row PTYs collapse grid rendering.
    let mut mux = test_mux(48, 160);
    mux.resize(0, 0);
    assert_eq!((mux.term_rows, mux.term_cols), (DEFAULT_ROWS, DEFAULT_COLS));
}

#[test]
fn resize_then_full_frame_repaints_with_new_geometry() {
    let mut mux = single_pane_tab_mux_with_size(24, 80);
    assert!(!compose_after(&mut mux, FullRedrawReason::FirstAttach).is_empty());

    mux.resize(30, 100);
    let frame = compose_after(&mut mux, FullRedrawReason::Resize);

    assert_eq!((mux.term_rows, mux.term_cols), (30, 100));
    assert!(
        !frame.is_empty(),
        "resize must produce a repaint for the attach client"
    );
}

#[test]
fn resize_shrink_terminal_edge_frame_stays_inside_new_geometry() {
    let mut mux = single_pane_tab_mux_with_size(24, 80);
    let (mut session, _rx) = test_session(20, 78);
    session.feed_pty(b"\x1b[1;1HLEFT-EDGE\x1b[1;70HOLD-RIGHT-EDGE\x1b[20;70HOLD-BOTTOM");
    mux.sessions.insert(1, session);
    drop(compose_after(&mut mux, FullRedrawReason::FirstAttach));

    mux.resize(10, 30);
    let frame = compose_after(&mut mux, FullRedrawReason::Resize);

    assert_frame_stays_within_geometry(&frame, 10, 30, "terminal-edge shrink");
}

#[test]
fn resize_shrink_split_frame_stays_inside_new_geometry() {
    let mut mux = split_tab_mux();
    for id in [1, 2] {
        let (mut session, _rx) = test_session(20, 38);
        session.feed_pty(format!("\x1b[1;1HPANE-{id}\x1b[20;30HOLD-SPLIT-{id}").as_bytes());
        mux.sessions.insert(id, session);
    }
    drop(compose_after(&mut mux, FullRedrawReason::FirstAttach));

    mux.resize(10, 40);
    let frame = mux.compose_pending_frame();

    assert_frame_stays_within_geometry(&frame, 10, 40, "interior split shrink");
}

#[test]
fn dialog_dismiss_frame_repaints_covered_pane_body() {
    // Dialog-dismiss ghost regression (PR #495): closing a dialog must repaint
    // the pane cells the backdrop covered, with no 2J clear. The frame
    // apply_action(Dismiss) returns is that repaint — the SocketBackend cell
    // diff turns the backdrop spaces back into pane content, so HELLO-PANE-BODY
    // (one same-style run) reappears and no backdrop ghost survives.
    let needle = b"HELLO-PANE-BODY";
    let contains = |frame: &[u8]| frame.windows(needle.len()).any(|w| w == needle);

    let mut mux = single_pane_tab_mux();
    let (mut session, _rx) = test_session(8, 20);
    session.feed_pty(b"\x1b[1;1HHELLO-PANE-BODY");
    mux.sessions.insert(1, session);

    let first = compose_after(&mut mux, FullRedrawReason::FirstAttach);
    assert!(contains(&first), "first frame must paint the pane body");

    // Open a dialog: the full-screen backdrop covers the pane body.
    mux.open_container_info_dialog();
    let opened = compose_after(&mut mux, FullRedrawReason::DialogChange);
    assert!(!contains(&opened), "backdrop must cover the pane body");

    // Dismiss returns the repaint frame directly; it must restore the body.
    let dismissed = apply_action_frame(&mut mux, Action::Dialog(DialogAction::Dismiss))
        .expect("dismiss must emit a repaint frame");
    assert!(!mux.dialog_open(), "Dismiss must pop the dialog");
    assert!(
        contains(&dismissed),
        "dialog dismiss must repaint the covered pane body (no backdrop ghost)"
    );
}

#[test]
fn partial_ratatui_frame_repaints_non_dirty_split_pane_body() {
    // Ratatui draw closures build a fresh current buffer before diffing against
    // the previous one. A partial frame that paints only the dirty pane body can
    // therefore turn every non-dirty split pane into blank cells in the emitted
    // diff. Keep dirty-row patches in the direct backend path only; Ratatui
    // fallback frames must repaint every visible pane body.
    let left_needle = b"LEFT-PANE-STABLE";
    let right_needle = b"RIGHT-PANE-UPDATE";
    let contains = |frame: &[u8], needle: &[u8]| frame.windows(needle.len()).any(|w| w == needle);

    let mut mux = split_tab_mux();
    for (id, label) in [(1, "LEFT-PANE-STABLE"), (2, "RIGHT-PANE-STABLE")] {
        let (mut session, _rx) = test_session(20, 38);
        session.feed_pty(format!("\x1b[1;1H{label}").as_bytes());
        mux.sessions.insert(id, session);
    }
    drop(compose_after(&mut mux, FullRedrawReason::FirstAttach));

    // Simulate an invalid/stale Ratatui backing buffer, matching what happens
    // after direct dirty-patch frames or attach-side terminal disruption. The
    // next fallback Ratatui frame must still be self-contained for pane bodies.
    drop(mux.ratatui_terminal.clear());
    drop(mux.ratatui_terminal.backend_mut().take_output());

    mux.sessions
        .get_mut(&2)
        .expect("right pane session")
        .feed_pty(b"\x1b]2;right pane title\x07\x1b[2;1HRIGHT-PANE-UPDATE");
    let frame = compose_after(&mut mux, FullRedrawReason::PtyOutput);

    assert!(
        contains(&frame, left_needle),
        "partial fallback must repaint non-dirty split pane body: {:?}",
        String::from_utf8_lossy(&frame)
    );
    assert!(
        contains(&frame, right_needle),
        "partial fallback must repaint dirty split pane body: {:?}",
        String::from_utf8_lossy(&frame)
    );
}

#[test]
fn bottom_chrome_rides_the_cell_buffer_on_every_frame() {
    let mut mux = single_pane_tab_mux();
    let (mut session, _rx) = test_session(20, 78);
    session.feed_pty(b"\x1b[1;1Hstable pane");
    // Retain scrollback so the scrolled-chrome step below can park the view
    // in history (the grid clamps the offset to the filled scrollback).
    for i in 0..30 {
        session.feed_pty(format!("line {i}\r\n").as_bytes());
    }
    mux.sessions.insert(1, session);

    let contains = |frame: &[u8], needle: &[u8]| frame.windows(needle.len()).any(|w| w == needle);

    let first = compose_after(&mut mux, FullRedrawReason::FirstAttach);
    assert!(
        contains(&first, b"resize pane"),
        "first full frame must assert raw bottom chrome: {:?}",
        String::from_utf8_lossy(&first)
    );

    // Chrome is widget cells now: Ratatui emits it when it changes, then the
    // previous buffer suppresses unchanged chrome on later frames.
    let unchanged = compose_after(&mut mux, status_change_redraw_reason());
    assert!(
        !contains(&unchanged, b"resize pane"),
        "unchanged chrome cells must not be re-emitted: {:?}",
        String::from_utf8_lossy(&unchanged)
    );
    assert!(
        !contains(&unchanged, b"exit scrollback"),
        "live view must not paint the scrollback hint: {:?}",
        String::from_utf8_lossy(&unchanged)
    );

    assert!(
        mux.sessions
            .get_mut(&1)
            .expect("test session")
            .set_scrollback_offset(1)
    );
    let changed = compose_after(&mut mux, FullRedrawReason::ScrollbackMovement);
    assert!(
        contains(&changed, b"scroll") && contains(&changed, b"exit"),
        "changed scrollback chrome must re-emit the hint row: {:?}",
        String::from_utf8_lossy(&changed)
    );
}

#[test]
fn scan_emitted_frame_reports_geometry_fingerprint() {
    // \x1b[2J (erase) + move to (5,10) + move to (40,160).
    let frame = b"\x1b[2J\x1b[5;10Hx\x1b[40;160Hy".to_vec();
    let metrics = scan_emitted_frame(&frame);
    assert_eq!(metrics.cursor_moves, 2);
    assert_eq!(metrics.max_row_addressed, 40);
    assert_eq!(metrics.max_col_addressed, 160);
    assert_eq!(metrics.full_screen_erases, 1);
    assert_eq!(metrics.painted_cells, 2);

    // A move with no col defaults col to 1; `f` is an alias for `H`.
    let frame = b"\x1b[12Hz".to_vec();
    let metrics = scan_emitted_frame(&frame);
    assert_eq!(metrics.cursor_moves, 1);
    assert_eq!(metrics.max_row_addressed, 12);
    assert_eq!(metrics.max_col_addressed, 1);
    assert_eq!(metrics.full_screen_erases, 0);
}

#[test]
fn scan_emitted_frame_counts_modern_render_metrics() {
    let frame = b"\x1b[0m\x1b]8;;https://example.test\x07x\x1b]8;;\x07";
    let metrics = scan_emitted_frame(frame);
    assert_eq!(metrics.bytes, frame.len());
    assert_eq!(metrics.sgr_resets, 1);
    assert_eq!(metrics.osc8_opens, 1);
    assert_eq!(metrics.osc8_closes, 1);
    assert_eq!(metrics.full_screen_erases, 0);
    assert_eq!(metrics.painted_cells, 1);
    assert!(
        !metrics.full_frame_repaint,
        "geometry-free scan should not claim full-frame repaint"
    );
    let full = crate::client_writer::scan_emitted_frame_with_geometry(b"12345678", Some((2, 5)));
    assert!(
        full.full_frame_repaint,
        "painted-cell threshold should flag full-frame repaint"
    );
}

#[test]
fn pty_osc8_hyperlink_emits_from_frame_metadata() {
    let mut mux = single_pane_tab_mux();
    let (mut session, _rx) = test_session(20, 78);
    session.feed_pty(b"\x1b]8;;https://example.test/docs\x07link\x1b]8;;\x07");
    mux.sessions.insert(1, session);

    let frame = compose_after(&mut mux, FullRedrawReason::FirstAttach);

    assert!(
        frame
            .windows(b"\x1b]8;;https://example.test/docs\x1b\\".len())
            .any(|w| w == b"\x1b]8;;https://example.test/docs\x1b\\"),
        "safe OSC 8 link must be emitted from frame metadata: {:?}",
        String::from_utf8_lossy(&frame)
    );
    assert!(
        frame.windows(b"link".len()).any(|w| w == b"link"),
        "linked glyphs must still render: {:?}",
        String::from_utf8_lossy(&frame)
    );
}

#[test]
fn unsafe_pty_osc8_hyperlink_is_not_emitted_from_frame_metadata() {
    let mut mux = single_pane_tab_mux();
    let (mut session, _rx) = test_session(20, 78);
    session.feed_pty(b"\x1b]8;;javascript:alert(1)\x07link\x1b]8;;\x07");
    mux.sessions.insert(1, session);

    let frame = compose_after(&mut mux, FullRedrawReason::FirstAttach);

    assert!(
        !frame
            .windows(b"javascript".len())
            .any(|w| w == b"javascript"),
        "unsafe OSC 8 URI must not be emitted: {:?}",
        String::from_utf8_lossy(&frame)
    );
    assert!(
        frame.windows(b"link".len()).any(|w| w == b"link"),
        "glyphs must render even when the hyperlink URI is filtered"
    );
}

#[test]
fn pty_sgr_metadata_emits_non_native_visible_attributes() {
    let mut mux = single_pane_tab_mux();
    let (mut session, _rx) = test_session(20, 78);
    session.feed_pty(b"\x1b[4:3;58:2:12:34:56;53mstyled");
    mux.sessions.insert(1, session);

    let frame = compose_after(&mut mux, FullRedrawReason::FirstAttach);
    let text = String::from_utf8_lossy(&frame);

    for sgr in ["\x1b[4:3m", "\x1b[58;2;12;34;56m", "\x1b[53m"] {
        assert!(
            text.contains(sgr),
            "SGR metadata {sgr:?} must be emitted in frame: {text:?}"
        );
    }
}

#[test]
fn scroll_region_ops_do_not_emit_decstbm_optimization() {
    let mut mux = single_pane_tab_mux();
    let (mut session, _rx) = test_session(20, 78);
    session.feed_pty(b"\x1b[1;5r\x1b[5;1H");
    for i in 0..8 {
        session.feed_pty(format!("\r\nline {i}").as_bytes());
    }
    session.feed_pty(b"\x1b[r");
    mux.sessions.insert(1, session);

    let frame = compose_after(&mut mux, FullRedrawReason::FirstAttach);
    let text = String::from_utf8_lossy(&frame);

    assert!(
        !text.contains("\x1b[1;5r") && !text.contains("\x1b[r"),
        "DECSTBM scroll-region optimization must stay disabled: {text:?}"
    );
    assert!(
        !text.contains("\x1b[1S")
            && !text.contains("\x1b[S")
            && !text.contains("\x1b[1T")
            && !text.contains("\x1b[T"),
        "scroll op optimization bytes must stay disabled: {text:?}"
    );
}

#[test]
fn wipe_policy_erases_only_on_first_attach_and_resize() {
    // I4: no screen erase outside FirstAttach/Resize. Every other
    // invalidation relies on Ratatui's previous buffer instead of blanking
    // the screen.
    let erase = b"\x1b[2J";
    let contains = |frame: &[u8]| frame.windows(erase.len()).any(|w| w == erase);

    for reason in [FullRedrawReason::FirstAttach, FullRedrawReason::Resize] {
        let mut mux = single_pane_tab_mux_with_size(24, 80);
        let frame = compose_after(&mut mux, reason);
        assert!(contains(&frame), "{reason:?} frame must erase the screen");
    }
    for reason in [
        FullRedrawReason::ExplicitRedraw,
        FullRedrawReason::FocusChange,
        FullRedrawReason::TabSwitch,
        FullRedrawReason::SplitClose,
        FullRedrawReason::LayoutChange,
        FullRedrawReason::StatusChange,
        FullRedrawReason::ScrollbackMovement,
        FullRedrawReason::DialogChange,
        FullRedrawReason::PtyOutput,
    ] {
        let mut mux = single_pane_tab_mux_with_size(24, 80);
        drop(compose_after(&mut mux, FullRedrawReason::FirstAttach));
        let frame = compose_after(&mut mux, reason);
        assert!(
            !contains(&frame),
            "{reason:?} frame must repaint in place, not erase"
        );
    }
}

#[test]
fn pending_status_change_uses_no_clear_diff_frame() {
    let mut mux = single_pane_tab_mux_with_size(24, 80);
    drop(compose_after(&mut mux, FullRedrawReason::FirstAttach));

    mux.invalidate(status_change_redraw_reason());
    assert!(mux.has_pending_render());
    let frame = mux.compose_pending_frame();

    assert!(
        !frame_contains_screen_erase(&frame),
        "status-only refresh must stay out of the clear tier"
    );
    assert!(
        !mux.has_pending_render(),
        "pending diff redraw should be drained after composition"
    );
}

#[test]
fn resize_shrink_then_grow_does_not_panic() {
    // Defect 614/634 regression: rapid resize including shrink-to-floor and grow
    // must not panic.
    let mut mux = single_pane_tab_mux_with_size(24, 80);
    // Shrink to a small size (above normalize_size floor which is ~5 rows, 3 cols).
    mux.resize(6, 4);
    assert_eq!((mux.term_rows, mux.term_cols), (6, 4));
    // Shrink to zero (normalized to defaults).
    mux.resize(0, 0);
    assert_eq!((mux.term_rows, mux.term_cols), (DEFAULT_ROWS, DEFAULT_COLS));
    // Grow back.
    mux.resize(50, 200);
    assert_eq!((mux.term_rows, mux.term_cols), (50, 200));
    // Full repaint after growth must not be empty.
    let frame = mux.compose_pending_frame();
    assert!(!frame.is_empty(), "grow must produce repaint");
}

#[test]
fn initial_spawn_request_is_data_only_agent_or_shell() {
    assert_eq!(
        initial_spawn_request("codex", None),
        SpawnRequest::Agent("codex".to_owned())
    );
    assert_eq!(initial_spawn_request("", None), SpawnRequest::Shell);
}

#[test]
fn initial_spawn_request_carries_provider_when_selected() {
    let provider = jackin_protocol::InitialProvider {
        label: jackin_protocol::Provider::Zai.label().to_owned(),
    };
    assert_eq!(
        initial_spawn_request("claude", Some(&provider)),
        SpawnRequest::AgentWithProvider {
            slug: "claude".to_owned(),
            provider_label: "Z.AI".to_owned(),
        }
    );
    // An empty agent still degrades to a shell even with a provider.
    assert_eq!(
        initial_spawn_request("", Some(&provider)),
        SpawnRequest::Shell
    );
}

#[test]
fn spawn_request_rejects_agent_outside_allowlist_before_pty_spawn() {
    let mut mux = test_mux(24, 80);
    mux.available_agents = vec!["codex".to_owned()];

    let err = mux
        .spawn_request(SpawnRequest::Agent("claude".to_owned()), &[])
        .unwrap_err();

    assert!(err.to_string().contains("rejected agent \"claude\""));
    assert!(mux.sessions.is_empty());
}

#[test]
fn command_palette_labels_single_pane_close_as_close_tab() {
    let mut mux = single_pane_tab_mux();
    mux.open_command_palette();

    assert!(matches!(
        mux.dialog_top(),
        Some(Dialog::CommandPalette {
            close_label: PaletteCloseLabel::CloseTab,
            ..
        })
    ));
}

#[test]
fn dialog_backdrop_preserves_status_bar_and_hides_pane_chrome() {
    fn mux_with_two_sessions() -> Multiplexer {
        let mut mux = split_tab_mux();
        let (session_one, _) = test_session(24, 80);
        let (session_two, _) = test_shell_session(24, 80);
        mux.sessions.insert(1, session_one);
        mux.sessions.insert(2, session_two);
        mux
    }

    fn assert_backdrop_opaque(mut mux: Multiplexer, context: &str) {
        let frame =
            String::from_utf8_lossy(&compose_after(&mut mux, FullRedrawReason::DialogChange))
                .to_string();

        // The brand pill renders as a green block with a black word and a white
        // chevron, so the cursor-diff stream splits `jackin` and `❯` with escape
        // codes. Assert the word plus the block colour rather than a contiguous
        // `jackin❯` substring.
        assert!(
            frame.contains("jackin") && frame.contains("48;2;0;255;65"),
            "{context} should preserve the top status brand (green block) while a dialog is open: {frame:?}"
        );
        assert!(
            !frame.contains(&format!(
                "{}┌",
                jackin_tui::ansi::rgb_fg(jackin_tui::BORDER_GRAY)
            )),
            "{context} should hide inactive pane borders behind the dialog: {frame:?}"
        );
    }

    let mut menu_mux = mux_with_two_sessions();
    menu_mux.open_command_palette();
    assert_backdrop_opaque(menu_mux, "menu dialog");

    let mut container_mux = mux_with_two_sessions();
    container_mux.open_container_info_dialog();
    assert_backdrop_opaque(container_mux, "container info dialog");

    let mut github_mux = mux_with_two_sessions();
    github_mux.pull_request_context_branch = Some(branch("feat/capsule-pr-context-bar"));
    github_mux.pull_request_context = Some(Arc::new(pull_request_fixture(436)));
    github_mux.workdir_context.gh_available = false;
    github_mux.open_github_context_dialog(Instant::now());
    assert_backdrop_opaque(github_mux, "GitHub context dialog");
}

#[test]
fn palette_close_single_pane_opens_confirm_directly() {
    let mut mux = single_pane_tab_mux();
    drop(compose_after(&mut mux, FullRedrawReason::FirstAttach));

    let frame = palette_command_frame(&mut mux, PaletteCommand::Close)
        .expect("single-pane close should redraw confirm dialog");

    assert!(matches!(
        mux.dialog_top(),
        Some(Dialog::ConfirmAction {
            kind: ConfirmKind::CloseTab,
            selected_yes: false
        })
    ));
    assert!(
        !frame_contains_screen_erase(&frame),
        "single-pane close confirm must not clear the full terminal screen"
    );
}

#[test]
fn palette_close_split_tab_opens_target_picker() {
    let mut mux = split_tab_mux();
    drop(compose_after(&mut mux, FullRedrawReason::FirstAttach));

    let frame = palette_command_frame(&mut mux, PaletteCommand::Close)
        .expect("split-tab close should redraw target picker");

    assert!(matches!(
        mux.dialog_top(),
        Some(Dialog::CloseTargetPicker {
            selected: 0,
            filter
        }) if filter.is_empty()
    ));
    assert!(
        !frame_contains_screen_erase(&frame),
        "split-tab close target picker must not clear the full terminal screen"
    );
}

#[test]
fn branch_context_visibility_keeps_content_area_reserved() {
    let mut mux = test_mux(24, 100);
    let now = Instant::now();
    // 24 rows - status(2) - top spacer(1) - hint(1) - bottom spacer(1) - branch(1) = 18
    assert_eq!(mux.content_rows, 18);

    mux.pull_request_context_cache.insert(
        branch("asa/pr-context"),
        PullRequestContextCacheEntry {
            checked_at: now,
            head: None,
            pull_request: Some(Arc::new(pull_request_fixture(434))),
        },
    );
    assert!(mux.apply_git_branch_context(Some("asa/pr-context"), now));
    assert_eq!(mux.content_rows, 18);
    assert_eq!(
        mux.pull_request_context.as_deref().map(|pr| pr.number),
        Some(434)
    );

    mux.pull_request_context_cache.insert(
        branch("feature/no-pr"),
        PullRequestContextCacheEntry {
            checked_at: now,
            head: None,
            pull_request: None,
        },
    );
    assert!(mux.apply_git_branch_context(Some("feature/no-pr"), now));
    assert_eq!(mux.content_rows, 18);
    assert!(mux.pull_request_context.is_none());

    assert!(mux.apply_git_branch_context(Some("main"), now));
    assert_eq!(mux.content_rows, 18);
    assert!(mux.pull_request_context.is_none());
}

#[test]
fn git_branch_context_updates_status_before_github_lookup() {
    let mut mux = test_mux(24, 100);
    let now = Instant::now();
    mux.pull_request_context_branch = Some(branch("old/pr"));
    mux.pull_request_context = Some(Arc::new(pull_request_fixture(434)));
    mux.reconcile_content_rows();
    // 24 rows - status(2) - top spacer(1) - hint(1) - bottom spacer(1) - branch(1) = 18
    assert_eq!(mux.content_rows, 18);

    mux.pull_request_context_cache.insert(
        branch("new/local-branch"),
        PullRequestContextCacheEntry {
            checked_at: now,
            head: None,
            pull_request: None,
        },
    );
    assert!(mux.apply_git_branch_context(Some("new/local-branch"), now));

    assert_eq!(
        mux.pull_request_context_branch.as_deref(),
        Some("new/local-branch")
    );
    assert!(mux.pull_request_context.is_none());
    assert_eq!(mux.content_rows, 18);
}

#[test]
fn git_branch_context_recognizes_repo_after_startup() {
    let mut mux = test_mux(24, 100);
    let now = Instant::now();
    mux.workdir_context.is_git_repo = false;
    mux.workdir_context.gh_available = false;

    assert!(mux.apply_git_branch_context(Some("feat/capsule-pr-context-bar"), now));

    assert!(mux.workdir_context.is_git_repo);
    assert_eq!(
        mux.context_bar_branch(),
        Some("feat/capsule-pr-context-bar")
    );
    assert!(mux.pull_request_context.is_none());
}

#[test]
fn apply_pull_request_context_loaded_drops_stale_request() {
    let mut mux = test_mux(24, 100);
    mux.pull_request_lookup.request_id = 5;
    mux.pull_request_lookup.in_flight = true;
    mux.pull_request_context_branch = Some(branch("feat/x"));
    let pr = pull_request_fixture(99);
    let changed = mux.apply_pull_request_context_loaded(
        3,
        Some(branch("feat/x")),
        None,
        PullRequestLookupOutcome::Resolved(Some(Arc::new(pr))),
        Instant::now(),
    );
    assert!(!changed, "stale request must not mutate state");
    assert!(
        mux.pull_request_lookup.in_flight,
        "stale request must leave in_flight untouched"
    );
    assert!(
        mux.pull_request_context.is_none(),
        "stale request must not write PR"
    );
}

#[test]
fn apply_pull_request_context_loaded_transient_failure_preserves_prior_cache() {
    let mut mux = test_mux(24, 100);
    let now = Instant::now();
    mux.pull_request_lookup.request_id = 7;
    mux.pull_request_lookup.in_flight = true;
    mux.pull_request_context_branch = Some(branch("feat/x"));
    mux.pull_request_context = Some(Arc::new(pull_request_fixture(123)));
    mux.pull_request_context_cache.insert(
        branch("feat/x"),
        PullRequestContextCacheEntry {
            checked_at: now.checked_sub(Duration::from_secs(5)).unwrap(),
            head: None,
            pull_request: Some(Arc::new(pull_request_fixture(123))),
        },
    );
    let changed = mux.apply_pull_request_context_loaded(
        7,
        Some(branch("feat/x")),
        None,
        PullRequestLookupOutcome::TransientFailure,
        now,
    );
    assert!(!changed, "transient failure must not mutate visible state");
    assert!(
        !mux.pull_request_lookup.in_flight,
        "transient failure must clear in_flight so next tick retries"
    );
    assert_eq!(
        mux.pull_request_context_cache
            .get("feat/x")
            .and_then(|e| e.pull_request.as_ref().map(|p| p.number)),
        Some(123),
        "cache must be untouched by transient failure"
    );
}

#[test]
fn apply_pull_request_context_loaded_refreshes_open_github_dialog() {
    let mut mux = test_mux(24, 100);
    let now = Instant::now();
    arm_pending_pr_lookup(&mut mux, "feat/x", 7);

    let changed = mux.apply_pull_request_context_loaded(
        7,
        Some(branch("feat/x")),
        None,
        PullRequestLookupOutcome::Resolved(Some(Arc::new(pull_request_fixture(436)))),
        now,
    );

    assert!(changed, "dialog refresh should request redraw");
    assert!(matches!(
        mux.dialog_top(),
        Some(Dialog::GitHubContext { copied: false, .. })
    ));
    assert_eq!(mux.pull_request_context_branch.as_deref(), Some("feat/x"));
    assert_eq!(
        mux.pull_request_context.as_ref().map(|pr| pr.number),
        Some(436)
    );
    assert!(!mux.pull_request_context_loading());
}

#[test]
fn transient_pull_request_failure_clears_open_dialog_loading_state() {
    let mut mux = test_mux(24, 100);
    let now = Instant::now();
    arm_pending_pr_lookup(&mut mux, "feat/x", 7);

    let changed = mux.apply_pull_request_context_loaded(
        7,
        Some(branch("feat/x")),
        None,
        PullRequestLookupOutcome::TransientFailure,
        now,
    );

    assert!(changed, "dialog loading state changed");
    assert!(matches!(
        mux.dialog_top(),
        Some(Dialog::GitHubContext { copied: false, .. })
    ));
    assert_eq!(mux.pull_request_context_branch.as_deref(), Some("feat/x"));
    assert!(mux.pull_request_context.is_none());
    assert!(!mux.pull_request_context_loading());
    assert!(
        !mux.pull_request_context_cache.contains_key("feat/x"),
        "transient failure must not cache a no-PR result"
    );
}

#[test]
fn apply_git_branch_context_loaded_drops_stale_request() {
    let mut mux = test_mux(24, 100);
    mux.git_branch_lookup.request_id = 4;
    mux.git_branch_lookup.in_flight = true;
    let changed = mux.apply_git_branch_context_loaded(
        2,
        GitContext::Branch {
            name: branch("feat/x"),
            head: None,
        },
        Instant::now(),
    );
    assert!(!changed);
    assert!(mux.git_branch_lookup.in_flight, "stale id leaves in_flight");
    assert!(mux.pull_request_context_branch.is_none());
}

#[test]
fn apply_git_branch_context_bumps_pr_request_id_on_branch_change() {
    let mut mux = test_mux(24, 100);
    let now = Instant::now();
    mux.pull_request_context_branch = Some(branch("feat/a"));
    mux.workdir_context.gh_available = false;
    let id_before = mux.pull_request_lookup.request_id;
    let _ = mux.apply_git_branch_context(Some("feat/b"), now);
    assert_eq!(
        mux.pull_request_lookup.request_id,
        id_before.wrapping_add(1),
        "branch change must bump request_id so stale gh worker responses are rejected"
    );
}

#[test]
fn apply_git_context_bumps_pr_request_id_on_same_branch_head_change() {
    let mut mux = test_mux(24, 100);
    let now = Instant::now();
    mux.pull_request_context_branch = Some(branch("feat/a"));
    mux.pull_request_context_head = Some(oid('1'));
    mux.pull_request_context = Some(Arc::new(pull_request_fixture(455)));
    mux.pull_request_context_cache.insert(
        branch("feat/a"),
        PullRequestContextCacheEntry {
            checked_at: now,
            head: Some(oid('1')),
            pull_request: Some(Arc::new(pull_request_fixture(455))),
        },
    );
    mux.workdir_context.gh_available = false;
    let id_before = mux.pull_request_lookup.request_id;

    let changed = mux.apply_git_context(
        GitContext::Branch {
            name: branch("feat/a"),
            head: Some(oid('2')),
        },
        now,
    );

    assert!(
        changed,
        "visible PR context must clear on same-branch HEAD change"
    );
    assert_eq!(
        mux.pull_request_lookup.request_id,
        id_before.wrapping_add(1),
        "HEAD change must bump request_id so stale gh worker responses are rejected"
    );
    assert!(
        mux.pull_request_context.is_none(),
        "old PR cache must not stay visible for the new HEAD"
    );
}

#[test]
fn purge_expired_pull_request_cache_entries_drops_old_entries() {
    let mut mux = test_mux(24, 100);
    let now = Instant::now();
    let ttl = PULL_REQUEST_CONTEXT_LOOKUP_INTERVAL * 2;
    mux.pull_request_context_cache.insert(
        branch("feat/fresh"),
        PullRequestContextCacheEntry {
            checked_at: now.checked_sub(Duration::from_secs(10)).unwrap(),
            head: None,
            pull_request: Some(Arc::new(pull_request_fixture(1))),
        },
    );
    mux.pull_request_context_cache.insert(
        branch("feat/old"),
        PullRequestContextCacheEntry {
            checked_at: now
                .checked_sub(ttl)
                .unwrap()
                .checked_sub(Duration::from_secs(1))
                .unwrap(),
            head: None,
            pull_request: Some(Arc::new(pull_request_fixture(2))),
        },
    );
    mux.purge_expired_pull_request_cache_entries(now);
    assert!(mux.pull_request_context_cache.contains_key("feat/fresh"));
    assert!(!mux.pull_request_context_cache.contains_key("feat/old"));
}

#[test]
fn pull_request_cache_fresh_at_strict_boundary() {
    let mut mux = test_mux(24, 100);
    let now = Instant::now();
    // Just-fresh: at the boundary minus 1 ms.
    mux.pull_request_context_cache.insert(
        branch("branch-a"),
        PullRequestContextCacheEntry {
            checked_at: now
                .checked_sub(PULL_REQUEST_CONTEXT_LOOKUP_INTERVAL)
                .unwrap()
                + Duration::from_millis(1),
            head: None,
            pull_request: None,
        },
    );
    // Just-stale: at the boundary plus 1 ms.
    mux.pull_request_context_cache.insert(
        branch("branch-b"),
        PullRequestContextCacheEntry {
            checked_at: now
                .checked_sub(PULL_REQUEST_CONTEXT_LOOKUP_INTERVAL)
                .unwrap()
                .checked_sub(Duration::from_millis(1))
                .unwrap(),
            head: None,
            pull_request: None,
        },
    );
    assert!(mux.pull_request_cache_is_fresh("branch-a", now));
    assert!(!mux.pull_request_cache_is_fresh("branch-b", now));
}

#[test]
fn pull_request_cache_fresh_requires_matching_head() {
    let mut mux = test_mux(24, 100);
    let now = Instant::now();
    mux.pull_request_context_head = Some(oid('2'));
    mux.pull_request_context_cache.insert(
        branch("branch-a"),
        PullRequestContextCacheEntry {
            checked_at: now,
            head: Some(oid('1')),
            pull_request: None,
        },
    );

    assert!(
        !mux.pull_request_cache_is_fresh("branch-a", now),
        "a cached no-PR answer from an older HEAD must not suppress a fresh lookup"
    );
}

#[test]
fn pull_request_force_refresh_bypasses_fresh_no_pr_cache() {
    let mut mux = test_mux(24, 100);
    let now = Instant::now();
    mux.pull_request_context_cache.insert(
        branch("branch-a"),
        PullRequestContextCacheEntry {
            checked_at: now,
            head: None,
            pull_request: None,
        },
    );

    assert!(mux.pull_request_cache_blocks_lookup(
        "branch-a",
        now,
        PullRequestLookupMode::RespectCache
    ));
    assert!(!mux.pull_request_cache_blocks_lookup(
        "branch-a",
        now,
        PullRequestLookupMode::ForceRefresh
    ));
}

#[test]
fn git_branch_context_keeps_current_pr_while_refreshing_same_branch() {
    let mut mux = test_mux(24, 100);
    let now = Instant::now();
    mux.pull_request_context_branch = Some(branch("feature/current"));
    mux.pull_request_context = Some(Arc::new(pull_request_fixture(436)));
    mux.pull_request_lookup.in_flight = true;
    mux.pull_request_context_cache.insert(
        branch("feature/current"),
        PullRequestContextCacheEntry {
            checked_at: now
                .checked_sub(PULL_REQUEST_CONTEXT_LOOKUP_INTERVAL)
                .unwrap(),
            head: None,
            pull_request: Some(Arc::new(pull_request_fixture(436))),
        },
    );

    assert!(!mux.apply_git_branch_context(Some("feature/current"), now));
    assert_eq!(
        mux.pull_request_context.as_deref().map(|pr| pr.number),
        Some(436)
    );
}

#[test]
fn cached_pull_request_stays_visible_during_forced_dialog_refresh() {
    let mut mux = test_mux(24, 100);
    mux.pull_request_context_branch = Some(branch("feature/current"));
    mux.pull_request_context = Some(Arc::new(pull_request_fixture(436)));
    mux.pull_request_lookup.in_flight = true;
    // Exercise the real dialog-open path so a future refactor that
    // skips force_spawn (or routes through a different dispatcher)
    // is caught here instead of by silent UX regression.
    mux.workdir_context.gh_available = false;
    mux.open_github_context_dialog(Instant::now());

    let view = mux.github_context_view();

    assert!(matches!(
        view.status,
        PullRequestStatus::Loaded(pr) if pr.number == 436
    ));
    assert!(
        !mux.pull_request_context_loading(),
        "known PR details should remain visible while a forced refresh runs in the background"
    );
}

#[test]
fn open_github_context_dialog_force_spawns_when_gh_available() {
    let mut mux = test_mux(24, 100);
    mux.workdir_context.gh_available = true;
    mux.workdir_context.is_git_repo = true;
    mux.workdir_context.default_branch = Some("main".to_owned());
    mux.pull_request_context_branch = Some(branch("feat/x"));
    let id_before = mux.pull_request_lookup.request_id;

    mux.open_github_context_dialog(Instant::now());

    assert!(
        mux.pull_request_lookup.in_flight,
        "dialog-open must fire a real worker spawn when gh_available is true"
    );
    assert_eq!(
        mux.pull_request_lookup.request_id,
        id_before.wrapping_add(1),
        "force-spawn must bump request_id"
    );
}

#[test]
fn open_github_context_dialog_force_spawns_when_startup_missed_gh() {
    let mut mux = test_mux(24, 100);
    mux.workdir_context.gh_available = false;
    mux.workdir_context.is_git_repo = true;
    mux.workdir_context.default_branch = Some("main".to_owned());
    mux.pull_request_context_branch = Some(branch("feat/x"));
    let id_before = mux.pull_request_lookup.request_id;

    mux.open_github_context_dialog(Instant::now());

    assert!(
        mux.pull_request_lookup.in_flight,
        "manual refresh must schedule a background lookup even when startup marked gh unavailable"
    );
    assert_eq!(
        mux.pull_request_lookup.request_id,
        id_before.wrapping_add(1),
        "manual refresh should not need a synchronous gh availability probe"
    );
    assert!(
        !mux.workdir_context.gh_available,
        "gh availability flips only after the background lookup succeeds"
    );
}

#[test]
fn background_pull_request_success_marks_gh_available_after_startup_miss() {
    let mut mux = test_mux(24, 100);
    let now = Instant::now();
    mux.workdir_context.gh_available = false;
    mux.workdir_context.is_git_repo = true;
    mux.pull_request_context_branch = Some(branch("feat/x"));
    mux.pull_request_lookup.request_id = 7;
    mux.pull_request_lookup.in_flight = true;

    let changed = mux.apply_pull_request_context_loaded(
        7,
        Some(branch("feat/x")),
        None,
        PullRequestLookupOutcome::Resolved(Some(Arc::new(pull_request_fixture(436)))),
        now,
    );

    assert!(changed);
    assert!(
        mux.workdir_context.gh_available,
        "successful background gh lookup should unblock later conservative refreshes"
    );
}

#[test]
fn open_github_context_dialog_bypasses_fresh_no_pr_cache() {
    let mut mux = test_mux(24, 100);
    let now = Instant::now();
    mux.workdir_context.gh_available = true;
    mux.workdir_context.is_git_repo = true;
    mux.workdir_context.default_branch = Some("main".to_owned());
    mux.pull_request_context_branch = Some(branch("feat/x"));
    mux.pull_request_context_cache.insert(
        branch("feat/x"),
        PullRequestContextCacheEntry {
            checked_at: now,
            head: None,
            pull_request: None,
        },
    );

    mux.open_github_context_dialog(now);

    assert!(
        mux.pull_request_lookup.in_flight,
        "manual dialog open must refresh even when a recent background lookup saw no PR"
    );
    assert!(
        mux.pull_request_context_loading(),
        "dialog should show resolving while the forced refresh is in flight"
    );
}

#[test]
fn apply_git_context_head_change_schedules_fresh_pr_lookup() {
    // gh_available=true so the spawn path runs end-to-end; we assert
    // in_flight=true after the head flip to prove the maybe_spawn at
    // the tail of `apply_git_context` fires (not just request_id bump).
    let mut mux = test_mux(24, 100);
    let now = Instant::now();
    mux.workdir_context.gh_available = true;
    mux.workdir_context.is_git_repo = true;
    mux.workdir_context.default_branch = Some("main".to_owned());
    mux.pull_request_context_branch = Some(branch("feat/a"));
    mux.pull_request_context_head = Some(oid('1'));

    mux.apply_git_context(
        GitContext::Branch {
            name: branch("feat/a"),
            head: Some(oid('2')),
        },
        now,
    );

    assert!(
        mux.pull_request_lookup.in_flight,
        "head flip must schedule a fresh gh worker via maybe_spawn"
    );
}

#[test]
fn apply_pull_request_context_loaded_refuses_head_mismatch() {
    // Defense-in-depth: request_id matched but mux.head drifted
    // between spawn and apply. The result MUST NOT overwrite
    // pull_request_context or land in the cache against the new head.
    let mut mux = test_mux(24, 100);
    let now = Instant::now();
    mux.pull_request_lookup.request_id = 9;
    mux.pull_request_lookup.in_flight = true;
    mux.pull_request_context_branch = Some(branch("feat/x"));
    mux.pull_request_context_head = Some(oid('a'));

    let changed = mux.apply_pull_request_context_loaded(
        9,
        Some(branch("feat/x")),
        Some(oid('b')),
        PullRequestLookupOutcome::Resolved(Some(Arc::new(pull_request_fixture(777)))),
        now,
    );

    assert!(
        mux.pull_request_context.is_none(),
        "head-drift result must not be assigned to visible context"
    );
    assert!(
        !mux.pull_request_context_cache.contains_key("feat/x"),
        "head-drift result must not poison the cache"
    );
    assert!(
        !changed || mux.dialog_top().is_none(),
        "head-drift apply only flips loading state; no PR data assigned"
    );
}

#[test]
fn apply_pull_request_context_loaded_refuses_head_drift_none_to_some() {
    // Spawn-time head was None (e.g. mid-write HEAD), apply-time
    // mux.head resolved to Some. Drift guard must refuse the spawn
    // payload — its data is keyed against the absent-head state.
    let mut mux = test_mux(24, 100);
    let now = Instant::now();
    mux.pull_request_lookup.request_id = 11;
    mux.pull_request_lookup.in_flight = true;
    mux.pull_request_context_branch = Some(branch("feat/x"));
    mux.pull_request_context_head = Some(oid('c'));

    let _unused = mux.apply_pull_request_context_loaded(
        11,
        Some(branch("feat/x")),
        None,
        PullRequestLookupOutcome::Resolved(Some(Arc::new(pull_request_fixture(778)))),
        now,
    );

    assert!(
        mux.pull_request_context.is_none(),
        "None→Some head drift refused"
    );
    assert!(!mux.pull_request_context_cache.contains_key("feat/x"));
}

#[test]
fn apply_pull_request_context_loaded_refuses_head_drift_some_to_none() {
    // Inverse: spawn captured a head, apply-time mux.head was
    // cleared (e.g. HEAD became unreadable between spawn and apply).
    let mut mux = test_mux(24, 100);
    let now = Instant::now();
    mux.pull_request_lookup.request_id = 13;
    mux.pull_request_lookup.in_flight = true;
    mux.pull_request_context_branch = Some(branch("feat/x"));
    mux.pull_request_context_head = None;

    let _unused = mux.apply_pull_request_context_loaded(
        13,
        Some(branch("feat/x")),
        Some(oid('d')),
        PullRequestLookupOutcome::Resolved(Some(Arc::new(pull_request_fixture(779)))),
        now,
    );

    assert!(
        mux.pull_request_context.is_none(),
        "Some→None head drift refused"
    );
    assert!(!mux.pull_request_context_cache.contains_key("feat/x"));
}

#[test]
fn apply_git_context_simultaneous_branch_and_head_change_invalidates_cache() {
    let mut mux = test_mux(24, 100);
    let now = Instant::now();
    mux.pull_request_context_branch = Some(branch("feat/a"));
    mux.pull_request_context_head = Some(oid('1'));
    mux.pull_request_context = Some(Arc::new(pull_request_fixture(455)));
    mux.pull_request_context_cache.insert(
        branch("feat/a"),
        PullRequestContextCacheEntry {
            checked_at: now,
            head: Some(oid('1')),
            pull_request: Some(Arc::new(pull_request_fixture(455))),
        },
    );
    mux.workdir_context.gh_available = false;
    let id_before = mux.pull_request_lookup.request_id;

    let changed = mux.apply_git_context(
        GitContext::Branch {
            name: branch("feat/b"),
            head: Some(oid('2')),
        },
        now,
    );

    assert!(changed, "branch+head flip must dirty the visible context");
    assert_eq!(
        mux.pull_request_lookup.request_id,
        id_before.wrapping_add(1),
        "simultaneous branch+head flip must bump request_id once"
    );
    assert_eq!(mux.pull_request_context_branch.as_deref(), Some("feat/b"));
    assert_eq!(
        mux.pull_request_context_head.as_deref(),
        Some("2222222222222222222222222222222222222222")
    );
    assert!(
        mux.pull_request_context.is_none(),
        "old PR cache entry under feat/a must not survive the branch flip"
    );
}

#[test]
fn read_branch_from_git_head_reads_normal_checkout() {
    let temp = tempfile::tempdir().unwrap();
    let git_dir = temp.path().join(".git");
    std::fs::create_dir_all(&git_dir).unwrap();
    std::fs::write(git_dir.join("HEAD"), "ref: refs/heads/feat/context\n").unwrap();

    assert_eq!(
        read_branch_from_git_head(temp.path()).as_deref(),
        Some("feat/context")
    );
}

#[test]
fn read_context_from_git_metadata_reads_loose_head_oid() {
    let temp = tempfile::tempdir().unwrap();
    let git_dir = temp.path().join(".git");
    std::fs::create_dir_all(git_dir.join("refs/heads/feat")).unwrap();
    std::fs::write(git_dir.join("HEAD"), "ref: refs/heads/feat/context\n").unwrap();
    std::fs::write(
        git_dir.join("refs/heads/feat/context"),
        "1111111111111111111111111111111111111111\n",
    )
    .unwrap();

    let context = read_context_from_git_metadata(temp.path()).unwrap();

    assert_eq!(
        context.branch_name().map(BranchName::as_str),
        Some("feat/context")
    );
    assert_eq!(
        context.head().map(Oid::as_str),
        Some("1111111111111111111111111111111111111111")
    );
}

#[test]
fn read_context_from_git_metadata_reads_packed_head_oid() {
    let temp = tempfile::tempdir().unwrap();
    let git_dir = temp.path().join(".git");
    std::fs::create_dir_all(&git_dir).unwrap();
    std::fs::write(git_dir.join("HEAD"), "ref: refs/heads/feat/context\n").unwrap();
    std::fs::write(
        git_dir.join("packed-refs"),
        "\
# pack-refs with: peeled fully-peeled sorted
2222222222222222222222222222222222222222 refs/tags/v0.1.0
1111111111111111111111111111111111111111 refs/heads/feat/context
^3333333333333333333333333333333333333333
",
    )
    .unwrap();

    let context = read_context_from_git_metadata(temp.path()).unwrap();

    assert_eq!(
        context.branch_name().map(BranchName::as_str),
        Some("feat/context")
    );
    assert_eq!(
        context.head().map(Oid::as_str),
        Some("1111111111111111111111111111111111111111")
    );
}

#[test]
fn read_packed_git_ref_oid_refreshes_after_metadata_change() {
    let temp = tempfile::tempdir().unwrap();
    let packed_refs = temp.path().join("packed-refs");
    std::fs::write(
        &packed_refs,
        "1111111111111111111111111111111111111111 refs/heads/feat/context\n",
    )
    .unwrap();

    assert_eq!(
        read_packed_git_ref_oid(&packed_refs, "refs/heads/feat/context").as_deref(),
        Some("1111111111111111111111111111111111111111")
    );

    std::fs::write(
        &packed_refs,
        "\
# changed
2222222222222222222222222222222222222222 refs/heads/feat/context
",
    )
    .unwrap();

    assert_eq!(
        read_packed_git_ref_oid(&packed_refs, "refs/heads/feat/context").as_deref(),
        Some("2222222222222222222222222222222222222222")
    );
}

#[test]
fn workdir_context_recognizes_direct_git_metadata_without_default_branch() {
    let temp = tempfile::tempdir().unwrap();
    let git_dir = temp.path().join(".git");
    std::fs::create_dir_all(&git_dir).unwrap();
    std::fs::write(git_dir.join("HEAD"), "ref: refs/heads/feat/context\n").unwrap();

    let context = WorkdirContext::resolve(temp.path());

    assert!(context.is_git_repo);
}

#[test]
fn read_branch_from_git_head_reads_worktree_gitdir_file() {
    let temp = tempfile::tempdir().unwrap();
    let (workdir, common_git) = make_worktree_layout(temp.path(), "workdir");
    let wt_git = common_git.join("worktrees/workdir");
    std::fs::write(wt_git.join("HEAD"), "ref: refs/heads/feat/worktree\n").unwrap();

    assert_eq!(
        read_branch_from_git_head(&workdir).as_deref(),
        Some("feat/worktree")
    );
}

#[test]
fn oid_parse_accepts_sha1_and_sha256_lengths_only() {
    assert!(Oid::parse(&"a".repeat(40)).is_some());
    assert!(Oid::parse(&"F".repeat(40)).is_some());
    assert!(Oid::parse(&"0".repeat(64)).is_some());
    assert!(Oid::parse(&"f".repeat(64)).is_some());
    assert!(Oid::parse(&"a".repeat(39)).is_none());
    assert!(Oid::parse(&"a".repeat(41)).is_none());
    assert!(Oid::parse(&"a".repeat(63)).is_none());
    assert!(Oid::parse(&"a".repeat(65)).is_none());
    // Non-hex character at SHA-1 length.
    let mut s = "a".repeat(39);
    s.push('g');
    assert!(Oid::parse(&s).is_none());
}

#[test]
fn read_context_from_git_metadata_reads_detached_head_oid() {
    let temp = tempfile::tempdir().unwrap();
    let git_dir = temp.path().join(".git");
    std::fs::create_dir_all(&git_dir).unwrap();
    std::fs::write(
        git_dir.join("HEAD"),
        "1111111111111111111111111111111111111111\n",
    )
    .unwrap();

    let context = read_context_from_git_metadata(temp.path()).unwrap();

    assert_eq!(context.branch_name(), None);
    assert_eq!(
        context.head().map(Oid::as_str),
        Some("1111111111111111111111111111111111111111")
    );
}

#[test]
fn read_context_from_git_metadata_handles_malformed_head_content() {
    let temp = tempfile::tempdir().unwrap();
    let git_dir = temp.path().join(".git");
    std::fs::create_dir_all(&git_dir).unwrap();
    // Neither `ref: ` prefix nor full hex OID — corruption / mid-write.
    std::fs::write(git_dir.join("HEAD"), "abc123\n").unwrap();

    let context = read_context_from_git_metadata(temp.path()).unwrap();

    assert_eq!(context.branch_name(), None);
    assert_eq!(
        context.head(),
        None,
        "malformed HEAD content must not be treated as an OID"
    );
}

#[test]
fn read_context_from_git_metadata_handles_malformed_gitfile_content() {
    let temp = tempfile::tempdir().unwrap();
    let workdir = temp.path();
    // `.git` is a file but does not start with `gitdir:` — corruption.
    std::fs::write(workdir.join(".git"), "not a gitdir pointer\n").unwrap();

    assert!(read_context_from_git_metadata(workdir).is_none());
}

#[test]
fn apply_git_context_flips_is_git_repo_on_detached_head() {
    let mut mux = test_mux(24, 100);
    mux.workdir_context.is_git_repo = false;
    let now = Instant::now();

    mux.apply_git_context(GitContext::Detached { head: oid('1') }, now);

    assert!(
        mux.workdir_context.is_git_repo,
        "detached HEAD must promote is_git_repo (branch is None but head is Some)"
    );
}

#[test]
fn read_context_from_git_metadata_resolves_worktree_head_via_commondir() {
    let temp = tempfile::tempdir().unwrap();
    let (workdir, common_git) = make_worktree_layout(temp.path(), "wt");
    let wt_git = common_git.join("worktrees/wt");
    std::fs::create_dir_all(common_git.join("refs/heads/feat")).unwrap();
    // Loose ref lives in the COMMON dir, not the per-worktree gitdir.
    std::fs::write(
        common_git.join("refs/heads/feat/wt"),
        "1111111111111111111111111111111111111111\n",
    )
    .unwrap();
    std::fs::write(wt_git.join("HEAD"), "ref: refs/heads/feat/wt\n").unwrap();
    std::fs::write(wt_git.join("commondir"), "../..\n").unwrap();

    let context = read_context_from_git_metadata(&workdir).unwrap();

    assert_eq!(context.branch_name(), Some(&branch("feat/wt")));
    assert_eq!(context.head(), Some(&oid('1')));
}

#[test]
fn read_packed_git_ref_oid_does_not_cache_truncated_read() {
    // packed-refs cap forces a synthetic-truncation scenario: write
    // exactly PACKED_REFS_MAX_BYTES of content so read_text_bounded's
    // length equals the cap, then mutate underlying bytes and confirm
    // the second read sees the new value (would not, if the truncated
    // first read had cached).
    let temp = tempfile::tempdir().unwrap();
    let packed_refs = temp.path().join("packed-refs-truncated");
    // Pad with comment lines + a real ref entry until total length
    // matches the cap exactly.
    let real_line = "1111111111111111111111111111111111111111 refs/heads/feat/x\n";
    let padding_per_line = "# padding to fill packed-refs to the cap byte limit aaaaaaaaaa\n";
    // Target one byte OVER the cap so metadata.len() > cap triggers
    // the real truncation path (not the exactly-cap edge case).
    let target_size = PACKED_REFS_MAX_BYTES as usize + 1;
    let mut buf = String::with_capacity(target_size);
    while buf.len() + real_line.len() + padding_per_line.len() <= target_size {
        buf.push_str(padding_per_line);
    }
    buf.push_str(real_line);
    let remaining = target_size.saturating_sub(buf.len());
    buf.extend(std::iter::repeat_n('#', remaining));
    buf.truncate(target_size);
    std::fs::write(&packed_refs, &buf).unwrap();

    drop(read_packed_git_ref_oid(&packed_refs, "refs/heads/feat/x"));

    // Mutate same-length bytes (overwrite oid in place); mtime advances.
    let buf2 = buf.replacen(
        "1111111111111111111111111111111111111111",
        "2222222222222222222222222222222222222222",
        1,
    );
    std::fs::write(&packed_refs, &buf2).unwrap();

    assert_eq!(
        read_packed_git_ref_oid(&packed_refs, "refs/heads/feat/x").as_deref(),
        Some("2222222222222222222222222222222222222222"),
        "truncated first read must not have cached; second read sees fresh content"
    );
}

#[test]
fn packed_refs_cache_eviction_bounds_entries_at_cap() {
    // Create CAP+1 distinct packed-refs paths and read each once.
    // After the (CAP+1)th insert, exactly CAP of the inserted
    // paths must remain — proves both the upper bound AND that
    // eviction removed only one entry (catches over-evict bugs
    // where the cache would degrade to a single entry).
    let temp = tempfile::tempdir().unwrap();
    let mut paths = Vec::new();
    for i in 0..=PACKED_REFS_CACHE_MAX_ENTRIES {
        let path = temp.path().join(format!("packed-refs-evict-{i}"));
        std::fs::write(
            &path,
            format!("1111111111111111111111111111111111111111 refs/heads/branch-{i}\n"),
        )
        .unwrap();
        drop(read_packed_git_ref_oid(
            &path,
            &format!("refs/heads/branch-{i}"),
        ));
        paths.push(path);
    }

    let count = with_packed_refs_cache(|cache| {
        paths
            .iter()
            .filter(|p| cache.contains_key(p.as_path()))
            .count()
    });
    // The just-inserted (CAP+1)th entry MUST be present; eviction
    // targets pre-existing entries, never the new insert.
    assert!(
        with_packed_refs_cache(|cache| cache.contains_key(paths.last().unwrap().as_path())),
        "newly-inserted entry must survive eviction"
    );
    // Exactly one of the previously-inserted CAP entries must have
    // been evicted: count of our tracked paths in the cache should
    // equal CAP, not less (over-evict) or more (no-op evict).
    assert_eq!(
        count, PACKED_REFS_CACHE_MAX_ENTRIES,
        "eviction must drop exactly one entry; saw {count} surviving of CAP={PACKED_REFS_CACHE_MAX_ENTRIES}"
    );
}

#[test]
fn read_git_ref_oid_loose_wins_over_packed() {
    let temp = tempfile::tempdir().unwrap();
    let git_dir = temp.path().to_path_buf();
    std::fs::create_dir_all(git_dir.join("refs/heads")).unwrap();
    std::fs::write(
        git_dir.join("refs/heads/feat-x"),
        "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa\n",
    )
    .unwrap();
    std::fs::write(
        git_dir.join("packed-refs"),
        "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb refs/heads/feat-x\n",
    )
    .unwrap();

    assert_eq!(
        read_git_ref_oid(&git_dir, None, "refs/heads/feat-x").as_deref(),
        Some("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"),
        "loose ref must win over packed-refs entry"
    );
}

#[test]
fn force_spawn_pull_request_context_lookup_skipped_when_in_flight() {
    let mut mux = test_mux(24, 100);
    mux.workdir_context.gh_available = true;
    mux.workdir_context.is_git_repo = true;
    mux.workdir_context.default_branch = Some("main".to_owned());
    mux.pull_request_context_branch = Some(branch("feat/x"));
    mux.pull_request_lookup.in_flight = true;
    let id_before = mux.pull_request_lookup.request_id;

    let spawned = mux.force_spawn_pull_request_context_lookup(Instant::now());

    assert!(
        !spawned,
        "force-spawn must no-op when a worker is in flight"
    );
    assert_eq!(
        mux.pull_request_lookup.request_id, id_before,
        "force-spawn skip must not bump request_id"
    );
}

#[test]
fn palette_exit_opens_exit_confirm() {
    let mut mux = single_pane_tab_mux();
    mux.handle_palette_command(PaletteCommand::Exit);

    assert!(matches!(
        mux.dialog_top(),
        Some(Dialog::ConfirmAction {
            kind: ConfirmKind::Exit,
            selected_yes: false
        })
    ));
}

#[test]
fn kitty_escape_in_agent_picker_returns_to_menu() {
    let mut mux = single_pane_tab_mux();
    mux.open_command_palette();
    let frame = handle_input_frame(&mut mux, InputEvent::Data(b"\r".to_vec()))
        .expect("New tab command should redraw");
    assert!(String::from_utf8_lossy(&frame).contains("New tab"));
    assert!(matches!(mux.dialog_top(), Some(Dialog::AgentPicker { .. })));

    let events = mux.input_parser.parse(b"\x1b[27;1u");
    assert_eq!(events, vec![InputEvent::Data(b"\x1b".to_vec())]);
    for event in events {
        handle_input_frame(&mut mux, event);
    }

    assert!(matches!(
        mux.dialog_top(),
        Some(Dialog::CommandPalette { .. })
    ));
}

#[test]
fn mouse_sgr_encoding_preserves_press_and_release() {
    assert_eq!(
        encode_mouse_for_protocol(0, 12, 3, true, jackin_term::MouseProtocolEncoding::Sgr).unwrap(),
        b"\x1b[<0;12;3M"
    );
    assert_eq!(
        encode_mouse_for_protocol(0, 12, 3, false, jackin_term::MouseProtocolEncoding::Sgr)
            .unwrap(),
        b"\x1b[<0;12;3m"
    );
}

#[test]
fn mouse_default_encoding_uses_xterm_fields() {
    assert_eq!(
        encode_mouse_for_protocol(0, 12, 3, true, jackin_term::MouseProtocolEncoding::Default)
            .unwrap(),
        b"\x1b[M ,#"
    );
    assert_eq!(
        encode_mouse_for_protocol(0, 12, 3, false, jackin_term::MouseProtocolEncoding::Default)
            .unwrap(),
        b"\x1b[M#,#"
    );
}

#[test]
fn mouse_mode_filter_respects_tracking_granularity() {
    use jackin_term::MouseProtocolMode;

    assert!(!mouse_event_allowed_for_mode(
        MouseProtocolMode::None,
        0,
        true
    ));
    assert!(mouse_event_allowed_for_mode(
        MouseProtocolMode::Press,
        0,
        true
    ));
    assert!(!mouse_event_allowed_for_mode(
        MouseProtocolMode::Press,
        0,
        false
    ));
    assert!(!mouse_event_allowed_for_mode(
        MouseProtocolMode::PressRelease,
        32,
        true
    ));
    assert!(mouse_event_allowed_for_mode(
        MouseProtocolMode::ButtonMotion,
        32,
        true
    ));
    assert!(!mouse_event_allowed_for_mode(
        MouseProtocolMode::ButtonMotion,
        SGR_NO_BUTTON_MOTION,
        true
    ));
    assert!(mouse_event_allowed_for_mode(
        MouseProtocolMode::AnyMotion,
        SGR_NO_BUTTON_MOTION,
        true
    ));
}

#[test]
fn wheel_forwards_to_mouse_enabled_tui() {
    let mut mux = single_pane_tab_mux();
    let (mut session, mut input_rx) = test_session(20, 78);
    session.feed_pty(b"\x1b[?1049h\x1b[?1003h\x1b[?1006h");
    mux.sessions.insert(1, session);

    let redraw = handle_input_frame(
        &mut mux,
        InputEvent::MousePress {
            row: STATUS_BAR_ROWS + 1,
            col: 1,
            button: 64,
        },
    );

    assert!(
        redraw.is_none(),
        "pane-owned wheel should not redraw jackin❯"
    );
    assert_eq!(
        input_rx.try_recv().expect("wheel should reach PTY"),
        b"\x1b[<64;1;1M"
    );
    input_rx.try_recv().expect_err("wheel should not produce extra PTY input");
    assert_eq!(mux.sessions.get(&1).unwrap().scrollback_offset(), 0);
}

#[test]
fn wheel_scrolls_jackin_scrollback_when_mouse_is_disabled() {
    for (agent, pane_kind) in pane_kind_cases() {
        let mut mux = single_pane_tab_mux();
        let (mut session, mut input_rx) = test_pane_session(20, 78, agent);
        for i in 0..40 {
            session.feed_pty(format!("line {i}\r\n").as_bytes());
        }
        assert_eq!(session.scrollback_offset(), 0);
        mux.sessions.insert(1, session);

        let redraw = handle_input_frame(
            &mut mux,
            InputEvent::MousePress {
                row: STATUS_BAR_ROWS + 1,
                col: 1,
                button: 64,
            },
        );

        assert!(
            redraw.is_some(),
            "{pane_kind} pane scrollback should redraw jackin❯"
        );
        input_rx.try_recv().expect_err(&format!("mouse-disabled {pane_kind} panes must not receive raw wheel bytes"));
        assert_eq!(mux.sessions.get(&1).unwrap().scrollback_offset(), 3);
    }
}

#[test]
fn wheel_back_to_live_repaints_body_and_footer() {
    let mut mux = single_pane_tab_mux();
    // Size the session to the pane so the live tail is exactly the grid.
    let pane = mux.visible_panes().into_iter().next().expect("one pane");
    let (mut session, _input_rx) = test_session(pane.inner.rows, pane.inner.cols);
    for i in 0..40 {
        session.feed_pty(format!("line {i}\r\n").as_bytes());
    }
    mux.sessions.insert(1, session);
    // The encoder skips cells identical to the reset baseline (the space in
    // "line 39"), so assert on the digit pair unique to the tail row.
    let contains = |frame: &[u8], needle: &[u8]| frame.windows(needle.len()).any(|w| w == needle);

    // Park the view in history; the frame switches to the scrollback footer.
    let scrolled = handle_input_frame(
        &mut mux,
        InputEvent::MousePress {
            row: STATUS_BAR_ROWS + 1,
            col: 1,
            button: 64,
        },
    )
    .expect("wheel into history must repaint");
    assert_eq!(mux.sessions.get(&1).unwrap().scrollback_offset(), 3);
    // The diff encoder skips cells that match the live footer, so "scrollback"
    // may be split across a cursor-move escape. Match the prefix that is always
    // emitted as a contiguous run.
    assert!(
        contains(&scrolled, b"exit scrollb"),
        "scrolled frame must show the scrollback footer: {:?}",
        String::from_utf8_lossy(&scrolled)
    );

    // Wheel-only return to the live tail: body and footer must repaint
    // together — the D2 regression left the scrollback view and the
    // scrollback footer on screen here.
    let live = handle_input_frame(
        &mut mux,
        InputEvent::MousePress {
            row: STATUS_BAR_ROWS + 1,
            col: 1,
            button: 65,
        },
    )
    .expect("wheel back to live must repaint");
    assert_eq!(mux.sessions.get(&1).unwrap().scrollback_offset(), 0);
    assert!(
        !contains(&live, b"scrollback"),
        "footer must return to the live hint: {:?}",
        String::from_utf8_lossy(&live)
    );
    assert!(
        contains(&live, b"9"),
        "body diff must include changed live-tail cells: {:?}",
        String::from_utf8_lossy(&live)
    );
    assert!(
        !frame_contains_screen_erase(&live),
        "returning to live must repaint in place, not wipe"
    );
}

#[test]
fn feed_while_scrolled_keeps_view_anchored() {
    let (mut session, _rx) = test_session(20, 78);
    for i in 0..40 {
        session.feed_pty(format!("line {i}\r\n").as_bytes());
    }
    assert!(session.scroll_by(5));
    let offset_before = session.scrollback_offset();
    let top_before = view_row_text(&session, 0);

    for i in 40..45 {
        session.feed_pty(format!("line {i}\r\n").as_bytes());
    }

    assert_eq!(
        session.scrollback_offset(),
        offset_before + 5,
        "offset must grow by the rows evicted into scrollback"
    );
    assert_eq!(
        view_row_text(&session, 0),
        top_before,
        "the row under the reader must hold still while the agent streams"
    );
}

/// Text of one visible row of the session's current scrollback view.
fn view_row_text(session: &Session, row: u16) -> String {
    let (grid_rows, _) = session.shadow_grid.size();
    let view = session
        .shadow_grid
        .scrollback_view(session.scrollback_offset(), grid_rows);
    (0..view.cols)
        .map(|col| {
            view.cell(row, col)
                .map_or(' ', |cell| cell.contents().chars().next().unwrap_or(' '))
        })
        .collect::<String>()
        .trim_end()
        .to_owned()
}

#[test]
fn cursor_reconciliation_hides_cursor_while_scrolled() {
    // Frame-model contract (§3.4): the cursor is hidden whenever the view is
    // not live, re-shown at the VT position when it is — derived per frame,
    // no assertion site outside the encoder.
    let contains = |frame: &[u8], needle: &[u8]| frame.windows(needle.len()).any(|w| w == needle);
    let mut mux = single_pane_tab_mux();
    let pane = mux.visible_panes().into_iter().next().expect("one pane");
    let (mut session, _rx) = test_session(pane.inner.rows, pane.inner.cols);
    for i in 0..40 {
        session.feed_pty(format!("line {i}\r\n").as_bytes());
    }
    mux.sessions.insert(1, session);
    let live = compose_after(&mut mux, FullRedrawReason::FirstAttach);
    assert!(
        contains(&live, b"\x1b[?25h"),
        "live pane with a visible VT cursor must show it"
    );

    let scrolled = handle_input_frame(
        &mut mux,
        InputEvent::MousePress {
            row: STATUS_BAR_ROWS + 1,
            col: 1,
            button: 64,
        },
    )
    .expect("wheel into history must repaint");
    assert!(
        scrolled.ends_with(b"\x1b[?25l") || contains(&scrolled, b"\x1b[?25l"),
        "scrolled pane must hide the cursor"
    );
    assert!(
        !scrolled.windows(6).any(|w| w == b"\x1b[?25h"),
        "scrolled pane must not re-show the cursor"
    );

    let back = handle_input_frame(
        &mut mux,
        InputEvent::MousePress {
            row: STATUS_BAR_ROWS + 1,
            col: 1,
            button: 65,
        },
    )
    .expect("wheel back to live must repaint");
    assert!(
        contains(&back, b"\x1b[?25h"),
        "returning to live must re-show the cursor"
    );
}

#[test]
fn mode_reconciliation_resets_agent_modes_on_focus_swap() {
    // The reconciliation replaces the focus_swap_reset + current_mode_state
    // pair: swapping focus from a pane with bracketed paste, application
    // cursor, and a kitty push to a plain pane must switch each mode off,
    // while the client-owned mouse/focus/alt-screen modes stay untouched.
    let contains = |frame: &[u8], needle: &[u8]| frame.windows(needle.len()).any(|w| w == needle);
    let mut mux = split_tab_mux();
    let panes = mux.visible_panes();
    for pane in &panes {
        let (session, rx) = test_session(pane.inner.rows, pane.inner.cols);
        drop(rx);
        mux.sessions.insert(pane.id, session);
    }
    mux.sessions
        .get_mut(&1)
        .expect("first pane")
        .feed_pty(b"\x1b[?2004h\x1b[?1h\x1b[>1u");
    let asserted = compose_after(&mut mux, FullRedrawReason::FirstAttach);
    for needle in [&b"\x1b[?2004h"[..], &b"\x1b[?1h"[..], &b"\x1b[>1u"[..]] {
        assert!(
            contains(&asserted, needle),
            "focused pane's modes must be asserted: missing {needle:?}"
        );
    }

    let target = panes.iter().find(|pane| pane.id == 2).expect("second pane");
    let swapped = handle_input_frame(
        &mut mux,
        InputEvent::MousePress {
            row: target.inner.row + 1,
            col: target.inner.col + 1,
            button: 0,
        },
    )
    .expect("focus swap must repaint");
    for needle in [&b"\x1b[?2004l"[..], &b"\x1b[?1l"[..], &b"\x1b[<u"[..]] {
        assert!(
            contains(&swapped, needle),
            "swap to a plain pane must switch agent modes off: missing {needle:?}"
        );
    }
    for forbidden in [
        &b"\x1b[?1000l"[..],
        &b"\x1b[?1003l"[..],
        &b"\x1b[?1006l"[..],
        &b"\x1b[?1004l"[..],
        &b"\x1b[?1049l"[..],
    ] {
        assert!(
            !contains(&swapped, forbidden),
            "reconciliation must not toggle client-owned mode {forbidden:?}"
        );
    }
}

#[test]
fn pane_scrollbar_renders_shared_component_glyphs_only() {
    let mut mux = single_pane_tab_mux();
    let (mut session, _rx) = test_session(20, 78);
    for i in 0..40 {
        session.feed_pty(format!("line {i}\r\n").as_bytes());
    }
    mux.sessions.insert(1, session);

    let frame = compose_after(&mut mux, FullRedrawReason::FirstAttach);
    let rendered = String::from_utf8_lossy(&frame);
    assert!(
        rendered.contains(jackin_tui::components::ScrollbarStyle::Line.vertical_thumb()),
        "pane scrollbar must use the shared Line thumb"
    );
    assert!(
        rendered.contains(jackin_tui::components::SCROLLBAR_TRACK),
        "pane scrollbar must paint the shared track"
    );
    assert!(
        !rendered.contains('█'),
        "hand-painted block thumb is a D14 regression"
    );
}

#[test]
fn scrollbar_click_jumps_scrollback() {
    let mut mux = single_pane_tab_mux();
    let (mut session, _rx) = test_session(20, 78);
    for i in 0..40 {
        session.feed_pty(format!("line {i}\r\n").as_bytes());
    }
    let filled = session.scrollback_filled();
    assert!(filled > 0);
    mux.sessions.insert(1, session);
    let pane = mux.visible_panes().into_iter().next().expect("one pane");
    let track_col = pane.outer.col + pane.outer.cols - 1;
    let track_top = pane.outer.row + 1;
    let track_bottom = pane.outer.row + pane.outer.rows - 2;

    // Click the top of the track → jump to the oldest retained rows.
    let frame = handle_input_frame(
        &mut mux,
        InputEvent::MousePress {
            row: track_top,
            col: track_col,
            button: 0,
        },
    );
    assert!(frame.is_some(), "scrollbar jump must repaint");
    assert_eq!(
        mux.sessions.get(&1).unwrap().scrollback_offset(),
        filled,
        "top-of-track click must jump to the top of history"
    );

    // Click the bottom of the track → back to the live tail.
    let frame = handle_input_frame(
        &mut mux,
        InputEvent::MousePress {
            row: track_bottom,
            col: track_col,
            button: 0,
        },
    );
    assert!(frame.is_some(), "scrollbar jump back to live must repaint");
    assert_eq!(
        mux.sessions.get(&1).unwrap().scrollback_offset(),
        0,
        "bottom-of-track click must return to the live tail"
    );
}

#[test]
fn diff_frames_repaint_in_place_without_screen_erase() {
    let mut mux = single_pane_tab_mux();
    let (mut session, _rx) = test_session(20, 78);
    session.feed_pty(b"hello capsule");
    mux.sessions.insert(1, session);
    let contains = |frame: &[u8], needle: &[u8]| frame.windows(needle.len()).any(|w| w == needle);

    let first = compose_after(&mut mux, FullRedrawReason::FocusChange);
    let second = compose_after(&mut mux, FullRedrawReason::FocusChange);
    assert!(
        !frame_contains_screen_erase(&first),
        "first diff frame must not erase the screen"
    );
    assert!(
        contains(&first, b"hello") && contains(&first, b"capsule"),
        "first diff frame must emit changed pane cells: {:?}",
        String::from_utf8_lossy(&first)
    );
    assert!(
        !frame_contains_screen_erase(&second),
        "unchanged diff frame must not erase the screen"
    );
    assert!(
        !contains(&second, b"hello") && !contains(&second, b"capsule"),
        "unchanged diff frame must trust Ratatui's previous buffer: {:?}",
        String::from_utf8_lossy(&second)
    );
}

#[test]
fn retained_scrollback_draws_scrollbar_at_live_tail() {
    for (agent, pane_kind) in pane_kind_cases() {
        let mut mux = single_pane_tab_mux();
        let (mut session, _input_rx) = test_pane_session(20, 78, agent);
        for i in 0..40 {
            session.feed_pty(format!("line {i}\r\n").as_bytes());
        }
        assert_eq!(session.scrollback_offset(), 0);
        assert!(
            session.scrollback_filled() > 0,
            "{pane_kind} setup should retain scrollback"
        );
        mux.sessions.insert(1, session);

        let frame = compose_after(&mut mux, FullRedrawReason::FirstAttach);

        assert_focused_scroll_chrome(
            &frame,
            &format!("{pane_kind} pane with retained scrollback at live tail"),
        );
        assert_eq!(mux.sessions.get(&1).unwrap().scrollback_offset(), 0);
    }
}

#[test]
fn wheel_noops_for_focused_normal_screen_pane_without_scrollback() {
    for (agent, pane_kind) in pane_kind_cases() {
        let mut mux = single_pane_tab_mux_with_size(55, 200);
        let (mut session, mut input_rx) = test_pane_session(51, 198, agent);
        session.feed_pty(b"\x1b[49;3Hcodex prompt");
        assert_eq!(session.scrollback_filled(), 0);
        mux.sessions.insert(1, session);

        let redraw = handle_input_frame(
            &mut mux,
            InputEvent::MousePress {
                row: STATUS_BAR_ROWS + 10,
                col: 10,
                button: 64,
            },
        );

        assert!(
            redraw.is_none(),
            "{pane_kind} normal-screen pane without scrollback should not redraw jackin❯"
        );
        input_rx.try_recv().expect_err(&format!("normal-screen {pane_kind} pane without scrollback must not receive cursor-key wheel fallback"));
        assert_eq!(mux.sessions.get(&1).unwrap().scrollback_offset(), 0);
    }
}

#[test]
fn wheel_scrolls_top_anchored_inline_history_for_all_panes() {
    for (agent, pane_kind) in pane_kind_cases() {
        let mut mux = single_pane_tab_mux_with_size(12, 40);
        let (mut session, mut input_rx) = test_pane_session(8, 38, agent);
        feed_top_anchored_inline_history(&mut session, 5, 12);
        session.feed_pty(b"\x1b[8;1Hlive prompt");
        assert!(
            session.scrollback_filled() >= 3,
            "{pane_kind} pane should retain top-anchored inline history"
        );
        mux.sessions.insert(1, session);

        let redraw = handle_input_frame(
            &mut mux,
            InputEvent::MousePress {
                row: STATUS_BAR_ROWS + 1,
                col: 1,
                button: 64,
            },
        );

        let frame = redraw.expect("inline history wheel should redraw");
        input_rx.try_recv().expect_err(&format!("{pane_kind} pane must not receive cursor-key wheel fallback"));
        assert_eq!(mux.sessions.get(&1).unwrap().scrollback_offset(), 3);
        assert_focused_scroll_chrome(
            &frame,
            &format!("normal-screen {pane_kind} pane with inline history"),
        );
        assert!(
            String::from_utf8_lossy(&frame).contains("history"),
            "normal-screen {pane_kind} wheel should render retained inline history"
        );
    }
}

#[test]
fn scrolled_inline_history_preserves_color_and_selection_highlight() {
    let mut mux = single_pane_tab_mux_with_size(12, 40);
    let (mut session, mut input_rx) = test_pane_session(8, 38, Some("codex"));
    session.feed_pty(b"\x1b[1;5r\x1b[5;1H");
    for i in 0..12 {
        session.feed_pty(format!("\r\n\x1b[2K\x1b[31mred history {i}\x1b[0m").as_bytes());
    }
    session.feed_pty(b"\x1b[r\x1b[8;1Hlive prompt");
    mux.sessions.insert(1, session);

    let frame = handle_input_frame(
        &mut mux,
        InputEvent::MousePress {
            row: STATUS_BAR_ROWS + 1,
            col: 1,
            button: 64,
        },
    )
    .expect("inline history wheel should redraw");

    input_rx.try_recv().expect_err("Codex-style inline history scroll must not forward wheel bytes");
    let rendered = String::from_utf8_lossy(&frame);
    assert!(
        rendered.contains("\x1b[38;5;1mred history"),
        "scrolled Codex inline history should preserve red SGR styling: {rendered:?}"
    );

    let inner = mux.visible_panes()[0].inner;
    let session = mux.sessions.get(&1).unwrap();
    let offset = session.scrollback_offset();
    let filled = session.scrollback_filled();
    let top_content_row = filled.saturating_sub(offset);
    mux.selection = Some(SelectionState {
        session_id: 1,
        inner,
        anchor_row: top_content_row,
        anchor_col: 0,
        end_row: top_content_row,
        end_col: 10,
    });
    let selected_frame = compose_after(&mut mux, FullRedrawReason::SelectionRepaint);
    let selected = String::from_utf8_lossy(&selected_frame);
    assert!(
        selected.contains("\x1b[7m\x1b[38;5;1mred history"),
        "selection overlay should repaint scrolled inline history with reverse-video red styling: {selected:?}"
    );
}

#[test]
fn wheel_scrolls_normal_screen_history_preserved_before_clear_for_all_panes() {
    for (agent, pane_kind) in pane_kind_cases() {
        let mut mux = single_pane_tab_mux_with_size(12, 40);
        let (mut session, mut input_rx) = test_pane_session(8, 38, agent);
        for i in 0..5 {
            session.feed_pty(format!("release note {i}\r\n").as_bytes());
        }
        assert_eq!(
            session.scrollback_filled(),
            0,
            "{pane_kind} setup output fits without native scrollback before clear"
        );

        session.feed_pty(b"\x1b[1;1H\x1b[Jlive prompt");
        assert!(
            session.scrollback_filled() >= 5,
            "{pane_kind} pane should preserve normal-screen rows erased by clear/redraw"
        );
        mux.sessions.insert(1, session);

        let redraw = handle_input_frame(
            &mut mux,
            InputEvent::MousePress {
                row: STATUS_BAR_ROWS + 1,
                col: 1,
                button: 64,
            },
        );

        let frame = redraw.expect("clear-preserved history wheel should redraw");
        input_rx.try_recv().expect_err(&format!("{pane_kind} pane must not receive cursor-key wheel fallback"));
        assert_eq!(mux.sessions.get(&1).unwrap().scrollback_offset(), 3);
        assert_focused_scroll_chrome(
            &frame,
            &format!("normal-screen {pane_kind} pane with clear-preserved history"),
        );
        assert!(
            String::from_utf8_lossy(&frame).contains("release"),
            "normal-screen {pane_kind} wheel should render rows preserved before clear"
        );
    }
}

#[test]
fn wheel_scrolls_csi_scroll_up_inline_history_for_all_panes() {
    for (agent, pane_kind) in pane_kind_cases() {
        let mut mux = single_pane_tab_mux_with_size(12, 40);
        let (mut session, mut input_rx) = test_pane_session(8, 38, agent);
        session.feed_pty(b"\x1b[1;5r\x1b[1;1Htop row\x1b[2;1Hsecond row\x1b[3;1Hthird row");
        session.feed_pty(b"\x1b[2S\x1b[r\x1b[8;1Hlive prompt");
        assert!(
            session.scrollback_filled() >= 2,
            "{pane_kind} pane should retain rows removed by top-anchored CSI S"
        );
        mux.sessions.insert(1, session);

        let redraw = handle_input_frame(
            &mut mux,
            InputEvent::MousePress {
                row: STATUS_BAR_ROWS + 1,
                col: 1,
                button: 64,
            },
        );

        let frame = redraw.expect("CSI S inline history wheel should redraw");
        input_rx.try_recv().expect_err(&format!("{pane_kind} pane must not receive cursor-key wheel fallback"));
        assert_eq!(mux.sessions.get(&1).unwrap().scrollback_offset(), 2);
        assert_focused_scroll_chrome(
            &frame,
            &format!("normal-screen {pane_kind} pane with CSI S inline history"),
        );
        assert!(
            String::from_utf8_lossy(&frame).contains("top"),
            "normal-screen {pane_kind} wheel should render CSI S retained history"
        );
    }
}

#[test]
fn wheel_sends_cursor_fallback_to_mouse_disabled_alt_screen_tui() {
    let mut mux = single_pane_tab_mux();
    let (mut session, mut input_rx) = test_shell_session(20, 78);
    session.feed_pty(b"\x1b[?1049h");
    mux.sessions.insert(1, session);

    let redraw = handle_input_frame(
        &mut mux,
        InputEvent::MousePress {
            row: STATUS_BAR_ROWS + 1,
            col: 1,
            button: 64,
        },
    );

    assert!(
        redraw.is_none(),
        "pane-owned fallback should not redraw jackin❯"
    );
    assert_wheel_cursor_fallback_sent(&mut input_rx, b"\x1b[A\x1b[A\x1b[A");
    assert_eq!(mux.sessions.get(&1).unwrap().scrollback_offset(), 0);
}

#[test]
fn wheel_sends_cursor_fallback_to_alt_screen_tui_with_retained_primary_scrollback() {
    let mut mux = single_pane_tab_mux();
    let (mut session, mut input_rx) = test_shell_session(20, 78);
    for i in 0..40 {
        session.feed_pty(format!("line {i}\r\n").as_bytes());
    }
    assert!(
        session.scrollback_filled() > 0,
        "setup should leave retained primary-screen scrollback"
    );
    session.feed_pty(b"\x1b[?1049h");
    assert!(
        session.alternate_screen(),
        "setup should leave pane in the alternate screen"
    );
    mux.sessions.insert(1, session);

    let redraw = handle_input_frame(
        &mut mux,
        InputEvent::MousePress {
            row: STATUS_BAR_ROWS + 1,
            col: 1,
            button: 64,
        },
    );

    assert!(
        redraw.is_none(),
        "alternate-screen fallback should not redraw jackin❯"
    );
    assert_wheel_cursor_fallback_sent(&mut input_rx, b"\x1b[A\x1b[A\x1b[A");
    assert_eq!(mux.sessions.get(&1).unwrap().scrollback_offset(), 0);
}

#[test]
fn wheel_cursor_fallback_respects_application_cursor_mode() {
    let mut mux = single_pane_tab_mux();
    let (mut session, mut input_rx) = test_session(20, 78);
    session.feed_pty(b"\x1b[?1049h\x1b[?1h");
    mux.sessions.insert(1, session);

    let redraw = handle_input_frame(
        &mut mux,
        InputEvent::MousePress {
            row: STATUS_BAR_ROWS + 1,
            col: 1,
            button: 65,
        },
    );

    assert!(
        redraw.is_none(),
        "pane-owned fallback should not redraw jackin❯"
    );
    assert_wheel_cursor_fallback_sent(&mut input_rx, b"\x1bOB\x1bOB\x1bOB");
}

#[test]
fn alt_screen_overflow_does_not_draw_scrollbar_without_retained_scrollback() {
    let mut mux = single_pane_tab_mux();
    let (mut session, _input_rx) = test_session(8, 20);
    session.feed_pty(b"\x1b[?1049h");
    for i in 0..20 {
        session.feed_pty(format!("line {i}\r\n").as_bytes());
    }
    assert_eq!(session.scrollback_filled(), 0);
    mux.sessions.insert(1, session);

    let frame = compose_after(&mut mux, FullRedrawReason::FirstAttach);
    assert_no_scroll_thumb(&frame, "alt-screen pane without retained scrollback");
}

#[test]
fn normal_screen_panes_do_not_draw_scrollbar_when_grid_is_full_without_scrollback() {
    for (agent, pane_kind) in pane_kind_cases() {
        let mut mux = single_pane_tab_mux();
        let (mut session, _input_rx) = test_pane_session(8, 20, agent);
        for row in 0..8 {
            session.feed_pty(format!("\x1b[{};1Hrow {row}", row + 1).as_bytes());
        }
        mux.sessions.insert(1, session);

        let frame = compose_after(&mut mux, FullRedrawReason::FirstAttach);
        assert_no_scroll_thumb(
            &frame,
            &format!("normal-screen {pane_kind} pane with full grid but no scrollback"),
        );
    }
}

#[test]
fn normal_screen_panes_do_not_draw_scrollbar_when_content_spans_viewport_without_scrollback() {
    for (agent, pane_kind) in pane_kind_cases() {
        let mut mux = single_pane_tab_mux();
        let (mut session, _input_rx) = test_pane_session(8, 20, agent);
        session.feed_pty(b"\x1b[1;1Htop transcript\x1b[8;1Hbottom status");
        assert_eq!(session.scrollback_filled(), 0);
        mux.sessions.insert(1, session);

        let frame = compose_after(&mut mux, FullRedrawReason::FirstAttach);
        assert_no_scroll_thumb(
            &frame,
            &format!(
                "normal-screen {pane_kind} pane with viewport-spanning content but no scrollback"
            ),
        );
    }
}

#[test]
fn normal_screen_panes_do_not_keep_scrollbar_when_cursor_moves_without_scrollback() {
    for (agent, pane_kind) in pane_kind_cases() {
        let mut mux = single_pane_tab_mux_with_size(55, 200);
        let (mut session, _input_rx) = test_pane_session(51, 198, agent);
        session.feed_pty(b"\x1b[1;1Hrelease notes\x1b[51;1Hstatus line\x1b[48;3Hx");
        assert_eq!(session.scrollback_filled(), 0);
        mux.sessions.insert(1, session);

        let frame = compose_after(&mut mux, FullRedrawReason::FirstAttach);
        assert_no_scroll_thumb(
            &frame,
            &format!("normal-screen {pane_kind} transcript pane after cursor moved up"),
        );
    }
}

#[test]
fn alt_screen_exit_resets_keyboard_modes_for_shell_prompt() {
    let (mut session, _input_rx) = test_session(8, 20);
    session.feed_pty(b"\x1b[?1049h\x1b[>1u\x1b[>4;2m");
    drop(session.drain_passthrough());

    session.feed_pty(b"\x1b[?1049l");
    let drained = session.drain_passthrough();

    assert!(
        drained.iter().any(|bytes| bytes == b"\x1b[<u"),
        "kitty keyboard reset missing from {drained:?}"
    );
    assert!(
        drained.iter().any(|bytes| bytes == b"\x1b[>4;0m"),
        "modifyOtherKeys reset missing from {drained:?}"
    );
}

#[test]
fn pointer_shape_updates_only_when_shape_changes() {
    let mut mux = test_mux(24, 80);
    mux.pointer_shapes_supported = true;
    mux.status_bar.identity_label = "jk-test-container".to_owned();
    mux.status_bar.instance_id_label = "test".to_owned();
    mux.pull_request_context_branch = Some(branch("feature/context"));
    let (tx, mut rx) = mpsc::unbounded_channel();
    mux.client.attach(tx);
    let hit = branch_context_bar_layout(
        mux.term_rows,
        mux.term_cols,
        mux.pull_request_context_branch.as_deref(),
        None,
        mux.pull_request_context.as_deref(),
        mux.pull_request_context_loading(),
        None,
        mux.status_bar.instance_id_label(),
    )
    .and_then(|layout| layout.left_region)
    .expect("branch context should fit");

    mux.update_pointer_shape_for_mouse(23, hit.start - 1, SGR_NO_BUTTON_MOTION);
    mux.client.flush_out_of_band();
    let first = rx.try_recv().expect("first pointer-shape update");
    assert!(first.ends_with(b"\x1b]22;pointer\x1b\\"));

    mux.update_pointer_shape_for_mouse(23, hit.start, SGR_NO_BUTTON_MOTION);
    mux.client.flush_out_of_band();
    rx.try_recv().expect_err("unchanged shape should not re-emit");
}

#[tokio::test]
async fn drain_and_exit_delivers_shutdown_before_closing_attach_socket() {
    let mut mux = test_mux(24, 80);
    let (daemon_stream, mut client_stream) = tokio::net::UnixStream::pair().unwrap();
    let (out_tx, out_rx) = mpsc::unbounded_channel();
    let (cmd_tx, _cmd_rx) = mpsc::unbounded_channel();
    mux.client.attach(out_tx);
    mux.attached_task = Some(tokio::spawn(handle_attach_client(
        daemon_stream,
        out_rx,
        cmd_tx,
    )));

    let read_shutdown = async {
        let mut tag = [0u8; 1];
        client_stream
            .read_exact(&mut tag)
            .await
            .expect("shutdown tag should be readable");
        read_server_frame(&mut client_stream, tag[0])
            .await
            .expect("shutdown frame should decode")
            .expect("shutdown frame should be present")
    };

    let ((), frame) = tokio::join!(drain_and_exit(&mut mux), read_shutdown);
    assert_eq!(frame, ServerFrame::Shutdown { reason: None });
}

#[test]
fn pointer_shape_updates_for_clickable_top_chrome() {
    let mut mux = single_pane_tab_mux();
    mux.pointer_shapes_supported = true;
    drop(compose_after(&mut mux, FullRedrawReason::ExplicitRedraw));
    let (tx, mut rx) = mpsc::unbounded_channel();
    mux.client.attach(tx);
    let tab_col = mux
        .status_bar
        .tab_regions
        .first()
        .map(|(start, _)| start.saturating_sub(1))
        .expect("tab region should render");

    mux.update_pointer_shape_for_mouse(0, tab_col, SGR_NO_BUTTON_MOTION);
    mux.client.flush_out_of_band();
    let tab_shape = rx.try_recv().expect("tab pointer-shape update");
    assert!(tab_shape.ends_with(b"\x1b]22;pointer\x1b\\"));

    let mut mux = single_pane_tab_mux();
    mux.pointer_shapes_supported = true;
    drop(compose_after(&mut mux, FullRedrawReason::ExplicitRedraw));
    let (tx, mut rx) = mpsc::unbounded_channel();
    mux.client.attach(tx);
    let menu_col = mux
        .status_bar
        .hint_region
        .map(|(start, _)| start.saturating_sub(1))
        .expect("menu region should render");

    mux.update_pointer_shape_for_mouse(0, menu_col, SGR_NO_BUTTON_MOTION);
    mux.client.flush_out_of_band();
    let menu_shape = rx.try_recv().expect("menu pointer-shape update");
    assert!(menu_shape.ends_with(b"\x1b]22;pointer\x1b\\"));
}

#[test]
fn pointer_shape_updates_for_clickable_dialog_copy_target() {
    let mut mux = single_pane_tab_mux();
    mux.pointer_shapes_supported = true;
    mux.status_bar.identity_label = "jk-test-container".to_owned();
    mux.open_container_info_dialog();
    let (tx, mut rx) = mpsc::unbounded_channel();
    mux.client.attach(tx);
    let dialog = mux.dialog_top().expect("container info dialog should open");
    let (row, col, _, _) = dialog.box_rect(mux.term_rows, mux.term_cols);

    mux.update_pointer_shape_for_mouse(
        row.saturating_add(1),
        // Hover the value column (the cyan link), past the widest label.
        col.saturating_add(22),
        SGR_NO_BUTTON_MOTION,
    );
    mux.client.flush_out_of_band();
    let shape = rx.try_recv().expect("dialog pointer-shape update");
    assert!(shape.ends_with(b"\x1b]22;pointer\x1b\\"));
}

#[test]
fn pointer_shape_updates_for_modified_link_hover() {
    let mut mux = single_pane_tab_mux();
    mux.pointer_shapes_supported = true;
    let (mut session, _input_rx) = test_shell_session(20, 78);
    session.feed_pty(b"\x1b[1;1Hvisit https://example.com/visible now");
    mux.sessions.insert(1, session);
    drop(compose_after(&mut mux, FullRedrawReason::FirstAttach));
    let inner = mux.visible_panes()[0].inner;
    let (tx, mut rx) = mpsc::unbounded_channel();
    mux.client.attach(tx);

    mux.update_pointer_shape_for_mouse(inner.row, inner.col + 7, 43);
    mux.client.flush_out_of_band();
    let shape = rx.try_recv().expect("link hover pointer-shape update");
    assert!(shape.ends_with(b"\x1b]22;pointer\x1b\\"));
}

#[test]
fn pointer_shape_updates_for_usage_dialog_tabs() {
    let mut mux = single_pane_tab_mux();
    mux.pointer_shapes_supported = true;
    let mut view = jackin_protocol::control::FocusedUsageView::unavailable("seed", 1);
    view.focused_provider = Some("OpenAI".to_owned());
    view.tabs = vec![jackin_protocol::control::UsageProviderTab {
        label: "OpenAI".to_owned(),
        status_label: "usage unavailable".to_owned(),
        account_label: "seed".to_owned(),
        plan_label: None,
        source_label: None,
        active: true,
    }];
    mux.dialog_push(Dialog::new_usage(view.clone()));
    let dialog = mux.dialog_top().expect("usage dialog should open");
    let (row, col, rows, cols) = dialog.box_rect(mux.term_rows, mux.term_cols);
    let area = ratatui::layout::Rect {
        x: col,
        y: row,
        width: cols,
        height: rows,
    };
    let inner = crate::tui::components::dialog_widgets::usage_dialog_inner_area(area);
    let tabs = crate::tui::components::dialog_widgets::usage_tab_strip_labels(
        &view,
        crate::tui::components::dialog::UsageDialogTab::Provider,
    );
    let tab_area = crate::tui::components::dialog_widgets::usage_tab_strip_area(inner, &tabs);
    let (tx, mut rx) = mpsc::unbounded_channel();
    mux.client.attach(tx);

    mux.update_pointer_shape_for_mouse(tab_area.y, tab_area.x, SGR_NO_BUTTON_MOTION);
    mux.client.flush_out_of_band();

    let shape = rx.try_recv().expect("usage tab pointer-shape update");
    assert!(shape.ends_with(b"\x1b]22;pointer\x1b\\"));
}

#[test]
fn dialog_copy_hover_uses_overlay_frame_without_screen_erase() {
    let mut mux = single_pane_tab_mux_with_size(32, 100);
    mux.pointer_shapes_supported = false;
    mux.status_bar.identity_label = "jk-test-container".to_owned();
    drop(compose_after(&mut mux, FullRedrawReason::FirstAttach));
    drop(
        apply_action_frame(&mut mux, Action::OpenContainerInfo)
            .expect("debug info dialog should render an overlay frame"),
    );

    let (hover_row, hover_col) = {
        let github = mux.github_context_view();
        let dialog = mux.dialog_top().expect("debug info dialog should be open");
        (0..mux.term_rows)
            .flat_map(|row| (0..mux.term_cols).map(move |col| (row, col)))
            .find(|(row, col)| {
                dialog.clickable_at(
                    row.saturating_add(1),
                    col.saturating_add(1),
                    mux.term_rows,
                    mux.term_cols,
                    Some(&github),
                )
            })
            .expect("debug info dialog should expose a copyable value")
    };

    let frame = apply_action_frame(
        &mut mux,
        Action::MouseChromeUpdate {
            row: hover_row,
            col: hover_col,
            button: SGR_NO_BUTTON_MOTION,
        },
    )
    .expect("dialog copy hover should repaint the hovered row");
    assert!(
        !frame_contains_screen_erase(&frame),
        "dialog copy hover must not clear the full screen: {:?}",
        String::from_utf8_lossy(&frame)
    );
}

#[test]
fn wheel_scrolls_container_info_dialog_horizontally() {
    let mut mux = single_pane_tab_mux_with_size(24, 80);
    mux.status_bar.identity_label =
        "jk-test-container-with-long-debug-value-that-overflows-dialog-width".to_owned();
    mux.open_container_info_dialog();

    let frame = apply_action_frame(
        &mut mux,
        Action::Wheel {
            row: 10,
            col: 10,
            button: 67,
        },
    )
    .expect("horizontal wheel over debug dialog should redraw");

    assert!(!frame.is_empty());
    let Some(Dialog::ContainerInfo { scroll, .. }) = mux.dialog_top() else {
        panic!("container info dialog should remain open");
    };
    assert!(
        scroll.scroll_x > 0,
        "native horizontal touchpad wheel should move the dialog body"
    );
}

#[test]
fn wheel_on_container_info_unsupported_axis_does_not_scroll() {
    let mut mux = single_pane_tab_mux_with_size(40, 160);
    mux.status_bar.identity_label = "jk-test-container".to_owned();
    mux.open_container_info_dialog();

    // The hover pass may invalidate (first pointer position over the
    // dialog), so assert on the scroll state — the wheel on an
    // unsupported axis must not move the body.
    drop(apply_action_frame(
        &mut mux,
        Action::Wheel {
            row: 10,
            col: 10,
            button: 65,
        },
    ));

    let Some(Dialog::ContainerInfo { scroll, .. }) = mux.dialog_top() else {
        panic!("container info dialog should remain open");
    };
    assert_eq!(scroll.scroll_y, 0);
}

#[test]
fn bottom_container_click_opens_container_info_without_copying() {
    let mut mux = test_mux(24, 80);
    mux.pointer_shapes_supported = false;
    mux.status_bar.identity_label = "jk-test-container".to_owned();
    mux.status_bar.instance_id_label = "test".to_owned();
    mux.status_bar.role = "the-architect".to_owned();
    mux.pull_request_context_branch = Some(branch("feature/context"));
    let (tx, mut rx) = mpsc::unbounded_channel();
    mux.client.attach(tx);
    let hit = branch_context_bar_layout(
        mux.term_rows,
        mux.term_cols,
        mux.pull_request_context_branch.as_deref(),
        None,
        mux.pull_request_context.as_deref(),
        mux.pull_request_context_loading(),
        None,
        mux.status_bar.instance_id_label(),
    )
    .and_then(|layout| layout.container_region)
    .expect("container should fit");

    let press_row = mux.term_rows - 1;
    let frame = handle_input_frame(
        &mut mux,
        InputEvent::MousePress {
            row: press_row,
            col: hit.start - 1,
            button: 0,
        },
    )
    .expect("container click should redraw");

    while let Ok(output) = rx.try_recv() {
        assert!(
            !output
                .windows(b"\x1b]52;c;".len())
                .any(|w| w == b"\x1b]52;c;"),
            "opening container info must not send OSC 52"
        );
    }
    assert!(!String::from_utf8_lossy(&frame).contains("Copied!"));
    let Some(Dialog::ContainerInfo {
        copied_row: None,
        workdir,
        ..
    }) = mux.dialog_top()
    else {
        panic!("identity click should open container info")
    };
    assert_eq!(workdir, "/workspace");
}

#[test]
fn bottom_context_click_opens_github_context_dialog() {
    let mut mux = test_mux(24, 100);
    mux.status_bar.identity_label = "jk-test-container".to_owned();
    mux.status_bar.instance_id_label = "test".to_owned();
    mux.pull_request_context_branch = Some(branch("feature/context"));
    mux.pull_request_context = Some(Arc::new(pull_request_fixture(434)));
    mux.workdir_context.gh_available = false;
    let hit = branch_context_bar_layout(
        mux.term_rows,
        mux.term_cols,
        mux.pull_request_context_branch.as_deref(),
        None,
        mux.pull_request_context.as_deref(),
        mux.pull_request_context_loading(),
        None,
        mux.status_bar.instance_id_label(),
    )
    .and_then(|layout| layout.left_region)
    .expect("GitHub context should fit");

    let press_row = mux.term_rows - 1;
    let frame = handle_input_frame(
        &mut mux,
        InputEvent::MousePress {
            row: press_row,
            col: hit.start - 1,
            button: 0,
        },
    )
    .expect("context click should redraw");

    let rendered = String::from_utf8_lossy(&frame);
    assert!(rendered.contains("GitHub context"));
    assert!(
        rendered.contains("copy GitHub URL"),
        "dialog hint must render with the dialog chrome: {rendered:?}"
    );
    let hint_row = mux.term_rows - 2;
    let bottom_row = mux.term_rows;
    assert!(
        rendered.contains(&format!("\x1b[{hint_row};")),
        "dialog hint should render in the reserved hint region: {rendered:?}"
    );
    // Outside a debug launch the bottom branch/context bar is hidden under a
    // dialog (commit 5f2076a6); this mux has no debug run id, so the final row
    // must stay clear — only the dialog hint renders below the dialog.
    assert!(
        !rendered.contains(&format!("\x1b[{bottom_row};")),
        "bottom branch/context bar must be hidden under a dialog outside debug: {rendered:?}"
    );
    assert!(matches!(
        mux.dialog_top(),
        Some(Dialog::GitHubContext { copied: false, .. })
    ));
    assert_eq!(
        mux.pull_request_context_branch.as_deref(),
        Some("feature/context")
    );
    assert_eq!(
        mux.pull_request_context.as_ref().map(|pr| pr.number),
        Some(434)
    );
}

#[test]
fn container_info_copy_feedback_expires() {
    let mut mux = test_mux(24, 80);
    mux.dialog_push(Dialog::ContainerInfo {
        container_name: "jk-test-container".to_owned(),
        role: "the-architect".to_owned(),
        focused_agent: Some("claude".to_owned()),
        workdir: "/workspace".to_owned(),
        diagnostics: crate::tui::components::dialog::ContainerInfoDiagnostics::default(),
        copied_row: Some(0),
        hovered_row: None,
        scroll: jackin_tui::components::DialogBodyScroll::new(),
    });
    let now = Instant::now();
    mux.dialog_copy_feedback_deadline = Some(now);

    assert!(mux.expire_dialog_copy_feedback(now));
    assert!(matches!(
        mux.dialog_top(),
        Some(Dialog::ContainerInfo {
            copied_row: None,
            ..
        })
    ));
}

#[test]
fn container_info_id_click_copies_and_renders_feedback() {
    let mut mux = test_mux(40, 120);
    mux.pointer_shapes_supported = false;
    mux.dialog_push(Dialog::ContainerInfo {
        container_name: "jk-test-container".to_owned(),
        role: "the-architect".to_owned(),
        focused_agent: Some("claude".to_owned()),
        workdir: "/workspace".to_owned(),
        diagnostics: crate::tui::components::dialog::ContainerInfoDiagnostics::default(),
        copied_row: None,
        hovered_row: None,
        scroll: jackin_tui::components::DialogBodyScroll::new(),
    });
    let (tx, mut rx) = mpsc::unbounded_channel();
    mux.client.attach(tx);
    let (box_row, box_col, _, _) = mux
        .dialog_top()
        .expect("container info dialog should be open")
        .box_rect(mux.term_rows, mux.term_cols);

    let frame = handle_input_frame(
        &mut mux,
        InputEvent::MousePress {
            row: box_row + 1,
            // Click the value column (the cyan link), past the widest label.
            col: box_col + 22,
            button: 0,
        },
    )
    .expect("container id click should redraw copy feedback");

    mux.client.flush_out_of_band();
    let mut saw_osc52 = false;
    while let Ok(output) = rx.try_recv() {
        saw_osc52 |= output
            .windows(b"\x1b]52;c;".len())
            .any(|w| w == b"\x1b]52;c;");
    }
    assert!(saw_osc52, "copy should emit OSC 52");
    assert!(String::from_utf8_lossy(&frame).contains("Copied!"));
    assert!(matches!(
        mux.dialog_top(),
        Some(Dialog::ContainerInfo {
            copied_row: Some(0),
            ..
        })
    ));
}

#[test]
fn prefix_ctrl_l_has_named_pane_clear_reason() {
    assert_eq!(
        prefix_full_redraw_reason(&PrefixCommand::ClearPane),
        FullRedrawReason::PaneClear
    );
}

#[test]
fn command_stdout_trimmed_returns_trimmed_stdout() {
    let mut command = Command::new("printf");
    command.arg("  branch-name\n");

    assert_eq!(
        command_stdout_trimmed(&mut command),
        Some("branch-name".to_owned())
    );
}

#[test]
fn command_stdout_trimmed_rejects_known_failure_status() {
    // `sleep 0.05` keeps the child alive long enough for the
    // try_wait poll loop to observe `Ok(None)` first and then the
    // failing `Ok(Some(1))` exit on the next tick. Without the
    // sleep the child can vanish between spawn and the first
    // try_wait, which collapses the Err(ECHILD) "status lost"
    // arm and the Ok(Some(false)) "failed" arm into one path.
    let mut command = Command::new("sh");
    command.args(["-c", "printf branch-name; sleep 0.05; exit 1"]);

    assert_eq!(command_stdout_trimmed(&mut command), None);
}

#[test]
fn gh_lookup_output_rejects_statusless_stderr_only_failure() {
    let err = command_output_or_lookup_error("gh", None, b"", b"HTTP 401: Bad credentials\n")
        .expect_err("stderr-only statusless gh output is a transient failure");

    assert!(
        err.to_string().contains("HTTP 401"),
        "stderr detail should survive for logs: {err}"
    );
}

// Action-boundary dispatch tests: drive apply_action directly without
// going through handle_input so the dispatch layer is testable without
// a live PTY or input parser in the loop.

#[test]
fn apply_action_dismiss_closes_top_dialog() {
    let mut mux = single_pane_tab_mux();
    mux.open_command_palette();
    assert!(mux.dialog_open(), "palette should be open");
    drop(compose_after(&mut mux, FullRedrawReason::FirstAttach));

    let frame = apply_action_frame(&mut mux, Action::Dialog(DialogAction::Dismiss))
        .expect("dialog dismiss should redraw");

    assert!(!mux.dialog_open(), "dismiss should close the dialog");
    assert_eq!(mux.mux_mode(), MuxMode::Normal);
    assert!(
        !frame_contains_screen_erase(&frame),
        "dialog dismiss must not clear the full terminal screen"
    );
}

#[test]
fn apply_action_open_palette_pushes_palette_dialog() {
    let mut mux = single_pane_tab_mux();
    assert!(!mux.dialog_open());
    drop(compose_after(&mut mux, FullRedrawReason::FirstAttach));

    let frame =
        apply_action_frame(&mut mux, Action::OpenPalette).expect("open palette should redraw");

    assert!(
        matches!(mux.dialog_top(), Some(Dialog::CommandPalette { .. })),
        "OpenPalette should push CommandPalette dialog"
    );
    assert_eq!(mux.mux_mode(), MuxMode::Dialog);
    assert!(
        !frame_contains_screen_erase(&frame),
        "open palette must not clear the full terminal screen"
    );
}

#[test]
fn apply_action_open_palette_closes_existing_dialog() {
    let mut mux = single_pane_tab_mux();
    mux.open_command_palette();
    assert!(mux.dialog_open());
    drop(compose_after(&mut mux, FullRedrawReason::FirstAttach));

    let frame =
        apply_action_frame(&mut mux, Action::OpenPalette).expect("close palette should redraw");

    assert!(
        !mux.dialog_open(),
        "palette toggle should close open dialog"
    );
    assert_eq!(mux.mux_mode(), MuxMode::Normal);
    assert!(
        !frame_contains_screen_erase(&frame),
        "close palette must not clear the full terminal screen"
    );
}

#[test]
fn apply_action_open_container_info_pushes_dialog() {
    let mut mux = single_pane_tab_mux();
    mux.status_bar.identity_label = "jk-test-container".to_owned();
    mux.status_bar.role = "test-role".to_owned();

    mux.apply_action(Action::OpenContainerInfo);

    assert!(matches!(
        mux.dialog_top(),
        Some(Dialog::ContainerInfo {
            container_name,
            copied_row: None,
            ..
        }) if container_name == "jk-test-container"
    ));
}

#[test]
fn apply_action_open_rename_tab_pushes_dialog() {
    let mut mux = single_pane_tab_mux();
    drop(compose_after(&mut mux, FullRedrawReason::FirstAttach));

    let frame = apply_action_frame(&mut mux, Action::OpenRenameTab(0))
        .expect("open rename dialog should redraw");

    assert!(matches!(
        mux.dialog_top(),
        Some(Dialog::RenameTab { tab_idx: 0, .. })
    ));
    assert!(mux.last_tab_click.is_none());
    assert!(
        !frame_contains_screen_erase(&frame),
        "open rename dialog must not clear the full terminal screen"
    );
}

#[test]
fn apply_action_switch_tab_moves_active_tab() {
    let mut mux = single_pane_tab_mux();
    mux.tabs.push(Tab::new_single("Shell", 2, "test"));
    drop(compose_after(&mut mux, FullRedrawReason::ExplicitRedraw));

    mux.apply_action(Action::SwitchTab(1));

    assert_eq!(mux.active_tab, 1);
}

#[test]
fn tab_bar_focus_key_maps_arrows_and_exit() {
    use super::input_dispatch::{TabBarFocusKey, tab_bar_focus_key};
    assert_eq!(tab_bar_focus_key(b"\x1b[C"), Some(TabBarFocusKey::Next)); // Right
    assert_eq!(tab_bar_focus_key(b"\x1b[D"), Some(TabBarFocusKey::Prev)); // Left
    assert_eq!(tab_bar_focus_key(b"\x1b[B"), Some(TabBarFocusKey::Exit)); // Down
    assert_eq!(tab_bar_focus_key(b"\x1b"), Some(TabBarFocusKey::Exit)); // Esc
    assert_eq!(tab_bar_focus_key(b"x"), None);
}

#[test]
fn tab_bar_focus_mode_arrows_switch_tabs_then_esc_returns_to_agent() {
    // P5: while the tab bar is focused, Left/Right switch agent tabs and Esc
    // returns focus to the agent content.
    let mut mux = single_pane_tab_mux();
    mux.tabs.push(Tab::new_single("Shell", 2, "test"));
    drop(compose_after(&mut mux, FullRedrawReason::ExplicitRedraw));

    mux.set_tab_bar_focused(true);
    assert!(mux.tab_bar_focused);

    mux.handle_input(InputEvent::Data(b"\x1b[C".to_vec())); // Right → next tab
    assert_eq!(mux.active_tab, 1);
    mux.handle_input(InputEvent::Data(b"\x1b[D".to_vec())); // Left → previous tab
    assert_eq!(mux.active_tab, 0);
    assert!(mux.tab_bar_focused, "arrows keep the bar focused");

    mux.handle_input(InputEvent::Data(b"\x1b".to_vec())); // Esc → back to agent
    assert!(!mux.tab_bar_focused);
}

#[test]
fn apply_action_status_bar_click_switches_tab() {
    let mut mux = single_pane_tab_mux();
    mux.tabs.push(Tab::new_single("Shell", 2, "test"));
    drop(compose_after(&mut mux, FullRedrawReason::ExplicitRedraw));
    let col = (1..mux.term_cols)
        .find(|col| mux.status_bar.tab_at_col(*col) == Some(1))
        .expect("second tab should have a clickable column")
        - 1;

    mux.apply_action(Action::StatusBarClick { col });

    assert_eq!(mux.active_tab, 1);
}

#[test]
fn apply_action_status_bar_double_click_opens_rename() {
    let mut mux = single_pane_tab_mux();
    drop(compose_after(&mut mux, FullRedrawReason::ExplicitRedraw));
    let col = (1..mux.term_cols)
        .find(|col| mux.status_bar.tab_at_col(*col) == Some(0))
        .expect("first tab should have a clickable column")
        - 1;

    mux.apply_action(Action::StatusBarClick { col });
    mux.apply_action(Action::StatusBarClick { col });

    assert!(matches!(
        mux.dialog_top(),
        Some(Dialog::RenameTab { tab_idx: 0, .. })
    ));
}

#[test]
fn apply_action_branch_context_bar_click_opens_container_info() {
    let mut mux = test_mux(24, 80);
    mux.status_bar.identity_label = "jk-test-container".to_owned();
    mux.status_bar.instance_id_label = "test".to_owned();
    mux.status_bar.role = "the-architect".to_owned();
    mux.pull_request_context_branch = Some(branch("feature/context"));
    let hit = branch_context_bar_layout(
        mux.term_rows,
        mux.term_cols,
        mux.pull_request_context_branch.as_deref(),
        None,
        mux.pull_request_context.as_deref(),
        mux.pull_request_context_loading(),
        None,
        mux.status_bar.instance_id_label(),
    )
    .and_then(|layout| layout.container_region)
    .expect("container should fit");

    mux.apply_action(Action::BranchContextBarClick {
        row: mux.term_rows - 1,
        col: hit.start - 1,
    });

    assert!(matches!(
        mux.dialog_top(),
        Some(Dialog::ContainerInfo {
            container_name,
            copied_row: None,
            ..
        }) if container_name == "jk-test-container"
    ));
}

#[test]
fn apply_action_palette_new_tab_pushes_agent_picker() {
    let mut mux = single_pane_tab_mux();
    mux.open_command_palette();
    drop(compose_after(&mut mux, FullRedrawReason::FirstAttach));

    let frame = apply_action_frame(&mut mux, Action::Palette(PaletteCommand::NewTab))
        .expect("palette new tab should redraw agent picker");

    assert!(matches!(mux.dialog_top(), Some(Dialog::AgentPicker { .. })));
    assert!(
        !frame_contains_screen_erase(&frame),
        "palette new tab agent picker must not clear the full terminal screen"
    );
}

#[test]
fn apply_action_open_agent_picker_pushes_picker() {
    let mut mux = single_pane_tab_mux();
    drop(compose_after(&mut mux, FullRedrawReason::FirstAttach));

    let frame = apply_action_frame(&mut mux, Action::OpenAgentPicker(PickerIntent::NewTab))
        .expect("open agent picker should redraw");

    assert!(matches!(mux.dialog_top(), Some(Dialog::AgentPicker { .. })));
    assert!(
        !frame_contains_screen_erase(&frame),
        "open agent picker must not clear the full terminal screen"
    );
}

#[test]
fn apply_action_detach_sets_detach_request() {
    let mut mux = single_pane_tab_mux();

    mux.apply_action(Action::Detach);

    assert!(mux.detach_requested);
}

#[test]
fn prefix_new_tab_routes_through_action_picker() {
    let mut mux = single_pane_tab_mux();
    drop(compose_after(&mut mux, FullRedrawReason::FirstAttach));

    let frame = prefix_command_frame(&mut mux, PrefixCommand::NewTab)
        .expect("prefix new-tab should redraw agent picker");

    assert!(matches!(mux.dialog_top(), Some(Dialog::AgentPicker { .. })));
    assert!(
        !frame_contains_screen_erase(&frame),
        "prefix new-tab must not clear the full terminal screen"
    );
}

#[test]
fn prefix_palette_uses_overlay_frame_without_screen_erase() {
    let mut mux = single_pane_tab_mux();
    drop(compose_after(&mut mux, FullRedrawReason::FirstAttach));

    let frame = prefix_command_frame(&mut mux, PrefixCommand::Palette)
        .expect("prefix palette should redraw command palette");

    assert!(matches!(
        mux.dialog_top(),
        Some(Dialog::CommandPalette { .. })
    ));
    assert!(
        !frame_contains_screen_erase(&frame),
        "prefix palette must not clear the full terminal screen"
    );
}

#[test]
fn prefix_move_focus_uses_diff_frame_without_screen_erase() {
    let mut mux = split_tab_mux();
    drop(compose_after(&mut mux, FullRedrawReason::FirstAttach));

    let frame = prefix_command_frame(&mut mux, PrefixCommand::MoveFocus(ArrowDir::Right))
        .expect("prefix focus move should redraw");

    assert_eq!(mux.tabs[mux.active_tab].focused_id, 2);
    assert!(
        !frame_contains_screen_erase(&frame),
        "prefix focus move must not clear the full terminal screen"
    );
}

#[test]
fn prefix_clear_pane_uses_diff_frame_without_screen_erase() {
    let mut mux = single_pane_tab_mux();
    let (session, mut input_rx) = test_shell_session(20, 78);
    mux.sessions.insert(1, session);
    drop(compose_after(&mut mux, FullRedrawReason::FirstAttach));

    let frame = prefix_command_frame(&mut mux, PrefixCommand::ClearPane)
        .expect("prefix clear-pane should redraw");

    assert_eq!(
        input_rx
            .try_recv()
            .expect("prefix clear-pane should send Ctrl+L"),
        b"\x0c"
    );
    assert!(
        !frame_contains_screen_erase(&frame),
        "prefix clear-pane must not clear the full terminal screen"
    );
}

#[test]
fn prefix_redraw_repaints_in_place_without_screen_erase() {
    // The explicit-redraw chord must not clear the full terminal; under the
    // wipe policy (I4) only FirstAttach/Resize erase.
    let mut mux = single_pane_tab_mux();
    drop(compose_after(&mut mux, FullRedrawReason::FirstAttach));

    let frame = prefix_command_frame(&mut mux, PrefixCommand::Redraw)
        .expect("prefix redraw should emit a repaint frame");

    assert!(
        !frame_contains_screen_erase(&frame),
        "prefix redraw repaints in place under the wipe policy (no 2J)"
    );
}

#[test]
fn apply_action_focus_pane_at_changes_focus() {
    let mut mux = split_tab_mux();
    drop(compose_after(&mut mux, FullRedrawReason::FirstAttach));
    let target = mux
        .visible_panes()
        .into_iter()
        .find(|pane| pane.id == 2)
        .expect("second pane should be visible")
        .inner;

    let frame = apply_action_frame(
        &mut mux,
        Action::FocusPaneAt {
            row: target.row,
            col: target.col,
        },
    )
    .expect("focus change should redraw");

    assert_eq!(mux.tabs[mux.active_tab].focused_id, 2);
    assert!(!frame.is_empty(), "focus redraw frame should be emitted");
    assert!(
        !frame_contains_screen_erase(&frame),
        "mouse focus change must not clear the full screen"
    );
}

#[test]
fn apply_action_move_focus_uses_diff_frame_without_screen_erase() {
    let mut mux = split_tab_mux();
    drop(compose_after(&mut mux, FullRedrawReason::FirstAttach));

    let frame = apply_action_frame(&mut mux, Action::MoveFocus(ArrowDir::Right))
        .expect("keyboard focus move should redraw");

    assert_eq!(mux.tabs[mux.active_tab].focused_id, 2);
    assert!(!frame.is_empty(), "focus redraw frame should be emitted");
    assert!(
        !frame_contains_screen_erase(&frame),
        "keyboard focus change must not clear the full screen"
    );
}

#[test]
fn apply_action_clear_focused_pane_uses_diff_frame_without_screen_erase() {
    let mut mux = single_pane_tab_mux();
    let (session, mut input_rx) = test_shell_session(20, 78);
    mux.sessions.insert(1, session);
    drop(compose_after(&mut mux, FullRedrawReason::FirstAttach));

    let frame =
        apply_action_frame(&mut mux, Action::ClearFocusedPane).expect("clear pane should redraw");

    assert_eq!(
        input_rx.try_recv().expect("clear pane should send Ctrl+L"),
        b"\x0c"
    );
    assert!(
        !frame.is_empty(),
        "clear pane redraw frame should be emitted"
    );
    assert!(
        !frame_contains_screen_erase(&frame),
        "clear pane must not clear the full terminal screen"
    );
}

#[test]
fn palette_clear_pane_uses_diff_frame_without_screen_erase() {
    let mut mux = single_pane_tab_mux();
    let (session, mut input_rx) = test_shell_session(20, 78);
    mux.sessions.insert(1, session);
    mux.open_command_palette();
    drop(compose_after(&mut mux, FullRedrawReason::FirstAttach));

    let frame = apply_action_frame(&mut mux, Action::Palette(PaletteCommand::ClearPane))
        .expect("palette clear pane should redraw");

    assert!(!mux.dialog_open(), "palette clear pane should close dialog");
    assert_eq!(
        input_rx.try_recv().expect("clear pane should send Ctrl+L"),
        b"\x0c"
    );
    assert!(
        !frame.is_empty(),
        "clear pane redraw frame should be emitted"
    );
    assert!(
        !frame_contains_screen_erase(&frame),
        "palette clear pane must not clear the full terminal screen"
    );
}

#[test]
fn apply_action_forward_mouse_sends_to_focused_pane() {
    let mut mux = single_pane_tab_mux();
    let (mut session, mut input_rx) = test_shell_session(20, 78);
    session.feed_pty(b"\x1b[?1003h\x1b[?1006h");
    mux.sessions.insert(1, session);

    let frame = apply_action_frame(
        &mut mux,
        Action::ForwardMouse {
            row: STATUS_BAR_ROWS + 1,
            col: 1,
            button: 0,
            press: true,
        },
    );

    assert!(frame.is_none(), "PTY mouse forward should not redraw");
    assert_eq!(
        input_rx.try_recv().expect("mouse press should reach PTY"),
        b"\x1b[<0;1;1M"
    );
}

#[test]
fn apply_action_dialog_consume_keeps_dialog_open() {
    let mut mux = single_pane_tab_mux();
    mux.open_command_palette();
    assert!(mux.dialog_open());
    drop(compose_after(&mut mux, FullRedrawReason::FirstAttach));

    // Consume should leave the dialog open (key was absorbed, no state change).
    let frame = apply_action_frame(&mut mux, Action::Dialog(DialogAction::Consume))
        .expect("dialog consume should redraw");

    assert!(mux.dialog_open(), "Consume must not close the dialog");
    assert!(
        !frame_contains_screen_erase(&frame),
        "dialog consume must not clear the full terminal screen"
    );
}

#[test]
fn apply_dialog_spawn_agent_provider_picker_uses_overlay_frame_without_screen_erase() {
    let mut mux = single_pane_tab_mux();
    mux.provider_keys.insert(
        jackin_protocol::Provider::Anthropic,
        "anthropic-test-token".to_owned(),
    );
    mux.provider_keys
        .insert(jackin_protocol::Provider::Zai, "zai-test-token".to_owned());
    mux.open_command_palette();
    drop(compose_after(&mut mux, FullRedrawReason::FirstAttach));

    let frame = apply_action_frame(
        &mut mux,
        Action::Dialog(DialogAction::SpawnAgent {
            agent: Some("claude".to_owned()),
            intent: PickerIntent::NewTab,
        }),
    )
    .expect("provider picker should redraw");

    assert!(matches!(
        mux.dialog_top(),
        Some(Dialog::ProviderPicker {
            agent: Some(agent),
            providers,
            intent: PickerIntent::NewTab,
            ..
        }) if agent == "claude" && providers.len() >= 2
    ));
    assert!(
        !frame_contains_screen_erase(&frame),
        "provider picker must not clear the full terminal screen"
    );
}

#[test]
fn provider_spawn_env_injects_codex_profile_only_for_codex_with_key() {
    let mut mux = test_mux(24, 80);
    mux.provider_keys
        .insert(jackin_protocol::Provider::Minimax, "mk".to_owned());

    // codex + MiniMax + resolved key → activate the v2 profile.
    let env = mux.provider_spawn_env("codex", jackin_protocol::Provider::Minimax);
    assert!(
        env.iter()
            .any(|(k, v)| k == "JACKIN_CODEX_PROFILE" && v == "minimax"),
        "codex+MiniMax with a key must activate the minimax profile"
    );

    // codex + OpenAI (native, no codex_profile) → no profile env.
    let env = mux.provider_spawn_env("codex", jackin_protocol::Provider::Openai);
    assert!(
        !env.iter().any(|(k, _)| k == "JACKIN_CODEX_PROFILE"),
        "native OpenAI must not set a Codex profile"
    );

    // claude + MiniMax → slug guard suppresses the Codex profile.
    let env = mux.provider_spawn_env("claude", jackin_protocol::Provider::Minimax);
    assert!(
        !env.iter().any(|(k, _)| k == "JACKIN_CODEX_PROFILE"),
        "non-codex agents must not set a Codex profile"
    );
}

#[test]
fn launch_model_uses_picked_provider_for_opencode() {
    // OpenCode has no model of its own: the picked provider supplies the `-m`
    // model. test_mux has no role-manifest model, so a wrong wiring shows as None.
    let mux = test_mux(24, 80);
    assert_eq!(
        mux.launch_model("opencode", Some("MiniMax")),
        Some("minimax/MiniMax-M3")
    );
    assert_eq!(
        mux.launch_model("opencode", Some("Z.AI")),
        Some("zai/glm-5.1")
    );
    assert_eq!(
        mux.launch_model("opencode", Some("Kimi")),
        Some("kimi/kimi-for-coding")
    );
    // Non-opencode agents ignore the provider for model selection (auth env only).
    assert_eq!(mux.launch_model("codex", Some("MiniMax")), None);
    // A provider with no opencode model falls back to the role-manifest model.
    assert_eq!(mux.launch_model("opencode", Some("Anthropic")), None);
}

#[test]
fn launch_model_prefers_manifest_provider_override_for_opencode() {
    let mut mux = test_mux(24, 80);
    mux.launch_config.provider_models.insert(
        "opencode".to_owned(),
        BTreeMap::from([("minimax".to_owned(), "minimax/custom".to_owned())]),
    );
    // The role's [opencode.providers.minimax].model override beats the built-in default.
    assert_eq!(
        mux.launch_model("opencode", Some("MiniMax")),
        Some("minimax/custom")
    );
    // A provider with no override still uses the built-in default.
    assert_eq!(
        mux.launch_model("opencode", Some("Z.AI")),
        Some("zai/glm-5.1")
    );
}

#[test]
fn provider_spawn_env_applies_claude_manifest_model_override() {
    let mut mux = test_mux(24, 80);
    mux.provider_keys
        .insert(jackin_protocol::Provider::Minimax, "mk".to_owned());
    mux.launch_config.provider_models.insert(
        "claude".to_owned(),
        BTreeMap::from([("minimax".to_owned(), "MiniMax-Pro".to_owned())]),
    );
    let env = mux.provider_spawn_env("claude", jackin_protocol::Provider::Minimax);
    let model_vars: Vec<_> = env
        .iter()
        .filter(|(k, _)| k.starts_with("ANTHROPIC_DEFAULT_") && k.ends_with("_MODEL"))
        .collect();
    assert!(
        !model_vars.is_empty(),
        "claude+MiniMax must set ANTHROPIC_DEFAULT_*_MODEL"
    );
    for (key, value) in model_vars {
        assert_eq!(
            value, "MiniMax-Pro",
            "{key} must carry the manifest override"
        );
    }
}

#[test]
fn provider_spawn_env_skips_codex_profile_when_key_unresolved() {
    // No MiniMax key captured → token unresolved. runtime-setup only writes the
    // profile file when the key is present, so the flag must NOT be pushed:
    // forcing `codex --profile minimax` against a missing file would hard-fail
    // instead of falling back to native auth.
    let mut mux = test_mux(24, 80);
    // Multiplexer::new seeds provider_keys from the ambient env; drop the
    // MiniMax key so the "unresolved" case holds regardless of MINIMAX_API_KEY.
    mux.provider_keys
        .remove(&jackin_protocol::Provider::Minimax);
    let env = mux.provider_spawn_env("codex", jackin_protocol::Provider::Minimax);
    assert!(
        !env.iter().any(|(k, _)| k == "JACKIN_CODEX_PROFILE"),
        "without a resolved key, codex must fall back to native auth, not force --profile"
    );
}

#[test]
fn env_for_spawn_keeps_allowlisted_drops_unknown() {
    let mux = test_mux(24, 80);
    let env = mux.env_for_spawn(&[
        ("JACKIN_CODEX_PROFILE".to_owned(), "minimax".to_owned()),
        ("TOTALLY_NOT_ALLOWLISTED".to_owned(), "x".to_owned()),
    ]);
    assert!(
        env.iter()
            .any(|(k, v)| k == "JACKIN_CODEX_PROFILE" && v == "minimax"),
        "JACKIN_CODEX_PROFILE must survive the passthrough allowlist"
    );
    assert!(
        !env.iter().any(|(k, _)| k == "TOTALLY_NOT_ALLOWLISTED"),
        "non-allowlisted keys must be dropped"
    );
}

#[test]
fn apply_action_dialog_click_routes_to_dialog_handler() {
    let mut mux = single_pane_tab_mux();
    mux.open_command_palette();
    assert!(mux.dialog_open());

    mux.apply_action(Action::DialogClick { row: 0, col: 0 });

    assert!(!mux.dialog_open(), "outside click should dismiss dialog");
}

#[test]
fn apply_action_focus_report_does_not_open_dialog() {
    let mut mux = single_pane_tab_mux();
    assert!(!mux.dialog_open());

    mux.apply_action(Action::FocusReport(true));
    assert!(!mux.dialog_open());

    mux.apply_action(Action::FocusReport(false));
    assert!(!mux.dialog_open());
}

#[test]
fn apply_action_mouse_chrome_update_sets_pointer_shape() {
    let mut mux = single_pane_tab_mux();
    mux.pointer_shapes_supported = true;
    drop(compose_after(&mut mux, FullRedrawReason::ExplicitRedraw));
    let (tx, mut rx) = mpsc::unbounded_channel();
    mux.client.attach(tx);
    let tab_col = mux
        .status_bar
        .tab_regions
        .first()
        .map(|(start, _)| start.saturating_sub(1))
        .expect("tab region should render");

    mux.apply_action(Action::MouseChromeUpdate {
        row: 0,
        col: tab_col,
        button: SGR_NO_BUTTON_MOTION,
    });

    mux.client.flush_out_of_band();
    let mut outputs = Vec::new();
    while let Ok(output) = rx.try_recv() {
        outputs.push(output);
    }
    let frame: Vec<u8> = outputs.iter().flatten().copied().collect();
    assert!(
        !frame.is_empty(),
        "mouse chrome action should emit hover repaint and pointer shape update"
    );
    assert!(
        !frame_contains_screen_erase(&frame),
        "mouse chrome hover must not clear the full screen"
    );
    assert!(
        outputs
            .iter()
            .any(|output| output.ends_with(b"\x1b]22;pointer\x1b\\")),
        "mouse chrome action should emit pointer shape update"
    );
}

#[test]
fn apply_action_wheel_scrolls_scrollback() {
    let mut mux = single_pane_tab_mux();
    let (mut session, mut input_rx) = test_shell_session(20, 78);
    for i in 0..40 {
        session.feed_pty(format!("line {i}\r\n").as_bytes());
    }
    mux.sessions.insert(1, session);
    drop(compose_after(&mut mux, FullRedrawReason::FirstAttach));

    let frame = apply_action_frame(
        &mut mux,
        Action::Wheel {
            row: STATUS_BAR_ROWS + 1,
            col: 1,
            button: 64,
        },
    )
    .expect("wheel over retained scrollback should redraw");

    input_rx.try_recv().expect_err("mouse-disabled pane must not receive raw wheel bytes");
    assert_eq!(mux.sessions.get(&1).unwrap().scrollback_offset(), 3);
    assert!(
        !frame.is_empty(),
        "scrollback redraw frame should be emitted"
    );
    assert!(
        !frame.windows(b"\x1b[2J".len()).any(|w| w == b"\x1b[2J"),
        "scrollback wheel movement should diff the pane instead of clearing the full terminal"
    );
}

#[test]
fn typed_input_snaps_scrollback_to_live_without_screen_erase() {
    let mut mux = single_pane_tab_mux();
    let (mut session, mut input_rx) = test_shell_session(20, 78);
    for i in 0..40 {
        session.feed_pty(format!("line {i}\r\n").as_bytes());
    }
    session.scroll_by(3);
    assert_eq!(session.scrollback_offset(), 3);
    mux.sessions.insert(1, session);
    drop(compose_after(&mut mux, FullRedrawReason::FirstAttach));

    let frame = apply_action_frame(&mut mux, Action::PaneData(b"x".to_vec()))
        .expect("typing while viewing scrollback should snap to live and repaint");

    assert_eq!(
        mux.sessions.get(&1).unwrap().scrollback_offset(),
        0,
        "typing should return the pane to the live tail"
    );
    assert_eq!(input_rx.try_recv().unwrap(), b"x");
    assert!(
        !frame.is_empty(),
        "scrollback snap repaint should emit a frame"
    );
    assert!(
        !frame_contains_screen_erase(&frame),
        "typing scrollback snap must not clear the full screen"
    );
}

#[test]
fn image_path_paste_uses_plain_bytes_when_bracketed_paste_is_off() {
    let mut mux = single_pane_tab_mux();
    let (session, mut input_rx) = test_shell_session(20, 78);
    mux.sessions.insert(1, session);

    assert!(mux.paste_text_to_focused_pane(b"/jackin/run/clipboard/clipboard-test.png"));

    assert_eq!(
        input_rx.try_recv().unwrap(),
        b"/jackin/run/clipboard/clipboard-test.png"
    );
    input_rx.try_recv().expect_err("plain paste should not produce extra PTY input");
}

#[test]
fn image_path_paste_uses_bracketed_paste_when_enabled() {
    let mut mux = single_pane_tab_mux();
    let (mut session, mut input_rx) = test_shell_session(20, 78);
    session.feed_pty(b"\x1b[?2004h");
    assert!(
        session.bracketed_paste(),
        "test session should track bracketed-paste mode"
    );
    mux.sessions.insert(1, session);

    assert!(mux.paste_text_to_focused_pane(b"/jackin/run/clipboard/clipboard-test.png"));

    assert_eq!(
        input_rx.try_recv().unwrap(),
        b"\x1b[200~/jackin/run/clipboard/clipboard-test.png\x1b[201~"
    );
    input_rx.try_recv().expect_err("bracketed paste should be one PTY input chunk");
}

#[test]
fn image_path_paste_reports_missing_focused_session() {
    let mut mux = single_pane_tab_mux();

    assert!(!mux.paste_text_to_focused_pane(b"/jackin/run/clipboard/clipboard-test.png"));
}

#[test]
fn apply_action_wheel_noops_at_scrollback_boundary() {
    let mut mux = single_pane_tab_mux();
    let (mut session, mut input_rx) = test_shell_session(20, 78);
    for i in 0..25 {
        session.feed_pty(format!("line {i}\r\n").as_bytes());
    }
    let filled = session.scrollback_filled();
    assert!(filled > 0, "setup should retain scrollback");
    mux.sessions.insert(1, session);

    let mut last = Some(Vec::new());
    for _ in 0..(filled + 2) {
        last = apply_action_frame(
            &mut mux,
            Action::Wheel {
                row: STATUS_BAR_ROWS + 1,
                col: 1,
                button: 64,
            },
        );
        if last.is_none() {
            break;
        }
    }

    input_rx.try_recv().expect_err("mouse-disabled pane must not receive raw wheel bytes");
    assert_eq!(mux.sessions.get(&1).unwrap().scrollback_offset(), filled);
    assert!(
        last.is_none(),
        "wheel event at max scrollback offset should not redraw"
    );
}

#[test]
fn apply_action_end_drag_resize_clears_drag_state() {
    let mut mux = single_pane_tab_mux();
    mux.drag = Some(DragState {
        tab_idx: 0,
        path: Vec::new(),
        orient: SplitOrient::Horizontal,
        rect: Rect::new(STATUS_BAR_ROWS, 0, mux.content_rows, mux.term_cols),
    });

    let frame = apply_action_frame(&mut mux, Action::EndDragResize)
        .expect("ending drag should redraw layout");

    assert!(mux.drag.is_none(), "drag state should be cleared");
    assert!(!frame.is_empty(), "layout redraw frame should be emitted");
}

#[test]
fn apply_action_mouse_release_ends_drag_resize() {
    let mut mux = single_pane_tab_mux();
    mux.drag = Some(DragState {
        tab_idx: 0,
        path: Vec::new(),
        orient: SplitOrient::Horizontal,
        rect: Rect::new(STATUS_BAR_ROWS, 0, mux.content_rows, mux.term_cols),
    });

    let frame = apply_action_frame(
        &mut mux,
        Action::MouseRelease {
            row: STATUS_BAR_ROWS,
            col: 1,
            button: 0,
        },
    )
    .expect("left-button release should redraw layout after drag");

    assert!(mux.drag.is_none(), "drag state should be cleared");
    assert!(!frame.is_empty(), "layout redraw frame should be emitted");
}

#[test]
fn apply_action_start_drag_resize_sets_drag_state() {
    let mut mux = split_tab_mux();
    let (row, col) = (0..mux.term_rows)
        .flat_map(|row| (0..mux.term_cols).map(move |col| (row, col)))
        .find(|(row, col)| mux.detect_drag_start(*row, *col).is_some())
        .expect("split tab should expose a draggable border");

    let frame = apply_action_frame(&mut mux, Action::StartDragResize { row, col });

    assert!(frame.is_none(), "drag start should not redraw yet");
    assert!(mux.drag.is_some(), "drag state should be active");
}

#[test]
fn apply_action_pane_primary_press_starts_drag_on_border() {
    let mut mux = split_tab_mux();
    let (row, col) = (0..mux.term_rows)
        .flat_map(|row| (0..mux.term_cols).map(move |col| (row, col)))
        .find(|(row, col)| mux.detect_drag_start(*row, *col).is_some())
        .expect("split tab should expose a draggable border");

    let frame = apply_action_frame(&mut mux, Action::PanePrimaryPress { row, col });

    assert!(frame.is_none(), "drag start should not redraw yet");
    assert!(mux.drag.is_some(), "drag state should be active");
}

#[test]
fn apply_action_pane_primary_press_only_arms_selection_for_shell() {
    let mut mux = single_pane_tab_mux();
    let (session, mut input_rx) = test_shell_session(20, 78);
    mux.sessions.insert(1, session);
    drop(compose_after(&mut mux, FullRedrawReason::FirstAttach));

    let frame = apply_action_frame(
        &mut mux,
        Action::PanePrimaryPress {
            row: STATUS_BAR_ROWS + 1,
            col: 1,
        },
    );

    input_rx.try_recv().expect_err("mouse-disabled pane should arm selection instead of receiving raw mouse");
    assert!(mux.selection.is_none(), "plain press should not select yet");
    assert!(
        mux.pending_selection.is_some(),
        "selection should be pending until drag motion"
    );
    assert!(
        frame.is_none(),
        "arming selection should not repaint or flash selection chrome"
    );
}

#[test]
fn pane_button_motion_promotes_pending_selection() {
    let mut mux = single_pane_tab_mux();
    let (session, _input_rx) = test_shell_session(20, 78);
    mux.sessions.insert(1, session);
    drop(compose_after(&mut mux, FullRedrawReason::FirstAttach));
    let press_row = STATUS_BAR_ROWS + 1;
    let press_col = 1;
    assert!(
        apply_action_frame(
            &mut mux,
            Action::PanePrimaryPress {
                row: press_row,
                col: press_col,
            }
        )
        .is_none()
    );

    let frame = apply_action_frame(
        &mut mux,
        Action::PaneButtonMotion {
            row: press_row + 1,
            col: press_col + 2,
        },
    )
    .expect("drag motion should promote pending selection and repaint");

    assert!(mux.pending_selection.is_none());
    let selection = mux
        .selection
        .expect("selection should be active after drag");
    assert_eq!((selection.anchor_row, selection.anchor_col), (0, 0));
    assert_eq!((selection.end_row, selection.end_col), (1, 2));
    assert!(
        !frame.is_empty(),
        "selection repaint frame should be emitted"
    );
    assert!(
        !frame_contains_screen_erase(&frame),
        "selection start must not clear the full screen"
    );
}

#[test]
fn mouse_release_without_drag_clears_pending_selection() {
    let mut mux = single_pane_tab_mux();
    let (session, _input_rx) = test_shell_session(20, 78);
    mux.sessions.insert(1, session);
    drop(compose_after(&mut mux, FullRedrawReason::FirstAttach));
    let row = STATUS_BAR_ROWS + 1;
    let col = 1;
    assert!(apply_action_frame(&mut mux, Action::PanePrimaryPress { row, col }).is_none());

    let frame = apply_action_frame(
        &mut mux,
        Action::MouseRelease {
            row,
            col,
            button: 0,
        },
    );

    assert!(frame.is_none(), "plain click release should not repaint");
    assert!(mux.pending_selection.is_none());
    assert!(
        mux.selection.is_none(),
        "plain click must not leave selection"
    );
}

#[test]
fn apply_action_start_selection_sets_selection_state() {
    let mut mux = single_pane_tab_mux();
    let (session, _input_rx) = test_shell_session(20, 78);
    mux.sessions.insert(1, session);
    drop(compose_after(&mut mux, FullRedrawReason::FirstAttach));

    let frame = apply_action_frame(
        &mut mux,
        Action::StartSelection {
            row: STATUS_BAR_ROWS + 1,
            col: 1,
        },
    )
    .expect("selection start should repaint");

    let selection = mux.selection.expect("selection should be active");
    assert_eq!((selection.anchor_row, selection.anchor_col), (0, 0));
    assert!(
        !frame.is_empty(),
        "selection repaint frame should be emitted"
    );
    assert!(
        !frame_contains_screen_erase(&frame),
        "selection start must not clear the full screen"
    );
}

#[test]
fn apply_action_selection_motion_updates_selection() {
    let mut mux = single_pane_tab_mux();
    let (session, _input_rx) = test_shell_session(20, 78);
    mux.sessions.insert(1, session);
    drop(compose_after(&mut mux, FullRedrawReason::FirstAttach));
    let inner = Rect::new(STATUS_BAR_ROWS + 1, 1, 10, 20);
    mux.selection = Some(SelectionState {
        session_id: 1,
        inner,
        anchor_row: 0,
        anchor_col: 0,
        end_row: 0,
        end_col: 0,
    });

    let frame = apply_action_frame(
        &mut mux,
        Action::SelectionMotion {
            row: inner.row + 2,
            col: inner.col + 3,
        },
    )
    .expect("selection motion should redraw");

    let selection = mux.selection.expect("selection should remain active");
    assert_eq!((selection.end_row, selection.end_col), (2, 3));
    assert!(
        !frame.is_empty(),
        "selection repaint frame should be emitted"
    );
    assert!(
        !frame_contains_screen_erase(&frame),
        "selection motion must not clear the full screen"
    );
}

#[test]
fn selection_motion_above_pane_scrolls_into_history() {
    let mut mux = single_pane_tab_mux();
    let (mut session, _input_rx) = test_shell_session(20, 78);
    for i in 0..40 {
        session.feed_pty(format!("line {i}\r\n").as_bytes());
    }
    assert!(session.scrollback_filled() > 0);
    mux.sessions.insert(1, session);
    drop(compose_after(&mut mux, FullRedrawReason::FirstAttach));
    let inner = mux.visible_panes()[0].inner;
    mux.selection = Some(SelectionState {
        session_id: 1,
        inner,
        anchor_row: 5,
        anchor_col: 0,
        end_row: 5,
        end_col: 0,
    });

    let frame = apply_action_frame(
        &mut mux,
        Action::SelectionMotion {
            row: inner.row.saturating_sub(1),
            col: inner.col,
        },
    )
    .expect("selection auto-scroll should repaint");

    assert_eq!(
        mux.sessions.get(&1).unwrap().scrollback_offset(),
        1,
        "dragging above pane should move selection into retained history"
    );
    assert!(
        !frame_contains_screen_erase(&frame),
        "selection edge auto-scroll must not clear the full screen"
    );
    let selection = mux.selection.expect("selection should remain active");
    let session = mux.sessions.get(&1).unwrap();
    assert_eq!(
        selection.end_row,
        session
            .scrollback_filled()
            .saturating_sub(session.scrollback_offset()),
        "selection end should clamp to the top visible content row"
    );
}

#[test]
fn selection_motion_below_pane_scrolls_toward_live_tail() {
    let mut mux = single_pane_tab_mux();
    let (mut session, _input_rx) = test_shell_session(20, 78);
    for i in 0..40 {
        session.feed_pty(format!("line {i}\r\n").as_bytes());
    }
    session.scroll_by(4);
    assert_eq!(
        session.scrollback_offset(),
        4,
        "test setup should start away from the live tail"
    );
    mux.sessions.insert(1, session);
    drop(compose_after(&mut mux, FullRedrawReason::FirstAttach));
    let inner = mux.visible_panes()[0].inner;
    mux.selection = Some(SelectionState {
        session_id: 1,
        inner,
        anchor_row: 5,
        anchor_col: 0,
        end_row: 5,
        end_col: 0,
    });

    let frame = apply_action_frame(
        &mut mux,
        Action::SelectionMotion {
            row: inner.row.saturating_add(inner.rows),
            col: inner.col,
        },
    )
    .expect("selection auto-scroll should repaint");

    assert_eq!(
        mux.sessions.get(&1).unwrap().scrollback_offset(),
        3,
        "dragging below pane should move selection toward the live tail"
    );
    assert!(
        !frame_contains_screen_erase(&frame),
        "selection edge auto-scroll must not clear the full screen"
    );
    let selection = mux.selection.expect("selection should remain active");
    let session = mux.sessions.get(&1).unwrap();
    let prefix = session
        .scrollback_offset()
        .min(session.scrollback_filled())
        .min(inner.rows as usize);
    assert_eq!(
        selection.end_row,
        session
            .scrollback_filled()
            .saturating_add(inner.rows.saturating_sub(1) as usize)
            .saturating_sub(prefix),
        "selection end should clamp to the bottom visible content row"
    );
}

#[test]
fn apply_action_pane_button_motion_updates_selection() {
    let mut mux = single_pane_tab_mux();
    let (session, _input_rx) = test_shell_session(20, 78);
    mux.sessions.insert(1, session);
    drop(compose_after(&mut mux, FullRedrawReason::FirstAttach));
    let inner = Rect::new(STATUS_BAR_ROWS + 1, 1, 10, 20);
    mux.selection = Some(SelectionState {
        session_id: 1,
        inner,
        anchor_row: 0,
        anchor_col: 0,
        end_row: 0,
        end_col: 0,
    });

    let frame = apply_action_frame(
        &mut mux,
        Action::PaneButtonMotion {
            row: inner.row + 2,
            col: inner.col + 3,
        },
    )
    .expect("button motion should repaint active selection");

    let selection = mux.selection.expect("selection should remain active");
    assert_eq!((selection.end_row, selection.end_col), (2, 3));
    assert!(
        !frame.is_empty(),
        "selection repaint frame should be emitted"
    );
    assert!(
        !frame_contains_screen_erase(&frame),
        "selection button motion must not clear the full screen"
    );
}

#[test]
fn finalize_selection_keeps_highlight_and_shows_copied_toast() {
    let mut mux = single_pane_tab_mux();
    let (mut session, _input_rx) = test_shell_session(20, 78);
    session.feed_pty(b"copy this text");
    mux.sessions.insert(1, session);
    drop(compose_after(&mut mux, FullRedrawReason::FirstAttach));
    let inner = mux.visible_panes()[0].inner;
    mux.selection = Some(SelectionState {
        session_id: 1,
        inner,
        anchor_row: 0,
        anchor_col: 0,
        end_row: 0,
        end_col: 8,
    });
    let (tx, mut rx) = mpsc::unbounded_channel();
    mux.client.attach(tx);

    let frame = apply_action_frame(&mut mux, Action::FinalizeSelection)
        .expect("finalizing dragged selection should repaint");

    assert!(
        mux.selection.is_some(),
        "copied selection should remain visible"
    );
    assert!(mux.selection_copied, "copied toast state should be active");
    assert!(
        mux.selection_copy_feedback_deadline.is_some(),
        "selection copied toast should expire automatically"
    );
    mux.client.flush_out_of_band();
    let clipboard = rx.try_recv().expect("selection should write OSC 52");
    assert!(
        clipboard
            .windows(b"\x1b]52;c;".len())
            .any(|w| w == b"\x1b]52;c;"),
        "selection should copy through OSC 52: {clipboard:?}"
    );
    let rendered = String::from_utf8_lossy(&frame);
    assert!(
        !frame_contains_screen_erase(&frame),
        "finalizing selection must not clear the full screen"
    );
    assert!(
        rendered.contains("Selection copied"),
        "copied selection toast should render: {rendered:?}"
    );
    assert!(
        !rendered.contains("selection copied"),
        "copied selection feedback must not replace the action hint row: {rendered:?}"
    );
}

#[test]
fn selection_copy_feedback_expires_without_clearing_highlight() {
    let mut mux = single_pane_tab_mux();
    let (session, _input_rx) = test_shell_session(20, 78);
    mux.sessions.insert(1, session);
    drop(compose_after(&mut mux, FullRedrawReason::FirstAttach));
    let inner = mux.visible_panes()[0].inner;
    mux.selection = Some(SelectionState {
        session_id: 1,
        inner,
        anchor_row: 0,
        anchor_col: 0,
        end_row: 0,
        end_col: 8,
    });
    mux.selection_copied = true;
    let now = Instant::now();
    mux.selection_copy_feedback_deadline = Some(now);

    assert!(mux.expire_selection_copy_feedback(now));
    assert!(
        mux.selection.is_some(),
        "selection highlight should persist"
    );
    assert!(!mux.selection_copied, "toast should hide after deadline");
    assert!(mux.selection_copy_feedback_deadline.is_none());
}

#[test]
fn click_after_copied_selection_clears_highlight() {
    let mut mux = single_pane_tab_mux();
    let (session, _input_rx) = test_shell_session(20, 78);
    mux.sessions.insert(1, session);
    drop(compose_after(&mut mux, FullRedrawReason::FirstAttach));
    let inner = mux.visible_panes()[0].inner;
    mux.selection = Some(SelectionState {
        session_id: 1,
        inner,
        anchor_row: 0,
        anchor_col: 0,
        end_row: 0,
        end_col: 8,
    });
    mux.selection_copied = true;
    drop(compose_after(&mut mux, selection_change_redraw_reason()));

    let frame = apply_action_frame(
        &mut mux,
        Action::PanePrimaryPress {
            row: inner.row,
            col: inner.col,
        },
    )
    .expect("click should clear copied selection");

    assert!(mux.selection.is_none(), "click should clear selection");
    assert!(!mux.selection_copied, "click should clear copied toast");
    assert!(mux.selection_copy_feedback_deadline.is_none());
    assert!(
        !frame_contains_screen_erase(&frame),
        "click-clearing selection must not clear the full screen"
    );
    assert!(
        !String::from_utf8_lossy(&frame).contains("Selection copied"),
        "selection toast should disappear after click"
    );
}

#[test]
fn typed_input_after_copied_selection_clears_and_forwards() {
    let mut mux = single_pane_tab_mux();
    let (session, mut input_rx) = test_shell_session(20, 78);
    mux.sessions.insert(1, session);
    drop(compose_after(&mut mux, FullRedrawReason::FirstAttach));
    let inner = mux.visible_panes()[0].inner;
    mux.selection = Some(SelectionState {
        session_id: 1,
        inner,
        anchor_row: 0,
        anchor_col: 0,
        end_row: 0,
        end_col: 8,
    });
    mux.selection_copied = true;

    let frame = apply_action_frame(&mut mux, Action::PaneData(b"x".to_vec()))
        .expect("typing should clear copied selection and repaint");

    assert!(mux.selection.is_none(), "typing should clear selection");
    assert!(!mux.selection_copied, "typing should clear copied toast");
    assert!(mux.selection_copy_feedback_deadline.is_none());
    assert_eq!(input_rx.try_recv().unwrap(), b"x");
    assert!(
        !frame_contains_screen_erase(&frame),
        "typing-clearing selection must not clear the full screen"
    );
    assert!(
        !String::from_utf8_lossy(&frame).contains("Selection copied"),
        "selection toast should disappear after typing"
    );
}

#[test]
fn split_close_frame_repaints_in_place_without_screen_erase() {
    // Layout reflow must converge through Ratatui's diff without flashing the
    // screen blank (I4).
    let mut mux = single_pane_tab_mux_with_size(24, 80);
    drop(compose_after(&mut mux, FullRedrawReason::FirstAttach));

    let frame = compose_after(&mut mux, FullRedrawReason::SplitClose);
    assert!(
        !frame.windows(4).any(|w| w == b"\x1b[2J"),
        "SplitClose must repaint in place under the wipe policy (no 2J)"
    );
}

#[test]
fn double_click_selects_word_and_copies_once() {
    let mut mux = single_pane_tab_mux();
    let (mut session, _input_rx) = test_shell_session(20, 78);
    session.feed_pty(b"see /model to change");
    mux.sessions.insert(1, session);
    drop(compose_after(&mut mux, FullRedrawReason::FirstAttach));
    let (tx, mut rx) = mpsc::unbounded_channel();
    mux.client.attach(tx);
    let inner = mux.visible_panes()[0].inner;
    // Cell (0, 6) sits inside "/model" (content columns 4..=9).
    let row = inner.row;
    let col = inner.col + 6;

    drop(apply_action_frame(
        &mut mux,
        Action::PanePrimaryPress { row, col },
    ));
    assert!(
        mux.selection.is_none(),
        "first press must stay a plain click"
    );
    drop(apply_action_frame(
        &mut mux,
        Action::PanePrimaryPress { row, col },
    ));

    let sel = mux.selection.expect("double-click selects the word");
    assert_eq!(
        (sel.anchor_row, sel.anchor_col, sel.end_row, sel.end_col),
        (0, 4, 0, 9),
        "selection must cover exactly /model"
    );
    assert!(mux.selection_copied, "word selection copies immediately");
    mux.client.flush_out_of_band();
    let clipboard = rx.try_recv().expect("word selection writes OSC 52");
    let needle = crate::tui::view::encode_osc52_clipboard_write("/model");
    assert!(
        clipboard
            .windows(needle.len())
            .any(|w| w == needle.as_slice()),
        "clipboard write must carry the bare word: {:?}",
        String::from_utf8_lossy(&clipboard)
    );

    // The release that ends the double-click must not copy again or drop
    // the highlight.
    drop(apply_action_frame(
        &mut mux,
        Action::MouseRelease {
            row,
            col,
            button: 0,
        },
    ));
    assert!(
        mux.selection.is_some(),
        "word selection stays highlighted after release"
    );
    mux.client.flush_out_of_band();
    rx.try_recv().expect_err("release after a word click must not write the clipboard twice");
}

#[test]
fn double_click_window_requires_same_cell_within_500ms() {
    use std::time::{Duration, Instant};

    use super::mouse_input::{PanePress, is_double_click};

    let base = Instant::now();
    let press = |session_id, content_row, col, at| PanePress {
        session_id,
        content_row,
        col,
        at,
    };
    let first = press(1, 4, 7, base);
    let quick = press(1, 4, 7, base + Duration::from_millis(100));
    let slow = press(1, 4, 7, base + Duration::from_millis(900));
    let other_col = press(1, 4, 8, base + Duration::from_millis(100));
    let other_row = press(1, 5, 7, base + Duration::from_millis(100));
    let other_session = press(2, 4, 7, base + Duration::from_millis(100));

    assert!(is_double_click(&first, &quick));
    assert!(!is_double_click(&first, &slow), "outside the 500 ms window");
    assert!(!is_double_click(&first, &other_col));
    assert!(!is_double_click(&first, &other_row));
    assert!(!is_double_click(&first, &other_session));
}

/// Attach a client channel and drain everything queued so far, returning a
/// receiver that only sees what the test triggers next.
fn attach_drained_client(mux: &mut Multiplexer) -> mpsc::UnboundedReceiver<Vec<u8>> {
    let (tx, mut rx) = mpsc::unbounded_channel();
    mux.client.attach(tx);
    mux.client.flush_out_of_band();
    while rx.try_recv().is_ok() {}
    rx
}

fn osc52_payloads(rx: &mut mpsc::UnboundedReceiver<Vec<u8>>) -> Vec<Vec<u8>> {
    let mut found = Vec::new();
    while let Ok(bytes) = rx.try_recv() {
        let mut rest = bytes.as_slice();
        while let Some(start) = rest
            .windows(b"\x1b]52;c;".len())
            .position(|w| w == b"\x1b]52;c;")
        {
            let after = &rest[start + 7..];
            let end = after.iter().position(|&b| b == 0x07).unwrap_or(after.len());
            found.push(after[..end].to_vec());
            rest = &after[end..];
        }
    }
    found
}

fn expected_osc52_payload(text: &str) -> Vec<u8> {
    let encoded = crate::tui::view::encode_osc52_clipboard_write(text);
    // strip "\x1b]52;c;" prefix and trailing BEL
    encoded[7..encoded.len() - 1].to_vec()
}

/// Flush pending out-of-band bytes and assert the OSC 52 writes seen so far
/// carry exactly `expected`, in order.
fn assert_osc52_payloads(
    mux: &mut Multiplexer,
    rx: &mut mpsc::UnboundedReceiver<Vec<u8>>,
    expected: &[&str],
) {
    mux.client.flush_out_of_band();
    let payloads = osc52_payloads(rx);
    assert_eq!(payloads.len(), expected.len(), "OSC 52 write count");
    for (payload, text) in payloads.iter().zip(expected) {
        assert_eq!(payload, &expected_osc52_payload(text));
    }
}

#[tokio::test]
async fn open_host_url_dialog_action_sends_typed_protocol_frame() {
    let mut mux = single_pane_tab_mux();
    let (tx, mut rx) = mpsc::unbounded_channel();
    mux.client.attach(tx);

    mux.apply_dialog_action(DialogAction::OpenHostUrl(
        "https://github.com/jackin-project/jackin/pull/565".to_owned(),
    ));

    let bytes = rx.try_recv().expect("host-open-url frame");
    let tag = bytes[0];
    let mut payload = &bytes[1..];
    let frame = read_server_frame(&mut payload, tag)
        .await
        .expect("decode host-open-url frame")
        .expect("host-open-url frame");
    assert_eq!(
        frame,
        ServerFrame::HostOpenUrl("https://github.com/jackin-project/jackin/pull/565".to_owned())
    );
}

#[test]
fn open_host_url_dialog_action_honors_operator_opt_out() {
    let mut mux = single_pane_tab_mux();
    let (tx, mut rx) = mpsc::unbounded_channel();
    mux.client.attach(tx);

    mux.open_host_url_from_dialog(
        "https://github.com/jackin-project/jackin/pull/565".to_owned(),
        false,
    );

    rx.try_recv().expect_err("disabled host URL opening must not emit a host-open frame");
    assert_eq!(
        mux.clipboard_image_notice.as_deref(),
        Some("Host link opening disabled by JACKIN_OPEN_LINKS")
    );
}

#[test]
fn open_host_url_dialog_action_rejects_unsupported_scheme() {
    let mut mux = single_pane_tab_mux();
    let (tx, mut rx) = mpsc::unbounded_channel();
    mux.client.attach(tx);

    mux.open_host_url_from_dialog("file:///Users/operator/private.txt".to_owned(), true);

    rx.try_recv().expect_err("unsupported host URL schemes must not emit a host-open frame");
    assert_eq!(
        mux.clipboard_image_notice.as_deref(),
        Some("Host link rejected: unsupported URL scheme")
    );
}

#[test]
fn host_url_open_policy_honors_operator_opt_out_values() {
    assert!(mouse_input::host_url_opening_allowed_for(None));
    assert!(mouse_input::host_url_opening_allowed_for(Some("allow")));
    for value in ["deny", "off", "no"] {
        assert!(
            !mouse_input::host_url_opening_allowed_for(Some(value)),
            "{value} should disable host URL opening"
        );
    }
}

#[tokio::test]
async fn modified_click_visible_url_sends_typed_protocol_frame() {
    let mut mux = single_pane_tab_mux();
    let (tx, mut rx) = mpsc::unbounded_channel();
    mux.client.attach(tx);

    let (mut session, mut input_rx) = test_shell_session(20, 78);
    session.feed_pty(b"visit https://example.com/jackin-preflight-url now\r\n");
    mux.sessions.insert(1, session);
    drop(compose_after(&mut mux, FullRedrawReason::FirstAttach));

    let inner = mux.visible_panes()[0].inner;
    apply_action_frame(
        &mut mux,
        Action::OpenVisibleUrlAt {
            row: inner.row,
            col: inner.col + "visit https://exa".len() as u16,
            button: 8,
        },
    );

    input_rx.try_recv().expect_err("host-open URL gesture should not forward mouse bytes to a mouse-disabled pane");
    let bytes = rx.try_recv().expect("host-open-url frame");
    let tag = bytes[0];
    let mut payload = &bytes[1..];
    let frame = read_server_frame(&mut payload, tag)
        .await
        .expect("decode host-open-url frame")
        .expect("host-open-url frame");
    assert_eq!(
        frame,
        ServerFrame::HostOpenUrl("https://example.com/jackin-preflight-url".to_owned())
    );
}

#[tokio::test]
async fn modified_click_in_mouse_enabled_pane_forwards_to_pty() {
    let mut mux = single_pane_tab_mux();
    let (tx, mut rx) = mpsc::unbounded_channel();
    mux.client.attach(tx);

    let (mut session, mut input_rx) = test_shell_session(20, 78);
    session.feed_pty(b"\x1b[?1000h\x1b[?1006hvisit https://example.com/rich-tui now\r\n");
    mux.sessions.insert(1, session);
    drop(compose_after(&mut mux, FullRedrawReason::FirstAttach));

    let inner = mux.visible_panes()[0].inner;
    apply_action_frame(
        &mut mux,
        Action::OpenVisibleUrlAt {
            row: inner.row,
            col: inner.col + "visit https://exa".len() as u16,
            button: 8,
        },
    );

    let forwarded = input_rx
        .try_recv()
        .expect("modified click should forward to mouse-enabled pane");
    assert!(
        forwarded.starts_with(b"\x1b[<8;"),
        "unexpected forwarded mouse bytes: {forwarded:02x?}"
    );
    rx.try_recv().expect_err("mouse-enabled pane should not emit a host-open-url frame");
}

#[tokio::test]
async fn modified_click_visible_file_path_sends_file_export_frames() {
    let temp = tempfile::tempdir().unwrap();
    let workdir = temp.path().join("workspace");
    std::fs::create_dir(&workdir).unwrap();
    std::fs::write(workdir.join("report.txt"), b"hello export").unwrap();

    let mut mux = single_pane_tab_mux();
    mux.workdir = workdir.clone();
    let (tx, mut rx) = mpsc::unbounded_channel();
    mux.client.attach(tx);
    mux.client.flush_out_of_band();
    while rx.try_recv().is_ok() {}

    let (mut session, mut input_rx) = test_shell_session(20, 78);
    session.feed_pty(b"artifact report.txt ready\r\n");
    mux.sessions.insert(1, session);
    drop(compose_after(&mut mux, FullRedrawReason::FirstAttach));

    let inner = mux.visible_panes()[0].inner;
    apply_action_frame(
        &mut mux,
        Action::OpenVisibleUrlAt {
            row: inner.row,
            col: inner.col + "artifact report".len() as u16,
            button: 8,
        },
    );
    mux.client.flush_out_of_band();

    input_rx.try_recv().expect_err("modified file export must not forward mouse bytes to the pane");
    let bytes = rx.try_recv().expect("file-export-start frame");
    let tag = bytes[0];
    let mut payload = &bytes[1..];
    let frame = read_server_frame(&mut payload, tag)
        .await
        .expect("decode file-export-start frame")
        .expect("file-export-start frame");
    let ServerFrame::FileExportStart(start) = frame else {
        panic!("expected FileExportStart");
    };
    assert_eq!(
        start.source_path,
        workdir
            .join("report.txt")
            .canonicalize()
            .unwrap()
            .display()
            .to_string()
    );
    assert_eq!(start.file_name, "report.txt");
    assert!(!start.reveal_after_export);
    assert!(!start.open_after_export);
}

#[test]
fn modified_click_plain_word_without_file_falls_through_quietly() {
    let mut mux = single_pane_tab_mux();
    let (tx, mut rx) = mpsc::unbounded_channel();
    mux.client.attach(tx);

    let (mut session, _input_rx) = test_shell_session(20, 78);
    session.feed_pty(b"plain words only\r\n");
    mux.sessions.insert(1, session);
    drop(compose_after(&mut mux, FullRedrawReason::FirstAttach));

    let inner = mux.visible_panes()[0].inner;
    apply_action_frame(
        &mut mux,
        Action::OpenVisibleUrlAt {
            row: inner.row,
            col: inner.col + "plain wo".len() as u16,
            button: 8,
        },
    );
    mux.client.flush_out_of_band();

    rx.try_recv().expect_err("plain modified-click must not emit host frames");
    assert_eq!(mux.clipboard_image_notice.as_deref(), None);
}

#[tokio::test]
async fn modified_click_prefers_osc8_target_over_visible_text() {
    let mut mux = single_pane_tab_mux();
    let (tx, mut rx) = mpsc::unbounded_channel();
    mux.client.attach(tx);

    let (mut session, mut input_rx) = test_shell_session(20, 78);
    session.feed_pty(
        b"\x1b]8;id=link;https://example.com/osc8\x07osc8_link\x1b]8;;\x07 and https://example.com/visible",
    );
    mux.sessions.insert(1, session);
    drop(compose_after(&mut mux, FullRedrawReason::FirstAttach));

    let inner = mux.visible_panes()[0].inner;
    apply_action_frame(
        &mut mux,
        Action::OpenVisibleUrlAt {
            row: inner.row,
            col: inner.col + 1,
            button: 8,
        },
    );

    input_rx.try_recv().expect_err("modified-click should stay host-open path");
    let bytes = rx
        .try_recv()
        .expect("host-open-url frame should prefer OSC 8 target");
    let tag = bytes[0];
    let mut payload = &bytes[1..];
    let frame = read_server_frame(&mut payload, tag)
        .await
        .expect("decode host-open-url frame")
        .expect("host-open-url frame");
    assert_eq!(
        frame,
        ServerFrame::HostOpenUrl("https://example.com/osc8".to_owned())
    );
}

#[tokio::test]
async fn modified_click_accepts_mailto_osc8_target() {
    let mut mux = single_pane_tab_mux();
    let (tx, mut rx) = mpsc::unbounded_channel();
    mux.client.attach(tx);

    let (mut session, mut input_rx) = test_shell_session(20, 78);
    session.feed_pty(
        b"\x1b]8;id=mail;mailto:operator@example.com\x07email\x1b]8;;\x07 and https://example.com/visible",
    );
    mux.sessions.insert(1, session);
    drop(compose_after(&mut mux, FullRedrawReason::FirstAttach));

    let inner = mux.visible_panes()[0].inner;
    apply_action_frame(
        &mut mux,
        Action::OpenVisibleUrlAt {
            row: inner.row,
            col: inner.col + 1,
            button: 8,
        },
    );

    input_rx.try_recv().expect_err("modified-click should stay host-open path");
    let bytes = rx
        .try_recv()
        .expect("host-open-url frame should allow mailto OSC 8 target");
    let tag = bytes[0];
    let mut payload = &bytes[1..];
    let frame = read_server_frame(&mut payload, tag)
        .await
        .expect("decode host-open-url frame")
        .expect("host-open-url frame");
    assert_eq!(
        frame,
        ServerFrame::HostOpenUrl("mailto:operator@example.com".to_owned())
    );
}

#[tokio::test]
async fn modified_click_accepts_visible_mailto_token() {
    let mut mux = single_pane_tab_mux();
    let (tx, mut rx) = mpsc::unbounded_channel();
    mux.client.attach(tx);

    let (mut session, mut input_rx) = test_shell_session(20, 78);
    session.feed_pty(b"contact mailto:operator@example.com now\r\n");
    mux.sessions.insert(1, session);
    drop(compose_after(&mut mux, FullRedrawReason::FirstAttach));

    let inner = mux.visible_panes()[0].inner;
    apply_action_frame(
        &mut mux,
        Action::OpenVisibleUrlAt {
            row: inner.row,
            col: inner.col + "contact mailto:opera".len() as u16,
            button: 8,
        },
    );

    input_rx.try_recv().expect_err("modified-click should stay host-open path");
    let bytes = rx
        .try_recv()
        .expect("host-open-url frame should allow visible mailto target");
    let tag = bytes[0];
    let mut payload = &bytes[1..];
    let frame = read_server_frame(&mut payload, tag)
        .await
        .expect("decode host-open-url frame")
        .expect("host-open-url frame");
    assert_eq!(
        frame,
        ServerFrame::HostOpenUrl("mailto:operator@example.com".to_owned())
    );
}

#[tokio::test]
async fn modified_click_rejects_unsafe_visible_url_without_forwarding() {
    let mut mux = single_pane_tab_mux();
    let (tx, mut rx) = mpsc::unbounded_channel();
    mux.client.attach(tx);

    let (mut session, mut input_rx) = test_shell_session(20, 78);
    session.feed_pty(b"local file:///tmp/report.html now\r\n");
    mux.sessions.insert(1, session);
    drop(compose_after(&mut mux, FullRedrawReason::FirstAttach));

    let inner = mux.visible_panes()[0].inner;
    apply_action_frame(
        &mut mux,
        Action::OpenVisibleUrlAt {
            row: inner.row,
            col: inner.col + "local file:///tmp/re".len() as u16,
            button: 8,
        },
    );

    input_rx.try_recv().expect_err("unsafe host-open gesture should not forward mouse bytes");
    rx.try_recv().expect_err("unsafe host-open gesture should not emit a host-open frame");
    assert_eq!(
        mux.clipboard_image_notice.as_deref(),
        Some("Host link rejected: unsupported URL scheme")
    );
}

#[tokio::test]
async fn modified_click_rejects_unsafe_osc8_url_without_forwarding() {
    let mut mux = single_pane_tab_mux();
    let (tx, mut rx) = mpsc::unbounded_channel();
    mux.client.attach(tx);

    let (mut session, mut input_rx) = test_shell_session(20, 78);
    session.feed_pty(
        b"\x1b]8;id=file;file:///tmp/report.html\x07local_file\x1b]8;;\x07 and https://example.com/visible",
    );
    mux.sessions.insert(1, session);
    drop(compose_after(&mut mux, FullRedrawReason::FirstAttach));

    let inner = mux.visible_panes()[0].inner;
    apply_action_frame(
        &mut mux,
        Action::OpenVisibleUrlAt {
            row: inner.row,
            col: inner.col + 1,
            button: 8,
        },
    );

    input_rx.try_recv().expect_err("unsafe OSC8 host-open gesture should not forward mouse bytes");
    rx.try_recv().expect_err("unsafe OSC8 host-open gesture should not emit a host-open frame");
    assert_eq!(
        mux.clipboard_image_notice.as_deref(),
        Some("Host link rejected: unsupported URL scheme")
    );
}

#[tokio::test]
async fn open_link_under_cursor_palette_action_sends_typed_protocol_frame() {
    let mut mux = single_pane_tab_mux();
    let (tx, mut rx) = mpsc::unbounded_channel();
    mux.client.attach(tx);

    let (mut session, mut input_rx) = test_shell_session(20, 78);
    session.feed_pty(b"\x1b[1;1Hvisit https://example.com/jackin-preflight-url now\x1b[1;15H");
    mux.sessions.insert(1, session);
    drop(compose_after(&mut mux, FullRedrawReason::FirstAttach));

    mux.handle_palette_command(PaletteCommand::OpenLinkUnderCursor);

    input_rx.try_recv().expect_err("open-link command must not forward bytes to the pane");
    let bytes = rx.try_recv().expect("host-open-url frame");
    let tag = bytes[0];
    let mut payload = &bytes[1..];
    let frame = read_server_frame(&mut payload, tag)
        .await
        .expect("decode host-open-url frame")
        .expect("host-open-url frame");
    assert_eq!(
        frame,
        ServerFrame::HostOpenUrl("https://example.com/jackin-preflight-url".to_owned())
    );
}

#[test]
fn modified_url_hover_renders_visible_target_without_forwarding_to_pty() {
    let mut mux = single_pane_tab_mux();
    let (mut session, mut input_rx) = test_shell_session(20, 78);
    session.feed_pty(b"\x1b[1;1Hvisit https://example.com/visible now");
    mux.sessions.insert(1, session);
    drop(compose_after(&mut mux, FullRedrawReason::FirstAttach));

    let inner = mux.visible_panes()[0].inner;
    let frame = handle_input_frame(
        &mut mux,
        InputEvent::MousePress {
            row: inner.row,
            col: inner.col + 7,
            // SGR passive motion + Alt. Ghostty reported this shape during
            // Phase 0 preflight for Option-hover in a mouse-disabled pane.
            button: 43,
        },
    )
    .expect("hovering a link should repaint the notice");

    input_rx.try_recv().expect_err("modified hover must not write bytes into a mouse-disabled pane");
    assert_eq!(
        mux.link_hover_url.as_deref(),
        Some("https://example.com/visible")
    );
    let frame = String::from_utf8_lossy(&frame);
    assert!(
        frame.contains("Open link: https://example.com/visible"),
        "hover notice missing from frame: {frame:?}"
    );
}

#[test]
fn modified_url_hover_prefers_osc8_target() {
    let mut mux = single_pane_tab_mux();
    let (mut session, _input_rx) = test_shell_session(20, 78);
    session.feed_pty(
        b"\x1b]8;id=link;https://example.com/osc8\x07https://example.com/visible\x1b]8;;\x07",
    );
    mux.sessions.insert(1, session);
    drop(compose_after(&mut mux, FullRedrawReason::FirstAttach));

    let inner = mux.visible_panes()[0].inner;
    drop(handle_input_frame(
        &mut mux,
        InputEvent::MousePress {
            row: inner.row,
            col: inner.col + 1,
            button: 51,
        },
    ));

    assert_eq!(
        mux.link_hover_url.as_deref(),
        Some("https://example.com/osc8")
    );
}

#[test]
fn unmodified_url_hover_clears_existing_link_notice() {
    let mut mux = single_pane_tab_mux();
    let (mut session, _input_rx) = test_shell_session(20, 78);
    session.feed_pty(b"\x1b[1;1Hvisit https://example.com/visible now");
    mux.sessions.insert(1, session);
    mux.link_hover_url = Some("https://example.com/visible".to_owned());
    drop(compose_after(&mut mux, FullRedrawReason::FirstAttach));

    let inner = mux.visible_panes()[0].inner;
    drop(handle_input_frame(
        &mut mux,
        InputEvent::MousePress {
            row: inner.row,
            col: inner.col + 7,
            button: SGR_NO_BUTTON_MOTION,
        },
    ));

    assert_eq!(mux.link_hover_url, None);
}

#[tokio::test]
async fn open_link_under_cursor_palette_prefers_osc8_target_over_visible_text() {
    let mut mux = single_pane_tab_mux();
    let (tx, mut rx) = mpsc::unbounded_channel();
    mux.client.attach(tx);

    let (mut session, mut input_rx) = test_shell_session(20, 78);
    session.feed_pty(
        b"\x1b]8;id=link;https://example.com/osc8\x07https://example.com/visible\x1b]8;;\x07\x1b[1;2H",
    );
    mux.sessions.insert(1, session);
    drop(compose_after(&mut mux, FullRedrawReason::FirstAttach));

    mux.handle_palette_command(PaletteCommand::OpenLinkUnderCursor);

    input_rx.try_recv().expect_err("open-link must stay attach path");
    let bytes = rx
        .try_recv()
        .expect("host-open-url frame should prefer OSC 8 target");
    let tag = bytes[0];
    let mut payload = &bytes[1..];
    let frame = read_server_frame(&mut payload, tag)
        .await
        .expect("decode host-open-url frame")
        .expect("host-open-url frame");
    assert_eq!(
        frame,
        ServerFrame::HostOpenUrl("https://example.com/osc8".to_owned())
    );
}

#[tokio::test]
async fn open_link_under_cursor_palette_rejects_unsafe_visible_url() {
    let mut mux = single_pane_tab_mux();
    let (tx, mut rx) = mpsc::unbounded_channel();
    mux.client.attach(tx);

    let (mut session, mut input_rx) = test_shell_session(20, 78);
    session.feed_pty(b"\x1b[1;1Hopen file:///tmp/report.html now\x1b[1;12H");
    mux.sessions.insert(1, session);
    drop(compose_after(&mut mux, FullRedrawReason::FirstAttach));

    mux.handle_palette_command(PaletteCommand::OpenLinkUnderCursor);

    input_rx.try_recv().expect_err("unsafe open-link command must not forward bytes to the pane");
    rx.try_recv().expect_err("unsafe open-link command must not emit a host-open frame");
    assert_eq!(
        mux.clipboard_image_notice.as_deref(),
        Some("Host link rejected: unsupported URL scheme")
    );
}

#[tokio::test]
async fn open_link_under_cursor_palette_action_reports_missing_url() {
    let mut mux = single_pane_tab_mux();
    let (tx, mut rx) = mpsc::unbounded_channel();
    mux.client.attach(tx);

    let (mut session, _input_rx) = test_shell_session(20, 78);
    session.feed_pty(b"\x1b[1;1Hplain text only\x1b[1;3H");
    mux.sessions.insert(1, session);
    drop(compose_after(&mut mux, FullRedrawReason::FirstAttach));

    mux.handle_palette_command(PaletteCommand::OpenLinkUnderCursor);

    rx.try_recv().expect_err("missing URL must not emit a host-open frame");
    assert_eq!(
        mux.clipboard_image_notice.as_deref(),
        Some("No host-open link under focused cursor")
    );
}

#[tokio::test]
async fn export_file_under_cursor_palette_action_sends_file_export_frames() {
    let temp = tempfile::tempdir().unwrap();
    let workdir = temp.path().join("workspace");
    std::fs::create_dir(&workdir).unwrap();
    std::fs::write(workdir.join("report.txt"), b"hello export").unwrap();

    let mut mux = single_pane_tab_mux();
    mux.workdir = workdir.clone();
    let (tx, mut rx) = mpsc::unbounded_channel();
    mux.client.attach(tx);
    mux.client.flush_out_of_band();
    while rx.try_recv().is_ok() {}

    let (mut session, mut input_rx) = test_shell_session(20, 78);
    session.feed_pty(b"\x1b[1;1Hsee report.txt now\x1b[1;7H");
    mux.sessions.insert(1, session);
    drop(compose_after(&mut mux, FullRedrawReason::FirstAttach));

    mux.handle_palette_command(PaletteCommand::ExportFileUnderCursorAndReveal);
    mux.client.flush_out_of_band();

    input_rx.try_recv().expect_err("export-under-cursor command must not forward bytes to the pane");
    let bytes = rx.try_recv().expect("file-export-start frame");
    let tag = bytes[0];
    let mut payload = &bytes[1..];
    let frame = read_server_frame(&mut payload, tag)
        .await
        .expect("decode file-export-start frame")
        .expect("file-export-start frame");
    let ServerFrame::FileExportStart(start) = frame else {
        panic!("expected FileExportStart");
    };
    assert_eq!(
        start.source_path,
        workdir
            .join("report.txt")
            .canonicalize()
            .unwrap()
            .display()
            .to_string()
    );
    assert_eq!(start.file_name, "report.txt");
    assert_eq!(start.size, "hello export".len() as u64);
    assert!(start.reveal_after_export);
    assert!(!start.open_after_export);
    assert!(
        mux.clipboard_image_notice
            .as_deref()
            .is_some_and(|notice| notice.contains("File export and reveal queued: report.txt"))
    );
}

#[tokio::test]
async fn export_selected_file_palette_action_sends_file_export_frames() {
    let temp = tempfile::tempdir().unwrap();
    let workdir = temp.path().join("workspace");
    std::fs::create_dir(&workdir).unwrap();
    std::fs::write(workdir.join("report.txt"), b"hello export").unwrap();

    let mut mux = single_pane_tab_mux();
    mux.workdir = workdir.clone();
    let (tx, mut rx) = mpsc::unbounded_channel();
    mux.client.attach(tx);
    mux.client.flush_out_of_band();
    while rx.try_recv().is_ok() {}

    let (mut session, mut input_rx) = test_shell_session(20, 78);
    session.feed_pty(b"\x1b[1;1Hsee report.txt now\x1b[1;1H");
    mux.sessions.insert(1, session);
    drop(compose_after(&mut mux, FullRedrawReason::FirstAttach));
    let inner = mux.visible_panes()[0].inner;
    mux.selection = Some(SelectionState {
        session_id: 1,
        inner,
        anchor_row: 0,
        anchor_col: 4,
        end_row: 0,
        end_col: 13,
    });

    mux.handle_palette_command(PaletteCommand::ExportSelectedFileAndOpen);
    mux.client.flush_out_of_band();

    input_rx.try_recv().expect_err("export-selected command must not forward bytes to the pane");
    let bytes = rx.try_recv().expect("file-export-start frame");
    let tag = bytes[0];
    let mut payload = &bytes[1..];
    let frame = read_server_frame(&mut payload, tag)
        .await
        .expect("decode file-export-start frame")
        .expect("file-export-start frame");
    let ServerFrame::FileExportStart(start) = frame else {
        panic!("expected FileExportStart");
    };
    assert_eq!(
        start.source_path,
        workdir
            .join("report.txt")
            .canonicalize()
            .unwrap()
            .display()
            .to_string()
    );
    assert_eq!(start.file_name, "report.txt");
    assert!(!start.reveal_after_export);
    assert!(start.open_after_export);
    assert!(
        mux.clipboard_image_notice
            .as_deref()
            .is_some_and(|notice| notice.contains("File export and open queued: report.txt"))
    );
}

#[test]
fn export_file_under_cursor_palette_action_reports_missing_path_token() {
    let mut mux = single_pane_tab_mux();
    let (tx, mut rx) = mpsc::unbounded_channel();
    mux.client.attach(tx);

    let (mut session, _input_rx) = test_shell_session(20, 78);
    session.feed_pty(b"\x1b[1;1H    \x1b[1;2H");
    mux.sessions.insert(1, session);
    drop(compose_after(&mut mux, FullRedrawReason::FirstAttach));

    mux.handle_palette_command(PaletteCommand::ExportFileUnderCursor);
    mux.client.flush_out_of_band();

    rx.try_recv().expect_err("missing path token must not emit file-export frames");
    assert_eq!(
        mux.clipboard_image_notice.as_deref(),
        Some("No exportable file path under focused cursor")
    );
}

#[test]
fn export_selected_file_palette_action_reports_missing_selection() {
    let mut mux = single_pane_tab_mux();
    let (tx, mut rx) = mpsc::unbounded_channel();
    mux.client.attach(tx);

    mux.handle_palette_command(PaletteCommand::ExportSelectedFile);
    mux.client.flush_out_of_band();

    rx.try_recv().expect_err("missing selection must not emit file-export frames");
    assert_eq!(
        mux.clipboard_image_notice.as_deref(),
        Some("No selected file path to export")
    );
}

#[tokio::test]
async fn stage_image_path_palette_action_sends_typed_protocol_frame() {
    let mut mux = single_pane_tab_mux();
    let (tx, mut rx) = mpsc::unbounded_channel();
    mux.client.attach(tx);
    mux.clipboard_image_insert_mode = ClipboardImageInsertMode::StageOnly;

    mux.handle_palette_command(PaletteCommand::StageImageFromClipboardPath);

    let bytes = rx.try_recv().expect("host-stage-image-path frame");
    let tag = bytes[0];
    let mut payload = &bytes[1..];
    let frame = read_server_frame(&mut payload, tag)
        .await
        .expect("decode host-stage-image-path frame")
        .expect("host-stage-image-path frame");
    assert_eq!(frame, ServerFrame::HostStageImageFromClipboardPath);
    assert_eq!(
        mux.clipboard_image_insert_mode,
        ClipboardImageInsertMode::PastePath
    );
}

#[tokio::test]
async fn paste_image_palette_action_sends_typed_protocol_frame() {
    let mut mux = single_pane_tab_mux();
    let (tx, mut rx) = mpsc::unbounded_channel();
    mux.client.attach(tx);
    mux.clipboard_image_insert_mode = ClipboardImageInsertMode::StageOnly;

    mux.handle_palette_command(PaletteCommand::PasteImageFromClipboard);

    let bytes = rx.try_recv().expect("host-paste-image frame");
    let tag = bytes[0];
    let mut payload = &bytes[1..];
    let frame = read_server_frame(&mut payload, tag)
        .await
        .expect("decode host-paste-image frame")
        .expect("host-paste-image frame");
    assert_eq!(frame, ServerFrame::HostPasteImageFromClipboard);
    assert_eq!(
        mux.clipboard_image_insert_mode,
        ClipboardImageInsertMode::PastePath
    );
}

#[tokio::test]
async fn stage_image_palette_action_sends_typed_protocol_frame() {
    let mut mux = single_pane_tab_mux();
    let (tx, mut rx) = mpsc::unbounded_channel();
    mux.client.attach(tx);

    mux.handle_palette_command(PaletteCommand::StageImageFromClipboard);

    let bytes = rx.try_recv().expect("host-stage-image frame");
    let tag = bytes[0];
    let mut payload = &bytes[1..];
    let frame = read_server_frame(&mut payload, tag)
        .await
        .expect("decode host-stage-image frame")
        .expect("host-stage-image frame");
    assert_eq!(frame, ServerFrame::HostStageImageFromClipboard);
    assert_eq!(
        mux.clipboard_image_insert_mode,
        ClipboardImageInsertMode::StageOnly
    );
}

#[tokio::test]
async fn chunked_image_start_reports_visible_receiving_notice() {
    let mut mux = single_pane_tab_mux();

    handle_client_frame(
        &mut mux,
        ClientFrame::ClipboardImageStart(jackin_protocol::attach::ClipboardImageStart {
            transfer_id: 42,
            format: jackin_protocol::attach::ClipboardImageFormat::Png,
            size: 16 * 1024 * 1024,
        }),
    )
    .await;

    assert_eq!(
        mux.clipboard_image_notice.as_deref(),
        Some("Image paste: receiving 16777216 bytes")
    );
    assert_eq!(
        mux.clipboard_image_insert_mode,
        ClipboardImageInsertMode::PastePath
    );
}

#[tokio::test]
async fn chunked_stage_image_start_reports_staging_receiving_notice() {
    let mut mux = single_pane_tab_mux();
    mux.clipboard_image_insert_mode = ClipboardImageInsertMode::StageOnly;

    handle_client_frame(
        &mut mux,
        ClientFrame::ClipboardImageStart(jackin_protocol::attach::ClipboardImageStart {
            transfer_id: 43,
            format: jackin_protocol::attach::ClipboardImageFormat::Png,
            size: 4 * 1024 * 1024,
        }),
    )
    .await;

    assert_eq!(
        mux.clipboard_image_notice.as_deref(),
        Some("Image staging: receiving 4194304 bytes")
    );
    assert_eq!(
        mux.clipboard_image_insert_mode,
        ClipboardImageInsertMode::StageOnly
    );
}

#[test]
fn stage_only_clipboard_image_response_does_not_paste_path() {
    let mut mux = single_pane_tab_mux();
    let (session, mut input_rx) = test_shell_session(20, 78);
    mux.sessions.insert(1, session);
    mux.clipboard_image_insert_mode = ClipboardImageInsertMode::StageOnly;

    mux.stage_clipboard_image_response_with(
        jackin_protocol::attach::ClipboardImage {
            format: jackin_protocol::attach::ClipboardImageFormat::Png,
            bytes: b"\x89PNG\r\n\x1a\n".to_vec(),
        },
        |_| Ok(PathBuf::from("/jackin/run/clipboard/clipboard-test.png")),
    );

    input_rx.try_recv().expect_err("stage-only response must not paste into the focused pane");
    assert_eq!(
        mux.clipboard_image_notice.as_deref(),
        Some("Image staged: /jackin/run/clipboard/clipboard-test.png (8 bytes)")
    );
    assert_eq!(
        mux.clipboard_image_insert_mode,
        ClipboardImageInsertMode::PastePath
    );
}

#[test]
fn clipboard_image_response_reports_when_path_cannot_be_pasted() {
    let mut mux = single_pane_tab_mux();

    mux.stage_clipboard_image_response_with(
        jackin_protocol::attach::ClipboardImage {
            format: jackin_protocol::attach::ClipboardImageFormat::Png,
            bytes: b"\x89PNG\r\n\x1a\n".to_vec(),
        },
        |_| Ok(PathBuf::from("/jackin/run/clipboard/clipboard-test.png")),
    );

    assert_eq!(
        mux.clipboard_image_notice.as_deref(),
        Some(
            "Image staged: /jackin/run/clipboard/clipboard-test.png (8 bytes; no writable focused pane; not pasted)"
        )
    );
    assert_eq!(
        mux.clipboard_image_insert_mode,
        ClipboardImageInsertMode::PastePath
    );
}

#[test]
fn drag_extending_a_word_click_recopies_on_release() {
    let mut mux = single_pane_tab_mux();
    let (mut session, _input_rx) = test_shell_session(20, 78);
    session.feed_pty(b"see /model to change");
    mux.sessions.insert(1, session);
    drop(compose_after(&mut mux, FullRedrawReason::FirstAttach));
    let inner = mux.visible_panes()[0].inner;
    let row = inner.row;
    let col = inner.col + 6; // inside "/model"
    let mut rx = attach_drained_client(&mut mux);

    drop(apply_action_frame(
        &mut mux,
        Action::PanePrimaryPress { row, col },
    ));
    drop(apply_action_frame(
        &mut mux,
        Action::PanePrimaryPress { row, col },
    ));
    assert!(mux.selection_copied, "word click copies immediately");

    // Extend the selection past the word, then release: the clipboard no
    // longer matches the highlight, so release must copy again.
    drop(apply_action_frame(
        &mut mux,
        Action::PaneButtonMotion {
            row,
            col: inner.col + 13,
        },
    ));
    assert!(
        !mux.selection_copied,
        "motion must invalidate the word-click copy"
    );
    drop(apply_action_frame(
        &mut mux,
        Action::MouseRelease {
            row,
            col: inner.col + 13,
            button: 0,
        },
    ));
    assert!(mux.selection_copied, "release re-copies the extended span");

    assert_osc52_payloads(&mut mux, &mut rx, &["/model", "/model to"]);
}

#[test]
fn double_click_on_scrolled_back_row_copies_the_history_word() {
    let mut mux = single_pane_tab_mux();
    let (mut session, _input_rx) = test_shell_session(20, 78);
    for i in 0..40 {
        session.feed_pty(format!("w{i:02}\r\n").as_bytes());
    }
    let filled = session.scrollback_filled();
    assert!(filled > 5, "history must exist for the scrolled press");
    session.scroll_by(5);
    mux.sessions.insert(1, session);
    drop(compose_after(&mut mux, FullRedrawReason::FirstAttach));
    let inner = mux.visible_panes()[0].inner;
    let row = inner.row; // top visible row = scrollback row filled-5
    let col = inner.col + 1;
    let mut rx = attach_drained_client(&mut mux);

    drop(apply_action_frame(
        &mut mux,
        Action::PanePrimaryPress { row, col },
    ));
    drop(apply_action_frame(
        &mut mux,
        Action::PanePrimaryPress { row, col },
    ));

    let sel = mux.selection.expect("double-click on history selects");
    assert_eq!(
        sel.anchor_row,
        filled - 5,
        "anchor must be the scrolled-to content row"
    );
    let expected = format!("w{:02}", filled - 5);
    assert_osc52_payloads(&mut mux, &mut rx, &[expected.as_str()]);
    assert_eq!(
        mux.sessions.get(&1).expect("session").scrollback_offset(),
        5,
        "word selection must not move the scrollback view"
    );
}

#[test]
fn triple_click_clears_then_two_more_presses_reselect() {
    let mut mux = single_pane_tab_mux();
    let (mut session, _input_rx) = test_shell_session(20, 78);
    session.feed_pty(b"see /model to change");
    mux.sessions.insert(1, session);
    drop(compose_after(&mut mux, FullRedrawReason::FirstAttach));
    let inner = mux.visible_panes()[0].inner;
    let row = inner.row;
    let col = inner.col + 6;
    let mut rx = attach_drained_client(&mut mux);

    drop(apply_action_frame(
        &mut mux,
        Action::PanePrimaryPress { row, col },
    ));
    drop(apply_action_frame(
        &mut mux,
        Action::PanePrimaryPress { row, col },
    ));
    assert!(mux.selection.is_some(), "second press selects the word");

    // Third quick press clears the highlight (and stamps a fresh cycle).
    drop(apply_action_frame(
        &mut mux,
        Action::PanePrimaryPress { row, col },
    ));
    assert!(mux.selection.is_none(), "third press clears");

    // Fourth quick press completes a new double-click on the same word.
    drop(apply_action_frame(
        &mut mux,
        Action::PanePrimaryPress { row, col },
    ));
    assert!(mux.selection.is_some(), "fourth press re-selects");

    assert_osc52_payloads(&mut mux, &mut rx, &["/model", "/model"]);
}

#[test]
fn double_click_on_a_second_word_needs_only_two_presses() {
    let mut mux = single_pane_tab_mux();
    let (mut session, _input_rx) = test_shell_session(20, 78);
    session.feed_pty(b"alpha beta");
    mux.sessions.insert(1, session);
    drop(compose_after(&mut mux, FullRedrawReason::FirstAttach));
    let inner = mux.visible_panes()[0].inner;
    let row = inner.row;
    let col_a = inner.col + 1; // inside "alpha"
    let col_b = inner.col + 7; // inside "beta"
    let mut rx = attach_drained_client(&mut mux);

    drop(apply_action_frame(
        &mut mux,
        Action::PanePrimaryPress { row, col: col_a },
    ));
    drop(apply_action_frame(
        &mut mux,
        Action::PanePrimaryPress { row, col: col_a },
    ));
    drop(apply_action_frame(
        &mut mux,
        Action::PanePrimaryPress { row, col: col_b },
    ));
    drop(apply_action_frame(
        &mut mux,
        Action::PanePrimaryPress { row, col: col_b },
    ));

    assert!(mux.selection.is_some(), "second word selected");
    assert_osc52_payloads(&mut mux, &mut rx, &["alpha", "beta"]);
}

#[test]
fn session_terminal_carries_the_attached_client_palette() {
    let mut mux = single_pane_tab_mux();
    mux.attached_terminal.default_fg = Some((1, 2, 3));
    mux.attached_terminal.default_bg = Some((4, 5, 6));
    let terminal = mux.session_terminal(10, 20);
    assert_eq!(terminal.rows, 10);
    assert_eq!(terminal.cols, 20);
    assert_eq!(terminal.default_fg, Some((1, 2, 3)));
    assert_eq!(terminal.default_bg, Some((4, 5, 6)));
}

#[test]
fn reattach_updates_capabilities_without_resetting_model_palette() {
    let mut mux = single_pane_tab_mux();
    let (session, mut rx) = test_session(20, 78);
    mux.sessions.insert(1, session);

    let ghostty = ClientTerminal {
        term: Some("xterm-ghostty".to_owned()),
        colorterm: Some("truecolor".to_owned()),
        default_fg: Some((1, 2, 3)),
        default_bg: Some((4, 5, 6)),
        ..ClientTerminal::default()
    };
    mux.attached_capabilities = ghostty.attach_capabilities();
    mux.pointer_shapes_supported = mux.attached_capabilities.pointer_shapes;
    mux.attached_terminal = ghostty;
    mux.apply_client_colors_to_sessions();

    let dumb = ClientTerminal {
        term: Some("dumb".to_owned()),
        ..ClientTerminal::default()
    };
    mux.attached_capabilities = dumb.attach_capabilities();
    mux.pointer_shapes_supported = mux.attached_capabilities.pointer_shapes;
    mux.attached_terminal = dumb;
    mux.apply_client_colors_to_sessions();

    assert!(!mux.attached_capabilities.pointer_shapes);
    assert!(!mux.pointer_shapes_supported);

    let session = mux.sessions.get_mut(&1).expect("session");
    session.feed_pty(b"\x1b]10;?\x07\x1b]11;?\x07\x1b[6n");
    drop(session.drain_passthrough());
    let replies = vec![
        rx.try_recv().expect("OSC 10 reply"),
        rx.try_recv().expect("OSC 11 reply"),
        rx.try_recv().expect("DSR reply"),
    ];
    assert_eq!(
        replies,
        [
            b"\x1b]10;rgb:0101/0202/0303\x07".to_vec(),
            b"\x1b]11;rgb:0404/0505/0606\x07".to_vec(),
            b"\x1b[1;1R".to_vec(),
        ],
        "reattach without colors must not reset model palette or DSR semantics"
    );
}

// Echo-back conformance harness — the I1 (screen == model) enforcer.
//
// Replays PTY bytes through the multiplexer, feeds every composed frame into
// a virtual client terminal (a second `DamageGrid` emulating the operator's
// outer terminal), and asserts cell-exact equality between each visible
// pane's grid and the client screen within the pane rect, plus the
// frame-model cursor contract. Composition is driven deterministically —
// direct `compose_pending_frame` / `compose_full_redraw` calls, no ticker,
// no sleeps.
//
// Synthetic streams below cover focused regressions; recorded PTY fixtures
// under `tests/fixtures/pty/` keep the same harness exercised against real
// CLI/TUI output captured outside the unit test process.

use crate::tui::model::{CursorVisibilityState, cursor_visible_for_state};
use jackin_term::{Cell, DamageGrid};

/// The outer terminal: a second `DamageGrid` sized to the attach client.
/// `apply` is `process()`; the capsule's own `?2026` brackets and mode
/// toggles parse harmlessly. Passthrough events are drained and dropped —
/// they carry no cell content.
struct VirtualClient {
    grid: DamageGrid,
}

impl VirtualClient {
    fn new(rows: u16, cols: u16) -> Self {
        Self {
            grid: DamageGrid::new(rows, cols, 0),
        }
    }

    fn apply(&mut self, frame: &[u8]) {
        self.grid.process(frame);
        drop(self.grid.drain_passthrough());
        drop(self.grid.dirty_spans());
    }

    fn resize(&mut self, rows: u16, cols: u16) {
        self.grid.set_size(rows, cols);
    }

    fn cell_text(cell: Option<&Cell>) -> String {
        match cell {
            Some(c) if !c.contents.is_empty() => c.contents().to_owned(),
            _ => " ".to_owned(),
        }
    }
}

/// Feed PTY bytes into one session and compose the resulting frame exactly
/// the way the daemon's event loop would: mark the pane dirty, compose, and
/// hand the bytes to the virtual client. Out-of-band passthrough and mode
/// transitions are drained and dropped — they never carry cells.
fn feed_and_compose(
    mux: &mut Multiplexer,
    client: &mut VirtualClient,
    session_id: u64,
    bytes: &[u8],
) {
    if let Some(session) = mux.sessions.get_mut(&session_id) {
        session.feed_pty(bytes);
        drop(session.drain_passthrough());
    }
    mux.invalidate(FullRedrawReason::PtyOutput);
    let frame = mux.compose_pending_frame();
    client.apply(&frame);
}

/// Drive one input event the way the daemon loop does: dispatch, then
/// compose whatever the recorded state changes produce and hand it to the
/// virtual client.
fn dispatch_and_compose(mux: &mut Multiplexer, client: &mut VirtualClient, event: InputEvent) {
    mux.handle_input(event);
    let frame = mux.compose_pending_frame();
    client.apply(&frame);
}

/// I1: after a frame, the client screen equals every visible pane's grid
/// (grapheme, full attribute set, wide flags) within the pane rect. Only
/// valid while no dialog covers the panes and no selection overlay is
/// active — those scenarios assert after the overlay is dismissed.
fn assert_screen_matches_model(mux: &mut Multiplexer, client: &VirtualClient, context: &str) {
    assert!(
        !mux.dialog_open(),
        "{context}: I1 cell comparison requires no dialog over the panes"
    );
    let (client_rows, client_cols) = client.grid.size();
    let client_view = client.grid.scrollback_view(0, client_rows);
    let panes = mux.visible_panes();
    assert!(!panes.is_empty(), "{context}: no visible panes");
    for pane in &panes {
        let session = mux
            .sessions
            .get(&pane.id)
            .unwrap_or_else(|| panic!("{context}: pane {} has no session", pane.id));
        let view = session
            .shadow_grid
            .scrollback_view(session.scrollback_offset(), pane.inner.rows);
        for row in 0..pane.inner.rows.min(view.rows) {
            for col in 0..pane.inner.cols.min(view.cols) {
                let screen_row = pane.inner.row + row;
                let screen_col = pane.inner.col + col;
                if screen_row >= client_rows || screen_col >= client_cols {
                    continue;
                }
                let model = view.cell(row, col);
                let client_cell = client_view.cell(screen_row, screen_col);
                let model_text = VirtualClient::cell_text(model);
                let client_text = VirtualClient::cell_text(client_cell);
                assert_eq!(
                    model_text, client_text,
                    "{context}: grapheme mismatch pane {} cell ({row},{col}) / screen ({screen_row},{screen_col})",
                    pane.id
                );
                let default = Cell::default();
                let model_cell = model.unwrap_or(&default);
                let client_cell = client_cell.unwrap_or(&default);
                assert_eq!(
                    model_cell.attrs, client_cell.attrs,
                    "{context}: attr mismatch pane {} cell ({row},{col}) text {model_text:?}",
                    pane.id
                );
                assert_eq!(
                    (model_cell.is_wide, model_cell.is_wide_continuation),
                    (client_cell.is_wide, client_cell.is_wide_continuation),
                    "{context}: wide-flag mismatch pane {} cell ({row},{col}) text {model_text:?}",
                    pane.id
                );
            }
        }
    }
}

/// Frame-model cursor contract: the client cursor is visible exactly when
/// `cursor_visible_for_state` says so for the focused pane, and when visible
/// it sits at the focused pane's VT cursor translated into screen space.
fn assert_cursor_contract(mux: &mut Multiplexer, client: &VirtualClient, context: &str) {
    let dialog_open = mux.dialog_open();
    let focused = mux.active_focused_id();
    let pane = focused.and_then(|id| mux.visible_panes().into_iter().find(|p| p.id == id));
    let expected_visible = match (focused, &pane) {
        (Some(id), Some(_)) => {
            let session = mux.sessions.get(&id).expect("focused session");
            cursor_visible_for_state(CursorVisibilityState {
                dialog_open,
                focused_pane_available: true,
                focused_session_received_output: session.received_output,
                scrollback_active: session.scrollback_offset() != 0,
                agent_cursor_hidden: session.shadow_grid.hide_cursor(),
            })
        }
        _ => false,
    };
    assert_eq!(
        !client.grid.hide_cursor(),
        expected_visible,
        "{context}: cursor visibility violates the frame-model contract"
    );
    if expected_visible {
        let id = focused.expect("visible cursor implies focused pane");
        let pane = pane.expect("visible cursor implies pane rect");
        let session = mux.sessions.get(&id).expect("focused session");
        let (vt_row, vt_col) = session.shadow_grid.cursor_position();
        assert_eq!(
            client.grid.cursor_position(),
            (pane.inner.row + vt_row, pane.inner.col + vt_col),
            "{context}: cursor position must be the focused pane's VT cursor in screen space"
        );
    }
}

fn assert_frame_conformance(mux: &mut Multiplexer, client: &VirtualClient, context: &str) {
    assert_screen_matches_model(mux, client, context);
    assert_cursor_contract(mux, client, context);
}

/// Single pane sized to the pane's inner rect, with the session installed and
/// the first-attach frame applied to a fresh virtual client.
fn attached_single_pane() -> (Multiplexer, VirtualClient, u64) {
    let mut mux = single_pane_tab_mux();
    let pane = mux.visible_panes().into_iter().next().expect("one pane");
    let (session, rx) = test_session(pane.inner.rows, pane.inner.cols);
    // The reply receiver is dropped intentionally: these scenarios never
    // read DSR replies. Sessions that need it use test_session directly.
    drop(rx);
    mux.sessions.insert(1, session);
    let mut client = VirtualClient::new(mux.term_rows, mux.term_cols);
    mux.invalidate(FullRedrawReason::FirstAttach);
    let frame = mux.compose_pending_frame();
    client.apply(&frame);
    (mux, client, 1)
}

/// Synthetic Codex-style stream chunk: SGR-colored, wrapped prose lines.
fn codex_chunk(i: usize) -> Vec<u8> {
    format!(
        "\x1b[38;5;39mcodex\x1b[0m line {i}: \x1b[1mthinking\x1b[0m about \x1b[38;2;0;255;65mrendering\x1b[0m\r\n"
    )
    .into_bytes()
}

#[test]
fn stream_keeps_screen_equal_to_model() {
    let (mut mux, mut client, sid) = attached_single_pane();
    for i in 0..60 {
        feed_and_compose(&mut mux, &mut client, sid, &codex_chunk(i));
        if i % 13 == 0 {
            assert_frame_conformance(&mut mux, &client, &format!("stream chunk {i}"));
        }
    }
    assert_frame_conformance(&mut mux, &client, "stream end");
}

#[test]
fn full_scroll_cycle_keeps_screen_equal_to_model() {
    let (mut mux, mut client, sid) = attached_single_pane();
    for i in 0..60 {
        feed_and_compose(&mut mux, &mut client, sid, &codex_chunk(i));
    }

    // Wheel up three steps into history.
    for step in 0..3 {
        dispatch_and_compose(
            &mut mux,
            &mut client,
            InputEvent::MousePress {
                row: STATUS_BAR_ROWS + 1,
                col: 1,
                button: 64,
            },
        );
        assert_frame_conformance(&mut mux, &client, &format!("wheel up step {step}"));
    }
    assert_ne!(mux.sessions.get(&sid).unwrap().scrollback_offset(), 0);

    // Stream while scrolled: the anchored view must stay equal to the model.
    for i in 60..70 {
        feed_and_compose(&mut mux, &mut client, sid, &codex_chunk(i));
    }
    assert_frame_conformance(&mut mux, &client, "anchored feed while scrolled");

    // Wheel back to the live tail — wheel only.
    while mux.sessions.get(&sid).unwrap().scrollback_offset() != 0 {
        dispatch_and_compose(
            &mut mux,
            &mut client,
            InputEvent::MousePress {
                row: STATUS_BAR_ROWS + 1,
                col: 1,
                button: 65,
            },
        );
    }
    assert_frame_conformance(&mut mux, &client, "wheel back to live");
}

#[test]
fn focus_swap_mid_stream_keeps_screen_equal_to_model() {
    let mut mux = split_tab_mux();
    let panes = mux.visible_panes();
    assert_eq!(panes.len(), 2);
    for pane in &panes {
        let (session, rx) = test_session_with_agent(
            pane.inner.rows,
            pane.inner.cols,
            Some(format!("agent-{}", pane.id)),
        );
        drop(rx);
        mux.sessions.insert(pane.id, session);
    }
    let mut client = VirtualClient::new(mux.term_rows, mux.term_cols);
    mux.invalidate(FullRedrawReason::FirstAttach);
    let frame = mux.compose_pending_frame();
    client.apply(&frame);

    for i in 0..20 {
        feed_and_compose(&mut mux, &mut client, panes[0].id, &codex_chunk(i));
        feed_and_compose(
            &mut mux,
            &mut client,
            panes[1].id,
            format!("pane two output {i}\r\n").as_bytes(),
        );
    }
    assert_frame_conformance(&mut mux, &client, "split stream");

    // Click into the second pane mid-stream, then keep streaming.
    let target = &panes[1];
    dispatch_and_compose(
        &mut mux,
        &mut client,
        InputEvent::MousePress {
            row: target.inner.row + 1,
            col: target.inner.col + 1,
            button: 0,
        },
    );
    for i in 20..30 {
        feed_and_compose(&mut mux, &mut client, panes[0].id, &codex_chunk(i));
    }
    assert_frame_conformance(&mut mux, &client, "focus swap mid-stream");
}

#[test]
fn resize_mid_stream_keeps_screen_equal_to_model() {
    let (mut mux, mut client, sid) = attached_single_pane();
    for i in 0..30 {
        feed_and_compose(&mut mux, &mut client, sid, &codex_chunk(i));
    }

    mux.resize(30, 100);
    client.resize(30, 100);
    let frame = mux.compose_pending_frame();
    client.apply(&frame);
    assert_frame_conformance(&mut mux, &client, "after grow resize");

    for i in 30..40 {
        feed_and_compose(&mut mux, &mut client, sid, &codex_chunk(i));
    }
    assert_frame_conformance(&mut mux, &client, "stream after resize");
}

#[test]
fn dialog_open_close_over_streaming_leaves_no_residue() {
    let (mut mux, mut client, sid) = attached_single_pane();
    for i in 0..20 {
        feed_and_compose(&mut mux, &mut client, sid, &codex_chunk(i));
    }

    mux.apply_action(Action::OpenGithubContext);
    let frame = mux.compose_pending_frame();
    assert!(!frame.is_empty(), "opening a dialog composes a frame");
    client.apply(&frame);
    assert!(mux.dialog_open());

    // Stream under the open dialog — frames keep flowing.
    for i in 20..30 {
        feed_and_compose(&mut mux, &mut client, sid, &codex_chunk(i));
    }

    mux.apply_dialog_action(DialogAction::Dismiss);
    let frame = mux.compose_pending_frame();
    client.apply(&frame);
    assert!(!mux.dialog_open());
    assert_frame_conformance(&mut mux, &client, "after dialog close over streaming");
}

#[test]
fn alt_screen_session_enter_exit_keeps_screen_equal_to_model() {
    let (mut mux, mut client, sid) = attached_single_pane();
    for i in 0..20 {
        feed_and_compose(&mut mux, &mut client, sid, &codex_chunk(i));
    }

    // Claude-style alt-screen TUI: enter, paint a frame, exit.
    feed_and_compose(&mut mux, &mut client, sid, b"\x1b[?1049h\x1b[2J\x1b[H");
    feed_and_compose(
        &mut mux,
        &mut client,
        sid,
        b"\x1b[1;1H\x1b[44m claude \x1b[0m\x1b[3;2HWelcome back\x1b[10;2H> ",
    );
    assert_frame_conformance(&mut mux, &client, "alt screen painted");

    feed_and_compose(&mut mux, &mut client, sid, b"\x1b[?1049l");
    assert_frame_conformance(&mut mux, &client, "after alt-screen exit");
}

#[test]
fn recorded_pty_fixtures_keep_screen_equal_to_model() {
    for (label, bytes) in [
        (
            "codex version fixture",
            include_bytes!("../../tests/fixtures/pty/codex-version.bin").as_slice(),
        ),
        (
            "vim alt-screen fixture",
            include_bytes!("../../tests/fixtures/pty/vim-tiny-open-edit-quit.bin").as_slice(),
        ),
    ] {
        let (mut mux, mut client, sid) = attached_single_pane();
        feed_and_compose(&mut mux, &mut client, sid, bytes);
        assert_frame_conformance(&mut mux, &client, label);
    }
}

#[test]
fn clear_screen_during_selection_overlay_converges_after_clear() {
    let (mut mux, mut client, sid) = attached_single_pane();
    for i in 0..20 {
        feed_and_compose(&mut mux, &mut client, sid, &codex_chunk(i));
    }
    let pane = mux.visible_panes().into_iter().next().expect("one pane");

    // Drag a selection so composition routes through the Ratatui path
    // (the direct-patch tier refuses while a selection is active).
    let press_row = pane.inner.row + 2;
    let press_col = pane.inner.col + 1;
    dispatch_and_compose(
        &mut mux,
        &mut client,
        InputEvent::MousePress {
            row: press_row,
            col: press_col,
            button: 0,
        },
    );
    dispatch_and_compose(
        &mut mux,
        &mut client,
        InputEvent::MousePress {
            row: press_row + 1,
            col: press_col + 10,
            button: 32,
        },
    );

    // The program clears its screen while the selection overlay is active.
    feed_and_compose(&mut mux, &mut client, sid, b"\x1b[2J\x1b[H$ ");

    // Release and click once to clear the selection overlay.
    dispatch_and_compose(
        &mut mux,
        &mut client,
        InputEvent::MouseRelease {
            row: press_row + 1,
            col: press_col + 10,
            button: 0,
        },
    );
    dispatch_and_compose(
        &mut mux,
        &mut client,
        InputEvent::MousePress {
            row: press_row,
            col: press_col,
            button: 0,
        },
    );
    dispatch_and_compose(
        &mut mux,
        &mut client,
        InputEvent::MouseRelease {
            row: press_row,
            col: press_col,
            button: 0,
        },
    );

    assert_frame_conformance(&mut mux, &client, "screen cleared during selection");
}

#[test]
fn selection_residue_cleared_after_copy_click() {
    let (mut mux, mut client, sid) = attached_single_pane();
    for i in 0..20 {
        feed_and_compose(&mut mux, &mut client, sid, &codex_chunk(i));
    }
    let pane = mux.visible_panes().into_iter().next().expect("one pane");
    let press_row = pane.inner.row + 2;
    let press_col = pane.inner.col + 1;
    for event in [
        InputEvent::MousePress {
            row: press_row,
            col: press_col,
            button: 0,
        },
        InputEvent::MousePress {
            row: press_row + 1,
            col: press_col + 8,
            button: 32,
        },
        InputEvent::MouseRelease {
            row: press_row + 1,
            col: press_col + 8,
            button: 0,
        },
        // The follow-up click clears the highlight.
        InputEvent::MousePress {
            row: press_row,
            col: press_col,
            button: 0,
        },
        InputEvent::MouseRelease {
            row: press_row,
            col: press_col,
            button: 0,
        },
    ] {
        dispatch_and_compose(&mut mux, &mut client, event);
    }
    assert!(mux.selection.is_none());
    assert_frame_conformance(&mut mux, &client, "after selection cleared");
}

// ---------------------------------------------------------------------------
// Model-expectation cases (PR 4): these assert the *correct* terminal-model
// semantics. They are red against the current jackin-term model and flip
// green when PR 4 lands; the echo-back equality above cannot catch them
// because the virtual client shares the model's bugs.
// ---------------------------------------------------------------------------

#[test]
fn combining_mark_joins_base_character() {
    let (mut mux, mut client, sid) = attached_single_pane();
    feed_and_compose(&mut mux, &mut client, sid, "e\u{301}!".as_bytes());
    let session = mux.sessions.get(&sid).unwrap();
    let view = session.shadow_grid.scrollback_view(0, 1);
    assert_eq!(
        view.cell(0, 0).map(Cell::contents),
        Some("e\u{301}"),
        "combining acute must join the base cell as one grapheme cluster"
    );
    assert_eq!(
        view.cell(0, 1).map(Cell::contents),
        Some("!"),
        "the next glyph lands in the next cell, not over the cluster"
    );
}

#[test]
fn vs16_emoji_stays_one_cluster() {
    let (mut mux, mut client, sid) = attached_single_pane();
    feed_and_compose(&mut mux, &mut client, sid, "\u{2601}\u{fe0f}X".as_bytes());
    let session = mux.sessions.get(&sid).unwrap();
    let view = session.shadow_grid.scrollback_view(0, 1);
    assert_eq!(
        view.cell(0, 0).map(Cell::contents),
        Some("\u{2601}\u{fe0f}"),
        "VS16 emoji presentation must stay in the base cell"
    );
    assert!(
        view.cell(0, 0).expect("VS16 lead").is_wide,
        "VS16 emoji presentation must occupy two model columns"
    );
    assert!(
        view.cell(0, 1)
            .expect("VS16 continuation")
            .is_wide_continuation,
        "VS16 emoji presentation must create a continuation cell"
    );
    assert_eq!(
        view.cell(0, 2).map(Cell::contents),
        Some("X"),
        "next glyph must land after the grown VS16 cluster"
    );
}

#[test]
fn halfwidth_katakana_dakuten_width_echoes_to_client() {
    let (mut mux, mut client, sid) = attached_single_pane();
    feed_and_compose(&mut mux, &mut client, sid, "\u{ff76}\u{ff9e}X".as_bytes());
    assert_frame_conformance(&mut mux, &client, "dakuten width echo-back");
    let session = mux.sessions.get(&sid).unwrap();
    let view = session.shadow_grid.scrollback_view(0, 1);
    assert_eq!(
        view.cell(0, 0).map(Cell::contents),
        Some("\u{ff76}\u{ff9e}"),
        "halfwidth katakana dakuten must stay in the base cell"
    );
    assert!(
        view.cell(0, 0).expect("dakuten lead").is_wide,
        "dakuten cluster must occupy two model columns"
    );
    assert!(
        view.cell(0, 1)
            .expect("dakuten continuation")
            .is_wide_continuation,
        "dakuten cluster must create a continuation cell"
    );
    assert_eq!(
        view.cell(0, 2).map(Cell::contents),
        Some("X"),
        "next glyph must land after the grown dakuten cluster"
    );
}

#[test]
fn zwj_family_emoji_stays_one_cluster() {
    let (mut mux, mut client, sid) = attached_single_pane();
    let family = "\u{1f468}\u{200d}\u{1f469}\u{200d}\u{1f467}";
    feed_and_compose(&mut mux, &mut client, sid, family.as_bytes());
    assert_frame_conformance(&mut mux, &client, "ZWJ family width echo-back");
    let session = mux.sessions.get(&sid).unwrap();
    let view = session.shadow_grid.scrollback_view(0, 1);
    assert_eq!(
        view.cell(0, 0).map(Cell::contents),
        Some(family),
        "the full ZWJ sequence must live in one cell"
    );
}

#[test]
fn wide_lead_overwrite_blanks_continuation() {
    let (mut mux, mut client, sid) = attached_single_pane();
    feed_and_compose(&mut mux, &mut client, sid, "\u{4f60}".as_bytes());
    feed_and_compose(&mut mux, &mut client, sid, b"\x1b[1;1HA");
    let session = mux.sessions.get(&sid).unwrap();
    let view = session.shadow_grid.scrollback_view(0, 1);
    let continuation = view.cell(0, 1).expect("continuation cell");
    assert!(
        !continuation.is_wide_continuation && continuation.contents.is_empty(),
        "overwriting the wide lead must blank the continuation cell, got {continuation:?}"
    );
}

#[test]
fn decstr_soft_reset_is_handled_in_grid() {
    let (mut mux, mut client, sid) = attached_single_pane();
    feed_and_compose(&mut mux, &mut client, sid, b"\x1b[1m\x1b[?25l\x1b[5;10r");
    feed_and_compose(&mut mux, &mut client, sid, b"\x1b[!p");
    let session = mux.sessions.get_mut(&sid).unwrap();
    assert!(
        !session.shadow_grid.hide_cursor(),
        "DECSTR must reset cursor visibility in the grid"
    );
    session.feed_pty(b"x");
    let passthrough = session.drain_passthrough();
    assert!(
        passthrough.iter().all(|seq| !seq.ends_with(b"p")),
        "DECSTR must never be forwarded to the client: {passthrough:?}"
    );
    let view = session.shadow_grid.scrollback_view(0, 1);
    let cell = view
        .cell(
            session.shadow_grid.cursor_position().0,
            session.shadow_grid.cursor_position().1.saturating_sub(1),
        )
        .expect("written cell");
    assert!(
        !cell.attrs.bold,
        "DECSTR must reset SGR attributes before the next write"
    );
}

#[test]
fn dsr_cursor_report_clamps_phantom_column() {
    let mut mux = single_pane_tab_mux();
    let pane = mux.visible_panes().into_iter().next().expect("one pane");
    let cols = pane.inner.cols;
    let (session, mut input_rx) = test_session(pane.inner.rows, cols);
    mux.sessions.insert(1, session);
    let mut client = VirtualClient::new(mux.term_rows, mux.term_cols);
    mux.invalidate(FullRedrawReason::FirstAttach);
    let frame = mux.compose_pending_frame();
    client.apply(&frame);

    // Fill the first row to the last column: the cursor enters the
    // deferred-wrap state whose internal column is cols (0-based phantom).
    let fill = "x".repeat(usize::from(cols));
    feed_and_compose(&mut mux, &mut client, 1, fill.as_bytes());
    feed_and_compose(&mut mux, &mut client, 1, b"\x1b[6n");

    let reply = input_rx.try_recv().expect("DSR reply goes to the agent");
    let reply = String::from_utf8(reply).expect("CPR is ASCII");
    let expected = format!("\x1b[1;{cols}R");
    assert_eq!(
        reply, expected,
        "CPR must clamp the phantom column to the last real column"
    );
}

#[test]
fn osc_color_query_answers_with_the_attached_terminal_palette() {
    // Codex's startup probe: it paints no backgrounds at all until OSC 11
    // is answered, so the reply must carry the attach client's real colors.
    let mut mux = single_pane_tab_mux();
    let pane = mux.visible_panes().into_iter().next().expect("one pane");
    let (session, mut input_rx) = test_session(pane.inner.rows, pane.inner.cols);
    mux.sessions.insert(1, session);
    mux.attached_terminal.default_fg = Some((0xe6, 0xe6, 0xe6));
    mux.attached_terminal.default_bg = Some((0x17, 0x17, 0x17));
    mux.apply_client_colors_to_sessions();
    let mut client = VirtualClient::new(mux.term_rows, mux.term_cols);
    mux.invalidate(FullRedrawReason::FirstAttach);
    let frame = mux.compose_pending_frame();
    client.apply(&frame);

    feed_and_compose(&mut mux, &mut client, 1, b"\x1b]10;?\x1b\\\x1b]11;?\x07");

    let fg_reply = input_rx.try_recv().expect("OSC 10 reply goes to the agent");
    assert_eq!(fg_reply, b"\x1b]10;rgb:e6e6/e6e6/e6e6\x1b\\");
    let bg_reply = input_rx.try_recv().expect("OSC 11 reply goes to the agent");
    assert_eq!(bg_reply, b"\x1b]11;rgb:1717/1717/1717\x07");
}

#[test]
fn decscusr_reconciles_per_pane_and_never_forwards_raw() {
    let contains = |frame: &[u8], needle: &[u8]| frame.windows(needle.len()).any(|w| w == needle);
    let (mut mux, mut client, sid) = attached_single_pane();
    feed_and_compose(&mut mux, &mut client, sid, b"hello");

    // The agent picks a bar cursor; the next frame reconciles it.
    if let Some(session) = mux.sessions.get_mut(&sid) {
        session.feed_pty(b"\x1b[5 q");
        let passthrough = session.drain_passthrough();
        assert!(
            passthrough.is_empty(),
            "DECSCUSR must never be forwarded raw: {passthrough:?}"
        );
    }
    mux.invalidate(FullRedrawReason::PtyOutput);
    let frame = mux.compose_pending_frame();
    assert!(
        contains(&frame, b"\x1b[5 q"),
        "frame must assert the pane's cursor style: {:?}",
        String::from_utf8_lossy(&frame)
    );
    client.apply(&frame);
    assert_frame_conformance(&mut mux, &client, "after DECSCUSR");

    // Unchanged style: no re-assertion on the next frame.
    feed_and_compose(&mut mux, &mut client, sid, b" world");
    mux.invalidate(FullRedrawReason::PtyOutput);
    let next = mux.compose_pending_frame();
    assert!(
        !contains(&next, b"\x1b[5 q"),
        "unchanged cursor style must not be re-asserted: {:?}",
        String::from_utf8_lossy(&next)
    );
}

/// Perf probe for the capsule rendering plan's PR 3 step 9. Runs in the
/// normal suite (it doubles as a 300-frame conformance soak); run with
/// `--nocapture` to read the p50/p95 compose duration and bytes/frame that
/// the PR body records.
#[test]
fn render_perf_probe() {
    let (mut mux, mut client, sid) = attached_single_pane();
    let mut durations_us: Vec<u128> = Vec::with_capacity(300);
    let mut bytes: Vec<usize> = Vec::with_capacity(300);
    for i in 0..300 {
        if let Some(session) = mux.sessions.get_mut(&sid) {
            session.feed_pty(&codex_chunk(i));
            drop(session.drain_passthrough());
        }
        mux.invalidate(FullRedrawReason::PtyOutput);
        let started = Instant::now();
        let frame = mux.compose_pending_frame();
        durations_us.push(started.elapsed().as_micros());
        bytes.push(frame.len());
        client.apply(&frame);
    }
    durations_us.sort_unstable();
    bytes.sort_unstable();
    #[expect(
        clippy::cast_sign_loss,
        reason = "percentile q is 0.0..=1.0; index stays within v.len()"
    )]
    let pick = |v: &[u128], q: f64| v[((v.len() - 1) as f64 * q) as usize];
    #[expect(
        clippy::cast_sign_loss,
        reason = "percentile q is 0.0..=1.0; index stays within v.len()"
    )]
    let pick_b = |v: &[usize], q: f64| v[((v.len() - 1) as f64 * q) as usize];
    {
        println!(
            "render_perf_probe: frames={} duration_us p50={} p95={} max={} bytes p50={} p95={} max={}",
            durations_us.len(),
            pick(&durations_us, 0.50),
            pick(&durations_us, 0.95),
            durations_us.last().copied().unwrap_or_default(),
            pick_b(&bytes, 0.50),
            pick_b(&bytes, 0.95),
            bytes.last().copied().unwrap_or_default(),
        );
    }
    assert_frame_conformance(&mut mux, &client, "perf probe end");
}

fn exit_dirty_selected_value(mux: &Multiplexer) -> usize {
    match mux.dialog_top() {
        Some(Dialog::ExitDirty { selected, .. }) => *selected,
        other => panic!("expected ExitDirty on top, got {other:?}"),
    }
}

#[test]
fn exit_dirty_down_arrow_advances_selection_via_handle_input() {
    // Zero live panes — exactly the dirty-exit modal scenario.
    let mut mux = test_mux(30, 100);
    mux.dialog_push(Dialog::new_exit_dirty(
        vec!["holla   1 changed".to_owned()],
        Arc::from([]),
    ));
    assert_eq!(exit_dirty_selected_value(&mux), 0);

    // Down arrow, as the input parser hands it to handle_input.
    mux.handle_input(InputEvent::Data(vec![0x1b, 0x5b, 0x42]));
    assert_eq!(
        exit_dirty_selected_value(&mux),
        1,
        "down arrow must advance the dirty-exit selection through the full daemon path"
    );

    mux.handle_input(InputEvent::Data(vec![0x1b, 0x5b, 0x42]));
    assert_eq!(exit_dirty_selected_value(&mux), 2);

    // Up arrow walks back.
    mux.handle_input(InputEvent::Data(vec![0x1b, 0x5b, 0x41]));
    assert_eq!(exit_dirty_selected_value(&mux), 1);
}

#[test]
fn exit_dirty_down_arrow_recomposes_a_changed_frame() {
    let mut mux = test_mux(30, 100);
    mux.dialog_push(Dialog::new_exit_dirty(
        vec!["holla   1 changed".to_owned()],
        Arc::from([]),
    ));
    // Paint the modal once so rendered == frame generation.
    let first = mux.compose_pending_frame();

    // Down arrow should invalidate and produce a non-empty, *changed* frame.
    mux.handle_input(InputEvent::Data(vec![0x1b, 0x5b, 0x42]));
    assert!(
        mux.has_pending_render(),
        "down arrow on the modal must mark a pending render"
    );
    let second = mux.compose_pending_frame();
    assert!(!second.is_empty(), "down arrow must recompose a frame");
    assert_ne!(
        first, second,
        "the recomposed frame must differ once the selection moved"
    );
}

// End-to-end screen repro: replay the daemon's emitted ANSI bytes into a real
// terminal grid (the same vte-backed emulator the capsule uses for PTY output)
// and read where the selection marker actually lands on screen.
fn marker_row_on_screen(grid: &DamageGrid, rows: u16, cols: u16) -> Option<u16> {
    for row in 0..rows {
        for col in 0..cols {
            if grid
                .cell(row, col)
                .is_some_and(|c| c.contents() == "\u{25b8}")
            {
                return Some(row);
            }
        }
    }
    None
}

#[test]
fn exit_dirty_marker_moves_on_screen_with_zero_panes() {
    let (rows, cols) = (44u16, 157u16);
    let mut mux = test_mux(rows, cols);
    mux.dialog_push(Dialog::new_exit_dirty(
        vec!["holla   1 changed \u{b7} 3 unpushed".to_owned()],
        Arc::from([]),
    ));
    mux.invalidate(FullRedrawReason::DialogChange);
    let mut grid = DamageGrid::new(rows, cols, 0);

    grid.process(&mux.compose_pending_frame());
    let before = marker_row_on_screen(&grid, rows, cols);

    mux.handle_input(InputEvent::Data(vec![0x1b, 0x5b, 0x42])); // down
    grid.process(&mux.compose_pending_frame());
    let after = marker_row_on_screen(&grid, rows, cols);

    assert!(
        before.is_some(),
        "marker must be visible on screen initially"
    );
    assert!(
        after > before,
        "down arrow must move the rendered \u{25b8} marker: before={before:?} after={after:?}"
    );
}

#[test]
fn exit_dirty_marker_moves_after_session_exits_realistic() {
    // Reproduce the real path: a live agent pane, the session exits, the
    // dirty-exit modal opens with zero panes, then the operator presses down.
    // The client is a persistent VirtualClient (its grid carries the agent
    // screen forward), so this exercises the exact diff baseline the operator's
    // terminal sees — unlike a modal opened on a blank mux.
    let (rows, cols) = (44u16, 157u16);
    let mut mux = single_pane_tab_mux_with_size(rows, cols);
    let (session, _rx) = test_session(rows, cols);
    mux.sessions.insert(1, session);
    let mut client = VirtualClient::new(rows, cols);

    // Agent paints something; the client mirrors it.
    feed_and_compose(&mut mux, &mut client, 1, b"agent output here\r\n");

    // The session exits — the daemon removes it (zero panes now).
    mux.remove_exited_session(1);

    // handle_last_session_exit opens the modal and invalidates.
    mux.dialog_push(Dialog::new_exit_dirty(
        vec!["holla   1 changed \u{b7} 3 unpushed".to_owned()],
        Arc::from([]),
    ));
    mux.invalidate(FullRedrawReason::DialogChange);
    let frame = mux.compose_pending_frame();
    client.apply(&frame);
    let before = marker_row_on_screen(&client.grid, rows, cols);

    // Operator presses down.
    dispatch_and_compose(
        &mut mux,
        &mut client,
        InputEvent::Data(vec![0x1b, 0x5b, 0x42]),
    );
    let after = marker_row_on_screen(&client.grid, rows, cols);

    assert!(
        before.is_some(),
        "modal marker must be visible after session exit"
    );
    assert!(
        after > before,
        "down arrow must move the rendered marker on the operator's screen: before={before:?} after={after:?}"
    );
}

#[tokio::test]
async fn last_session_exit_does_not_repush_modal_while_dialog_open() {
    // Regression: the event loop calls handle_last_session_exit on every client
    // frame while no sessions are live. With the dirty-exit modal already open,
    // re-entering must NOT push a second modal (which reset the selection to 0
    // every keypress, capping navigation at row 1).
    let mut mux = test_mux(44, 157);
    mux.dialog_push(Dialog::new_exit_dirty(
        vec!["holla   1 changed".to_owned()],
        Arc::from([]),
    ));
    let depth_before = mux.dialog_stack.len();

    let exited = handle_last_session_exit(&mut mux, None).await;

    assert!(!exited, "must keep the loop alive while the modal is open");
    assert_eq!(
        mux.dialog_stack.len(),
        depth_before,
        "must not re-push the modal while a dialog is already open"
    );
}

#[test]
fn exit_dirty_down_arrow_reaches_last_row() {
    // The selection must advance all the way to the final row (Discard), not cap
    // at row 1 — guards against an off-by-one or re-push regression.
    let mut mux = test_mux(44, 157);
    mux.dialog_push(Dialog::new_exit_dirty(
        vec!["holla   1 changed".to_owned()],
        Arc::from([]),
    ));
    for _ in 0..5 {
        mux.handle_input(InputEvent::Data(vec![0x1b, 0x5b, 0x42])); // down
    }
    match mux.dialog_top() {
        Some(Dialog::ExitDirty { selected, .. }) => {
            assert_eq!(
                *selected, 3,
                "five downs must land on the last row (Discard)"
            );
        }
        other => panic!("expected ExitDirty, got {other:?}"),
    }
}

#[test]
fn build_exit_inspect_rows_groups_repos_with_header_and_file_rows() {
    use crate::exit_assess::DirtyRepo;
    use crate::tui::components::dialog::InspectRow;
    use jackin_core::worktree_dirty::ChangedFile;

    let repos = vec![
        DirtyRepo {
            path: "/workspace/alpha".to_owned(),
            changed: vec![
                ChangedFile {
                    status: 'M',
                    path: "src/main.rs".to_owned(),
                },
                ChangedFile {
                    status: '?',
                    path: "new.rs".to_owned(),
                },
            ],
            unpushed: 0,
        },
        DirtyRepo {
            path: "/workspace/beta".to_owned(),
            changed: vec![],
            unpushed: 1,
        },
    ];
    let rows = build_exit_inspect_rows(&repos);
    // First entry must be a Repo header.
    assert!(matches!(rows.first(), Some(InspectRow::Repo(_))));
    // Two repos → exactly two Repo headers.
    let repo_count = rows
        .iter()
        .filter(|r| matches!(r, InspectRow::Repo(_)))
        .count();
    assert_eq!(repo_count, 2, "one header per repo");
    // alpha has two changed files → two File rows follow its header.
    let file_count = rows
        .iter()
        .filter(|r| matches!(r, InspectRow::File(_)))
        .count();
    assert_eq!(file_count, 2, "only changed files produce File rows");
    // Repo labels are derived from the final path component.
    if let Some(InspectRow::Repo(label)) = rows.first() {
        assert_eq!(label, "alpha");
    }
    // File rows are formatted as "<status> <path>".
    let file_rows: Vec<_> = rows
        .iter()
        .filter_map(|r| {
            if let InspectRow::File(s) = r {
                Some(s.as_str())
            } else {
                None
            }
        })
        .collect();
    assert_eq!(file_rows, ["M src/main.rs", "? new.rs"]);
}
