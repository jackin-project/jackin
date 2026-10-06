// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn daemon_pty_lifecycle_reaches_shutdown_after_last_session_exit() -> Result<()> {
    use tokio::io::{AsyncReadExt as _, AsyncWriteExt as _};
    use tokio::net::UnixStream;
    use tokio::time::{Duration, timeout};

    let _telemetry_guard = crate::support::telemetry_test_guard_async().await;
    let root = tempfile::tempdir()?;
    let workdir = root.path().join("workspace");
    std::fs::create_dir(&workdir)?;
    let socket_path = root.path().join("run/jackin.sock");
    let config = CapsuleConfig {
        role: "lifecycle-test".to_owned(),
        workdir: workdir.display().to_string(),
        shell_identity: Some(jackin_protocol::SessionIdentity {
            uid: 2_000,
            gid: 2_000,
        }),
        ..CapsuleConfig::default()
    };
    let daemon_socket = socket_path.clone();
    let daemon = tokio::spawn(async move {
        let mut telemetry = crate::telemetry::init()?;
        run_daemon_for_test(String::new(), config, &mut telemetry, &daemon_socket).await
    });

    let lifecycle = async {
        let mut client = timeout(Duration::from_secs(5), async {
            loop {
                match UnixStream::connect(&socket_path).await {
                    Ok(stream) => break stream,
                    Err(_) => tokio::time::sleep(Duration::from_millis(10)).await,
                }
            }
        })
        .await?;

        jackin_protocol::capsule_transport::client_handshake_async(&mut client).await?;

        client
            .write_all(&crate::protocol::attach::encode_client(
                ClientFrame::Hello {
                    rows: 24,
                    cols: 80,
                    spawn: None,
                    env: Vec::new(),
                    terminal: ClientTerminal::default(),
                    focus_session: None,
                    context: None,
                },
            )?)
            .await?;

        let mut saw_welcome = false;
        let mut initial_output = Vec::new();
        while !saw_welcome {
            let mut tag = [0u8; 1];
            client.read_exact(&mut tag).await?;
            let frame = read_server_frame(&mut client, tag[0])
                .await?
                .ok_or_else(|| anyhow::anyhow!("daemon closed before Welcome"))?;
            match frame {
                ServerFrame::Welcome { session_count } => {
                    assert_eq!(session_count, 1);
                    saw_welcome = true;
                }
                ServerFrame::Output(bytes) => initial_output.extend(bytes),
                other => anyhow::bail!("unexpected pre-Welcome frame: {other:?}"),
            }
        }

        let mut output = initial_output;
        let mut saw_sentinel = contains_lifecycle_sentinel(&output);
        client
            .write_all(&crate::protocol::attach::encode_client(
                ClientFrame::Input(
                    b"printf 'CAPSULE_LIFECYCLE_SENTINEL\\n'; read -r _; exit\n".to_vec(),
                ),
            )?)
            .await?;

        timeout(Duration::from_secs(5), async {
            while !saw_sentinel {
                let mut tag = [0u8; 1];
                client.read_exact(&mut tag).await?;
                let frame = read_server_frame(&mut client, tag[0])
                    .await?
                    .ok_or_else(|| anyhow::anyhow!("daemon closed before shell output"))?;
                saw_sentinel = lifecycle_output_frame(frame, &mut output)?;
            }
            anyhow::Ok(())
        })
        .await
        .map_err(|_| {
            anyhow::anyhow!(
                "timed out waiting for shell output; output so far: {:?}",
                String::from_utf8_lossy(&output)
            )
        })??;

        client
            .write_all(&crate::protocol::attach::encode_client(
                ClientFrame::Input(b"\n".to_vec()),
            )?)
            .await?;

        let mut saw_shutdown = false;
        timeout(Duration::from_secs(5), async {
            while !saw_shutdown {
                let mut tag = [0u8; 1];
                client.read_exact(&mut tag).await?;
                let frame = read_server_frame(&mut client, tag[0])
                    .await?
                    .ok_or_else(|| anyhow::anyhow!("daemon closed before Shutdown"))?;
                saw_shutdown = lifecycle_shutdown_frame(frame, &mut output)?;
            }
            anyhow::Ok(())
        })
        .await
        .map_err(|_| {
            anyhow::anyhow!(
                "timed out waiting for daemon Shutdown; output so far: {:?}",
                String::from_utf8_lossy(&output)
            )
        })??;

        assert!(
            output
                .windows(b"CAPSULE_LIFECYCLE_SENTINEL".len())
                .any(|window| window == b"CAPSULE_LIFECYCLE_SENTINEL"),
            "shell output did not reach the attached client: {:?}",
            String::from_utf8_lossy(&output)
        );
        Ok::<(), anyhow::Error>(())
    }
    .await;

    if let Err(error) = lifecycle {
        daemon.abort();
        drop(daemon.await);
        return Err(error);
    }

    let daemon_result = timeout(Duration::from_secs(5), daemon).await??;
    daemon_result?;
    Ok(())
}

