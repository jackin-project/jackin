// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::time::Duration;

use jackin_core::{Config, Phase, SandboxName, sandbox_layout};
use jackin_openshell::{Gateway, GatewayError, GatewayOp};
use jackin_session::{LaunchStep, PurgeError, PurgeMode, PurgeOutcome, UnknownReason, WorkState};
use jackin_testkit::{Fault, Rig, TestResult, Trigger, guard};
use tokio_util::sync::CancellationToken;

use super::support::{digests, export, snapshot, state};

async fn refused(rig: &Rig, config: &Config, name: &SandboxName, case: &str) -> TestResult {
    let handle = rig
        .world
        .gateway
        .sandbox(name)
        .ok_or("the sandbox is gone")?;
    let other = rig.handle(2).await?;
    let (files, other_files) = (digests(&handle), digests(&other));
    let before = snapshot(rig).await?;
    let shown = rig.world.session.work_state(config, name).await;
    assert_ne!(shown, WorkState::Exported);

    let result = rig
        .world
        .session
        .purge(config, name, PurgeMode::Guarded)
        .await;

    let Err(PurgeError::WorkNotExported { sandbox, state }) = result else {
        return Err(format!("{case}: expected a refusal, saw {result:?}").into());
    };
    assert_eq!((&sandbox, &*state), (name, &shown));
    assert!(rig.world.gateway.sandbox(name).is_some());
    assert_eq!(digests(&handle), files);
    assert_eq!(digests(&other), other_files);
    let after = snapshot(rig).await?;
    assert_eq!((after.rest, after.exported), (before.rest, before.exported));
    Ok(())
}

#[tokio::test]
async fn a_guarded_purge_refuses_every_state_that_is_not_exported_and_changes_nothing() -> TestResult
{
    guard(async {
        let rig = Rig::new().await?;
        rig.launch(1).await?;
        rig.launch(2).await?;
        let handle = rig.handle(1).await?;
        let name = handle.name();
        let config = &rig.config;

        std::fs::write(handle.root().join("project/new.txt"), b"u")?;
        refused(&rig, config, &name, "untracked").await?;
        handle.commit_in_project("c.txt", b"c", "work").await?;
        refused(&rig, config, &name, "branch").await?;
        export(&rig, &name).await?;
        handle.git(&["checkout", "-q", "--detach"]).await?;
        handle.commit_in_project("d.txt", b"d", "detached").await?;
        refused(&rig, config, &name, "detached").await?;
        export(&rig, &name).await?;
        assert_eq!(state(&rig, &name).await, WorkState::Exported);
        handle.git(&["branch", "jackin/evil"]).await?;
        refused(&rig, config, &name, "unusable").await?;
        handle.git(&["branch", "-q", "-D", "jackin/evil"]).await?;

        refused(&rig, &Config::empty(), &name, "no workspace").await?;
        for phase in [
            Phase::Stopped,
            Phase::Error { exit_code: Some(7) },
            Phase::Completed { exit_code: 0 },
        ] {
            rig.world.gateway.force_phase(&name, phase);
            refused(&rig, config, &name, &format!("{phase:?}")).await?;
        }
        rig.world.gateway.force_phase(&name, Phase::Ready);
        rig.world.gateway.inject(Fault::FailAt {
            trigger: Trigger::Program(sandbox_layout::GIT.clone()),
            error: GatewayError::Protocol {
                message: "injected".to_owned(),
            },
        });
        let blind = rig
            .world
            .session
            .purge(config, &name, PurgeMode::Guarded)
            .await;
        assert!(
            matches!(
                &blind,
                Err(PurgeError::WorkNotExported { state, .. })
                    if **state == WorkState::Unknown(UnknownReason::InspectionFailed)
            ),
            "{blind:?}"
        );
        assert!(rig.world.gateway.sandbox(&name).is_some());
        Ok(())
    })
    .await
}

#[tokio::test]
async fn a_launch_without_a_repository_is_refused_too() -> TestResult {
    guard(async {
        let rig = Rig::new().await?;
        rig.launch(2).await?;
        rig.world.gateway.inject(Fault::FailAt {
            trigger: Trigger::Op(GatewayOp::UploadTree),
            error: GatewayError::Protocol {
                message: "injected".to_owned(),
            },
        });
        let failed = rig.launch_with(1, CancellationToken::new()).await?;
        assert!(matches!(failed, Err(failure) if failure.step == LaunchStep::Upload));
        refused(&rig, &rig.config, &rig.sandbox_name(1)?, "no repository").await
    })
    .await
}

