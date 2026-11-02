// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin_openshell::GatewayError;
use jackin_session::CheckId;
use jackin_session::CheckStatus::{Fail, Pass, Skip, Warn};
use jackin_testkit::{ProcessReply, TestResult, guard};

use super::support::{Access, Health, Setup, Socket, expected, run, statuses};

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
async fn a_symlink_to_a_live_socket_passes_like_the_socket_itself() -> TestResult {
    guard(async {
        let outcome = run(Setup {
            socket: Socket::BehindSymlink,
            ..Setup::healthy()
        })
        .await?;
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
                    health: Health::Fails,
                    ..Setup::healthy()
                },
                [Pass, Pass, Pass, Pass, Fail, Skip, Pass],
            ),
            (
                "not serving",
                Setup {
                    health: Health::NotServing,
                    ..Setup::healthy()
                },
                [Pass, Pass, Pass, Pass, Fail, Pass, Pass],
            ),
            (
                "openshell exits non-zero",
                Setup {
                    openshell: ProcessReply::exited(1, b"", b"broken\n"),
                    ..Setup::healthy()
                },
                [Fail, Pass, Pass, Pass, Pass, Pass, Pass],
            ),
            (
                "ssh exits non-zero",
                Setup {
                    ssh: ProcessReply::exited(255, b"", b"broken\n"),
                    ..Setup::healthy()
                },
                [Pass, Fail, Pass, Pass, Pass, Pass, Pass],
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
                    socket: Socket::Missing,
                    ..Setup::healthy()
                },
                [Pass, Pass, Pass, Pass, Pass, Pass, Warn],
            ),
            (
                "a symlink to a removed target",
                Setup {
                    socket: Socket::Dangling,
                    ..Setup::healthy()
                },
                [Pass, Pass, Pass, Pass, Pass, Pass, Warn],
            ),
            (
                "a regular file",
                Setup {
                    socket: Socket::RegularFile,
                    ..Setup::healthy()
                },
                [Pass, Pass, Pass, Pass, Pass, Pass, Warn],
            ),
            (
                "a directory",
                Setup {
                    socket: Socket::Directory,
                    ..Setup::healthy()
                },
                [Pass, Pass, Pass, Pass, Pass, Pass, Warn],
            ),
            (
                "a symlink loop",
                Setup {
                    socket: Socket::Loop,
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

fn detail(outcome: &super::support::Run, id: CheckId) -> Result<&str, Box<dyn std::error::Error>> {
    Ok(outcome
        .report
        .checks
        .iter()
        .find(|check| check.id == id)
        .ok_or("no such check")?
        .detail
        .as_str())
}

#[tokio::test]
async fn version_details_carry_no_terminal_control_or_bidi_characters() -> TestResult {
    guard(async {
        let hostile = |text: &str| format!("\u{1b}]0;x\u{7}\u{202e}{text}\u{9b}\u{202e}\n");
        let outcome = run(Setup {
            openshell: ProcessReply::exited(0, hostile("openshell 0.1.6").as_bytes(), b""),
            ssh: ProcessReply::exited(0, b"", hostile("OpenSSH_9.9p1").as_bytes()),
            ..Setup::healthy()
        })
        .await?;
        assert_eq!(statuses(&outcome.report), expected([Pass; 7]));
        assert_eq!(
            detail(&outcome, CheckId::OpenshellBinary)?,
            "openshell 0.1.6"
        );
        assert_eq!(detail(&outcome, CheckId::SshBinary)?, "OpenSSH_9.9p1");
        Ok(())
    })
    .await
}
