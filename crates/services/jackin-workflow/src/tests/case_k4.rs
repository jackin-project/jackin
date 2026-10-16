// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! K4 table-driven checks: reopen saved state, inject a lost prompt
//! receipt, and confirm recovery never resends the prompt.

use super::super::{
    Actor, CommandState, EffectKind, ErrorKind, InstanceInventory, LiveInstance, Observation,
    OperatorDecision,
};
use super::support::Fixture;

#[test]
fn k4_recovery_rows() {
    for row in [
        "normal_recovery",
        "uncertain_delivery",
        "restart_epoch",
        "idempotent_start",
        "capture_pause",
    ] {
        match row {
            "normal_recovery" => normal_recovery(),
            "uncertain_delivery" => uncertain_delivery(),
            "restart_epoch" => restart_epoch(),
            "idempotent_start" => idempotent_start(),
            "capture_pause" => capture_pause(),
            _ => unreachable!("k4 row {row}"),
        }
    }
}

/// Normal case: a fully receipted turn survives a restart byte for
/// byte (owner epoch excepted: reopening bumps it by exactly one).
fn normal_recovery() {
    let fixture = Fixture::new();
    let service = fixture.open();
    let epoch = service.epoch();
    let started = service
        .start_run("ebc1278f", 3, "key-normal")
        .expect("start normal run");
    assert_eq!(started.status, "active");
    assert_eq!(started.stage, "prepare");
    assert_eq!(started.round, 0);

    let (command_id, _digest) = service
        .begin_dispatch(&started.run_id, "session:7", "implement the fix", 0, 0)
        .expect("begin dispatch");
    service
        .command_written(&command_id, 18)
        .expect("record receipt");
    service
        .command_turn_ended(&command_id, "observed")
        .expect("record turn end");
    let implementing = service
        .submit_observation(
            &started.run_id,
            started.version,
            epoch,
            Actor::Host,
            &Observation::TurnStarted,
        )
        .expect("advance on turn start");
    assert_eq!(implementing.stage, "implement");
    let advanced = service
        .submit_observation(
            &started.run_id,
            implementing.version,
            epoch,
            Actor::Host,
            &Observation::TurnEnded,
        )
        .expect("advance on turn end");
    assert_eq!(advanced.stage, "capture");
    drop(service);

    let live = fixture.reopen();
    assert_eq!(live.epoch(), epoch + 1);
    let report = live
        .recover(&InstanceInventory::default())
        .expect("recover normal run");
    assert_eq!(report.run_id.as_deref(), Some(started.run_id.as_str()));
    let saved = live
        .run_summary(Some(&started.run_id))
        .expect("reload saved run");
    assert_eq!(saved.status, advanced.status);
    assert_eq!(saved.stage, advanced.stage);
    assert_eq!(saved.round, advanced.round);
    assert_eq!(saved.version, advanced.version);
    assert_eq!(saved.candidate, advanced.candidate);
    let command = live
        .store()
        .load_command(&command_id)
        .expect("reload command")
        .expect("command survives restart");
    assert_eq!(command.state, CommandState::TurnEnded.as_str());
}

/// Lost receipt: a command stuck in `dispatching` across a restart
/// becomes `delivery_unknown`, blocks automatic input, is never
/// resubmitted, and resumes only through an inspected operator
/// resolution.
fn uncertain_delivery() {
    let fixture = Fixture::new();
    let service = fixture.open();
    let started = service
        .start_run("ebc1278f", 3, "key-uncertain")
        .expect("start uncertain run");
    let (command_id, _digest) = service
        .begin_dispatch(&started.run_id, "session:7", "implement the fix", 0, 0)
        .expect("begin dispatch");
    // The receipt is lost: the process dies before command_written.
    drop(service);

    let live = fixture.reopen();
    let report = live
        .recover(&InstanceInventory::default())
        .expect("recover uncertain run");
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
        .load_command(&command_id)
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
        &command_id,
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
            &command_id,
            OperatorDecision::ContinueFromCurrent,
            "operator",
            "session 7 shows the prompt on screen",
        )
        .expect("resolve inspected input");
    assert_eq!(resolved.status, "active");
    // No duplicate submit: exactly one command row exists and the
    // original stays marked uncertain with its resolution link.
    let dispatching = live
        .store()
        .commands_in_state(&started.run_id, CommandState::Dispatching)
        .expect("list dispatching");
    assert!(dispatching.is_empty(), "recovery must never resubmit");
    let original = live
        .store()
        .load_command(&command_id)
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

