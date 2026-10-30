// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use crate::{Actor, NextState, Observation, RunStatus, RunView, Stage, advance};

fn view(status: RunStatus, stage: Stage) -> RunView {
    RunView {
        status,
        stage,
        round: 3,
        candidate_generation: 0,
    }
}

#[test]
fn turn_start_advances_prepare_to_implement() {
    assert_eq!(
        advance(
            &view(RunStatus::Active, Stage::Prepare),
            3,
            &Observation::TurnStarted,
            Actor::Host
        ),
        Ok(NextState {
            status: RunStatus::Active,
            stage: Stage::Implement,
            round: 3,
            event_kind: "turn_started",
        })
    );
}

#[test]
fn terminal_paused_and_blocked_runs_refuse_advance() {
    assert!(
        advance(
            &view(RunStatus::Accepted, Stage::Implement),
            3,
            &Observation::TurnStarted,
            Actor::Host
        )
        .is_err(),
        "terminal accepted run must refuse advance"
    );
    assert!(
        advance(
            &view(RunStatus::NeedsOperator, Stage::Implement),
            3,
            &Observation::TurnEnded,
            Actor::Host
        )
        .is_err(),
        "operator-blocked run must refuse advance"
    );
    assert!(
        advance(
            &view(RunStatus::Paused, Stage::Prepare),
            3,
            &Observation::TurnStarted,
            Actor::Host
        )
        .is_err(),
        "paused run must refuse advance"
    );
}
