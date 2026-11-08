// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::time::Duration;

use jackin_core::{ConfigEdit, Labels, WorkspaceName};
use jackin_openshell::GatewayOp;
use jackin_session::{Gateways, LaunchError, WorkspaceGuardError};
use jackin_testkit::{Fault, Trigger};
use tokio_util::sync::CancellationToken;

use super::support::{TestResult, failure, guard, launch_rig, ready};

fn remove(workspace: &WorkspaceName) -> Vec<ConfigEdit> {
    vec![ConfigEdit::RemoveWorkspace(workspace.clone())]
}

#[tokio::test]
async fn a_workspace_removal_waits_for_a_launch_that_is_creating_its_sandbox() -> TestResult {
    guard(async {
        let rig = launch_rig().await?;
        let create = Trigger::Op(GatewayOp::Create);
        rig.gateway().inject(Fault::HoldAt {
            trigger: create.clone(),
        });
        let plan = rig.plan(1)?;
        let workspace = plan.workspace().clone();
        let launching = rig.run_planned(&rig.world.session, plan, CancellationToken::new());
        let removing = async {
            assert!(rig.gateway().until_held(&create).await);
            let gateways = Gateways::single(rig.world.session.clone());
            let hold = gateways
                .ensure_workspace_unused(&rig.config, &workspace, &CancellationToken::new())
                .await
                .map_err(|error| error.to_string())?;
            rig.world
                .config
                .edit(remove(&workspace), &CancellationToken::new())
                .await
                .map_err(|error| error.to_string())?;
            drop(hold);
            Ok::<_, String>(())
        };
        let releasing = async {
            assert!(rig.gateway().until_held(&create).await);
            tokio::time::sleep(Duration::from_millis(200)).await;
            rig.gateway().release(&create);
        };
        let (outcome, removed, ()) = tokio::join!(launching, removing, releasing);

        ready(outcome)?;
        assert!(
            removed.is_err_and(|error| error.contains("platform-backend")),
            "the removal ran while the launch was creating its sandbox"
        );
        assert!(
            rig.world
                .config
                .load()
                .await?
                .workspace(&workspace)
                .is_some()
        );
        Ok(())
    })
    .await
}

#[tokio::test]
async fn a_launch_whose_workspace_was_removed_creates_no_sandbox() -> TestResult {
    guard(async {
        let rig = launch_rig().await?;
        let plan = rig.plan(1)?;
        let workspace = plan.workspace().clone();
        rig.world
            .config
            .edit(remove(&workspace), &CancellationToken::new())
            .await?;

        let outcome = rig
            .run_planned(&rig.world.session, plan, CancellationToken::new())
            .await;

        let failed = failure(outcome)?;
        assert!(
            matches!(&*failed.error, LaunchError::WorkspaceRemoved(name) if *name == workspace),
            "{failed:?}"
        );
        assert!(rig.world.gateway.sandboxes().is_empty());
        Ok(())
    })
    .await
}

#[tokio::test]
async fn unreadable_workspace_holders_remain_attributable_or_conservatively_block_removal()
-> TestResult {
    guard(async {
        let rig = launch_rig().await?;
        for id in [1, 2] {
            rig.launch(id).await?;
        }
        let workspace = rig.plan(1)?.workspace().clone();
        let matching = rig.handle(1).await?;
        let unrelated = rig.handle(2).await?;
        let matching_labels = Labels::from_pairs(
            matching
                .labels()
                .iter()
                .filter(|(key, _)| key.to_string() != "jackin.dev/lead-agent")
                .map(|(key, value)| (key.to_string(), value.to_string())),
        )?;
        let unrelated_labels =
            Labels::from_pairs(unrelated.labels().iter().map(|(key, value)| {
                let value = if key.to_string() == "jackin.dev/workspace" {
                    "other-workspace".to_owned()
                } else {
                    value.to_string()
                };
                (key.to_string(), value)
            }))?;
        matching.set_labels(matching_labels);
        unrelated.set_labels(unrelated_labels);
        let gateways = Gateways::single(rig.world.session.clone());

        let matched = gateways
            .ensure_workspace_unused(&rig.config, &workspace, &CancellationToken::new())
            .await;
        let Err(WorkspaceGuardError::InUse { holders, .. }) = matched else {
            return Err(format!("matching unreadable sandbox was ignored: {matched:?}").into());
        };
        assert_eq!(holders.readable.len(), 0);
        assert_eq!(holders.unreadable.len(), 1);
        assert_eq!(holders.unreadable[0].sandbox, matching.name());
        assert!(holders.unattributable.is_empty());

        let missing_workspace = Labels::from_pairs(
            matching
                .labels()
                .iter()
                .filter(|(key, _)| key.to_string() != "jackin.dev/workspace")
                .map(|(key, value)| (key.to_string(), value.to_string())),
        )?;
        matching.set_labels(missing_workspace);
        let unattributable = gateways
            .ensure_workspace_unused(&rig.config, &workspace, &CancellationToken::new())
            .await;
        let Err(WorkspaceGuardError::InUse { holders, .. }) = unattributable else {
            return Err(format!("unattributable sandbox was ignored: {unattributable:?}").into());
        };
        assert!(holders.readable.is_empty());
        assert!(holders.unreadable.is_empty());
        assert_eq!(holders.unattributable.len(), 1);
        assert_eq!(holders.unattributable[0].sandbox, matching.name());
        Ok(())
    })
    .await
}
