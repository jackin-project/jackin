// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin_protocol::usage_broker::{
    UsageAccountCapability, UsageBrokerOperation, UsageBrokerRequest, UsageBrokerResponse,
    UsageGenerationView, UsageRefreshPhase,
};
use tokio::io::{AsyncBufReadExt as _, AsyncWriteExt as _};

async fn respond(
    mut stream: tokio::net::UnixStream,
    capability: UsageAccountCapability,
    generation: u64,
    phase: UsageRefreshPhase,
) {
    let response = UsageBrokerResponse::State {
        state: Box::new(UsageGenerationView {
            capability,
            generation,
            phase,
            snapshot: None,
            error: None,
            retry_at_epoch: None,
        }),
    };
    let mut bytes = serde_json::to_vec(&response).unwrap();
    bytes.push(b'\n');
    stream.write_all(&bytes).await.unwrap();
    stream.shutdown().await.unwrap();
}

#[test]
fn scoped_refresh_admits_all_targets_and_completes_fast_join_while_first_is_held() {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    runtime.block_on(async {
        let temp = tempfile::tempdir().unwrap();
        let socket = temp.path().join("usage.sock");
        let listener = tokio::net::UnixListener::bind(&socket).unwrap();
        // One account forwarded by two instances must retain both credential scopes.
        let capability = UsageAccountCapability {
            account_id: "same-account".to_owned(),
            surface_id: "codex".to_owned(),
        };
        let slow = crate::usage::UsageRefreshTarget {
            instance_id: "a-slow".to_owned(),
            agent: "codex".to_owned(),
            provider: None,
            capability: capability.clone(),
        };
        let fast = crate::usage::UsageRefreshTarget {
            instance_id: "z-fast".to_owned(),
            ..slow.clone()
        };
        let client =
            jackin_usage::host::UsageBrokerClient::at(socket, env!("CARGO_PKG_VERSION").to_owned());
        let (release_slow, slow_release) = tokio::sync::oneshot::channel();
        let (fast_completed, fast_completion) = tokio::sync::oneshot::channel();
        let slow_started = std::sync::Arc::new(tokio::sync::Notify::new());
        let server = tokio::spawn(async move {
            let mut admitted = std::collections::BTreeSet::new();
            let mut stages = std::collections::BTreeMap::<String, [bool; 3]>::new();
            let mut slow_release = Some(slow_release);
            let mut fast_completed = Some(fast_completed);
            let mut joins = Vec::new();
            for _ in 0..6 {
                let (stream, _) = listener.accept().await.unwrap();
                let mut reader = tokio::io::BufReader::new(stream);
                let mut line = String::new();
                reader.read_line(&mut line).await.unwrap();
                let request: UsageBrokerRequest = serde_json::from_str(&line).unwrap();
                let stream = reader.into_inner();
                let (instance_id, capability, stage) = match request.operation {
                    UsageBrokerOperation::CurrentForCapability {
                        instance_id,
                        capability,
                    } => (instance_id, capability, 0),
                    UsageBrokerOperation::RefreshForCapability {
                        instance_id,
                        capability,
                        observed_generation,
                        force,
                    } => {
                        assert_eq!(observed_generation, 0);
                        assert_eq!(force, instance_id == "z-fast");
                        admitted.insert(instance_id.clone());
                        (instance_id, capability, 1)
                    }
                    UsageBrokerOperation::JoinForCapability {
                        instance_id,
                        capability,
                        generation,
                        timeout_ms,
                    } => {
                        assert_eq!(generation, 1);
                        assert_eq!(timeout_ms, 30_000);
                        assert_eq!(admitted.len(), 2, "all scopes admitted before joining");
                        (instance_id, capability, 2)
                    }
                    operation => panic!("unexpected operation: {operation:?}"),
                };
                assert_eq!(capability.account_id, "same-account");
                assert!(matches!(instance_id.as_str(), "a-slow" | "z-fast"));
                let seen = &mut stages.entry(instance_id.clone()).or_default()[stage];
                assert!(!*seen, "duplicate scope operation");
                *seen = true;
                match stage {
                    0 => respond(stream, capability, 0, UsageRefreshPhase::Idle).await,
                    1 => respond(stream, capability, 1, UsageRefreshPhase::Queued).await,
                    _ if instance_id == "a-slow" => {
                        let release = slow_release.take().unwrap();
                        slow_started.notify_one();
                        joins.push(tokio::spawn(async move {
                            release.await.unwrap();
                            respond(stream, capability, 1, UsageRefreshPhase::Completed).await;
                        }));
                    }
                    _ => {
                        let completed = fast_completed.take().unwrap();
                        let started = slow_started.clone();
                        joins.push(tokio::spawn(async move {
                            started.notified().await;
                            respond(stream, capability, 1, UsageRefreshPhase::Completed).await;
                            completed.send(()).unwrap();
                        }));
                    }
                }
            }
            for join in joins {
                join.await.unwrap();
            }
            assert_eq!(stages.len(), 2);
            assert!(stages.values().all(|seen| *seen == [true; 3]));
        });
        let worker = tokio::task::spawn_blocking(move || {
            super::super::multiplexer_utils::refresh_usage_targets_with_client(
                &client,
                vec![slow.clone(), fast.clone(), slow.clone(), fast.clone()],
                Some(fast.clone()),
                Some(&fast),
            )
        });
        // The timeout is a deadlock watchdog; the oracle is explicit broker completion
        // while the slow response remains gated by an unreleased channel.
        let independent_completion =
            tokio::time::timeout(std::time::Duration::from_secs(5), fast_completion).await;
        let _ = release_slow.send(());
        let refreshes = worker.await.unwrap();
        server.await.unwrap();
        independent_completion.unwrap().unwrap();
        assert_eq!(refreshes.len(), 2);
        assert_eq!(refreshes[0].target.instance_id, "a-slow");
        assert_eq!(refreshes[1].target.instance_id, "z-fast");
        assert!(refreshes.iter().all(|refresh| {
            refresh.result.as_ref().unwrap().phase == UsageRefreshPhase::Completed
        }));
    });
}
