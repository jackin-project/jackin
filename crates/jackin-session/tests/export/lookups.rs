// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::error::Error;
use std::time::Duration;

use jackin_core::{Labels, Phase, SandboxName};
use jackin_openshell::{Gateway, GatewayError, GatewayOp};
use jackin_session::{
    PurgeError, PurgeMode, PurgeOutcome, RECOVERY_LOOKUP_BUDGET, UnknownReason, WorkState,
};
use jackin_testkit::{Fault, Rig, TestResult, Trigger, default_limits, guard_within};
use tokio_util::sync::CancellationToken;

use super::tap::{Lookup, Tap};

const WITHIN: Duration = Duration::from_secs(30);

fn lost(op: GatewayOp) -> GatewayError {
    GatewayError::Timeout {
        op,
        after: Duration::from_secs(30),
        group: None,
    }
}

async fn launched() -> Result<(Rig, SandboxName), Box<dyn Error>> {
    let rig = Rig::new().await?;
    rig.launch(1).await?;
    let name = rig.sandbox_name(1)?;
    Ok((rig, name))
}

fn elapsed(
    before: &jackin_openshell::SandboxStamp,
    after: &jackin_openshell::SandboxStamp,
) -> Result<Duration, Box<dyn Error>> {
    let nanos = (i128::from(after.seconds) - i128::from(before.seconds)) * 1_000_000_000
        + i128::from(after.nanos)
        - i128::from(before.nanos);
    Ok(Duration::from_nanos(u64::try_from(nanos)?))
}

fn is_cancelled(result: &Result<PurgeOutcome, PurgeError>, name: &SandboxName) -> bool {
    matches!(result, Err(PurgeError::Cancelled { sandbox }) if sandbox == name)
}

#[tokio::test]
async fn a_cancelled_purge_deletes_nothing_wherever_the_cancel_lands() -> TestResult {
    guard_within(WITHIN, async {
        let (rig, name) = launched().await?;
        let token = CancellationToken::new();
        token.cancel();
        let before_any_lookup = rig
            .world
            .session
            .purge(&rig.config, &name, PurgeMode::Forced, &token)
            .await;
        assert!(
            is_cancelled(&before_any_lookup, &name),
            "{before_any_lookup:?}"
        );
        assert!(rig.gateway().sandbox(&name).is_some());

        let (rig, name) = launched().await?;
        let hold = Trigger::Op(GatewayOp::Program);
        rig.gateway().inject(Fault::HoldAt {
            trigger: hold.clone(),
        });
        let token = CancellationToken::new();
        let purging = rig
            .world
            .session
            .purge(&rig.config, &name, PurgeMode::Forced, &token);
        let driver = async {
            assert!(rig.gateway().until_held(&hold).await);
            token.cancel();
        };
        let (during_inspection, ()) = tokio::join!(purging, driver);
        assert!(
            is_cancelled(&during_inspection, &name),
            "{during_inspection:?}"
        );
        assert!(rig.gateway().sandbox(&name).is_some());

        let (rig, name) = launched().await?;
        rig.gateway().inject(Fault::HoldAt {
            trigger: hold.clone(),
        });
        let token = CancellationToken::new();
        let purging = rig
            .world
            .session
            .purge(&rig.config, &name, PurgeMode::Forced, &token);
        let driver = async {
            assert!(rig.gateway().until_held(&hold).await);
            rig.gateway().inject(Fault::CancelAt {
                trigger: Trigger::Op(GatewayOp::Find),
                token: token.clone(),
            });
            rig.gateway().release(&hold);
        };
        let (at_the_second_lookup, ()) = tokio::join!(purging, driver);
        assert!(
            is_cancelled(&at_the_second_lookup, &name),
            "{at_the_second_lookup:?}"
        );
        assert!(rig.gateway().sandbox(&name).is_some());
        Ok(())
    })
    .await
}

#[tokio::test]
async fn a_delete_that_was_already_requested_is_not_abandoned_by_a_cancel() -> TestResult {
    guard_within(WITHIN, async {
        let (rig, name) = launched().await?;
        let hold = Trigger::Op(GatewayOp::Delete);
        rig.gateway().inject(Fault::HoldAt {
            trigger: hold.clone(),
        });
        let token = CancellationToken::new();
        let purging = rig
            .world
            .session
            .purge(&rig.config, &name, PurgeMode::Forced, &token);
        let driver = async {
            assert!(rig.gateway().until_held(&hold).await);
            token.cancel();
            for _ in 0..8 {
                tokio::task::yield_now().await;
            }
            assert!(
                rig.gateway().sandbox(&name).is_some(),
                "the held delete must not have landed yet"
            );
            rig.gateway().release(&hold);
        };
        let (purged, ()) = tokio::join!(purging, driver);
        assert!(
            matches!(&purged, Ok(PurgeOutcome::Deleted { sandbox, .. }) if *sandbox == name),
            "{purged:?}"
        );
        assert!(rig.gateway().sandbox(&name).is_none());
        Ok(())
    })
    .await
}

