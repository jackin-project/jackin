// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::collections::BTreeMap;
use std::error::Error;

use jackin_core::{
    NameSuffix, Phase, SandboxName, SandboxPath, SandboxSlug, SupervisorState, sandbox_layout,
};
use jackin_session::{LaunchError, LaunchStep};

use super::support::{TestResult, failure, guard, has_file, launch_id, launch_rig, ready, running};
use jackin_testkit::SandboxHandle;

type Snapshot = (
    BTreeMap<SandboxPath, Vec<u8>>,
    Phase,
    String,
    Option<SupervisorState>,
);

fn snapshot(handle: &SandboxHandle) -> Snapshot {
    (
        handle.files(),
        handle.phase(),
        format!("{:?}", handle.labels()),
        handle.supervisor(),
    )
}

#[tokio::test]
async fn a_new_launch_id_makes_a_new_sandbox_and_leaves_the_live_one_untouched() -> TestResult {
    guard(async {
        for exited in [false, true] {
            let rig = launch_rig().await?;
            ready(rig.launch_observed(1).await?)?;
            let first = rig.handle(1).await?;
            if exited {
                first.exit_codex(0);
            }
            let before = snapshot(&first);
            let second = ready(rig.launch_observed(2).await?)?;
            assert_eq!(second.sandbox, rig.sandbox_name(2)?);
            assert_eq!(rig.gateway().sandboxes().len(), 2);
            assert_eq!(first.name().as_str(), "jackin-platfor-ij4f");
            assert_eq!(second.sandbox.as_str(), "jackin-platfor-uxe1");
            assert_eq!(snapshot(&first), before);
            assert!(!has_file(&first, &sandbox_layout::env_file(&launch_id(2))));
            let other = rig.handle(2).await?;
            assert_eq!(other.supervisor(), Some(running(1)?));
            assert!(has_file(
                &other,
                &sandbox_layout::uploaded_marker(&launch_id(2))
            ));
            assert!(has_file(
                &other,
                &sandbox_layout::setup_done_marker(&launch_id(2))
            ));
            assert_eq!(rig.world.engine.builds().len(), 3);
            let label = |handle: &SandboxHandle| {
                handle
                    .labels()
                    .iter()
                    .find(|(key, _)| key.to_string() == "jackin.dev/image")
                    .map(|(_, value)| value.to_string())
            };
            assert!(label(&first).is_some());
            assert_eq!(label(&first), label(&other));
        }
        Ok(())
    })
    .await
}

#[tokio::test]
async fn a_name_collision_or_an_ambiguous_label_is_a_typed_stop_that_changes_nothing() -> TestResult
{
    guard(async {
        let rig = launch_rig().await?;
        ready(rig.launch_observed(9).await?)?;
        let name = rig.sandbox_name(1)?;
        let other = rig.foreign_sandbox(name.clone(), 3).await?;
        let before = snapshot(&other);
        let failed = failure(rig.launch_observed(1).await?)?;
        assert_eq!(failed.step, LaunchStep::Sandbox);
        assert!(
            matches!(&*failed.error, LaunchError::NameCollision { name: found, other: Some(launch) } if *found == name && *launch == launch_id(3)),
            "{:?}",
            failed.error
        );
        assert_eq!(rig.gateway().sandboxes().len(), 2);
        assert_eq!(snapshot(&other), before);

        let rig = launch_rig().await?;
        ready(rig.launch_observed(9).await?)?;
        let make = |suffix: &str| -> Result<SandboxName, Box<dyn Error>> {
            Ok(SandboxName::new(&SandboxSlug::parse("dup")?, &NameSuffix::parse(suffix)?))
        };
        let (a, b) = (make("aaaa")?, make("bbbb")?);
        let first = rig.foreign_sandbox(a.clone(), 1).await?;
        let second = rig.foreign_sandbox(b.clone(), 1).await?;
        let (before_a, before_b) = (snapshot(&first), snapshot(&second));
        let failed = failure(rig.launch_observed(1).await?)?;
        assert_eq!(failed.step, LaunchStep::Sandbox);
        assert!(
            matches!(&*failed.error, LaunchError::AmbiguousLaunch { launch, sandboxes } if *launch == launch_id(1) && *sandboxes == [a, b]),
            "{:?}",
            failed.error
        );
        assert_eq!(rig.gateway().sandboxes().len(), 3);
        assert_eq!((snapshot(&first), snapshot(&second)), (before_a, before_b));
        Ok(())
    })
    .await
}
