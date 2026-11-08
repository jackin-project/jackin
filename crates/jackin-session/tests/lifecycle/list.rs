// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::collections::{BTreeMap, BTreeSet};
use std::time::Duration;

use jackin_core::{Labels, Phase, SandboxLabels, SandboxName, SandboxPath, Sha256, sandbox_layout};
use jackin_openshell::PowerAction;
use jackin_session::SandboxInspection;
use jackin_testkit::{Rig, SandboxHandle, TestResult, guard};
use tokio_util::sync::CancellationToken;

fn digests(handle: &SandboxHandle) -> BTreeMap<SandboxPath, Sha256> {
    handle
        .files()
        .into_iter()
        .map(|(path, bytes)| (path, Sha256::of(&bytes)))
        .collect()
}

fn name(text: &str) -> Result<SandboxName, Box<dyn std::error::Error>> {
    Ok(SandboxName::parse(text)?)
}

fn without(labels: &Labels, key: &str) -> Result<Labels, Box<dyn std::error::Error>> {
    Ok(Labels::from_pairs(
        labels
            .iter()
            .filter(|(found, _)| found.to_string() != key)
            .map(|(found, value)| (found.to_string(), value.to_string())),
    )?)
}

#[tokio::test]
async fn list_returns_exactly_the_managed_sandboxes_with_their_labels_and_phases() -> TestResult {
    guard(async {
        let rig = Rig::new().await?;
        for id in [1, 2, 3] {
            rig.launch(id).await?;
        }
        let gateway = &rig.world.gateway;
        let stopped = rig.sandbox_name(2)?;
        let errored = rig.sandbox_name(3)?;
        rig.world
            .session
            .power(&stopped, PowerAction::Stop, &CancellationToken::new())
            .await?;
        gateway.force_phase(&errored, Phase::Error { exit_code: Some(7) });
        let renamed = name("jackin-custom-9999")?;
        let shown = rig.handle(1).await?.labels();
        gateway.insert_unmanaged(&renamed, shown, Phase::Ready)?;
        gateway.insert_unmanaged(
            &name("jackin-ghost-0000")?,
            Labels::from_pairs([])?,
            Phase::Ready,
        )?;
        let not_managed =
            Labels::from_pairs([("jackin.dev/managed".to_owned(), "false".to_owned())])?;
        gateway.insert_unmanaged(&name("jackin-off-0001")?, not_managed, Phase::Ready)?;
        let before: Vec<_> = gateway
            .sandboxes()
            .iter()
            .map(|handle| {
                (
                    handle.name(),
                    digests(handle),
                    handle.runs(&sandbox_layout::GIT).len(),
                )
            })
            .collect();

        let listing = rig.world.session.list(&rig.world.cancel).await?;

        let listed: BTreeSet<SandboxName> = listing
            .sandboxes
            .iter()
            .map(|summary| summary.name.clone())
            .collect();
        let expected = BTreeSet::from([
            rig.sandbox_name(1)?,
            stopped.clone(),
            errored.clone(),
            renamed.clone(),
        ]);
        assert_eq!(listed, expected);
        assert!(listing.unreadable.is_empty());
        for summary in &listing.sandboxes {
            let handle = gateway
                .sandbox(&summary.name)
                .ok_or("a listed sandbox is gone")?;
            assert_eq!(
                summary.labels,
                SandboxLabels::from_labels(&handle.labels())?
            );
            assert_eq!(summary.phase, handle.phase());
        }
        let phase_of = |wanted: &SandboxName| {
            listing
                .sandboxes
                .iter()
                .find(|summary| &summary.name == wanted)
                .map(|summary| summary.phase)
        };
        assert_eq!(phase_of(&stopped), Some(Phase::Stopped));
        assert_eq!(
            phase_of(&errored),
            Some(Phase::Error { exit_code: Some(7) })
        );
        let after: Vec<_> = gateway
            .sandboxes()
            .iter()
            .map(|handle| {
                (
                    handle.name(),
                    digests(handle),
                    handle.runs(&sandbox_layout::GIT).len(),
                )
            })
            .collect();
        assert_eq!(after, before);
        Ok(())
    })
    .await
}

