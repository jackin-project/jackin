// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

#[test]
fn conformance_clipboard_continuations_validate_correlation_and_start_identity() {
    use jackin_protocol::attach::{
        AttachControlOperation, AttachControlRequest, ClipboardImageChunk, ClipboardImageEnd,
        ClipboardImageFormat, ClipboardImageStart,
    };

    let mut mux = test_mux(24, 80);
    let (out_tx, mut out_rx) = mpsc::unbounded_channel();
    let (completion_tx, _completion_rx) = mpsc::unbounded_channel();
    mux.client_registry
        .client
        .attach_with_completions(out_tx, completion_tx);
    let start_context = jackin_protocol::TelemetryContext::v1();
    handle_client_frame(
        &mut mux,
        ClientFrame::AttachControl(AttachControlRequest {
            request_id: 91,
            context: start_context.clone(),
            operation: AttachControlOperation::ClipboardImageStart(ClipboardImageStart {
                transfer_id: 7,
                format: ClipboardImageFormat::Png,
                size: 3,
            }),
        }),
    );
    assert!(mux.clipboard.attach_control_operations.contains_key(&7));

    handle_client_frame(
        &mut mux,
        ClientFrame::AttachControl(AttachControlRequest {
            request_id: 91,
            context: jackin_protocol::TelemetryContext {
                invocation_id: Some("not-a-uuid".to_owned()),
                ..jackin_protocol::TelemetryContext::v1()
            },
            operation: AttachControlOperation::ClipboardImageChunk(ClipboardImageChunk {
                transfer_id: 7,
                offset: 0,
                bytes: vec![1, 2, 3],
            }),
        }),
    );
    assert!(mux.clipboard.attach_control_operations.contains_key(&7));

    handle_client_frame(
        &mut mux,
        ClientFrame::AttachControl(AttachControlRequest {
            request_id: 92,
            context: start_context.clone(),
            operation: AttachControlOperation::ClipboardImageEnd(ClipboardImageEnd {
                transfer_id: 7,
                sha256: [0; jackin_protocol::attach::FILE_EXPORT_DIGEST_BYTES],
            }),
        }),
    );
    assert!(
        mux.clipboard.attach_control_operations.contains_key(&7),
        "a continuation with a different request identity must not consume the transfer"
    );

    handle_client_frame(
        &mut mux,
        ClientFrame::AttachControl(AttachControlRequest {
            request_id: 91,
            context: start_context,
            operation: AttachControlOperation::ClipboardImageChunk(ClipboardImageChunk {
                transfer_id: 7,
                offset: 0,
                bytes: vec![1, 2, 3],
            }),
        }),
    );
    assert!(
        mux.clipboard.attach_control_operations.contains_key(&7),
        "the rejected chunk must not advance the transfer offset"
    );

    let responses = std::iter::from_fn(|| out_rx.try_recv().ok())
        .filter_map(|encoded| {
            jackin_protocol::attach::decode_server(encoded[0], encoded[5..].to_vec()).ok()
        })
        .collect::<Vec<_>>();
    assert!(responses.iter().any(|response| matches!(
        response,
        ServerFrame::AttachControlResponse(response)
            if response.request_id == 91
                && response.result
                    == jackin_protocol::attach::AttachControlResult::InvalidCorrelation
    )));
    assert!(responses.iter().any(|response| matches!(
        response,
        ServerFrame::AttachControlResponse(response)
            if response.request_id == 92
                && response.result == jackin_protocol::attach::AttachControlResult::Rejected
    )));
}

#[test]
fn pane_sgr_regions_coalesces_one_styled_run_and_skips_default() {
    let regions = sgr_regions_for(b"\x1b[4:3mab\x1b[24mcd");
    assert_eq!(regions.len(), 1, "got {regions:?}");
    let (rect, metadata) = regions[0];
    assert_eq!((rect.x, rect.y, rect.width, rect.height), (3, 2, 2, 1));
    assert_eq!(metadata.underline_style, termpane::UnderlineStyle::Curly);
}

#[test]
fn pane_sgr_regions_splits_adjacent_differing_runs() {
    let regions = sgr_regions_for(b"\x1b[4:3mab\x1b[4:2mcd");
    assert_eq!(regions.len(), 2, "got {regions:?}");
    assert_eq!((regions[0].0.x, regions[0].0.width), (3, 2));
    assert_eq!(
        regions[0].1.underline_style,
        termpane::UnderlineStyle::Curly
    );
    assert_eq!((regions[1].0.x, regions[1].0.width), (5, 2));
    assert_eq!(
        regions[1].1.underline_style,
        termpane::UnderlineStyle::Double
    );
}

#[test]
fn pane_sgr_regions_empty_when_nothing_styled() {
    assert!(sgr_regions_for(b"plain text").is_empty());
}

#[test]
fn pane_sgr_regions_clamps_run_to_inner_width() {
    let regions = sgr_regions_for_inner(b"\x1b[4:3mabcdef", Rect::new(0, 0, 5, 4));
    assert_eq!(regions.len(), 1, "got {regions:?}");
    assert_eq!(regions[0].0.width, 4, "run must clamp to inner cols");
    assert_eq!(
        regions[0].1.underline_style,
        termpane::UnderlineStyle::Curly
    );
}

