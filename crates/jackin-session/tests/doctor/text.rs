// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin_session::CheckId;
use jackin_testkit::{TestResult, guard};

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
