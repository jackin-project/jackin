// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Recovery-wiring regression: a pending dispatch intent recorded
//! through the fault-test seam (queued->dispatching, no transport)
//! must recover as `delivery_unknown` + `needs_operator` after a
//! restart, resolve only through an inspected operator decision, and
//! never resubmit.

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
    // The seam commits queued->dispatching with no transport send.
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
    // The process dies before any receipt: no command_written call.
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

    // Resume alone never clears uncertainty.
    let blocked = live.resume_run(&started.run_id, "operator");
    assert_eq!(
        blocked.expect_err("resume must block").kind(),
        ErrorKind::Blocked
    );
    // Resolution without a session inspection is refused too.
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
    // An inspected operator decision continues from current state.
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
    // No resubmit: no command returns to dispatching, exactly one
    // command row exists, and the original stays marked uncertain
    // with its resolution link.
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

/// There is no automatic resubmit path: the service source must
/// contain no resubmit call site (only comments name the forbidden
/// behavior). If this trips, the new call site needs removal, not
/// an updated assertion.
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
