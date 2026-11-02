// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin_session::{GitVersion, HostGitError, LaunchError, LaunchStep, check_git};
use jackin_testkit::{ProcessReply, RepoFixture};
use tokio_util::sync::CancellationToken;

use super::support::{Rig, TestResult, assert_nothing_created, failure, guard, ready};

fn script(rig: &Rig, reply: ProcessReply) {
    rig.world.runner.scripted().expect_git_version(reply);
}

async fn fresh() -> Result<Rig, Box<dyn std::error::Error>> {
    let repo = RepoFixture::new().await?;
    repo.file("a.txt", b"alpha\n").await?;
    repo.commit().await?;
    Rig::new(repo).await
}

fn at(major: u32, minor: u32, patch: u32) -> GitVersion {
    GitVersion {
        major,
        minor,
        patch,
    }
}

async fn checked(
    text: &str,
) -> Result<Result<GitVersion, HostGitError>, Box<dyn std::error::Error>> {
    let rig = fresh().await?;
    script(
        &rig,
        ProcessReply::exited(0, format!("git version {text}\n").as_bytes(), b""),
    );
    Ok(check_git(rig.world.runner.as_ref(), &CancellationToken::new()).await)
}

#[tokio::test]
async fn each_git_line_is_admitted_only_from_its_patched_release() -> TestResult {
    guard(async {
        let refused = [
            ("2.9.0", at(2, 45, 4)),
            ("2.43.0", at(2, 45, 4)),
            ("2.44.9", at(2, 45, 4)),
            ("2.45.0", at(2, 45, 4)),
            ("2.45.3", at(2, 45, 4)),
            ("2.46.3", at(2, 46, 4)),
            ("2.47.2", at(2, 47, 3)),
            ("2.48.1", at(2, 48, 2)),
            ("2.49.0", at(2, 49, 1)),
            ("2.50.0", at(2, 50, 1)),
        ];
        for (text, required) in refused {
            let outcome = checked(text).await?;
            assert!(
                matches!(&outcome, Err(HostGitError::TooOld { required: found, .. }) if *found == required),
                "{text}: {outcome:?}"
            );
        }
        let admitted = [
            "2.45.4",
            "2.46.4",
            "2.47.3",
            "2.48.2",
            "2.49.1",
            "2.50.1",
            "2.51.0",
            "2.52.0",
            "2.55.0",
            "3.0.0",
            "2.46.10",
            "2.100.0",
            "2.54.0 (Apple Git-157)",
        ];
        for text in admitted {
            let outcome = checked(text).await?;
            assert!(outcome.is_ok(), "{text}: {outcome:?}");
        }
        Ok(())
    })
    .await
}

#[tokio::test]
async fn a_bad_git_fails_typed_before_any_gateway_state() -> TestResult {
    guard(async {
        let rig = fresh().await?;
        script(&rig, ProcessReply::exited(0, b"git version 2.50.0\n", b""));
        let failed = failure(rig.launch(1).await?)?;
        assert_eq!(failed.step, LaunchStep::Upload);
        assert!(matches!(
            *failed.error,
            LaunchError::HostGit(HostGitError::TooOld { .. })
        ));
        assert_nothing_created(&rig)?;

        let rig = fresh().await?;
        script(&rig, ProcessReply::exited(0, b"git version 2.50.1\n", b""));
        ready(rig.launch(1).await?)?;
        for reply in [
            ProcessReply::exited(0, b"git version banana\n", b""),
            ProcessReply::exited(1, b"", b"boom"),
            ProcessReply::spawn("no such file"),
        ] {
            let rig = fresh().await?;
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
