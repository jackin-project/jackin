// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin_session::{CheckId, CheckStatus, check_git};
use jackin_testkit::{ProcessReply, TestResult, World, guard};
use tokio_util::sync::CancellationToken;

use super::support::{Setup, run, statuses};

fn git_status(report: &jackin_session::DoctorReport) -> Option<CheckStatus> {
    statuses(report)
        .into_iter()
        .find_map(|(id, status)| (id == CheckId::GitBinary).then_some(status))
}

#[tokio::test]
async fn the_git_check_agrees_with_the_launch_check_for_the_same_version_output() -> TestResult {
    guard(async {
        let outputs: [(&[u8], CheckStatus); 8] = [
            (b"git version 2.44.9\n", CheckStatus::Fail),
            (b"git version 2.45.3\n", CheckStatus::Fail),
            (b"git version 2.45.4\n", CheckStatus::Pass),
            (b"git version 2.50.0\n", CheckStatus::Fail),
            (b"git version 2.50.1 (Apple Git-155)\n", CheckStatus::Pass),
            (b"git version 2.51.0\n", CheckStatus::Pass),
            (b"git version 2.54.0 (Apple Git-157)\n", CheckStatus::Pass),
            (b"no idea\n", CheckStatus::Fail),
        ];
        for (output, wanted) in outputs {
            let reply = || ProcessReply::exited(0, output, b"");
            let outcome = run(Setup {
                git: reply(),
                ..Setup::healthy()
            })
            .await?;
            let world = World::builder().build()?;
            world.runner.scripted().expect_git_version(reply());
            let launch = check_git(world.runner.as_ref(), &CancellationToken::new()).await;
            let expected = if launch.is_ok() {
                CheckStatus::Pass
            } else {
                CheckStatus::Fail
            };
            assert_eq!(expected, wanted, "{output:?}");
            assert_eq!(git_status(&outcome.report), Some(wanted), "{output:?}");
            let detail = outcome
                .report
                .checks
                .iter()
                .find(|check| check.id == CheckId::GitBinary)
                .map(|check| check.detail.clone())
                .unwrap_or_default();
            if output == b"git version 2.44.9\n" {
                assert!(detail.contains("2.45.4"), "{detail}");
            }
        }
        Ok(())
    })
    .await
}
