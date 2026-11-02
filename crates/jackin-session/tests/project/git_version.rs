// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin_session::{HostGitError, LaunchError, LaunchStep, MINIMUM_GIT, check_git};
use jackin_testkit::{ProcessReply, RepoFixture};
use tokio_util::sync::CancellationToken;

use super::support::{Rig, TestResult, assert_nothing_created, failure, guard, ready};

fn script(rig: &Rig, reply: ProcessReply) {
    rig.world
        .runner
        .scripted()
        .expect_git(&["--version"], reply);
}

async fn rig() -> Result<Rig, Box<dyn std::error::Error>> {
    let repo = RepoFixture::new().await?;
    repo.file("a.txt", b"alpha\n").await?;
    repo.commit().await?;
    Rig::new(repo).await
}

#[tokio::test]
async fn the_version_floor_is_numeric_and_a_bad_git_fails_typed_before_any_gateway_state()
-> TestResult {
    guard(async {
        for text in [
            "git version 2.54.0 (Apple Git-157)\n",
            "git version 2.45.0\n",
        ] {
            let rig = rig().await?;
            script(&rig, ProcessReply::exited(0, text.as_bytes(), b""));
            let found = check_git(rig.world.runner.as_ref(), &CancellationToken::new()).await?;
            assert!(found >= MINIMUM_GIT);
            ready(rig.launch(1).await?)?;
        }
        for text in ["git version 2.44.2\n", "git version 2.9.0\n"] {
            let rig = rig().await?;
            script(&rig, ProcessReply::exited(0, text.as_bytes(), b""));
            let error = check_git(rig.world.runner.as_ref(), &CancellationToken::new())
                .await
                .unwrap_err();
            assert!(
                matches!(error, HostGitError::TooOld { required, .. } if required == MINIMUM_GIT)
            );
            let failed = failure(rig.launch(1).await?)?;
            assert_eq!(failed.step, LaunchStep::Upload);
            assert!(matches!(
                *failed.error,
                LaunchError::HostGit(HostGitError::TooOld { .. })
            ));
            assert_nothing_created(&rig)?;
        }
        for reply in [
            ProcessReply::exited(0, b"git version banana\n", b""),
            ProcessReply::exited(1, b"", b"boom"),
            ProcessReply::spawn("no such file"),
        ] {
            let rig = rig().await?;
            script(&rig, reply);
            let error = check_git(rig.world.runner.as_ref(), &CancellationToken::new())
                .await
                .unwrap_err();
            assert!(matches!(error, HostGitError::Unusable { .. }), "{error:?}");
            let failed = failure(rig.launch(1).await?)?;
            assert!(matches!(
                *failed.error,
                LaunchError::HostGit(HostGitError::Unusable { .. })
            ));
            assert_nothing_created(&rig)?;
        }
        Ok(())
    })
    .await
}
