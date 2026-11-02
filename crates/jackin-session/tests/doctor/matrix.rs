// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin_openshell::GatewayError;
use jackin_session::CheckStatus::{Fail, Pass, Skip, Warn};
use jackin_testkit::{ProcessReply, TestResult, guard};

use super::support::{Access, Setup, expected, run, statuses};

#[tokio::test]
async fn healthy_fakes_pass_every_check_in_order() -> TestResult {
    guard(async {
        let outcome = run(Setup::healthy()).await?;
        assert_eq!(statuses(&outcome.report), expected([Pass; 7]));
        assert!(outcome.report.is_healthy());
        Ok(())
    })
    .await
}

#[tokio::test]
async fn no_chosen_gateway_skips_only_the_gateway_checks() -> TestResult {
    guard(async {
        let outcome = run(Setup {
            access: Access::NotChosen,
            ..Setup::healthy()
        })
        .await?;
        assert_eq!(
            statuses(&outcome.report),
            expected([Pass, Pass, Pass, Skip, Skip, Skip, Pass])
        );
        assert!(outcome.report.is_healthy());
        Ok(())
    })
    .await
}

#[tokio::test]
async fn each_problem_flips_only_its_own_check_and_nothing_is_repaired() -> TestResult {
    guard(async {
        let missing = || ProcessReply::spawn("not found");
        let cases = [
            (
                "no openshell",
                Setup {
                    openshell: missing(),
                    ..Setup::healthy()
                },
                [Fail, Pass, Pass, Pass, Pass, Pass, Pass],
            ),
            (
                "no ssh",
                Setup {
                    ssh: missing(),
                    ..Setup::healthy()
                },
                [Pass, Fail, Pass, Pass, Pass, Pass, Pass],
            ),
            (
                "unregistered",
                Setup {
                    listed: false,
                    ..Setup::healthy()
                },
                [Pass, Pass, Pass, Fail, Pass, Pass, Pass],
            ),
            (
                "failed access",
                Setup {
                    access: Access::Failed(GatewayError::Protocol {
                        message: "down".to_owned(),
                    }),
                    ..Setup::healthy()
                },
                [Pass, Pass, Pass, Pass, Fail, Skip, Pass],
            ),
            (
                "unhealthy",
                Setup {
                    unhealthy: true,
                    ..Setup::healthy()
                },
                [Pass, Pass, Pass, Pass, Fail, Skip, Pass],
            ),
            (
                "wrong version",
                Setup {
                    pinned: "0.1.60".to_owned(),
                    ..Setup::healthy()
                },
                [Pass, Pass, Pass, Pass, Pass, Fail, Pass],
            ),
            (
                "prefix of the version",
                Setup {
                    pinned: "0.1".to_owned(),
                    ..Setup::healthy()
                },
                [Pass, Pass, Pass, Pass, Pass, Fail, Pass],
            ),
            (
                "no socket",
                Setup {
                    socket: false,
                    ..Setup::healthy()
                },
                [Pass, Pass, Pass, Pass, Pass, Pass, Warn],
            ),
        ];
        for (label, setup, flipped) in cases {
            let outcome = run(setup).await?;
            assert_eq!(statuses(&outcome.report), expected(flipped), "{label}");
            assert_eq!(
                outcome.report.is_healthy(),
                !flipped.contains(&Fail),
                "{label}"
            );
            assert!(outcome.world.gateway.sandboxes().is_empty(), "{label}");
            assert!(outcome.world.gateway.providers().is_empty(), "{label}");
        }
        Ok(())
    })
    .await
}
