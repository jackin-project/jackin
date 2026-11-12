// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin_core::sandbox_layout;
use jackin_openshell::{GatewayError, GatewayOp};
use jackin_session::ExportError;
use jackin_testkit::{Fault, ProgramScript, Rig, TestResult, Trigger, guard};
use tokio_util::sync::CancellationToken;

use super::support::{branch_with_commit, export, exports, ref_of, snapshot, tip};

#[tokio::test]
async fn sandbox_git_failures_are_typed_without_invented_exit_codes_and_change_nothing()
-> TestResult {
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
        let bare = rig.sandbox_name(1)?;

        let refused = export(&rig, &bare).await;

        assert!(
            matches!(&refused, Err(ExportError::NoRepository { sandbox }) if *sandbox == bare),
            "{refused:?}"
        );
        assert_eq!(snapshot(&rig).await?.rest, before.rest);

        let rig = Rig::new().await?;
        rig.launch(1).await?;
        let handle = rig.handle(1).await?;
        let name = handle.name();
        handle.commit_in_project("a.txt", b"a", "work").await?;
        let before = snapshot(&rig).await?;
        rig.world.gateway.script_program(
            &sandbox_layout::GIT,
            ProgramScript {
                exit_code: 0,
                stdout: b".git\n".to_vec(),
                stderr: Vec::new(),
            },
        );
        let garbled = export(&rig, &name).await;
        assert!(
            matches!(&garbled, Err(ExportError::SandboxOutput { sandbox, .. }) if *sandbox == name),
            "{garbled:?}"
        );
        assert_eq!(snapshot(&rig).await?.rest, before.rest);

        let rig = Rig::new().await?;
        rig.launch(1).await?;
        let handle = rig.handle(1).await?;
        let name = handle.name();
        handle.commit_in_project("a.txt", b"a", "work").await?;
        let before = snapshot(&rig).await?;
        std::fs::write(handle.root().join("project/.git/packed-refs"), b"garbage\n")?;
        let failed = export(&rig, &name).await;
        assert!(
            matches!(failed, Err(ExportError::SandboxGit { exit_code: 128, .. })),
            "{failed:?}"
        );
        assert_eq!(snapshot(&rig).await?.rest, before.rest);
        Ok(())
    })
    .await
}

#[tokio::test]
async fn a_ref_name_collision_is_a_ref_conflict_and_the_update_is_all_or_nothing() -> TestResult {
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

#[tokio::test]
async fn a_stale_lock_on_one_target_ref_fails_the_update_as_a_host_git_error_and_exports_no_ref()
-> TestResult {
    guard(async {
        let rig = Rig::new().await?;
        rig.launch(1).await?;
        let handle = rig.handle(1).await?;
        let name = handle.name();
        branch_with_commit(&handle, "a", "a.txt").await?;
        branch_with_commit(&handle, "b", "b.txt").await?;
        let target = rig
            .repo
            .root()
            .as_path()
            .join(".git")
            .join(ref_of(&name, "b"));
        tokio::fs::create_dir_all(target.parent().ok_or("no parent")?).await?;
        let lock = target.with_extension("lock");
        tokio::fs::write(&lock, b"").await?;
        let before = snapshot(&rig).await?;

        let failed = export(&rig, &name).await;

        assert!(
            matches!(failed, Err(ExportError::HostGit { .. })),
            "{failed:?}"
        );
        let after = snapshot(&rig).await?;
        assert!(exports(&after).is_empty(), "{:?}", exports(&after));
        assert_eq!(after.rest, before.rest);

        tokio::fs::remove_file(&lock).await?;
        export(&rig, &name).await?;
        let healed = exports(&snapshot(&rig).await?);
        assert!(
            healed.contains_key(&ref_of(&name, "a")) && healed.contains_key(&ref_of(&name, "b"))
        );
        Ok(())
    })
    .await
}
