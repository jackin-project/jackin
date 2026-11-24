// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::error::Error;
use std::time::Duration;

use jackin_core::{NameSuffix, Phase, SandboxName, SandboxSlug, sandbox_layout};
use jackin_openshell::{Gateway, GatewayError, GatewayOp, PowerAction};
use jackin_session::{
    LaunchError, LaunchOutcome, LaunchStep, RECOVERY_LOOKUP_BUDGET, RestoreCandidate,
    RestoreFinding,
};
use jackin_testkit::{Fault, Outcome, Trigger, guard_within};
use tokio_util::sync::CancellationToken;

use super::support::{TestResult, converged, failure, guard, launch_id, launch_rig, ready};

const WITHIN: Duration = Duration::from_secs(20);

fn finding(outcome: Outcome) -> Result<RestoreFinding, Box<dyn Error>> {
    match outcome.result {
        Ok(LaunchOutcome::Cancelled(RestoreCandidate {
            launch_id: id,
            finding,
        })) => {
            assert_eq!(id, launch_id(1));
            Ok(finding)
        }
        other => Err(format!("expected a cancelled launch, saw {other:?}").into()),
    }
}

fn cancelled() -> CancellationToken {
    let token = CancellationToken::new();
    token.cancel();
    token
}

fn unreachable_gateway() -> GatewayError {
    GatewayError::Unreachable {
        endpoint: "fake".to_owned(),
        message: "down".to_owned(),
    }
}

fn lost(op: GatewayOp) -> GatewayError {
    GatewayError::Timeout {
        op,
        after: Duration::from_secs(30),
        group: None,
    }
}

fn dup(suffix: &str) -> Result<SandboxName, Box<dyn Error>> {
    Ok(SandboxName::new(
        &SandboxSlug::parse("dup")?,
        &NameSuffix::parse(suffix)?,
    ))
}

#[tokio::test]
async fn a_failed_recovery_lookup_is_unconfirmed_and_never_absent() -> TestResult {
    guard(async {
        let rig = launch_rig().await?;
        rig.gateway().inject(Fault::FailNext {
            op: GatewayOp::Find,
            error: unreachable_gateway(),
        });
        let found = finding(rig.run(rig.request(1)?, cancelled()).await?)?;
        assert_eq!(
            found,
            RestoreFinding::Unconfirmed {
                last_observed: None
            }
        );
        assert!(rig.gateway().sandboxes().is_empty());

        ready(rig.launch_observed(1).await?)?;
        assert_eq!(rig.gateway().sandboxes().len(), 1);
        converged(&rig, &rig.handle(1).await?, 1)?;
        Ok(())
    })
    .await
}

#[tokio::test]
async fn a_hung_recovery_lookup_gives_up_after_its_budget_as_unconfirmed() -> TestResult {
    guard_within(WITHIN, async {
        let rig = launch_rig().await?;
        let hold = Trigger::Op(GatewayOp::Find);
        rig.gateway().inject(Fault::HoldAt {
            trigger: hold.clone(),
        });
        let before = rig.world.clock.stamp();
        let found = finding(rig.run(rig.request(1)?, cancelled()).await?)?;
        let after = rig.world.clock.stamp();
        assert_eq!(
            found,
            RestoreFinding::Unconfirmed {
                last_observed: None
            }
        );
        assert_eq!(
            Duration::new(
                u64::try_from(after.seconds - before.seconds)?,
                after.nanos.abs_diff(before.nanos)
            ),
            RECOVERY_LOOKUP_BUDGET
        );
        rig.gateway().release(&hold);

        ready(rig.launch_observed(1).await?)?;
        assert_eq!(rig.gateway().sandboxes().len(), 1);
        Ok(())
    })
    .await
}

#[tokio::test]
async fn several_sandboxes_for_one_launch_are_ambiguous_after_a_cancel() -> TestResult {
    guard(async {
        let rig = launch_rig().await?;
        ready(rig.launch_observed(9).await?)?;
        let (a, b) = (dup("aaaa")?, dup("bbbb")?);
        rig.foreign_sandbox(a.clone(), 1).await?;
        rig.foreign_sandbox(b.clone(), 1).await?;
        let found = finding(rig.run(rig.request(1)?, cancelled()).await?)?;
        assert_eq!(found, RestoreFinding::Ambiguous(vec![a, b]));
        assert_eq!(rig.gateway().sandboxes().len(), 3);

        let refused = failure(rig.launch_observed(1).await?)?;
        assert!(
            matches!(&*refused.error, LaunchError::AmbiguousLaunch { launch, .. } if *launch == launch_id(1)),
            "{:?}",
            refused.error
        );
        assert_eq!(rig.gateway().sandboxes().len(), 3);
        Ok(())
    })
    .await
}

