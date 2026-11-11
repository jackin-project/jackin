// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::time::Duration;

use jackin_openshell::{Gateway, GatewayOp};
use jackin_session::ExportError;
use jackin_testkit::{Fault, Rig, TestResult, Trigger, guard};
use tokio_util::sync::CancellationToken;

use super::support::{
    branch_with_commit, bundles_left, export, exports, ref_of, snapshot, staging_left,
};

#[tokio::test]
async fn two_concurrent_exports_of_one_sandbox_both_succeed_and_leave_no_staging_and_no_bundle()
-> TestResult {
    guard(async {
        let rig = Rig::new().await?;
        rig.launch(1).await?;
        let handle = rig.handle(1).await?;
        let name = handle.name();
        let tip = handle.commit_in_project("a.txt", b"a", "work").await?;
        let session = &rig.world.session;

        let (first, second) = tokio::join!(
            session.export(&rig.config, &name, CancellationToken::new()),
            session.export(&rig.config, &name, CancellationToken::new()),
        );

        assert_eq!(first?.branches, second?.branches);
        let refs = exports(&snapshot(&rig).await?);
        assert_eq!(
            refs.get(&ref_of(&name, "main")).map(String::as_str),
            Some(tip.as_str())
        );
        assert_eq!(staging_left(&rig)?, Vec::<String>::new());
        assert_eq!(bundles_left(&handle), Vec::<String>::new());
        Ok(())
    })
    .await
}

#[tokio::test]
async fn an_export_that_bundled_an_older_tip_never_moves_the_recovery_ref_back() -> TestResult {
    guard(async {
        let rig = Rig::new().await?;
        rig.launch(1).await?;
        let handle = rig.handle(1).await?;
        let name = handle.name();
        let older = handle.commit_in_project("a.txt", b"a", "older").await?;
        let gateway = &rig.world.gateway;
        let held = Trigger::Op(GatewayOp::Download);
        gateway.inject(Fault::HoldAt {
            trigger: held.clone(),
        });

        let slow = export(&rig, &name);
        let fast = async {
            assert!(gateway.until_held(&held).await);
            let newer = handle.commit_in_project("b.txt", b"b", "newer").await;
            let report = export(&rig, &name).await;
            gateway.release(&held);
            (newer, report)
        };
        let (slow, (newer, fast)) = tokio::join!(slow, fast);

        let newer = newer?;
        assert_ne!(older, newer);
        let landed = exports(&snapshot(&rig).await?);
        assert_eq!(
            landed.get(&ref_of(&name, "main")).map(String::as_str),
            Some(newer.as_str()),
            "slow: {slow:?}"
        );
        let fast = fast?;
        assert_eq!(fast.branches.len(), 1);
        assert_eq!(fast.branches[0].tip, newer);
        if let Ok(report) = slow {
            assert_eq!(report.branches.len(), 1);
            assert_eq!(report.branches[0].tip, newer);
        }
        assert_eq!(staging_left(&rig)?, Vec::<String>::new());
        assert_eq!(bundles_left(&handle), Vec::<String>::new());
        Ok(())
    })
    .await
}

#[tokio::test]
async fn an_older_export_never_moves_a_ref_an_earlier_export_created_back() -> TestResult {
    guard(async {
        let rig = Rig::new().await?;
        rig.launch(1).await?;
        let handle = rig.handle(1).await?;
        let name = handle.name();
        handle.commit_in_project("first.txt", b"f", "first").await?;
        export(&rig, &name).await?;
        let older = handle.commit_in_project("a.txt", b"a", "older").await?;
        let gateway = &rig.world.gateway;
        let held = Trigger::Op(GatewayOp::Download);
        gateway.inject(Fault::HoldAt {
            trigger: held.clone(),
        });
        let slow = export(&rig, &name);
        let fast = async {
            assert!(gateway.until_held(&held).await);
            let newer = handle.commit_in_project("b.txt", b"b", "newer").await;
            let report = export(&rig, &name).await;
            gateway.release(&held);
            (newer, report)
        };
        let (slow, (newer, fast)) = tokio::join!(slow, fast);
        let newer = newer?;
        assert_ne!(older, newer);
        fast?;
        let landed = exports(&snapshot(&rig).await?);
        assert_eq!(
            landed.get(&ref_of(&name, "main")).map(String::as_str),
            Some(newer.as_str()),
            "slow: {slow:?}"
        );
        Ok(())
    })
    .await
}

#[tokio::test]
async fn an_export_finishes_the_refs_that_another_run_has_not_published_yet() -> TestResult {
    guard(async {
        let rig = Rig::new().await?;
        rig.launch(1).await?;
        let handle = rig.handle(1).await?;
        let name = handle.name();
        let x = branch_with_commit(&handle, "x", "x.txt").await?;
        let y = branch_with_commit(&handle, "y", "y.txt").await?;
        let gateway = &rig.world.gateway;
        let held = Trigger::Op(GatewayOp::Download);
        gateway.inject(Fault::HoldAt {
            trigger: held.clone(),
        });

        let slow = export(&rig, &name);
        let other = async {
            assert!(gateway.until_held(&held).await);
            let source = handle.root().join("project").display().to_string();
            let target = format!("refs/heads/x:{}", ref_of(&name, "x"));
            let fetched = rig.repo.git(&["fetch", "-q", &source, &target]).await;
            gateway.release(&held);
            fetched
        };
        let (slow, fetched) = tokio::join!(slow, other);
        fetched?;

        let report = slow?;
        assert_eq!(report.branches.len(), 2);
        let landed = exports(&snapshot(&rig).await?);
        assert_eq!(
            landed.get(&ref_of(&name, "x")).map(String::as_str),
            Some(x.as_str())
        );
        assert_eq!(
            landed.get(&ref_of(&name, "y")).map(String::as_str),
            Some(y.as_str())
        );
        assert_eq!(staging_left(&rig)?, Vec::<String>::new());
        Ok(())
    })
    .await
}

#[tokio::test]
async fn an_export_never_ships_a_replacement_sandbox_into_the_recovery_refs() -> TestResult {
    guard(async {
        let rig = Rig::new().await?;
        rig.launch(1).await?;
        let name = rig.handle(1).await?.name();
        rig.handle(1)
            .await?
            .commit_in_project("a.txt", b"a", "original")
            .await?;
        let gateway = &rig.world.gateway;
        let held = Trigger::Op(GatewayOp::Program);
        gateway.inject(Fault::HoldAt {
            trigger: held.clone(),
        });

        let exporting = export(&rig, &name);
        let swapping = async {
            assert!(gateway.until_held(&held).await);
            gateway.delete_sandbox(&name).await?;
            rig.world.clock.advance(Duration::from_secs(60));
            rig.launch(1).await?;
            rig.handle(1)
                .await?
                .commit_in_project("b.txt", b"b", "replacement")
                .await?;
            gateway.release(&held);
            Ok::<_, Box<dyn std::error::Error>>(())
        };
        let (exported, swapped) = tokio::join!(exporting, swapping);

        swapped?;
        let Err(ExportError::Replaced { sandbox }) = exported else {
            return Err(format!("expected a refusal, saw {exported:?}").into());
        };
        assert_eq!(sandbox, name);
        assert_eq!(exports(&snapshot(&rig).await?).len(), 0);
        assert_eq!(staging_left(&rig)?, Vec::<String>::new());
        Ok(())
    })
    .await
}
