// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RunStatus {
    Active,
    Paused,
    NeedsOperator,
    Accepted,
    Failed,
    Cancelled,
}

impl RunStatus {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Active => "active",
            Self::Paused => "paused",
            Self::NeedsOperator => "needs_operator",
            Self::Accepted => "accepted",
            Self::Failed => "failed",
            Self::Cancelled => "cancelled",
        }
    }

    #[must_use]
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "active" => Some(Self::Active),
            "paused" => Some(Self::Paused),
            "needs_operator" => Some(Self::NeedsOperator),
            "accepted" => Some(Self::Accepted),
            "failed" => Some(Self::Failed),
            "cancelled" => Some(Self::Cancelled),
            _ => None,
        }
    }

    #[must_use]
    pub const fn is_terminal(self) -> bool {
        matches!(self, Self::Accepted | Self::Failed | Self::Cancelled)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Stage {
    Prepare,
    Implement,
    Capture,
    Test,
    Review,
    Decide,
}

impl Stage {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Prepare => "prepare",
            Self::Implement => "implement",
            Self::Capture => "capture",
            Self::Test => "test",
            Self::Review => "review",
            Self::Decide => "decide",
        }
    }

    #[must_use]
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "prepare" => Some(Self::Prepare),
            "implement" => Some(Self::Implement),
            "capture" => Some(Self::Capture),
            "test" => Some(Self::Test),
            "review" => Some(Self::Review),
            "decide" => Some(Self::Decide),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CommandState {
    Queued,
    Dispatching,
    Written,
    TurnEnded,
    DeliveryUnknown,
}

impl CommandState {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Queued => "queued",
            Self::Dispatching => "dispatching",
            Self::Written => "written",
            Self::TurnEnded => "turn_ended",
            Self::DeliveryUnknown => "delivery_unknown",
        }
    }

    #[must_use]
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "queued" => Some(Self::Queued),
            "dispatching" => Some(Self::Dispatching),
            "written" => Some(Self::Written),
            "turn_ended" => Some(Self::TurnEnded),
            "delivery_unknown" => Some(Self::DeliveryUnknown),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EffectKind {
    Launch,
    Capture,
    Backup,
}

impl EffectKind {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Launch => "launch",
            Self::Capture => "capture",
            Self::Backup => "backup",
        }
    }

    #[must_use]
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "launch" => Some(Self::Launch),
            "capture" => Some(Self::Capture),
            "backup" => Some(Self::Backup),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EffectStatus {
    Intended,
    Adopted,
    Dispatched,
    Done,
    Abandoned,
}

