// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin_session::{CheckId, CheckStatus};
use jackin_testkit::{ProcessReply, TestResult, guard};

use super::support::{Setup, run};

#[tokio::test]
async fn every_check_has_one_label_and_one_fix_and_the_report_lists_them_in_order() -> TestResult {
    guard(async {
        let outcome = run(Setup::healthy()).await?;
        let reported: Vec<CheckId> = outcome.report.checks.iter().map(|check| check.id).collect();
        assert_eq!(reported, CheckId::ALL);
        let texts: Vec<(&str, &str)> = CheckId::ALL
            .into_iter()
            .map(|id| (id.label(), id.fix()))
            .collect();
        assert_eq!(
            texts,
            [
                (
                    "OpenShell binary",
                    "Install the pinned OpenShell release and put openshell on PATH"
                ),
                (
                    "SSH client",
                    "Install an OpenSSH client and put ssh on PATH"
                ),
                ("Git binary", "Install git and put it on PATH"),
                (
                    "Gateway registered",
                    "Register the gateway with openshell gateway add"
                ),
                (
                    "Gateway health",
                    "Start the gateway; jackin does not start or repair it"
                ),
                (
                    "Pinned version",
                    "Run the OpenShell release that jackin is pinned to"
                ),
                (
                    "Docker engine socket",
                    "Start the Docker engine, or set DOCKER_HOST to its socket"
                ),
            ]
        );
        Ok(())
    })
    .await
}

const OTHER_FIXES: [(CheckId, &str); 6] = [
    (
        CheckId::OpenshellBinary,
        "Install the pinned OpenShell release and put openshell on PATH",
    ),
    (
        CheckId::SshBinary,
        "Install an OpenSSH client and put ssh on PATH",
    ),
    (
        CheckId::GatewayRegistered,
        "Register the gateway with openshell gateway add",
    ),
    (
        CheckId::GatewayHealth,
        "Start the gateway; jackin does not start or repair it",
    ),
    (
        CheckId::PinnedVersion,
        "Run the OpenShell release that jackin is pinned to",
    ),
    (
        CheckId::EngineSocket,
        "Start the Docker engine, or set DOCKER_HOST to its socket",
    ),
];

#[tokio::test]
async fn the_git_fix_names_the_cause_and_every_other_check_keeps_its_one_fix() -> TestResult {
    guard(async {
        let cases = [
            (
                "a git that is too old",
                ProcessReply::exited(0, b"git version 2.0.0\n", b""),
                CheckStatus::Fail,
                "Upgrade git to the required version",
            ),
            (
                "a git that cannot start",
                ProcessReply::spawn("not found"),
                CheckStatus::Fail,
                "Install git and put it on PATH",
            ),
            (
                "a git that prints no version",
                ProcessReply::exited(0, b"no idea\n", b""),
                CheckStatus::Fail,
                "Install git and put it on PATH",
            ),
            (
                "a git that passes",
                ProcessReply::exited(0, b"git version 2.50.1 (Apple Git-155)\n", b""),
                CheckStatus::Pass,
                "Install git and put it on PATH",
            ),
        ];
        for (label, git, status, fix) in cases {
            let outcome = run(Setup {
                git,
                ..Setup::healthy()
            })
            .await?;
            let found: Vec<(CheckStatus, &str)> = outcome
                .report
                .checks
                .iter()
                .filter(|check| check.id == CheckId::GitBinary)
                .map(|check| (check.status, check.fix))
                .collect();
            assert_eq!(found, [(status, fix)], "{label}");
            let others: Vec<(CheckId, &str)> = outcome
                .report
                .checks
                .iter()
                .filter(|check| check.id != CheckId::GitBinary)
                .map(|check| (check.id, check.fix))
                .collect();
            assert_eq!(others, OTHER_FIXES, "{label}");
        }
        Ok(())
    })
    .await
}
