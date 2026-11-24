// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::time::Duration;

use jackin_core::{Phase, SandboxName};
use jackin_openshell::{GatewayOp, PowerAction};
use jackin_session::LifecycleError;
use jackin_testkit::{Fault, Rig, TestResult, Trigger, guard_within};
use tokio_util::sync::CancellationToken;

const WITHIN: Duration = Duration::from_secs(20);

async fn power(
    rig: &Rig,
    name: &SandboxName,
    action: PowerAction,
    token: &CancellationToken,
    readable: bool,
) -> Result<(), LifecycleError> {
    let session = &rig.world.session;
    if readable {
        session.power(name, action, token).await.map(drop)
    } else {
        session.power_unreadable(name, action, token).await
    }
}

#[tokio::test]
async fn a_cancel_interrupts_a_hung_lookup_in_a_power_change_and_changes_nothing() -> TestResult {
    guard_within(WITHIN, async {
        for readable in [true, false] {
            let rig = Rig::new().await?;
            rig.launch(1).await?;
            let handle = rig.handle(1).await?;
            let name = handle.name();
            let hold = Trigger::Op(GatewayOp::Find);
            rig.gateway().inject(Fault::HoldAt {
                trigger: hold.clone(),
            });
            let token = CancellationToken::new();
            let stopping = power(&rig, &name, PowerAction::Stop, &token, readable);
            let driver = async {
                assert!(rig.gateway().until_held(&hold).await);
                token.cancel();
            };
            let (result, ()) = tokio::join!(stopping, driver);
            assert!(
                matches!(result, Err(LifecycleError::Cancelled)),
                "{readable}: {result:?}"
            );
            assert_eq!(handle.phase(), Phase::Ready, "{readable}");
            rig.gateway().release(&hold);
        }
        Ok(())
    })
    .await
}

#[tokio::test]
async fn a_cancel_after_the_power_request_was_sent_leaves_the_sandbox_changing_not_unchanged()
-> TestResult {
    guard_within(WITHIN, async {
        for readable in [true, false] {
            let rig = Rig::new().await?;
            rig.launch(1).await?;
            let handle = rig.handle(1).await?;
            let name = handle.name();
            let session = &rig.world.session;
            session
                .power(&name, PowerAction::Stop, &CancellationToken::new())
                .await?;
            assert_eq!(handle.phase(), Phase::Stopped);

            let token = CancellationToken::new();
            rig.gateway().inject(Fault::CancelAt {
                trigger: Trigger::Op(GatewayOp::Power),
                token: token.clone(),
            });
            let result = power(&rig, &name, PowerAction::Start, &token, readable).await;
            assert!(
                matches!(&result, Err(LifecycleError::PowerCancelled { sandbox, wanted })
                    if *sandbox == name && *wanted == Phase::Ready),
                "{readable}: {result:?}"
            );
            assert_ne!(handle.phase(), Phase::Stopped, "{readable}");
        }
        Ok(())
    })
    .await
}