impl EffectStatus {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Intended => "intended",
            Self::Adopted => "adopted",
            Self::Dispatched => "dispatched",
            Self::Done => "done",
            Self::Abandoned => "abandoned",
        }
    }

    #[must_use]
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "intended" => Some(Self::Intended),
            "adopted" => Some(Self::Adopted),
            "dispatched" => Some(Self::Dispatched),
            "done" => Some(Self::Done),
            "abandoned" => Some(Self::Abandoned),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Actor {
    Host,
    Worker,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Observation {
    TurnStarted,

    TurnEnded,

    TestStarted,
    TestPassed,
    TestFailed,
    ReviewPassed,
    ReviewFailed,

    InfraFailed,
    SessionLost,
    UncertainDelivery,
    CaptureStalled,

    VerifiedCurrent,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RunView {
    pub status: RunStatus,
    pub stage: Stage,
    pub round: u32,
    pub candidate_generation: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NextState {
    pub status: RunStatus,
    pub stage: Stage,
    pub round: u32,
    pub event_kind: &'static str,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReducerRefusal {
    pub reason: &'static str,
}

pub fn advance(
    view: &RunView,
    max_repair_turns: u32,
    observation: &Observation,
    actor: Actor,
) -> Result<NextState, ReducerRefusal> {
    if view.status.is_terminal() {
        return Err(ReducerRefusal {
            reason: "run is terminal",
        });
    }
    if view.status == RunStatus::NeedsOperator {
        return Err(ReducerRefusal {
            reason: "run needs operator resolution first",
        });
    }
    if view.status == RunStatus::Paused && !matches!(observation, Observation::TurnEnded) {
        return Err(ReducerRefusal {
            reason: "run is paused; only an in-deadline turn may finish",
        });
    }
    match observation {
        Observation::TurnStarted => {
            if view.stage != Stage::Prepare {
                return Err(ReducerRefusal {
                    reason: "turn start outside the prepare stage",
                });
            }
            Ok(NextState {
                status: view.status,
                stage: Stage::Implement,
                round: view.round,
                event_kind: "turn_started",
            })
        }
        Observation::TestStarted => {
            if view.stage != Stage::Capture {
                return Err(ReducerRefusal {
                    reason: "test start outside the capture stage",
                });
            }
            Ok(NextState {
                status: view.status,
                stage: Stage::Test,
                round: view.round,
                event_kind: "test_started",
            })
        }
        Observation::TurnEnded => {
            if view.stage != Stage::Implement {
                return Err(ReducerRefusal {
                    reason: "turn end outside the implement stage",
                });
            }
            Ok(NextState {
                status: view.status,
                stage: Stage::Capture,
                round: view.round,
                event_kind: "turn_ended",
            })
        }
        Observation::TestPassed => {
            if view.stage != Stage::Test {
                return Err(ReducerRefusal {
                    reason: "test result outside the test stage",
                });
            }
            Ok(NextState {
                status: view.status,
                stage: Stage::Review,
                round: view.round,
                event_kind: "test_passed",
            })
        }
        Observation::TestFailed | Observation::ReviewFailed => {
            if !matches!(view.stage, Stage::Test | Stage::Review) {
                return Err(ReducerRefusal {
                    reason: "verification result outside test/review",
                });
            }
            if view.round < max_repair_turns {
                Ok(NextState {
                    status: view.status,
                    stage: Stage::Implement,
                    round: view.round + 1,
                    event_kind: "repair_turn",
                })
            } else {
                Ok(NextState {
                    status: RunStatus::Failed,
                    stage: view.stage,
                    round: view.round,
                    event_kind: "repair_budget_exhausted",
                })
            }
        }
        Observation::ReviewPassed => {
            if view.stage != Stage::Review {
                return Err(ReducerRefusal {
                    reason: "review result outside the review stage",
                });
            }
            Ok(NextState {
                status: view.status,
                stage: Stage::Decide,
                round: view.round,
                event_kind: "review_passed",
            })
        }
        Observation::VerifiedCurrent => {
            if actor != Actor::Host {
                return Err(ReducerRefusal {
                    reason: "only the host can accept a run",
                });
            }
            if view.stage != Stage::Decide {
                return Err(ReducerRefusal {
                    reason: "acceptance requires the decide stage",
                });
            }
            Ok(NextState {
                status: RunStatus::Accepted,
                stage: Stage::Decide,
                round: view.round,
                event_kind: "accepted",
            })
        }
        Observation::InfraFailed => Ok(NextState {
            status: RunStatus::NeedsOperator,
            stage: view.stage,
            round: view.round,
            event_kind: "infra_failed",
        }),
        Observation::SessionLost => Ok(NextState {
            status: RunStatus::NeedsOperator,
            stage: view.stage,
            round: view.round,
            event_kind: "session_lost",
        }),
        Observation::UncertainDelivery => Ok(NextState {
            status: RunStatus::NeedsOperator,
            stage: view.stage,
            round: view.round,
            event_kind: "uncertain_delivery",
        }),
        Observation::CaptureStalled => Ok(NextState {
            status: RunStatus::NeedsOperator,
            stage: view.stage,
            round: view.round,
            event_kind: "capture_stalled",
        }),
    }
}