#[test]
fn begin_exec_picker_supersedes_pending_reply_and_dialog() {
    let mut mux = test_mux(40, 20);
    let (tx1, mut rx1) = tokio::sync::oneshot::channel();
    mux.begin_exec_picker("cmd1".to_owned(), vec![], tx1, None);

    // A second jackin-exec request arrives while the first picker is pending.
    let (tx2, _rx2) = tokio::sync::oneshot::channel();
    mux.begin_exec_picker("cmd2".to_owned(), vec![], tx2, None);

    // The prior client must get a structured denial, not a hung/closed socket.
    match rx1.try_recv() {
        Ok(ControlResponse {
            msg: ServerMsg::ExecDenied { reason },
            ..
        }) => {
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

#[test]
fn broker_client_capsule_deduplicates_each_account_and_adopts_terminal_generation() {
    use std::collections::{BTreeMap, BTreeSet};
    use std::io::{BufRead as _, BufReader, Write as _};
    use std::os::unix::net::UnixListener;

    use jackin_protocol::control::{
        FocusedUsageView, QuotaBucketView, StatusSlot, UsageConfidence, UsageSeverity,
        UsageSnapshotStatus, UsageSource,
    };
    use jackin_protocol::usage_broker::{
        USAGE_BROKER_PROTOCOL_VERSION, UsageAccountCapability, UsageBrokerOperation,
        UsageBrokerRequest, UsageBrokerResponse, UsageGenerationView, UsageRefreshPhase,
    };

    let temp = tempfile::tempdir().unwrap();
    let socket = temp.path().join("usage.sock");
    let listener = UnixListener::bind(&socket).unwrap();
    let capability = UsageAccountCapability {
        account_id: "allowed-a".to_owned(),
        surface_id: "codex".to_owned(),
    };
    let second_capability = UsageAccountCapability {
        account_id: "allowed-b".to_owned(),
        surface_id: "codex".to_owned(),
    };
    let server_capabilities = BTreeSet::from([capability.clone(), second_capability.clone()]);
    let server = std::thread::spawn(move || {
        let mut seen = BTreeMap::<UsageAccountCapability, [bool; 3]>::new();
        for _ in 0..6 {
            let (mut stream, _) = listener.accept().unwrap();
            let request = {
                let mut line = String::new();
                BufReader::new(&mut stream).read_line(&mut line).unwrap();
                serde_json::from_str::<UsageBrokerRequest>(line.trim()).unwrap()
            };
            assert_eq!(request.protocol_version, USAGE_BROKER_PROTOCOL_VERSION);
            let (request_capability, generation, phase, snapshot, stage) = match request.operation {
                UsageBrokerOperation::CurrentForCapability { capability } => {
                    (capability, 0, UsageRefreshPhase::Idle, None, 0)
                }
                UsageBrokerOperation::RefreshForCapability {
                    capability,
                    observed_generation,
                    ..
                } => {
                    assert_eq!(observed_generation, 0);
                    (capability, 1, UsageRefreshPhase::Queued, None, 1)
                }
                UsageBrokerOperation::JoinForCapability {
                    capability,
                    generation,
                    ..
                } => {
                    assert_eq!(generation, 1);
                    let mut view = FocusedUsageView::unavailable("fixture", 1);
                    view.status = UsageSnapshotStatus::Fresh;
                    view.source = UsageSource::ProviderApi;
                    view.confidence = UsageConfidence::Authoritative;
                    view.account.provider_label = "OpenAI / Codex".to_owned();
                    view.account.account_label =
                        format!("{}@capsule.example.test", capability.account_id);
                    view.buckets = vec![QuotaBucketView {
                        label: "Weekly".to_owned(),
                        used_label: None,
                        limit_label: None,
                        remaining_percent: Some(71),
                        reset_label: None,
                        resets_at: None,
                        status_slot: Some(StatusSlot::Weekly),
                        pace_label: None,
                        status: UsageSnapshotStatus::Fresh,
                        used_money: None,
                        limit_money: None,
                        severity: UsageSeverity::Normal,
                    }];
                    (capability, 1, UsageRefreshPhase::Completed, Some(view), 2)
                }
                operation => panic!("unexpected relay operation: {operation:?}"),
            };
            seen.entry(request_capability.clone()).or_default()[stage] = true;
            let response = UsageBrokerResponse::State {
                state: Box::new(UsageGenerationView {
                    capability: request_capability,
                    generation,
                    phase,
                    snapshot,
                    error: None,
                    retry_at_epoch: None,
                }),
            };
            let mut bytes = serde_json::to_vec(&response).unwrap();
            bytes.push(b'\n');
            stream.write_all(&bytes).unwrap();
        }
        assert_eq!(
            seen.keys().cloned().collect::<BTreeSet<_>>(),
            server_capabilities
        );
        assert!(seen.values().all(|stages| stages == &[true, true, true]));
    });
    let client =
        jackin_usage::host::UsageBrokerClient::at(socket, env!("CARGO_PKG_VERSION").to_owned());
    let target = crate::usage::UsageRefreshTarget {
        agent: "codex".to_owned(),
        provider: Some("OpenAI".to_owned()),
        capability: capability.clone(),
    };
    let second_target = crate::usage::UsageRefreshTarget {
        capability: second_capability.clone(),
        ..target.clone()
    };

    let refreshes = multiplexer_utils::refresh_usage_targets_with_client(
        &client,
        vec![
            target.clone(),
            second_target.clone(),
            target.clone(),
            second_target.clone(),
        ],
        Some(target.clone()),
        Some(&target),
    );
    server.join().unwrap();

    assert_eq!(refreshes.len(), 2);
    let states = refreshes
        .into_iter()
        .map(|refresh| {
            let state = refresh.result.unwrap();
            (refresh.target.capability, state)
        })
        .collect::<BTreeMap<_, _>>();
    assert_eq!(states.len(), 2);
    for capability in [capability, second_capability] {
        let state = states.get(&capability).expect("account state");
        assert_eq!(state.phase, UsageRefreshPhase::Completed);
        assert_eq!(
            state.snapshot.as_ref().unwrap().account.account_label,
            format!("{}@capsule.example.test", capability.account_id)
        );
    }
}

#[test]
fn socket_peer_credentials_scope_session_controls_and_attach() {
    let mut mux = single_pane_tab_mux();
    let own = jackin_protocol::SessionIdentity {
        uid: 2_101,
        gid: 2_101,
    };
    let sibling = jackin_protocol::SessionIdentity {
        uid: 2_102,
        gid: 2_102,
    };
    mux.launch_env.launch_config.instance_identities =
        BTreeMap::from([("own".to_owned(), own), ("sibling".to_owned(), sibling)]);
    let (mut own_session, _own_rx) = test_session_with_agent(24, 80, Some("codex".to_owned()));
    own_session.identity = own;
    let (mut sibling_session, _sibling_rx) =
        test_session_with_agent(24, 80, Some("claude".to_owned()));
    sibling_session.identity = sibling;
    mux.session_supervisor.sessions.insert(1, own_session);
    mux.session_supervisor.sessions.insert(2, sibling_session);
    let own_capability = mux
        .session_supervisor
        .sessions
        .get(1)
        .expect("own session")
        .control_capability
        .clone();
    let sibling_capability = mux
        .session_supervisor
        .sessions
        .get(2)
        .expect("sibling session")
        .control_capability
        .clone();

    assert!(
        attach_peer_is_authorized(&mux, Some(0)),
        "operator attach stays valid"
    );
    assert!(
        !attach_peer_is_authorized(&mux, Some(own.uid)),
        "an admitted session UID cannot attach"
    );
    assert!(
        !attach_peer_is_authorized(&mux, None),
        "missing peer credentials fail closed"
    );

    assert_session_peer_authorization(&mux, own, &own_capability, &sibling_capability);
    assert_operator_and_unknown_peer_authorization(&mux);
}