/// Restart epoch: reopening bumps the persisted owner epoch, and
/// automatic input stamped with the old epoch is rejected.
fn restart_epoch() {
    let fixture = Fixture::new();
    let service = fixture.open();
    let old_epoch = service.epoch();
    let started = service
        .start_run("ebc1278f", 3, "key-epoch")
        .expect("start epoch run");
    let implementing = service
        .submit_observation(
            &started.run_id,
            started.version,
            old_epoch,
            Actor::Worker,
            &Observation::TurnStarted,
        )
        .expect("current-epoch input applies");
    service
        .submit_observation(
            &started.run_id,
            implementing.version,
            old_epoch,
            Actor::Worker,
            &Observation::TurnEnded,
        )
        .expect("current-epoch turn end applies");
    drop(service);

    let live = fixture.reopen();
    assert_eq!(live.epoch(), old_epoch + 1);
    let current = live
        .run_summary(Some(&started.run_id))
        .expect("reload saved run");
    let stale = live.submit_observation(
        &started.run_id,
        current.version,
        old_epoch,
        Actor::Worker,
        &Observation::TestPassed,
    );
    assert_eq!(
        stale.expect_err("old epoch must be rejected").kind(),
        ErrorKind::StaleEpoch
    );
}

/// Idempotent start: the same key returns the existing run while a
/// different key with an active run returns busy (no queue).
fn idempotent_start() {
    let fixture = Fixture::new();
    let service = fixture.open();
    let first = service
        .start_run("ebc1278f", 3, "key-same")
        .expect("start first run");
    let again = service
        .start_run("ebc1278f", 3, "key-same")
        .expect("same key returns the run");
    assert_eq!(first.run_id, again.run_id);
    let busy = service.start_run("ebc1278f", 3, "key-other");
    assert_eq!(
        busy.expect_err("second key must be busy").kind(),
        ErrorKind::Busy
    );
}

/// Capture-pause recovery (C4 semantics): a capture effect whose
/// instance is left paused escalates to the operator instead of
/// resuming blindly, then resumes after inspection.
fn capture_pause() {
    let fixture = Fixture::new();
    let service = fixture.open();
    let started = service
        .start_run("ebc1278f", 3, "key-capture")
        .expect("start capture run");
    let effect = service
        .plan_effect(&started.run_id, EffectKind::Capture, "worker-1:capture")
        .expect("plan capture");
    service
        .adopt_effect(&started.run_id, &effect.operation_id, "instance-1")
        .expect("adopt capture");
    drop(service);

    let live = fixture.reopen();
    let inventory = InstanceInventory {
        instances: vec![LiveInstance {
            instance_id: "instance-1".to_owned(),
            ownership_label: "worker-1:capture".to_owned(),
            capture_paused: true,
            session_alive: true,
            capsule_epoch: None,
            process_identity: None,
        }],
        backup_refs: Vec::new(),
    };
    let report = live.recover(&inventory).expect("recover capture run");
    assert!(
        report
            .actions
            .iter()
            .any(|action| action.contains("staging paused")),
        "recovery reports paused staging, got: {:?}",
        report.actions
    );
    let saved = live
        .run_summary(Some(&started.run_id))
        .expect("reload saved run");
    assert_eq!(saved.status, "needs_operator");
    let resumed = live
        .resume_run(&started.run_id, "operator")
        .expect("resume after capture inspection");
    assert_eq!(resumed.status, "active");
}
