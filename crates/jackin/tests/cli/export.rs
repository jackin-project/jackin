// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::collections::BTreeMap;

use jackin::ExitClass;
use jackin_core::{ConfigEdit, WorkspaceName};
use jackin_openshell::PowerAction;
use jackin_testkit::{HostSnapshot, Rig, SandboxHandle, TestResult, guard};

use super::support::{Failure, Harness, failed};
use tokio_util::sync::CancellationToken;

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
        rig.world
            .session
            .power(&name, PowerAction::Stop, &CancellationToken::new())
            .await?;
        let stopped = harness.run(&["export", name.as_str()]).await?;
        failed(&stopped, ExitClass::Failure, &[name.as_str()])?;
        let unknown = harness.run(&["export", "jackin-absent-0009"]).await?;
        failed(&unknown, ExitClass::Failure, &["jackin-absent-0009"])?;

        rig.world
            .session
            .power(&name, PowerAction::Start, &CancellationToken::new())
            .await?;
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

async fn linked_commit(handle: &SandboxHandle, name: &str) -> Result<String, Failure> {
    let path = handle.root().join(name).display().to_string();
    handle
        .git(&["worktree", "add", "-q", "--detach", &path])
        .await?;
    std::fs::write(std::path::Path::new(&path).join("linked.txt"), b"linked\n")?;
    handle.git(&["-C", &path, "add", "-A"]).await?;
    handle
        .git(&["-C", &path, "commit", "-q", "-m", "linked"])
        .await?;
    Ok(handle
        .git(&["-C", &path, "rev-parse", "HEAD"])
        .await?
        .trim()
        .to_owned())
}

fn printed(stdout: &str) -> Result<BTreeMap<String, String>, Failure> {
    stdout
        .lines()
        .map(|line| {
            line.split_once('\t')
                .map(|(host_ref, tip)| (host_ref.to_owned(), tip.to_owned()))
                .ok_or_else(|| Failure::from(format!("not a ref and a tip: {line:?}")))
        })
        .collect()
}

async fn published(rig: &Rig, before: &HostSnapshot) -> Result<BTreeMap<String, String>, Failure> {
    let after = HostSnapshot::take(rig.repo.root()).await?;
    Ok(after
        .exported
        .into_iter()
        .filter(|(host_ref, tip)| before.exported.get(host_ref) != Some(tip))
        .collect())
}

#[tokio::test]
async fn export_prints_every_ref_it_published_including_a_detached_linked_worktree() -> TestResult {
    guard(async {
        let rig = Rig::new().await?;
        rig.launch(1).await?;
        let handle = rig.handle(1).await?;
        let name = handle.name();
        let mut harness = Harness::new(&rig.world);
        harness.cwd = rig.repo.root().clone();
        harness.write_config(&rig.config.render())?;

        let quiet = harness.run(&["export", name.as_str()]).await?;
        quiet.ok()?;
        assert_eq!(quiet.stdout, "");
        assert!(
            quiet.stderr.contains("nothing to export"),
            "{}",
            quiet.stderr
        );

        let before = HostSnapshot::take(rig.repo.root()).await?;
        let linked = linked_commit(&handle, "w2").await?;
        let only = harness.run(&["export", name.as_str()]).await?;
        only.ok()?;
        let expected = published(&rig, &before).await?;
        assert_eq!(
            expected.keys().collect::<Vec<_>>(),
            [&format!("refs/heads/jackin/{name}/worktree/w2")]
        );
        assert_eq!(expected.values().collect::<Vec<_>>(), [&linked]);
        assert_eq!(printed(&only.stdout)?, expected, "{}", only.stdout);
        assert!(
            !only.stderr.contains("nothing to export"),
            "{}",
            only.stderr
        );

        rig.launch(2).await?;
        let handle = rig.handle(2).await?;
        let name = handle.name();
        let before = HostSnapshot::take(rig.repo.root()).await?;
        handle.git(&["checkout", "-q", "-b", "feature"]).await?;
        handle
            .commit_in_project("branch.txt", b"branch\n", "branch")
            .await?;
        handle.git(&["checkout", "-q", "--detach"]).await?;
        handle
            .commit_in_project("detached.txt", b"detached\n", "detached")
            .await?;
        linked_commit(&handle, "w3").await?;
        let all = harness.run(&["export", name.as_str()]).await?;
        all.ok()?;
        let expected = published(&rig, &before).await?;
        assert_eq!(expected.len(), 3, "{expected:?}");
        assert_eq!(printed(&all.stdout)?, expected, "{}", all.stdout);
        Ok(())
    })
    .await
}