#[tokio::test]
async fn a_purge_of_a_sandbox_jackin_does_not_manage_says_so_and_deletes_nothing() -> TestResult {
    guard_within(WITHIN, async {
        let rig = Rig::new().await?;
        let ghost = SandboxName::parse("jackin-ghost-0000")?;
        rig.gateway()
            .insert_unmanaged(&ghost, Labels::from_pairs([])?, Phase::Ready)?;
        for mode in [PurgeMode::Guarded, PurgeMode::Forced] {
            let result = rig
                .world
                .session
                .purge(&rig.config, &ghost, mode, &CancellationToken::new())
                .await;
            assert!(
                matches!(&result, Err(PurgeError::NotManaged { sandbox }) if *sandbox == ghost),
                "{mode:?}: {result:?}"
            );
            assert!(rig.gateway().sandbox(&ghost).is_some());
        }
        Ok(())
    })
    .await
}

#[tokio::test]
async fn a_sandbox_that_becomes_unmanaged_between_the_two_lookups_is_not_called_gone() -> TestResult
{
    guard_within(WITHIN, async {
        let (rig, name) = launched().await?;
        let hold = Trigger::Op(GatewayOp::Program);
        rig.gateway().inject(Fault::HoldAt {
            trigger: hold.clone(),
        });
        let idle = CancellationToken::new();
        let purging = rig
            .world
            .session
            .purge(&rig.config, &name, PurgeMode::Forced, &idle);
        let swapping = async {
            assert!(rig.gateway().until_held(&hold).await);
            rig.gateway().delete_sandbox(&name).await?;
            rig.gateway()
                .insert_unmanaged(&name, Labels::from_pairs([])?, Phase::Ready)?;
            rig.gateway().release(&hold);
            Ok::<(), Box<dyn Error>>(())
        };
        let (result, swapped) = tokio::join!(purging, swapping);
        swapped?;
        assert!(
            matches!(&result, Err(PurgeError::Replaced { sandbox }) if *sandbox == name),
            "{result:?}"
        );
        assert!(rig.gateway().sandbox(&name).is_some());
        Ok(())
    })
    .await
}

#[tokio::test]
async fn a_gateway_that_lists_two_sandboxes_for_one_name_gets_nothing_deleted() -> TestResult {
    guard_within(WITHIN, async {
        let (rig, name) = launched().await?;
        let tap = Tap::new(&rig);
        tap.list_each_sandbox_twice_by_name();
        let session = tap.session(&rig, default_limits());
        let result = session
            .purge(
                &rig.config,
                &name,
                PurgeMode::Forced,
                &CancellationToken::new(),
            )
            .await;
        assert!(
            matches!(
                &result,
                Err(PurgeError::Gateway(GatewayError::Protocol { .. }))
            ),
            "{result:?}"
        );
        assert!(rig.gateway().sandbox(&name).is_some());
        Ok(())
    })
    .await
}

#[tokio::test]
async fn a_delete_whose_response_is_lost_is_settled_by_a_lookup() -> TestResult {
    guard_within(WITHIN, async {
        let (rig, name) = launched().await?;
        rig.gateway().inject(Fault::LoseResponse {
            op: GatewayOp::Delete,
            error: lost(GatewayOp::Delete),
        });
        let gone = rig
            .world
            .session
            .purge(
                &rig.config,
                &name,
                PurgeMode::Forced,
                &CancellationToken::new(),
            )
            .await?;
        assert!(
            matches!(&gone, PurgeOutcome::Deleted { sandbox, .. } if *sandbox == name),
            "{gone:?}"
        );
        assert!(rig.gateway().sandbox(&name).is_none());

        let (rig, name) = launched().await?;
        rig.gateway().inject(Fault::FailNext {
            op: GatewayOp::Delete,
            error: lost(GatewayOp::Delete),
        });
        let still_there = rig
            .world
            .session
            .purge(
                &rig.config,
                &name,
                PurgeMode::Forced,
                &CancellationToken::new(),
            )
            .await;
        assert!(
            matches!(
                &still_there,
                Err(PurgeError::DeleteUnconfirmed { cause, .. })
                    if matches!(**cause, GatewayError::Timeout { .. })
            ),
            "{still_there:?}"
        );
        assert!(rig.gateway().sandbox(&name).is_some());

        let (rig, name) = launched().await?;
        let hold = Trigger::Op(GatewayOp::Delete);
        rig.gateway().inject(Fault::HoldAt {
            trigger: hold.clone(),
        });
        let idle = CancellationToken::new();
        let purging = rig
            .world
            .session
            .purge(&rig.config, &name, PurgeMode::Forced, &idle);
        let driver = async {
            assert!(rig.gateway().until_held(&hold).await);
            rig.gateway().force_phase(&name, Phase::Deleting);
            rig.gateway().inject(Fault::FailNext {
                op: GatewayOp::Delete,
                error: lost(GatewayOp::Delete),
            });
            rig.gateway().release(&hold);
        };
        let (leaving, ()) = tokio::join!(purging, driver);
        assert!(
            matches!(&leaving, Ok(PurgeOutcome::Deleted { sandbox, .. }) if *sandbox == name),
            "{leaving:?}"
        );
        Ok(())
    })
    .await
}

