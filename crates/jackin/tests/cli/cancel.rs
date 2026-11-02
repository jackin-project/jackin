// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin::ExitClass;
use jackin_core::{HookKind, sandbox_layout};
use jackin_openshell::GatewayOp;
use jackin_testkit::{Fault, HostSnapshot, Rig, TestResult, Trigger, guard, launch_id};
use tokio_util::sync::CancellationToken;

use super::support::{Failure, Harness, ScriptedTerminal, Step};

async fn rigged() -> Result<(Rig, Harness), Failure> {
    let rig = Rig::new().await?;
    let mut harness = Harness::new(&rig.world);
    harness.cwd = rig.repo.root().clone();
    harness.write_config(&rig.config.render())?;
    Ok((rig, harness))
}

#[tokio::test]
async fn a_cancelled_load_keeps_the_sandbox_names_it_and_a_rerun_completes() -> TestResult {
    guard(async {
        let (rig, harness) = rigged().await?;
        let token = CancellationToken::new();
        rig.gateway().inject(Fault::CancelAt {
            trigger: Trigger::Op(GatewayOp::UploadTree),
            token: token.clone(),
        });
        let id = launch_id(1).to_string();
        let ran = harness
            .command(&["load", "--launch-id", &id])
            .cancel(token)
            .run()
            .await?;
        assert_eq!(ran.exit, ExitClass::Cancelled, "{}", ran.stderr);
        assert!(ran.stdout.is_empty());
        let name = rig.sandbox_name(1)?;
        let message = ran
            .stderr
            .lines()
            .find(|line| line.starts_with("cancelled"))
            .ok_or("no cancelled line")?;
        assert!(
            message.contains(name.as_str()) && message.contains(&id),
            "{}",
            ran.stderr
        );
        assert_eq!(rig.world.gateway.sandboxes().len(), 1);

        harness.run(&["load", "--launch-id", &id]).await?.ok()?;
        assert_eq!(rig.world.gateway.sandboxes().len(), 1);
        Ok(())
    })
    .await
}

#[tokio::test]
async fn a_cancelled_build_stores_no_image_and_the_same_build_uncancelled_succeeds() -> TestResult {
    guard(async {
        let (rig, harness) = rigged().await?;
        let token = CancellationToken::new();
        token.cancel();
        let ran = harness.command(&["build"]).cancel(token).run().await?;
        assert_eq!(ran.exit, ExitClass::Cancelled, "{}", ran.stderr);
        assert!(ran.stdout.is_empty());
        assert!(rig.world.engine.images().is_empty());

        harness.run(&["build"]).await?.ok()?;
        assert!(!rig.world.engine.images().is_empty());
        Ok(())
    })
    .await
}

#[tokio::test]
async fn a_cancelled_export_writes_no_ref_and_the_same_export_uncancelled_succeeds() -> TestResult {
    guard(async {
        let (rig, harness) = rigged().await?;
        rig.launch(1).await?;
        let handle = rig.handle(1).await?;
        handle.git(&["checkout", "-q", "-b", "feature"]).await?;
        handle
            .commit_in_project("new.txt", b"new\n", "work")
            .await?;
        let name = handle.name();
        let token = CancellationToken::new();
        token.cancel();
        let ran = harness
            .command(&["export", name.as_str()])
            .cancel(token)
            .run()
            .await?;
        assert_eq!(ran.exit, ExitClass::Cancelled, "{}", ran.stderr);
        assert!(
            HostSnapshot::take(rig.repo.root())
                .await?
                .exported
                .is_empty()
        );

        harness.run(&["export", name.as_str()]).await?.ok()?;
        assert!(
            !HostSnapshot::take(rig.repo.root())
                .await?
                .exported
                .is_empty()
        );
        Ok(())
    })
    .await
}

#[tokio::test]
async fn a_restart_cancelled_after_preflight_exits_cancelled_and_leaves_the_start_request_alone()
-> TestResult {
    guard(async {
        let (rig, harness) = rigged().await?;
        rig.launch(1).await?;
        let handle = rig.handle(1).await?;
        handle.exit_codex(2);
        let start = handle.file(&sandbox_layout::RUN_START);
        let token = CancellationToken::new();
        rig.gateway().inject(Fault::CancelAt {
            trigger: Trigger::Program(sandbox_layout::hook_path(HookKind::Preflight)),
            token: token.clone(),
        });
        let (terminal, _) = ScriptedTerminal::new(vec![Step::Bytes(b"\x10\x11".to_vec())])?;
        let name = handle.name();
        let ran = harness
            .command(&["attach", name.as_str(), "--restart"])
            .terminal(terminal)
            .cancel(token)
            .run()
            .await?;
        assert_eq!(ran.exit, ExitClass::Cancelled, "{}", ran.stderr);
        assert_eq!(handle.file(&sandbox_layout::RUN_START), start);
        Ok(())
    })
    .await
}