#[tokio::test]
async fn a_second_sandbox_that_appears_during_the_launch_makes_the_cancel_ambiguous() -> TestResult
{
    guard_within(WITHIN, async {
        let rig = launch_rig().await?;
        let hold = Trigger::Op(GatewayOp::UploadTree);
        rig.gateway().inject(Fault::HoldAt {
            trigger: hold.clone(),
        });
        let token = CancellationToken::new();
        let extra = dup("bbbb")?;
        let driver = async {
            assert!(rig.gateway().until_held(&hold).await);
            rig.foreign_sandbox(extra.clone(), 1).await?;
            token.cancel();
            rig.gateway().release(&hold);
            Ok::<(), Box<dyn Error>>(())
        };
        let (outcome, driven) = tokio::join!(rig.run(rig.request(1)?, token.clone()), driver);
        driven?;
        let mut expected = vec![rig.sandbox_name(1)?, extra];
        expected.sort();
        assert_eq!(finding(outcome?)?, RestoreFinding::Ambiguous(expected));
        assert_eq!(rig.gateway().sandboxes().len(), 2);
        Ok(())
    })
    .await
}

#[tokio::test]
async fn a_lost_create_response_is_never_reported_absent_by_a_later_cancelled_run() -> TestResult {
    guard(async {
        let rig = launch_rig().await?;
        rig.gateway().inject(Fault::HideUntilCreateConflict);
        rig.gateway().inject(Fault::LoseResponse {
            op: GatewayOp::Create,
            error: lost(GatewayOp::Create),
        });
        let failed = failure(rig.launch_observed(1).await?)?;
        assert_eq!(failed.step, LaunchStep::Sandbox);
        assert_eq!(rig.gateway().sandboxes().len(), 1);

        let found = finding(rig.run(rig.request(1)?, cancelled()).await?)?;
        assert_eq!(found, RestoreFinding::Observed(rig.sandbox_name(1)?));

        ready(rig.launch_observed(1).await?)?;
        assert_eq!(rig.gateway().sandboxes().len(), 1);
        converged(&rig, &rig.handle(1).await?, 1)?;
        Ok(())
    })
    .await
}

#[tokio::test]
async fn a_cancel_after_a_lost_create_response_names_the_created_sandbox() -> TestResult {
    guard(async {
        let rig = launch_rig().await?;
        let token = CancellationToken::new();
        rig.gateway().inject(Fault::CancelAt {
            trigger: Trigger::Op(GatewayOp::Create),
            token: token.clone(),
        });
        rig.gateway().inject(Fault::LoseResponse {
            op: GatewayOp::Create,
            error: lost(GatewayOp::Create),
        });
        let found = finding(rig.run(rig.request(1)?, token).await?)?;
        assert_eq!(found, RestoreFinding::Observed(rig.sandbox_name(1)?));
        assert_eq!(rig.gateway().sandboxes().len(), 1);

        ready(rig.launch_observed(1).await?)?;
        assert_eq!(rig.gateway().sandboxes().len(), 1);
        converged(&rig, &rig.handle(1).await?, 1)?;
        Ok(())
    })
    .await
}

