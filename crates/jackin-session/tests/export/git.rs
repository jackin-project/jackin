// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin_openshell::{GatewayError, GatewayOp};
use jackin_session::ExportError;
use jackin_testkit::{Fault, Rig, TestResult, Trigger, guard};
use tokio_util::sync::CancellationToken;

use super::support::{branch_with_commit, export, exports, ref_of, snapshot, tip};

#[tokio::test]
async fn a_sandbox_without_a_repository_is_a_sandbox_git_error_and_changes_nothing() -> TestResult {
    guard(async {
        let rig = Rig::new().await?;
        rig.world.gateway.inject(Fault::FailAt {
            trigger: Trigger::Op(GatewayOp::UploadTree),
            error: GatewayError::Protocol {
                message: "injected".to_owned(),
            },
        });
        let launched = rig.launch_with(1, CancellationToken::new()).await?;
        assert!(launched.is_err(), "{launched:?}");
        let before = snapshot(&rig).await?;

        let refused = export(&rig, &rig.sandbox_name(1)?).await;

        assert!(
            matches!(
                refused,
                Err(ExportError::SandboxGit { exit_code, .. }) if exit_code != 0
            ),
            "{refused:?}"
        );
        assert_eq!(snapshot(&rig).await?.rest, before.rest);
        Ok(())
    })
    .await
}

#[tokio::test]
async fn a_ref_name_collision_is_a_ref_conflict_and_the_fetch_is_all_or_nothing() -> TestResult {
    guard(async {
        let rig = Rig::new().await?;
        rig.launch(1).await?;
        let handle = rig.handle(1).await?;
        let name = handle.name();
        branch_with_commit(&handle, "z", "z.txt").await?;
        let old_a = branch_with_commit(&handle, "a", "a.txt").await?;
        handle.git(&["checkout", "-q", "main"]).await?;
        export(&rig, &name).await?;
        let first = exports(&snapshot(&rig).await?);
        assert_eq!(
            first.get(&ref_of(&name, "a")).map(String::as_str),
            Some(old_a.as_str())
        );

        handle.git(&["branch", "-q", "-D", "z"]).await?;
        branch_with_commit(&handle, "z/deep", "deep.txt").await?;
        handle.git(&["checkout", "-q", "a"]).await?;
        handle
            .git(&[
                "-c",
                "user.name=t",
                "-c",
                "user.email=t@e.invalid",
                "commit",
                "-q",
                "--amend",
                "-m",
                "amended",
            ])
            .await?;
        let new_a = tip(&handle, "a").await?;
        let before = snapshot(&rig).await?;

        let conflict = export(&rig, &name).await;

        let Err(ExportError::RefConflict { host_ref, .. }) = conflict else {
            return Err(format!("expected a conflict, saw {conflict:?}").into());
        };
        assert_eq!(host_ref, ref_of(&name, "z/deep"));
        let after = snapshot(&rig).await?;
        assert_eq!(exports(&after), first);
        assert_eq!(after.rest, before.rest);

        rig.repo
            .git(&["update-ref", "-d", &ref_of(&name, "z")])
            .await?;
        export(&rig, &name).await?;
        let healed = exports(&snapshot(&rig).await?);
        assert_eq!(
            healed.get(&ref_of(&name, "a")).map(String::as_str),
            Some(new_a.as_str())
        );
        assert!(healed.contains_key(&ref_of(&name, "z/deep")));
        Ok(())
    })
    .await
}
