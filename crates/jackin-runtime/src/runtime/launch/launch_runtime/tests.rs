// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

fn capsule_config_with(instances: &[(&str, &str)]) -> jackin_protocol::CapsuleConfig {
    let mut config = jackin_protocol::CapsuleConfig::default();
    for (id, agent) in instances {
        config.instances.push((*id).to_owned());
        config.agents.insert((*id).to_owned(), (*agent).to_owned());
    }
    config
}

#[test]
fn initial_argv_prefers_first_matching_instance() {
    let config = capsule_config_with(&[
        ("claude-work", "claude"),
        ("claude-personal", "claude"),
        ("codex-work", "codex"),
    ]);
    assert_eq!(
        initial_daemon_argv(jackin_core::Agent::Claude, &config),
        "claude-work"
    );
    assert_eq!(
        initial_daemon_argv(jackin_core::Agent::Codex, &config),
        "codex-work"
    );
}

#[test]
fn initial_argv_falls_back_to_first_instance_then_slug() {
    let config = capsule_config_with(&[("codex-work", "codex")]);
    assert_eq!(
        initial_daemon_argv(jackin_core::Agent::Claude, &config),
        "codex-work"
    );
    let empty = jackin_protocol::CapsuleConfig::default();
    assert_eq!(
        initial_daemon_argv(jackin_core::Agent::Claude, &empty),
        "claude"
    );
}

#[tokio::test]
async fn sibling_auth_prewarm_join_barrier_waits_for_detached_work() {
    let (started_tx, started_rx) = tokio::sync::oneshot::channel();
    let (release_tx, release_rx) = tokio::sync::oneshot::channel();
    let (finished_tx, mut finished_rx) = tokio::sync::oneshot::channel();
    let prewarm = tokio::spawn(async move {
        started_tx.send(()).unwrap();
        release_rx.await.unwrap();
        finished_tx.send(()).unwrap();
    });

    let mut wait = Box::pin(await_sibling_auth_prewarm(Some(prewarm)));
    tokio::select! {
        result = &mut wait => panic!("prewarm join returned before detached work finished: {result:?}"),
        _ = started_rx => {}
    }
    assert!(
        finished_rx.try_recv().is_err(),
        "detached prewarm must still be running before its release gate"
    );

    release_tx.send(()).unwrap();
    wait.await.unwrap();
    finished_rx.await.unwrap();
}