#[tokio::test]
async fn a_cancel_after_a_lost_write_response_is_observed_and_the_retry_starts_once() -> TestResult
{
    guard_within(WITHIN, async {
        let rig = launch_rig().await?;
        let hold = Trigger::Write(sandbox_layout::RUN_START.clone());
        rig.gateway().inject(Fault::HoldAt {
            trigger: hold.clone(),
        });
        let token = CancellationToken::new();
        let driver = async {
            assert!(rig.gateway().until_held(&hold).await);
            rig.gateway().inject(Fault::LoseResponse {
                op: GatewayOp::WriteFile,
                error: lost(GatewayOp::WriteFile),
            });
            token.cancel();
            rig.gateway().release(&hold);
        };
        let (outcome, ()) = tokio::join!(rig.run(rig.request(1)?, token.clone()), driver);
        assert_eq!(
            finding(outcome?)?,
            RestoreFinding::Observed(rig.sandbox_name(1)?)
        );
        let handle = rig.handle(1).await?;
        assert_eq!(handle.file(&sandbox_layout::RUN_START), Some(b"1".to_vec()));

        ready(rig.launch_observed(1).await?)?;
        assert_eq!(rig.gateway().sandboxes().len(), 1);
        converged(&rig, &rig.handle(1).await?, 1)?;
        Ok(())
    })
    .await
}

#[tokio::test]
async fn a_cancel_after_a_lost_power_response_is_observed_and_the_retry_converges() -> TestResult {
    guard(async {
        let rig = launch_rig().await?;
        ready(rig.launch_observed(1).await?)?;
        let name = rig.sandbox_name(1)?;
        rig.gateway().set_power(&name, PowerAction::Stop).await?;
        assert_eq!(rig.handle(1).await?.phase(), Phase::Stopped);

        let token = CancellationToken::new();
        rig.gateway().inject(Fault::CancelAt {
            trigger: Trigger::Op(GatewayOp::Power),
            token: token.clone(),
        });
        rig.gateway().inject(Fault::LoseResponse {
            op: GatewayOp::Power,
            error: lost(GatewayOp::Power),
        });
        let found = finding(rig.run(rig.request(1)?, token).await?)?;
        assert_eq!(found, RestoreFinding::Observed(name.clone()));
        assert_ne!(rig.handle(1).await?.phase(), Phase::Stopped);

        ready(rig.launch_observed(1).await?)?;
        assert_eq!(rig.gateway().sandboxes().len(), 1);
        assert_eq!(rig.handle(1).await?.phase(), Phase::Ready);
        Ok(())
    })
    .await
}

#[tokio::test]
async fn a_cancel_interrupts_a_hung_launch_lookup_without_waiting_for_the_gateway() -> TestResult {
    guard_within(WITHIN, async {
        let rig = launch_rig().await?;
        let hold = Trigger::Op(GatewayOp::Find);
        rig.gateway().inject(Fault::HoldAt {
            trigger: hold.clone(),
        });
        let token = CancellationToken::new();
        let driver = async {
            assert!(rig.gateway().until_held(&hold).await);
            token.cancel();
        };
        let (outcome, ()) = tokio::join!(rig.run(rig.request(1)?, token.clone()), driver);
        assert_eq!(
            finding(outcome?)?,
            RestoreFinding::Absent { requested: false }
        );
        assert!(rig.gateway().sandboxes().is_empty());
        rig.gateway().release(&hold);
        Ok(())
    })
    .await
}

#[tokio::test]
async fn a_cancel_after_a_failed_sandbox_was_deleted_says_a_sandbox_was_requested() -> TestResult {
    guard(async {
        let rig = launch_rig().await?;
        let token = CancellationToken::new();
        rig.gateway().inject(Fault::ProvisionFails { creates: 1 });
        rig.gateway().inject(Fault::CancelAt {
            trigger: Trigger::Op(GatewayOp::Delete),
            token: token.clone(),
        });
        let found = finding(rig.run(rig.request(1)?, token).await?)?;
        assert_eq!(found, RestoreFinding::Absent { requested: true });
        assert!(rig.gateway().sandboxes().is_empty());

        ready(rig.launch_observed(1).await?)?;
        assert_eq!(rig.gateway().sandboxes().len(), 1);
        Ok(())
    })
    .await
}

#[tokio::test]
async fn a_cancel_with_a_failing_recovery_lookup_keeps_the_last_observed_sandbox() -> TestResult {
    guard_within(WITHIN, async {
        let rig = launch_rig().await?;
        let hold = Trigger::Op(GatewayOp::UploadTree);
        rig.gateway().inject(Fault::HoldAt {
            trigger: hold.clone(),
        });
        let token = CancellationToken::new();
        let driver = async {
            assert!(rig.gateway().until_held(&hold).await);
            rig.gateway().inject(Fault::FailNext {
                op: GatewayOp::Find,
                error: unreachable_gateway(),
            });
            token.cancel();
            rig.gateway().release(&hold);
        };
        let (outcome, ()) = tokio::join!(rig.run(rig.request(1)?, token.clone()), driver);
        assert_eq!(
            finding(outcome?)?,
            RestoreFinding::Unconfirmed {
                last_observed: Some(rig.sandbox_name(1)?),
            }
        );

        ready(rig.launch_observed(1).await?)?;
        assert_eq!(rig.gateway().sandboxes().len(), 1);
        converged(&rig, &rig.handle(1).await?, 1)?;
        Ok(())
    })
    .await
}