#[tokio::test]
async fn an_exported_sandbox_is_deleted_alone_and_a_forced_purge_reports_what_it_deleted()
-> TestResult {
    guard(async {
        let rig = Rig::new().await?;
        rig.launch(1).await?;
        rig.launch(2).await?;
        rig.launch(3).await?;
        let session = &rig.world.session;
        let gateway = &rig.world.gateway;
        let (first, second, third) = (
            rig.sandbox_name(1)?,
            rig.sandbox_name(2)?,
            rig.sandbox_name(3)?,
        );
        let providers = gateway.providers();

        let gone = session
            .purge(&rig.config, &first, PurgeMode::Guarded)
            .await?;
        assert_eq!(
            gone,
            PurgeOutcome::Deleted {
                sandbox: first.clone(),
                was: WorkState::Exported
            }
        );
        assert!(gateway.sandbox(&first).is_none());
        assert_eq!(gateway.providers(), providers);

        let handle = rig.handle(2).await?;
        handle.commit_in_project("w.txt", b"w", "work").await?;
        let shown = state(&rig, &second).await;
        let forced = session
            .purge(&rig.config, &second, PurgeMode::Forced)
            .await?;
        assert_eq!(
            forced,
            PurgeOutcome::Deleted {
                sandbox: second.clone(),
                was: shown
            }
        );
        assert!(gateway.sandbox(&second).is_none());

        gateway.inject(Fault::FailAt {
            trigger: Trigger::Program(sandbox_layout::GIT.clone()),
            error: GatewayError::Protocol {
                message: "injected".to_owned(),
            },
        });
        let blind = session
            .purge(&rig.config, &third, PurgeMode::Forced)
            .await?;
        assert!(
            matches!(
                blind,
                PurgeOutcome::Deleted {
                    was: WorkState::Unknown(UnknownReason::InspectionFailed),
                    ..
                }
            ),
            "{blind:?}"
        );
        assert_eq!(gateway.providers(), providers);
        Ok(())
    })
    .await
}

#[tokio::test]
async fn a_repeated_purge_is_already_gone_a_failed_delete_keeps_the_sandbox_and_a_deleting_one_is_gone()
-> TestResult {
    guard(async {
        let rig = Rig::new().await?;
        rig.launch(1).await?;
        rig.launch(2).await?;
        let session = &rig.world.session;
        let gateway = &rig.world.gateway;
        let (first, second) = (rig.sandbox_name(1)?, rig.sandbox_name(2)?);

        gateway.inject(Fault::FailNext {
            op: GatewayOp::Delete,
            error: GatewayError::Protocol {
                message: "injected".to_owned(),
            },
        });
        let failed = session.purge(&rig.config, &first, PurgeMode::Guarded).await;
        assert!(matches!(failed, Err(PurgeError::Gateway(_))), "{failed:?}");
        assert!(gateway.sandbox(&first).is_some());

        gateway.force_phase(&second, Phase::Deleting);
        let leaving = session
            .purge(&rig.config, &second, PurgeMode::Forced)
            .await?;
        assert_eq!(
            leaving,
            PurgeOutcome::AlreadyGone {
                sandbox: second.clone()
            }
        );

        session
            .purge(&rig.config, &first, PurgeMode::Guarded)
            .await?;
        for mode in [PurgeMode::Guarded, PurgeMode::Forced] {
            for config in [&rig.config, &Config::empty()] {
                let again = session.purge(config, &first, mode).await?;
                assert_eq!(
                    again,
                    PurgeOutcome::AlreadyGone {
                        sandbox: first.clone()
                    }
                );
            }
        }
        Ok(())
    })
    .await
}