#[tokio::test]
async fn missing_or_invalid_agent_labels_remain_named_in_list_and_inspect() -> TestResult {
    guard(async {
        let rig = Rig::new().await?;
        rig.launch(1).await?;
        rig.launch(2).await?;
        let missing = rig.handle(1).await?;
        let invalid = rig.handle(2).await?;
        let missing_labels = without(&missing.labels(), "jackin.dev/lead-agent")?;
        let invalid_labels = Labels::from_pairs(
            without(&invalid.labels(), "jackin.dev/agents")?
                .iter()
                .map(|(key, value)| (key.to_string(), value.to_string()))
                .chain([("jackin.dev/agents".to_owned(), "bad".to_owned())]),
        )?;
        missing.set_labels(missing_labels);
        invalid.set_labels(invalid_labels);

        let listing = rig.world.session.list(&rig.world.cancel).await?;
        assert!(listing.sandboxes.is_empty());
        assert_eq!(listing.unreadable.len(), 2);
        for (handle, key) in [
            (&missing, "jackin.dev/lead-agent"),
            (&invalid, "jackin.dev/agents"),
        ] {
            let entry = listing
                .unreadable
                .iter()
                .find(|entry| entry.name == handle.name())
                .ok_or("degraded sandbox name was dropped")?;
            assert_eq!(entry.phase, Phase::Ready);
            assert_eq!(entry.created_at, handle.created_at());
            assert!(entry.workspace.is_some());
            assert!(entry.role.is_some());
            assert!(entry.launch.is_some());
            assert!(entry.reason().contains("missing or invalid"));
            assert!(!entry.reason().contains("bad"));
            let inspected = rig
                .world
                .session
                .inspect(&handle.name(), &rig.world.cancel)
                .await?;
            let SandboxInspection::Unreadable(inspected) = inspected else {
                return Err("strict inspection accepted invalid agent labels".into());
            };
            assert_eq!(inspected.name, handle.name());
            assert_eq!(inspected.phase, Phase::Ready);
            assert_eq!(inspected.workspace, entry.workspace);
            assert_eq!(inspected.role, entry.role);
            assert_eq!(inspected.launch, entry.launch);
            assert_eq!(inspected.reason(), entry.reason());
            match &inspected.error {
                jackin_core::LabelError::Missing { key: found }
                | jackin_core::LabelError::Invalid { key: found, .. } => {
                    assert_eq!(found, &key);
                }
                other => return Err(format!("unexpected label error: {other:?}").into()),
            }
        }
        Ok(())
    })
    .await
}

#[tokio::test]
async fn a_managed_sandbox_with_unreadable_labels_is_reported_apart_and_the_rest_is_listed()
-> TestResult {
    guard(async {
        let rig = Rig::new().await?;
        rig.launch(1).await?;
        let broken = name("jackin-broken-0002")?;
        let managed_only =
            Labels::from_pairs([("jackin.dev/managed".to_owned(), "true".to_owned())])?;
        rig.world
            .gateway
            .insert_unmanaged(&broken, managed_only, Phase::Ready)?;

        let listing = rig.world.session.list(&rig.world.cancel).await?;

        let listed: Vec<_> = listing.sandboxes.iter().map(|s| s.name.clone()).collect();
        assert_eq!(listed, vec![rig.sandbox_name(1)?]);
        let unreadable: Vec<_> = listing.unreadable.iter().map(|u| u.name.clone()).collect();
        assert_eq!(unreadable, vec![broken]);
        Ok(())
    })
    .await
}

#[tokio::test]
async fn list_and_inspect_carry_the_creation_time_the_gateway_recorded() -> TestResult {
    guard(async {
        let rig = Rig::new().await?;
        rig.launch(1).await?;
        rig.world.clock.advance(Duration::from_secs(3_600));
        rig.launch(2).await?;
        let first = rig.handle(1).await?;
        let second = rig.handle(2).await?;
        let (early, late) = (
            first.created_at().ok_or("no creation time")?,
            second.created_at().ok_or("no creation time")?,
        );
        assert!(late.0 - early.0 >= 3_600, "{early:?} {late:?}");

        let listing = rig.world.session.list(&rig.world.cancel).await?;
        let listed: BTreeMap<SandboxName, Option<jackin_core::Timestamp>> = listing
            .sandboxes
            .iter()
            .map(|summary| (summary.name.clone(), summary.created_at))
            .collect();
        assert_eq!(
            listed,
            BTreeMap::from([(first.name(), Some(early)), (second.name(), Some(late)),])
        );
        let inspected = rig
            .world
            .session
            .inspect(&second.name(), &CancellationToken::new())
            .await?;
        let SandboxInspection::Readable(inspected) = inspected else {
            return Err("expected readable inspection".into());
        };
        assert_eq!(inspected.summary.created_at, Some(late));
        Ok(())
    })
    .await
}
