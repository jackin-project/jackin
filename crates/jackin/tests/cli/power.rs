// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin::ExitClass;
use jackin_core::{Labels, Phase, SandboxName, SandboxPath, SupervisorState};
use jackin_testkit::{Rig, TestResult, guard};

use super::support::{Failure, Harness, failed};

fn project_file(path: &str) -> Result<SandboxPath, Failure> {
    Ok(SandboxPath::parse(path)?)
}

#[tokio::test]
async fn stop_then_start_keeps_the_files_and_returns_the_supervisor_to_waiting() -> TestResult {
    guard(async {
        let rig = Rig::new().await?;
        rig.launch(1).await?;
        let handle = rig.handle(1).await?;
        let name = handle.name();
        let harness = Harness::new(&rig.world);
        harness.write_config(&rig.config.render())?;
        let source = project_file("/sandbox/project/src/main.rs")?;
        let kept = handle.file(&source);
        assert!(kept.is_some());

        harness.run(&["stop", name.as_str()]).await?.ok()?;
        assert_eq!(handle.phase(), Phase::Stopped);
        harness.run(&["start", name.as_str()]).await?.ok()?;
        assert_eq!(handle.phase(), Phase::Ready);
        assert_eq!(handle.file(&source), kept);
        assert_eq!(handle.supervisor(), Some(SupervisorState::Waiting));
        Ok(())
    })
    .await
}

#[tokio::test]
async fn an_errored_unknown_or_unmanaged_sandbox_cannot_be_stopped_or_started() -> TestResult {
    guard(async {
        let rig = Rig::new().await?;
        rig.launch(1).await?;
        let handle = rig.handle(1).await?;
        let name = handle.name();
        rig.world
            .gateway
            .force_phase(&name, Phase::Error { exit_code: Some(3) });
        let ghost = SandboxName::parse("jackin-ghost-0000")?;
        rig.world
            .gateway
            .insert_unmanaged(&ghost, Labels::from_pairs([])?, Phase::Ready)?;
        let harness = Harness::new(&rig.world);
        harness.write_config(&rig.config.render())?;

        for verb in ["stop", "start"] {
            let ran = harness.run(&[verb, name.as_str()]).await?;
            failed(&ran, ExitClass::Failure, &[name.as_str(), "3"])?;
            assert_eq!(handle.phase(), Phase::Error { exit_code: Some(3) });
            for other in [ghost.as_str(), "jackin-absent-0009"] {
                failed(
                    &harness.run(&[verb, other]).await?,
                    ExitClass::Failure,
                    &[other],
                )?;
            }
        }
        let ghost_handle = rig.world.gateway.sandbox(&ghost).ok_or("ghost gone")?;
        assert_eq!(ghost_handle.phase(), Phase::Ready);
        Ok(())
    })
    .await
}