#[tokio::test]
async fn a_guarded_purge_never_deletes_a_sandbox_that_replaced_the_one_it_inspected() -> TestResult
{
    guard(async {
        let rig = Rig::new().await?;
        rig.launch(1).await?;
        let name = rig.handle(1).await?.name();
        let gateway = &rig.world.gateway;
        let held = Trigger::Op(GatewayOp::Program);
        gateway.inject(Fault::HoldAt {
            trigger: held.clone(),
        });

        let purging = rig
            .world
            .session
            .purge(&rig.config, &name, PurgeMode::Guarded);
        let swapping = async {
            assert!(gateway.until_held(&held).await);
            gateway.delete_sandbox(&name).await?;
            rig.world.clock.advance(Duration::from_secs(60));
            rig.launch(1).await?;
            gateway.release(&held);
            Ok::<_, Box<dyn std::error::Error>>(())
        };
        let (purged, swapped) = tokio::join!(purging, swapping);

        swapped?;
        let Err(PurgeError::Replaced { sandbox }) = purged else {
            return Err(format!("expected a refusal, saw {purged:?}").into());
        };
        assert_eq!(sandbox, name);
        assert!(gateway.sandbox(&name).is_some());
        Ok(())
    })
    .await
}

#[tokio::test]
async fn a_guarded_purge_refuses_a_sandbox_whose_creation_time_is_unknown() -> TestResult {
    guard(async {
        let rig = Rig::new().await?;
        rig.launch(1).await?;
        let handle = rig.handle(1).await?;
        let name = handle.name();
        export(&rig, &name).await?;
        handle.forget_created_at();

        let purged = rig
            .world
            .session
            .purge(&rig.config, &name, PurgeMode::Guarded)
            .await;

        let Err(PurgeError::Replaced { sandbox }) = purged else {
            return Err(format!("expected a refusal, saw {purged:?}").into());
        };
        assert_eq!(sandbox, name);
        assert!(rig.world.gateway.sandbox(&name).is_some());
        let forced = rig
            .world
            .session
            .purge(&rig.config, &name, PurgeMode::Forced)
            .await;

        let Err(PurgeError::Replaced { sandbox }) = forced else {
            return Err(format!("expected a refusal, saw {forced:?}").into());
        };
        assert_eq!(sandbox, name);
        assert!(rig.world.gateway.sandbox(&name).is_some());
        Ok(())
    })
    .await
}

#[tokio::test]
async fn a_forced_purge_never_deletes_a_replacement_when_the_creation_time_was_unknown() -> TestResult
{
    guard(async {
        let rig = Rig::new().await?;
        rig.launch(1).await?;
        let handle = rig.handle(1).await?;
        let name = handle.name();
        handle.forget_created_at();
        let gateway = &rig.world.gateway;
        let held = Trigger::Op(GatewayOp::Program);
        gateway.inject(Fault::HoldAt {
            trigger: held.clone(),
        });

        let purging = rig
            .world
            .session
            .purge(&rig.config, &name, PurgeMode::Forced);
        let swapping = async {
            assert!(gateway.until_held(&held).await);
            gateway.delete_sandbox(&name).await?;
            rig.world.clock.advance(Duration::from_secs(60));
            rig.launch(1).await?;
            gateway.release(&held);
            Ok::<_, Box<dyn std::error::Error>>(())
        };
        let (purged, swapped) = tokio::join!(purging, swapping);

        swapped?;
        let Err(PurgeError::Replaced { sandbox }) = purged else {
            return Err(format!("expected a refusal, saw {purged:?}").into());
        };
        assert_eq!(sandbox, name);
        assert!(gateway.sandbox(&name).is_some());
        Ok(())
    })
    .await
}

#[tokio::test]
async fn a_forced_purge_never_deletes_a_sandbox_that_replaced_the_one_it_inspected() -> TestResult {
    guard(async {
        let rig = Rig::new().await?;
        rig.launch(1).await?;
        let name = rig.handle(1).await?.name();
        let gateway = &rig.world.gateway;
        let held = Trigger::Op(GatewayOp::Program);
        gateway.inject(Fault::HoldAt {
            trigger: held.clone(),
        });

        let purging = rig
            .world
            .session
            .purge(&rig.config, &name, PurgeMode::Forced);
        let swapping = async {
            assert!(gateway.until_held(&held).await);
            gateway.delete_sandbox(&name).await?;
            rig.world.clock.advance(Duration::from_secs(60));
            rig.launch(1).await?;
            gateway.release(&held);
            Ok::<_, Box<dyn std::error::Error>>(())
        };
        let (purged, swapped) = tokio::join!(purging, swapping);

        swapped?;
        let Err(PurgeError::Replaced { sandbox }) = purged else {
            return Err(format!("expected a refusal, saw {purged:?}").into());
        };
        assert_eq!(sandbox, name);
        assert!(gateway.sandbox(&name).is_some());
        Ok(())
    })
    .await
}