#[tokio::test]
async fn a_delete_whose_outcome_cannot_be_settled_is_reported_unconfirmed() -> TestResult {
    guard_within(WITHIN, async {
        for (case, lookup) in [
            (
                "a failing lookup",
                Lookup::Fail(GatewayError::Unreachable {
                    endpoint: "down".to_owned(),
                    message: "down".to_owned(),
                }),
            ),
            ("a lookup that never answers", Lookup::Hang),
        ] {
            let (rig, name) = launched().await?;
            rig.gateway().inject(Fault::LoseResponse {
                op: GatewayOp::Delete,
                error: lost(GatewayOp::Delete),
            });
            let tap = Tap::new(&rig);
            tap.lookups_after_a_delete(lookup);
            let session = tap.session(&rig, default_limits());
            let before = rig.world.clock.stamp();
            let result = session
                .purge(
                    &rig.config,
                    &name,
                    PurgeMode::Forced,
                    &CancellationToken::new(),
                )
                .await;
            let after = rig.world.clock.stamp();
            assert!(
                matches!(&result, Err(PurgeError::DeleteUnconfirmed { sandbox, .. }) if *sandbox == name),
                "{case}: {result:?}"
            );
            if case.contains("never") {
                assert_eq!(elapsed(&before, &after)?, RECOVERY_LOOKUP_BUDGET);
            }
        }
        Ok(())
    })
    .await
}

#[tokio::test]
async fn a_cancelled_work_state_inspection_is_reported_as_cancelled_not_as_a_failed_inspection()
-> TestResult {
    guard_within(WITHIN, async {
        let (rig, name) = launched().await?;
        let token = CancellationToken::new();
        token.cancel();
        let shown = rig
            .world
            .session
            .work_state(&rig.config, &name, &token)
            .await;
        assert_eq!(shown, WorkState::Unknown(UnknownReason::Cancelled));

        let hold = Trigger::Op(GatewayOp::Program);
        rig.gateway().inject(Fault::HoldAt {
            trigger: hold.clone(),
        });
        let token = CancellationToken::new();
        let inspecting = rig.world.session.work_state(&rig.config, &name, &token);
        let driver = async {
            assert!(rig.gateway().until_held(&hold).await);
            token.cancel();
        };
        let (shown, ()) = tokio::join!(inspecting, driver);
        assert_eq!(shown, WorkState::Unknown(UnknownReason::Cancelled));
        Ok(())
    })
    .await
}

async fn settled_after(
    change: impl AsyncFnOnce(&Rig, &SandboxName) -> TestResult,
) -> Result<Result<PurgeOutcome, PurgeError>, Box<dyn Error>> {
    let (rig, name) = launched().await?;
    let hold = Trigger::Op(GatewayOp::Delete);
    rig.gateway().inject(Fault::HoldAt {
        trigger: hold.clone(),
    });
    let idle = CancellationToken::new();
    let purging = rig
        .world
        .session
        .purge(&rig.config, &name, PurgeMode::Forced, &idle);
    let driver = async {
        assert!(rig.gateway().until_held(&hold).await);
        change(&rig, &name).await?;
        rig.gateway().inject(Fault::FailNext {
            op: GatewayOp::Delete,
            error: lost(GatewayOp::Delete),
        });
        rig.gateway().release(&hold);
        Ok::<(), Box<dyn Error>>(())
    };
    let (result, driven) = tokio::join!(purging, driver);
    driven?;
    Ok(result)
}

#[tokio::test]
async fn a_delete_with_an_unknown_outcome_is_settled_by_whether_the_same_sandbox_is_still_there()
-> TestResult {
    guard_within(WITHIN, async {
        let unmanaged = settled_after(async |rig, name| {
            rig.gateway().delete_sandbox(name).await?;
            rig.gateway()
                .insert_unmanaged(name, Labels::from_pairs([])?, Phase::Ready)?;
            Ok(())
        })
        .await?;
        assert!(
            matches!(unmanaged, Ok(PurgeOutcome::Deleted { .. })),
            "{unmanaged:?}"
        );

        let replaced = settled_after(async |rig, name| {
            rig.gateway().delete_sandbox(name).await?;
            rig.world.clock.advance(Duration::from_secs(60));
            rig.launch(1).await?;
            Ok(())
        })
        .await?;
        assert!(
            matches!(replaced, Ok(PurgeOutcome::Deleted { .. })),
            "{replaced:?}"
        );

        let unknown_age = settled_after(async |rig, _| {
            rig.handle(1).await?.forget_created_at();
            Ok(())
        })
        .await?;
        assert!(
            matches!(unknown_age, Err(PurgeError::DeleteUnconfirmed { .. })),
            "{unknown_age:?}"
        );

        let same = settled_after(async |_, _| Ok(())).await?;
        assert!(
            matches!(same, Err(PurgeError::DeleteUnconfirmed { .. })),
            "{same:?}"
        );
        Ok(())
    })
    .await
}