#[tokio::test]
async fn an_unsettled_create_that_no_lookup_can_see_is_unconfirmed_not_absent() -> TestResult {
    guard(async {
        let rig = launch_rig().await?;
        let token = CancellationToken::new();
        rig.gateway()
            .inject(Fault::HideEverywhereUntilCreateConflict);
        rig.gateway().inject(Fault::CancelAt {
            trigger: Trigger::Op(GatewayOp::Create),
            token: token.clone(),
        });
        rig.gateway().inject(Fault::LoseResponse {
            op: GatewayOp::Create,
            error: lost(GatewayOp::Create),
        });
        let found = finding(rig.run(rig.request(1)?, token).await?)?;
        assert_eq!(
            found,
            RestoreFinding::Unconfirmed {
                last_observed: None
            }
        );
        assert_eq!(rig.gateway().sandboxes().len(), 1);

        ready(rig.launch_observed(1).await?)?;
        assert_eq!(rig.gateway().sandboxes().len(), 1);
        converged(&rig, &rig.handle(1).await?, 1)?;
        Ok(())
    })
    .await
}

#[tokio::test]
async fn a_created_sandbox_that_no_lookup_can_see_keeps_its_name_as_last_observed() -> TestResult {
    guard(async {
        let rig = launch_rig().await?;
        let token = CancellationToken::new();
        rig.gateway()
            .inject(Fault::HideEverywhereUntilCreateConflict);
        rig.gateway().inject(Fault::CancelAt {
            trigger: Trigger::Op(GatewayOp::Create),
            token: token.clone(),
        });
        let found = finding(rig.run(rig.request(1)?, token).await?)?;
        assert_eq!(
            found,
            RestoreFinding::Unconfirmed {
                last_observed: Some(rig.sandbox_name(1)?),
            }
        );
        assert_eq!(rig.gateway().sandboxes().len(), 1);

        ready(rig.launch_observed(1).await?)?;
        assert_eq!(rig.gateway().sandboxes().len(), 1);
        Ok(())
    })
    .await
}

#[tokio::test]
async fn a_name_hit_names_the_sandbox_when_the_label_lookup_fails() -> TestResult {
    guard(async {
        let rig = launch_rig().await?;
        let token = CancellationToken::new();
        rig.gateway().inject(Fault::CancelAt {
            trigger: Trigger::Op(GatewayOp::UploadTree),
            token: token.clone(),
        });
        let first = finding(rig.run(rig.request(1)?, token).await?)?;
        assert_eq!(first, RestoreFinding::Observed(rig.sandbox_name(1)?));

        rig.gateway().inject(Fault::FailNext {
            op: GatewayOp::Find,
            error: unreachable_gateway(),
        });
        let found = finding(rig.run(rig.request(1)?, cancelled()).await?)?;
        assert_eq!(
            found,
            RestoreFinding::Unconfirmed {
                last_observed: Some(rig.sandbox_name(1)?),
            }
        );
        Ok(())
    })
    .await
}

#[tokio::test]
async fn a_name_hit_on_another_launch_never_becomes_the_last_observed_sandbox() -> TestResult {
    guard(async {
        let rig = launch_rig().await?;
        ready(rig.launch_observed(9).await?)?;
        rig.foreign_sandbox(rig.sandbox_name(1)?, 3).await?;
        rig.gateway().inject(Fault::FailNext {
            op: GatewayOp::Find,
            error: unreachable_gateway(),
        });
        let found = finding(rig.run(rig.request(1)?, cancelled()).await?)?;
        assert_eq!(
            found,
            RestoreFinding::Unconfirmed {
                last_observed: None
            }
        );
        Ok(())
    })
    .await
}

