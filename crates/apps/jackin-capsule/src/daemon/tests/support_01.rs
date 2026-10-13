// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;
pub(super) fn serialized_control_spans(
    context: jackin_protocol::TelemetryContext,
) -> (bool, Vec<jackin_diagnostics::TestSpanSnapshot>) {
    let (export, subscriber) = jackin_diagnostics::observability::test_capsule_layers(false);
    let guard = tracing::subscriber::set_default(subscriber);
    let wire = serde_json::to_vec(&jackin_protocol::control::ControlRequest {
        ctx: context,
        session_capability: None,
        msg: ClientMsg::Status,
    })
    .unwrap();
    let decoded: jackin_protocol::control::ControlRequest = serde_json::from_slice(&wire).unwrap();
    let operation = control_server_operation(&decoded.ctx, &decoded.msg);
    let accepted = operation.is_ok();
    if let Ok(Some(operation)) = operation {
        operation.complete(jackin_telemetry::schema::enums::OutcomeValue::Success, None);
    }
    drop(guard);
    export.force_flush();
    (accepted, export.finished_spans())
}

pub(super) fn sgr_regions_for_inner(
    bytes: &[u8],
    inner: Rect,
) -> Vec<(ratatui::layout::Rect, SgrMetadata)> {
    let mut grid = DamageGrid::new(2, 10, 100);
    grid.process(bytes);
    let view = grid.scrollback_view(0, 2);
    let panes = vec![VisiblePane {
        id: 1,
        outer: inner,
        inner,
        focused: false,
    }];
    let pane_screens = vec![(1u64, crate::tui::view::PaneScreen::View(view))];
    compositor::pane_sgr_regions(&panes, &pane_screens)
}

pub(super) fn sgr_regions_for(bytes: &[u8]) -> Vec<(ratatui::layout::Rect, SgrMetadata)> {
    sgr_regions_for_inner(bytes, Rect::new(2, 3, 5, 10))
}

#[derive(Debug)]
pub(super) struct NullChildKiller;

impl ChildKiller for NullChildKiller {
    fn kill(&mut self) -> io::Result<()> {
        Ok(())
    }

    fn clone_killer(&self) -> Box<dyn ChildKiller + Send + Sync> {
        Box::new(Self)
    }
}