#[test]
fn status_tick_select_arm_stays_above_pty_output() {
    let source = include_str!("../../daemon.rs");
    let tick_arm = source
        .find("_ = state_ticker.tick()")
        .unwrap_or_else(|| panic!("state ticker select arm missing"));
    let output_arm = source
        .find("Some(event) = mux.control.event_rx.recv()")
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
    mux.session_supervisor.sessions.insert(1, session);
    mux.open_spawn_failure_dialog("boom: agent slug rejected".to_owned());
    let frame = compose_after(&mut mux, FullRedrawReason::DialogChange);
    assert!(
        contains(&frame, b"boom:") && contains(&frame, b"rejected"),
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

#[test]
fn record_agent_history_uses_injected_clock() {
    use jackin_core::ManualClock;
    use std::sync::Arc;
    use std::time::{Duration, UNIX_EPOCH};

    let base = UNIX_EPOCH + Duration::from_secs(1_700_000_000);
    let clock = Arc::new(ManualClock::with_system_base(base));
    let mut mux = test_mux(40, 80);
    {
        let shared: Arc<ManualClock> = Arc::clone(&clock);
        mux.clock = shared;
    }

    mux.record_agent_history(1, "alpha".into(), Some("claude".into()), None);
    let started = mux.session_supervisor.agent_history[0].started_at;
    assert_eq!(
        started,
        DateTime::<Utc>::from(base),
        "started_at must reflect injected wall clock"
    );

    clock.advance(Duration::from_secs(30));
    mux.mark_agent_session_exited(1);
    let exited = mux.session_supervisor.agent_history[0].exited_at.unwrap();
    assert_eq!(
        exited,
        DateTime::<Utc>::from(base + Duration::from_secs(30))
    );
}

#[test]
fn conformance_wire_generated_codename_reaches_child_without_export() -> Result<()> {
    const CHILD: &str = "JACKIN_CODENAME_PRIVACY_WIRE_CHILD";
    if std::env::var_os(CHILD).is_none() {
        let status = Command::new(std::env::current_exe()?)
            .args([
                "--exact",
                "daemon::tests::case_02::conformance_wire_generated_codename_reaches_child_without_export",
                "--nocapture",
            ])
            .env(CHILD, "1")
            .status()?;
        anyhow::ensure!(status.success(), "isolated codename privacy test failed");
        return Ok(());
    }

    let _telemetry_guard = crate::support::telemetry_test_guard();

    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()?;
    let testbed = runtime.block_on(async { jackin_otlp_testbed::Testbed::start() })?;
    let runtime_guard = runtime.enter();
    jackin_diagnostics::init_wire_test_export(
        &testbed.endpoint(),
        jackin_diagnostics::ServiceIdentity::CAPSULE,
    )?;
    let workdir = tempfile::tempdir()?;
    let mut mux = test_mux(24, 80);
    mux.launch_env.workdir = workdir.path().to_path_buf();
    let session_id = mux.spawn_session(None, &[], None)?;
    let codename = mux.session_supervisor.tabs[0].codename.clone();
    anyhow::ensure!(
        mux.session_supervisor.codename_live.contains(&codename)
            && mux.session_supervisor.agent_history[0].codename == codename,
        "generated codename did not reach tab/history state"
    );
    anyhow::ensure!(
        mux.session_supervisor.sessions.get(session_id).is_some_and(
            |session| session.send_input(b"printf '%s' \"$JACKIN_AGENT_CODENAME\"; exit\n")
        ),
        "failed to query child codename environment"
    );
    let child_reported_codename = runtime.block_on(async {
        tokio::time::timeout(Duration::from_secs(2), async {
            loop {
                if let Some(SessionEvent::Output { data, .. }) = mux.control.event_rx.recv().await
                    && String::from_utf8_lossy(&data).contains(&codename)
                {
                    break true;
                }
            }
        })
        .await
        .unwrap_or(false)
    });
    anyhow::ensure!(
        child_reported_codename,
        "spawned shell did not receive generated codename"
    );
    let operation =
        jackin_telemetry::root_operation(&jackin_telemetry::operation::TELEMETRY_VALIDATE, &[])
            .map_err(|reason| anyhow::anyhow!("validation operation rejected: {reason:?}"))?;
    jackin_telemetry::emit_event(
        &jackin_telemetry::event::TELEMETRY_VALIDATE,
        jackin_telemetry::FieldSet::default(),
    )
    .map_err(|reason| anyhow::anyhow!("validation event rejected: {reason:?}"))?;
    jackin_telemetry::counter(&jackin_telemetry::metric::TELEMETRY_VALIDATE)
        .add(1, &[])
        .map_err(|reason| anyhow::anyhow!("validation metric rejected: {reason:?}"))?;
    operation.complete(jackin_telemetry::schema::enums::OutcomeValue::Success, None);
    jackin_diagnostics::flush_wire_test_export()?;
    drop(runtime_guard);
    anyhow::ensure!(
        runtime.block_on(testbed.wait_for_all_signals(Duration::from_secs(2))),
        "codename route did not export all three signals"
    );
    anyhow::ensure!(
        testbed.prohibited_value_violations(&[&codename]).is_empty(),
        "generated codename escaped to OTLP"
    );
    jackin_diagnostics::shutdown_capsule_tracing();
    Ok(())
}
