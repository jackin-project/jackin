// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::time::Duration;

use jackin_core::{ConfigEdit, HostPath, WorkspaceName};
use jackin_openshell::GatewayOp;
use jackin_session::{Gateways, LaunchError};
use jackin_testkit::{Fault, RepoFixture, Trigger};
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
                .edit(remove(&workspace))
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
        rig.world.config.edit(remove(&workspace)).await?;

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
async fn a_launch_whose_workspace_was_replaced_creates_no_sandbox() -> TestResult {
    guard(async {
        let rig = launch_rig().await?;
        let plan = rig.plan(1)?;
        let workspace = plan.workspace().clone();
        let mut replacement = rig
            .config
            .workspace(&workspace)
            .cloned()
            .ok_or("workspace entry")?;
        replacement.path = HostPath::parse(&rig.home.as_path().join("replaced"))?;
        rig.world
            .config
            .edit(vec![
                ConfigEdit::RemoveWorkspace(workspace.clone()),
                ConfigEdit::AddWorkspace {
                    name: workspace.clone(),
                    workspace: replacement,
                },
            ])
            .await?;

        let outcome = rig
            .run_planned(&rig.world.session, plan, CancellationToken::new())
            .await;

        let failed = failure(outcome)?;
        assert!(
            matches!(&*failed.error, LaunchError::WorkspaceReplaced(name) if *name == workspace),
            "{failed:?}"
        );
        assert!(rig.world.gateway.sandboxes().is_empty());
        Ok(())
    })
    .await
}

#[tokio::test]
async fn a_launch_whose_repository_was_replaced_at_the_same_path_creates_no_sandbox() -> TestResult
{
    guard(async {
        let rig = launch_rig().await?;
        let plan = rig.plan(1)?;
        let workspace = plan.workspace().clone();
        let path = rig.project.path().as_path().to_path_buf();
        let moved = rig.home.as_path().join("jackin-moved-away");
        std::fs::rename(&path, &moved)?;
        let replacement = RepoFixture::new().await?;
        std::fs::rename(replacement.root().as_path(), &path)?;

        let outcome = rig
            .run_planned(&rig.world.session, plan, CancellationToken::new())
            .await;

        let failed = failure(outcome)?;
        assert!(
            matches!(&*failed.error, LaunchError::WorkspaceReplaced(name) if *name == workspace),
            "{failed:?}"
        );
        assert!(rig.world.gateway.sandboxes().is_empty());
        Ok(())
    })
    .await
}