#[tokio::test]
async fn a_sandbox_that_is_being_deleted_is_not_offered_as_a_restore_candidate() -> TestResult {
    guard(async {
        let rig = launch_rig().await?;
        ready(rig.launch_observed(1).await?)?;
        let name = rig.sandbox_name(1)?;
        rig.gateway().force_phase(&name, Phase::Deleting);
        let found = finding(rig.run(rig.request(1)?, cancelled()).await?)?;
        assert_eq!(
            found,
            RestoreFinding::Unconfirmed {
                last_observed: Some(name),
            }
        );
        Ok(())
    })
    .await
}

#[tokio::test]
async fn a_found_sandbox_is_reported_without_waiting_for_the_name_lookup() -> TestResult {
    guard_within(WITHIN, async {
        let rig = launch_rig().await?;
        ready(rig.launch_observed(1).await?)?;
        let hold = Trigger::Op(GatewayOp::Find);
        for _ in 0..2 {
            rig.gateway().inject(Fault::HoldAt {
                trigger: hold.clone(),
            });
        }
        let before = rig.world.clock.stamp();
        let driver = async {
            assert!(rig.gateway().until_held(&hold).await);
            rig.gateway().release(&hold);
        };
        let (outcome, ()) = tokio::join!(rig.run(rig.request(1)?, cancelled()), driver);
        let after = rig.world.clock.stamp();
        assert_eq!(
            finding(outcome?)?,
            RestoreFinding::Observed(rig.sandbox_name(1)?)
        );
        assert_eq!((after.seconds, after.nanos), (before.seconds, before.nanos));
        rig.gateway().release(&hold);
        Ok(())
    })
    .await
}

#[tokio::test]
async fn a_name_lookup_that_hangs_after_an_empty_label_lookup_is_unconfirmed_within_the_budget()
-> TestResult {
    guard_within(WITHIN, async {
        let rig = launch_rig().await?;
        let hold = Trigger::Op(GatewayOp::Find);
        rig.gateway().inject(Fault::HoldAt {
            trigger: hold.clone(),
        });
        let before = rig.world.clock.stamp();
        let driver = async {
            assert!(rig.gateway().until_held(&hold).await);
            rig.gateway().inject(Fault::HoldAt {
                trigger: hold.clone(),
            });
            rig.gateway().release(&hold);
        };
        let (outcome, ()) = tokio::join!(rig.run(rig.request(1)?, cancelled()), driver);
        let after = rig.world.clock.stamp();
        assert_eq!(
            finding(outcome?)?,
            RestoreFinding::Unconfirmed {
                last_observed: None
            }
        );
        assert_eq!(
            Duration::new(
                u64::try_from(after.seconds - before.seconds)?,
                after.nanos.abs_diff(before.nanos)
            ),
            RECOVERY_LOOKUP_BUDGET
        );
        rig.gateway().release(&hold);
        Ok(())
    })
    .await
}

#[tokio::test]
async fn a_cancel_after_an_empty_lookup_that_followed_a_created_sandbox_stays_unconfirmed()
-> TestResult {
    guard_within(WITHIN, async {
        let rig = launch_rig().await?;
        let hold = Trigger::Op(GatewayOp::Find);
        rig.gateway()
            .inject(Fault::HideEverywhereUntilCreateConflict);
        rig.gateway().inject(Fault::HoldAt {
            trigger: hold.clone(),
        });
        let token = CancellationToken::new();
        let driver = async {
            loop {
                assert!(rig.gateway().until_held(&hold).await);
                if rig.gateway().sandboxes().is_empty() {
                    rig.gateway().inject(Fault::HoldAt {
                        trigger: hold.clone(),
                    });
                    rig.gateway().release(&hold);
                } else {
                    rig.gateway().inject(Fault::CancelAt {
                        trigger: hold.clone(),
                        token: token.clone(),
                    });
                    rig.gateway().release(&hold);
                    break;
                }
            }
        };
        let (outcome, ()) = tokio::join!(rig.run(rig.request(1)?, token.clone()), driver);
        assert_eq!(
            finding(outcome?)?,
            RestoreFinding::Unconfirmed {
                last_observed: Some(rig.sandbox_name(1)?),
            }
        );
        assert_eq!(rig.gateway().sandboxes().len(), 1);
        Ok(())
    })
    .await
}
