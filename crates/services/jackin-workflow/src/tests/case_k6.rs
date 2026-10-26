// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::support::Fixture;
use crate::{GRAPH_STAGES, SessionAttachment, graph_node_state};

#[test]
fn k6_list_rows() {
    let fixture = Fixture::new();
    let service = fixture.open();
    assert!(service.list_runs(50).expect("list empty").is_empty());

    let first = service
        .start_run("plan-a", 3, "key-k6-a")
        .expect("start first");
    let same = service
        .start_run("plan-a", 3, "key-k6-a")
        .expect("idempotent start");
    assert_eq!(first.run_id, same.run_id);
    let busy = service.start_run("plan-b", 3, "key-k6-b");
    assert!(busy.is_err(), "second start must be busy");

    let listed = service.list_runs(50).expect("list one");
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].run_id, first.run_id);
    assert_eq!(listed[0].plan_hash, "plan-a");
    assert_eq!(service.list_runs(1).expect("bounded").len(), 1);
}

#[test]
fn k6_detail_carries_sessions_effects_and_evidence() {
    let fixture = Fixture::new();
    let service = fixture.open();
    let started = service
        .start_run("plan-detail", 3, "key-k6-detail")
        .expect("start detail run");
    service
        .store()
        .record_session(&SessionAttachment {
            run_id: &started.run_id,
            worker_id: "implementer",
            instance_id: Some("inst-9"),
            capsule_epoch: Some(2),
            session_id: Some(7),
            process_identity: Some("pid:9"),
            agent_profile: None,
            auth_profile: None,
        })
        .expect("record session");

    let detail = service
        .run_detail(&started.run_id, -1, 200)
        .expect("run detail");
    assert_eq!(detail.summary.run_id, started.run_id);
    assert_eq!(detail.sessions.len(), 1);
    assert_eq!(detail.sessions[0].worker_id, "implementer");
    assert_eq!(detail.sessions[0].session_id, Some(7));
    assert!(
        detail.effects.iter().any(|effect| effect.kind == "launch"),
        "launch effect recorded, got: {:?}",
        detail.effects
    );
    assert!(
        detail
            .events
            .iter()
            .any(|event| event.kind == "run_started"),
        "run_started event recorded, got: {:?}",
        detail.events
    );

    let last = detail
        .events
        .iter()
        .map(|event| event.seq)
        .max()
        .unwrap_or(-1);
    let tail = service
        .store()
        .load_events(&started.run_id, last, 200)
        .expect("cursor tail");
    assert!(tail.is_empty(), "cursor past the end reads nothing");

    let missing = service.run_detail("run-does-not-exist", -1, 200);
    assert_eq!(
        missing.expect_err("unknown run").kind(),
        crate::ErrorKind::NotFound
    );
}

#[test]
fn k6_graph_states_follow_the_fixed_stages() {
    assert_eq!(GRAPH_STAGES.len(), 6);
    assert_eq!(graph_node_state("prepare", "test"), "done");
    assert_eq!(graph_node_state("test", "test"), "current");
    assert_eq!(graph_node_state("review", "test"), "pending");
    assert_eq!(graph_node_state("decide", "prepare"), "pending");
    assert_eq!(graph_node_state("bogus", "test"), "unknown");
}
