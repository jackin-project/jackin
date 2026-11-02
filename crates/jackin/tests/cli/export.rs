// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin::ExitClass;
use jackin_core::{ConfigEdit, WorkspaceName};
use jackin_openshell::PowerAction;
use jackin_testkit::{HostSnapshot, Rig, SandboxHandle, TestResult, guard};

use super::support::{Failure, Harness, failed};

async fn work() -> Result<(Rig, Harness, SandboxHandle, String), Failure> {
    let rig = Rig::new().await?;
    rig.launch(1).await?;
    let handle = rig.handle(1).await?;
    handle.git(&["checkout", "-q", "-b", "feature"]).await?;
    handle
        .commit_in_project("new.txt", b"new\n", "work")
        .await?;
    let tip = handle
        .git(&["rev-parse", "feature"])
        .await?
        .trim()
        .to_owned();
    let mut harness = Harness::new(&rig.world);
    harness.cwd = rig.repo.root().clone();
    harness.write_config(&rig.config.render())?;
    Ok((rig, harness, handle, tip))
}

#[tokio::test]
async fn export_fetches_each_branch_into_the_jackin_namespace_without_touching_the_host_tree()
-> TestResult {
    guard(async {
        let (rig, harness, handle, tip) = work().await?;
        let name = handle.name();
        let before = HostSnapshot::take(rig.repo.root()).await?;
        let ran = harness.run(&["export", name.as_str()]).await?;
        ran.ok()?;
        let wanted = format!("refs/heads/jackin/{name}/feature");
        let after = HostSnapshot::take(rig.repo.root()).await?;
        assert_eq!(after.exported.get(&wanted), Some(&tip));
        assert!(ran.stdout.contains(&wanted), "{}", ran.stdout);
        assert_eq!(after.rest, before.rest);

        let again = harness.run(&["export", name.as_str()]).await?;
        again.ok()?;
        assert_eq!(
            HostSnapshot::take(rig.repo.root()).await?.exported,
            after.exported
        );

        rig.launch(2).await?;
        let fresh = rig.sandbox_name(2)?;
        harness.run(&["export", fresh.as_str()]).await?.ok()?;
        let all = HostSnapshot::take(rig.repo.root()).await?.exported;
        assert!(
            all.keys().all(|name| !name.contains(fresh.as_str())),
            "{all:?}"
        );
        Ok(())
    })
    .await
}

#[tokio::test]
async fn export_of_a_stopped_unknown_or_unregistered_sandbox_fails_and_writes_no_ref() -> TestResult
{
    guard(async {
        let (rig, harness, handle, _tip) = work().await?;
        let name = handle.name();
        let before = HostSnapshot::take(rig.repo.root()).await?;
        rig.world.session.power(&name, PowerAction::Stop).await?;
        let stopped = harness.run(&["export", name.as_str()]).await?;
        failed(&stopped, ExitClass::Failure, &[name.as_str()])?;
        let unknown = harness.run(&["export", "jackin-absent-0009"]).await?;
        failed(&unknown, ExitClass::Failure, &["jackin-absent-0009"])?;

        rig.world.session.power(&name, PowerAction::Start).await?;
        let mut config = rig.config.clone();
        let workspace = WorkspaceName::parse("platform-backend")?;
        config.apply(ConfigEdit::RemoveWorkspace(workspace))?;
        harness.write_config(&config.render())?;
        let removed = harness.run(&["export", name.as_str()]).await?;
        failed(&removed, ExitClass::Failure, &["platform-backend"])?;
        let after = HostSnapshot::take(rig.repo.root()).await?;
        assert_eq!(after.exported, before.exported);
        assert_eq!(after.rest, before.rest);
        Ok(())
    })
    .await
}