pub(super) struct NullMasterPty;

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
    fn process_group_leader(&self) -> Option<libc::pid_t> {
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

pub(super) fn test_mux(rows: u16, cols: u16) -> Multiplexer {
    Multiplexer::new(
        rows,
        cols,
        CapsuleConfig {
            role: "test-role".to_owned(),
            workdir: "/workspace".to_owned(),
            instances: Vec::new(),
            agents: BTreeMap::new(),
            models: BTreeMap::new(),
            efforts: BTreeMap::new(),
            auth_modes: BTreeMap::new(),
            accounts: BTreeMap::new(),
            usage_capabilities: BTreeMap::new(),
            credential_provider_surfaces: BTreeMap::new(),
            labels: BTreeMap::new(),
            claude_marketplaces: Vec::new(),
            claude_plugins: Vec::new(),
            exec_bindings: Vec::new(),
            dirty_exit_policy: None,
            isolated_worktrees: Vec::new(),
            workspace_mounts: Vec::new(),
            worktree_git_targets: Vec::new(),
            instance_home_dirs: BTreeMap::new(),
            instance_cache_dirs: BTreeMap::new(),
            instance_forwarded_dirs: BTreeMap::new(),
            instance_credential_files: BTreeMap::new(),
            instance_mount_paths: BTreeMap::new(),
            instance_identities: BTreeMap::new(),
            shell_identity: Some(jackin_protocol::SessionIdentity {
                uid: 2_000,
                gid: 2_000,
            }),
        },
    )
    .unwrap_or_else(|error| panic!("test multiplexer construction failed: {error}"))
}

pub(super) fn append_lifecycle_output(output: &mut Vec<u8>, bytes: Vec<u8>) -> bool {
    output.extend(bytes);
    contains_lifecycle_sentinel(output)
}

pub(super) fn record_lifecycle_output(output: &mut Vec<u8>, bytes: Vec<u8>) {
    output.extend(bytes);
}

pub(super) fn lifecycle_output_frame(frame: ServerFrame, output: &mut Vec<u8>) -> Result<bool> {
    match frame {
        ServerFrame::Output(bytes) => Ok(append_lifecycle_output(output, bytes)),
        ServerFrame::Shutdown { reason } => shutdown_before_sentinel(reason),
        other => anyhow::bail!("unexpected pre-shutdown frame: {other:?}"),
    }
}

pub(super) fn lifecycle_shutdown_frame(frame: ServerFrame, output: &mut Vec<u8>) -> Result<bool> {
    match frame {
        ServerFrame::Output(bytes) => {
            record_lifecycle_output(output, bytes);
            Ok(false)
        }
        ServerFrame::Shutdown { reason } => clean_shutdown(reason),
        other => anyhow::bail!("unexpected lifecycle frame: {other:?}"),
    }
}

pub(super) fn contains_lifecycle_sentinel(output: &[u8]) -> bool {
    output
        .windows(b"CAPSULE_LIFECYCLE_SENTINEL".len())
        .any(|window| window == b"CAPSULE_LIFECYCLE_SENTINEL")
}

pub(super) fn shutdown_before_sentinel(reason: Option<String>) -> Result<bool> {
    clean_shutdown(reason)?;
    anyhow::bail!("daemon shut down before shell output")
}

pub(super) fn clean_shutdown(reason: Option<String>) -> Result<bool> {
    if let Some(reason) = reason {
        anyhow::bail!("clean shell exit carried reason: {reason:?}");
    }
    Ok(true)
}

pub(super) fn compose_after(mux: &mut Multiplexer, reason: FullRedrawReason) -> Vec<u8> {
    mux.invalidate(reason);
    mux.compose_pending_frame()
}

pub(super) fn handle_input_frame(mux: &mut Multiplexer, event: InputEvent) -> Option<Vec<u8>> {
    mux.handle_input(event);
    let frame = mux.compose_pending_frame();
    (!frame.is_empty()).then_some(frame)
}

pub(super) fn apply_action_frame(mux: &mut Multiplexer, action: Action) -> Option<Vec<u8>> {
    mux.apply_action(action);
    let frame = mux.compose_pending_frame();
    (!frame.is_empty()).then_some(frame)
}

pub(super) fn seed_usage_dialog_for_refresh_test(mux: &mut Multiplexer) {
    let (mut session, _session_rx) = test_session_with_agent(24, 80, Some("codex".to_owned()));
    session.provider = Some(crate::session::SessionProvider {
        label: "OpenAI".to_owned(),
        env_overrides: Vec::new(),
    });
    mux.session_supervisor.sessions.insert(1, session);
    mux.session_supervisor.tabs[0] = Tab::new_single("Codex", 1, "test");
    let mut stale = jackin_protocol::control::FocusedUsageView::unavailable("seed", 1);
    stale.updated_label = "seed".to_owned();
    stale.status_bar_label = "seed".to_owned();
    mux.dialog_push(Dialog::new_usage(stale));
}

pub(super) fn palette_command_frame(mux: &mut Multiplexer, cmd: PaletteCommand) -> Option<Vec<u8>> {
    mux.handle_palette_command(cmd);
    let frame = mux.compose_pending_frame();
    (!frame.is_empty()).then_some(frame)
}

pub(super) fn prefix_command_frame(mux: &mut Multiplexer, cmd: PrefixCommand) -> Option<Vec<u8>> {
    mux.handle_prefix_command(cmd);
    let frame = mux.compose_pending_frame();
    (!frame.is_empty()).then_some(frame)
}

pub(crate) fn single_pane_tab_mux() -> Multiplexer {
    single_pane_tab_mux_with_size(24, 80)
}

pub(super) fn single_pane_tab_mux_with_size(rows: u16, cols: u16) -> Multiplexer {
    let mut mux = test_mux(24, 80);
    mux.resize(rows, cols);
    mux.session_supervisor
        .tabs
        .push(Tab::new_single("Shell", 1, "test"));
    // Drain the construction-time Resize invalidation the way the real
    // attach burst does, so tests observe only their own state changes.
    drop(mux.compose_pending_frame());
    mux
}

pub(super) fn assert_session_peer_authorization(
    mux: &Multiplexer,
    own: jackin_protocol::SessionIdentity,
    own_capability: &str,
    sibling_capability: &str,
) {
    assert!(control_request_allowed(
        mux,
        Some(own.uid),
        Some(own_capability),
        &ClientMsg::SessionSend {
            session: 1,
            text: "own".to_owned(),
        }
    ));
    assert!(control_request_allowed(
        mux,
        Some(own.uid),
        Some(own_capability),
        &ClientMsg::StatusCapture { session_id: 1 }
    ));
    assert!(control_request_allowed(
        mux,
        Some(own.uid),
        Some(own_capability),
        &ClientMsg::ReportRuntimeEvent {
            session_id: 1,
            source_id: "hook-codex-1".to_owned(),
            runtime: "codex".to_owned(),
            event: "Stop".to_owned(),
            payload: None,
        }
    ));
    assert!(control_request_allowed(
        mux,
        Some(own.uid),
        Some(own_capability),
        &ClientMsg::Events { session: Some(1) }
    ));
    assert!(control_request_allowed(
        mux,
        Some(own.uid),
        Some(own_capability),
        &ClientMsg::ExecCommand {
            command: "gh".to_owned(),
            args: vec!["auth".to_owned(), "status".to_owned()],
        }
    ));
    assert!(!control_request_allowed(
        mux,
        Some(own.uid),
        Some(sibling_capability),
        &ClientMsg::ExecCommand {
            command: "gh".to_owned(),
            args: Vec::new(),
        }
    ));
    assert!(!control_request_allowed(
        mux,
        Some(own.uid),
        Some(own_capability),
        &ClientMsg::SessionSend {
            session: 2,
            text: "sibling".to_owned(),
        }
    ));
    assert!(!control_request_allowed(
        mux,
        Some(own.uid),
        Some(own_capability),
        &ClientMsg::StatusCapture { session_id: 2 }
    ));
    assert!(!control_request_allowed(
        mux,
        Some(own.uid),
        None,
        &ClientMsg::Status
    ));
    assert!(!control_request_allowed(
        mux,
        Some(own.uid),
        None,
        &ClientMsg::Events { session: None }
    ));
    assert!(!control_request_allowed(
        mux,
        Some(own.uid),
        None,
        &ClientMsg::ExecCommand {
            command: "op".to_owned(),
            args: Vec::new(),
        }
    ));
    assert!(!control_request_allowed(
        mux,
        None,
        None,
        &ClientMsg::Status
    ));
}

pub(super) fn assert_operator_and_unknown_peer_authorization(mux: &Multiplexer) {
    assert!(control_request_allowed(
        mux,
        Some(0),
        None,
        &ClientMsg::Status
    ));
    assert!(control_request_allowed(
        mux,
        Some(0),
        None,
        &ClientMsg::ExecCommand {
            command: "gh".to_owned(),
            args: Vec::new(),
        }
    ));
    assert!(control_request_allowed(
        mux,
        Some(9_999),
        None,
        &ClientMsg::Snapshot
    ));
    assert!(!control_request_allowed(
        mux,
        Some(9_999),
        None,
        &ClientMsg::ExecCommand {
            command: "gh".to_owned(),
            args: Vec::new(),
        }
    ));
}

pub(super) fn frame_contains_screen_erase(frame: &[u8]) -> bool {
    frame.windows(b"\x1b[2J".len()).any(|w| w == b"\x1b[2J")
}

pub(super) fn pull_request_fixture(number: u64) -> PullRequestInfo {
    PullRequestInfo {
        number,
        title: "Surface PR context in Capsule".to_owned(),
        url: format!("https://github.com/jackin-project/jackin/pull/{number}"),
        is_draft: false,
        checks: None,
    }
}

pub(super) fn oid(nibble: char) -> Oid {
    assert!(nibble.is_ascii_hexdigit(), "nibble must be 0-9/a-f");
    Oid::parse(&nibble.to_string().repeat(40)).expect("40 hex chars is a valid Oid")
}

pub(super) fn branch(name: &str) -> BranchName {
    BranchName::parse(name).expect("test branch names must parse")
}
