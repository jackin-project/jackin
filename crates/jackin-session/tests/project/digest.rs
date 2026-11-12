// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin_core::sandbox_layout;
use jackin_openshell::{GatewayError, TransferOp};
use jackin_session::{LaunchError, LaunchStep};
use jackin_testkit::{Fault, RepoFixture};

use super::support::{Rig, TestResult, failure, guard, launch_id};

#[tokio::test]
async fn a_bundle_that_lands_corrupt_fails_the_launch_before_any_repository_exists() -> TestResult {
    guard(async {
        let repo = RepoFixture::new().await?;
        repo.file("a.txt", b"alpha\n").await?;
        repo.commit().await?;
        let rig = Rig::new(repo).await?;
        rig.world.gateway.inject(Fault::CorruptUpload);
        let failed = failure(rig.launch(1).await?)?;
        assert_eq!(failed.step, LaunchStep::Upload);
        assert!(matches!(
            *failed.error,
            LaunchError::Gateway(GatewayError::Transfer {
                op: TransferOp::Digest,
                ..
            })
        ));
        let handle = rig.handle(1).await?;
        assert!(!handle.root().join("project/.git").exists());
        assert!(
            handle
                .file(&sandbox_layout::uploaded_marker(&launch_id(1)))
                .is_none()
        );
        Ok(())
    })
    .await
}
