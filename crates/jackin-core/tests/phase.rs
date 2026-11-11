// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin_core::Phase;

type Row = (bool, bool, Option<i32>, bool);

fn expected(phase: Phase) -> Row {
    match phase {
        Phase::Provisioning | Phase::Starting | Phase::Stopping | Phase::Deleting => {
            (false, false, None, false)
        }
        Phase::Suspending | Phase::Unknown => (false, false, None, false),
        Phase::Ready => (true, false, None, true),
        Phase::Stopped | Phase::Suspended => (false, false, None, true),
        Phase::Completed { exit_code } => (false, true, Some(exit_code), true),
        Phase::Error { exit_code } => (false, true, exit_code, true),
    }
}

#[test]
fn phase_predicates_follow_the_gateway_mapping() {
    let phases = [
        Phase::Provisioning,
        Phase::Starting,
        Phase::Ready,
        Phase::Stopping,
        Phase::Stopped,
        Phase::Completed { exit_code: 0 },
        Phase::Completed { exit_code: 3 },
        Phase::Error { exit_code: Some(3) },
        Phase::Error { exit_code: None },
        Phase::Deleting,
        Phase::Suspending,
        Phase::Suspended,
        Phase::Unknown,
    ];
    for phase in phases {
        let actual = (
            phase.accepts_exec(),
            phase.main_process_ended(),
            phase.exit_code(),
            phase.is_settled(),
        );
        assert_eq!(actual, expected(phase), "{phase:?}");
    }
}

#[test]
fn every_phase_prints_its_words() {
    let table = [
        (Phase::Provisioning, "provisioning"),
        (Phase::Starting, "starting"),
        (Phase::Ready, "ready"),
        (Phase::Stopping, "stopping"),
        (Phase::Stopped, "stopped"),
        (
            Phase::Completed { exit_code: 0 },
            "completed with exit code 0",
        ),
        (
            Phase::Error { exit_code: Some(3) },
            "error with exit code 3",
        ),
        (
            Phase::Error { exit_code: None },
            "error without an exit code",
        ),
        (Phase::Deleting, "deleting"),
        (Phase::Suspending, "suspending"),
        (Phase::Suspended, "suspended"),
        (Phase::Unknown, "unknown"),
    ];
    for (phase, words) in table {
        assert_eq!(phase.to_string(), words);
    }
}
