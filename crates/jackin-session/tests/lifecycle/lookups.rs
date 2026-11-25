// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::time::Duration;

use jackin_core::{Labels, Phase, SandboxName};
use jackin_openshell::{GatewayOp, PowerAction};
use jackin_session::LifecycleError;
use jackin_testkit::{Fault, Rig, TestResult, Trigger, guard_within};
use tokio_util::sync::CancellationToken;

const WITHIN: Duration = Duration::from_secs(20);

#[tokio::test]
async fn list_and_inspect_end_with_cancelled_when_their_token_is_cancelled() -> TestResult {
    guard_within(WITHIN, async {
        let rig = Rig::new().await?;
        rig.launch(1).await?;
        let name = rig.sandbox_name(1)?;
        let session = &rig.world.session;

        let cancelled = CancellationToken::new();
        cancelled.cancel();
        let listed = session.list(&cancelled).await;
        assert!(
            matches!(listed, Err(LifecycleError::Cancelled)),
            "{listed:?}"
        );
        let inspected = session.inspect(&name, &cancelled).await;
        assert!(
            matches!(inspected, Err(LifecycleError::Cancelled)),
            "{inspected:?}"
        );

        for op in [GatewayOp::Find, GatewayOp::Detail] {
            let hold = Trigger::Op(op);
            rig.gateway().inject(Fault::HoldAt {
                trigger: hold.clone(),
            });
            let token = CancellationToken::new();
            let reading = async {
                if op == GatewayOp::Find {
                    session.list(&token).await.map(drop)
                } else {
                    session.inspect(&name, &token).await.map(drop)
                }
            };
            let driver = async {
                assert!(rig.gateway().until_held(&hold).await);
                token.cancel();
            };
            let (result, ()) = tokio::join!(reading, driver);
            assert!(
                matches!(result, Err(LifecycleError::Cancelled)),
                "{op:?}: {result:?}"
            );
            rig.gateway().release(&hold);
        }
        Ok(())
    })
    .await
}

#[tokio::test]
async fn a_sandbox_jackin_does_not_manage_is_named_as_such_and_never_acted_on() -> TestResult {
    guard_within(WITHIN, async {
        let rig = Rig::new().await?;
        let ghost = SandboxName::parse("jackin-ghost-0000")?;
        rig.gateway()
            .insert_unmanaged(&ghost, Labels::from_pairs([])?, Phase::Ready)?;
        let missing = SandboxName::parse("jackin-missing-0001")?;
        let session = &rig.world.session;
        let token = CancellationToken::new();

        let inspected = session.inspect(&ghost, &token).await;
        assert!(
            matches!(&inspected, Err(LifecycleError::NotManaged(name)) if *name == ghost),
            "{inspected:?}"
        );
        let stopped = session.power(&ghost, PowerAction::Stop, &token).await;
        assert!(
            matches!(&stopped, Err(LifecycleError::NotManaged(name)) if *name == ghost),
            "{stopped:?}"
        );
        assert_eq!(rig.gateway().calls(GatewayOp::Power), 0);

        let absent = session.power(&missing, PowerAction::Stop, &token).await;
        assert!(
            matches!(&absent, Err(LifecycleError::NotFound(name)) if *name == missing),
            "{absent:?}"
        );
        let absent = session.inspect(&missing, &token).await;
        assert!(
            matches!(&absent, Err(LifecycleError::NotFound(name)) if *name == missing),
            "{absent:?}"
        );
        Ok(())
    })
    .await
}
