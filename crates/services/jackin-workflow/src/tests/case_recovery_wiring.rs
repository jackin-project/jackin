// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::super::{CommandState, ErrorKind, InstanceInventory, OperatorDecision};
use super::support::Fixture;

#[test]
fn restart_with_pending_dispatch_recovers_uncertain_and_needs_operator() {
    let fixture = Fixture::new();
    let service = fixture.open();
    let started = service
        .start_run("ebc1278f", 3, "key-dispatch-wire")
        .expect("start dispatch run");
    let (_summary, dispatch) = service
        .record_dispatch_intent(&started.run_id, "session:7", "implement the fix", 0, 0)
        .expect("record dispatch intent");
    let command = service
        .store()
        .load_command(&dispatch.command_id)
        .expect("reload intent")
        .expect("intent row exists");
    assert_eq!(command.state, CommandState::Dispatching.as_str());
    let events = service
        .store()
        .load_events(&started.run_id, -1, 50)
        .expect("reload events");
    let kinds: Vec<&str> = events.iter().map(|event| event.kind.as_str()).collect();
    assert!(
        kinds.contains(&"command_queued"),
        "intent records the queued row, got: {kinds:?}"
    );
    drop(service);

    let live = fixture.reopen();
    let report = live
        .recover(&InstanceInventory::default())
        .expect("recover pending dispatch");
    assert!(
        report
            .actions
            .iter()
            .any(|action| action.contains("delivery_unknown")),
        "recovery marks the interrupted dispatch, got: {:?}",
        report.actions
    );
    let command = live
        .store()
        .load_command(&dispatch.command_id)
        .expect("reload command")
        .expect("command survives restart");
    assert_eq!(command.state, CommandState::DeliveryUnknown.as_str());
    let saved = live
        .run_summary(Some(&started.run_id))
        .expect("reload saved run");
    assert_eq!(saved.status, "needs_operator");

    let blocked = live.resume_run(&started.run_id, "operator");
    assert_eq!(
        blocked.expect_err("resume must block").kind(),
        ErrorKind::Blocked
    );
    let uninspected = live.resolve_input(
        &started.run_id,
        &dispatch.command_id,
        OperatorDecision::ContinueFromCurrent,
        "operator",
        "  ",
    );
    assert_eq!(
        uninspected
            .expect_err("uninspected resolve must block")
            .kind(),
        ErrorKind::Blocked
    );
    let resolved = live
        .resolve_input(
            &started.run_id,
            &dispatch.command_id,
            OperatorDecision::ContinueFromCurrent,
            "operator",
            "session 7 shows the prompt on screen",
        )
        .expect("resolve inspected input");
    assert_eq!(resolved.status, "active");
    let dispatching = live
        .store()
        .commands_in_state(&started.run_id, CommandState::Dispatching)
        .expect("list dispatching");
    assert!(dispatching.is_empty(), "recovery must never resubmit");
    let queued = live
        .store()
        .commands_in_state(&started.run_id, CommandState::Queued)
        .expect("list queued");
    assert!(queued.is_empty(), "recovery must not requeue either");
    let original = live
        .store()
        .load_command(&dispatch.command_id)
        .expect("reload original")
        .expect("original survives");
    assert_eq!(original.state, CommandState::DeliveryUnknown.as_str());
    assert!(
        original
            .resolution
            .is_some_and(|resolution| resolution.contains("continue_from_current")),
        "original keeps its resolution link"
    );
}

#[test]
fn no_auto_resubmit_call_site() {
    let source = include_str!("../service.rs");
    for (number, line) in source.lines().enumerate() {
        let stripped = line.trim_start();
        if stripped.starts_with("//") || stripped.starts_with("///") || stripped.starts_with("//!")
        {
            continue;
        }
        assert!(
            !line.contains("resubmit("),
            "service.rs line {} is a resubmit call site: {line}",
            number + 1
        );
    }
}
