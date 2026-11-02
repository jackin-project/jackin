// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin_openshell::contract::{
    self, CORE_CHECKS, ContractGroup, ContractReport, TERMINAL_CHECKS, TRANSFER_CHECKS,
};
use jackin_openshell::{GatewayError, GatewayOp};
use jackin_testkit::{FakeGateway, Fault};

use super::support::{TestResult, fake, fixture, guard};

fn failures(report: &ContractReport) -> Vec<(&'static str, String)> {
    report
        .checks
        .iter()
        .filter_map(|check| {
            check
                .outcome
                .as_ref()
                .err()
                .map(|message| (check.name, message.clone()))
        })
        .collect()
}

fn names(report: &ContractReport) -> Vec<&'static str> {
    report.checks.iter().map(|check| check.name).collect()
}

async fn run_group(
    fake: &FakeGateway,
    group: ContractGroup,
) -> Result<ContractReport, Box<dyn std::error::Error>> {
    let (_project, fixture) = fixture(fake).await?;
    Ok(contract::run(fake, &fixture, group).await)
}

#[tokio::test]
async fn core_group_passes_on_the_fake_with_every_published_check_ok() -> TestResult {
    guard(async {
        let fake = fake()?;
        let report = run_group(&fake, ContractGroup::Core).await?;
        assert_eq!(names(&report), CORE_CHECKS);
        assert_eq!(failures(&report), Vec::new());
        assert!(fake.sandboxes().is_empty());
        assert!(fake.providers().is_empty());
        Ok(())
    })
    .await
}

#[tokio::test]
async fn transfer_group_passes_on_the_fake() -> TestResult {
    guard(async {
        let fake = fake()?;
        let report = run_group(&fake, ContractGroup::Transfer).await?;
        assert_eq!(names(&report), TRANSFER_CHECKS);
        assert_eq!(failures(&report), Vec::new());
        Ok(())
    })
    .await
}

#[tokio::test]
async fn terminal_group_passes_on_the_fake_including_the_lease_checks() -> TestResult {
    guard(async {
        let fake = fake()?;
        let report = run_group(&fake, ContractGroup::Terminal).await?;
        assert_eq!(names(&report), TERMINAL_CHECKS);
        assert_eq!(failures(&report), Vec::new());
        Ok(())
    })
    .await
}

#[tokio::test]
async fn a_faulted_fake_fails_exactly_the_matching_check_and_still_runs_the_rest() -> TestResult {
    guard(async {
        let find = fake()?;
        find.inject(Fault::FailNext {
            op: GatewayOp::Find,
            error: GatewayError::Unreachable {
                endpoint: "fake".to_owned(),
                message: "injected".to_owned(),
            },
        });
        let report = run_group(&find, ContractGroup::Core).await?;
        assert_eq!(names(&report), CORE_CHECKS);
        let failed = failures(&report);
        assert_eq!(failed.len(), 1, "{failed:?}");
        assert_eq!(failed[0].0, "create_find_by_label");

        let corrupt = fake()?;
        corrupt.inject(Fault::CorruptDownload);
        let report = run_group(&corrupt, ContractGroup::Transfer).await?;
        assert_eq!(names(&report), TRANSFER_CHECKS);
        let failed = failures(&report);
        assert_eq!(failed.len(), 1, "{failed:?}");
        assert_eq!(failed[0].0, "download_with_digest");

        let upload = fake()?;
        upload.inject(Fault::CorruptUpload);
        let report = run_group(&upload, ContractGroup::Transfer).await?;
        assert_eq!(names(&report), TRANSFER_CHECKS);
        let failed = failures(&report);
        assert_eq!(failed.len(), 1, "{failed:?}");
        assert_eq!(failed[0].0, "upload_file_with_digest");

        let terminal = fake()?;
        terminal.inject(Fault::FailNext {
            op: GatewayOp::Terminal,
            error: GatewayError::ResourceExhausted {
                what: "terminals".to_owned(),
            },
        });
        let report = run_group(&terminal, ContractGroup::Terminal).await?;
        assert_eq!(names(&report), TERMINAL_CHECKS);
        let failed = failures(&report);
        assert_eq!(failed.len(), 1, "{failed:?}");
        assert_eq!(failed[0].0, "shell_exit_code");
        assert!(terminal.sandboxes().is_empty());
        Ok(())
    })
    .await
}
