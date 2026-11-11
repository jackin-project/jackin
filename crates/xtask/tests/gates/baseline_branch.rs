// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin_xtask::{CompareError, CompareSource, baseline_branch};

use crate::workspace::{REV, TestResult, green};

struct Scripted(Result<String, String>);

impl CompareSource for Scripted {
    fn status(&self, rev: &str) -> Result<String, CompareError> {
        assert_eq!(rev, REV);
        self.0.clone().map_err(CompareError::new)
    }
}

#[test]
fn baseline_branch_ahead_and_identical_pass() -> TestResult {
    let repo = green()?.repo()?;
    for status in ["ahead", "identical"] {
        let pins = baseline_branch(&repo, &Scripted(Ok(status.to_owned())))?;
        assert_eq!(pins.len(), 1);
        assert_eq!(pins[0].rev, REV);
        assert_eq!(pins[0].status, status);
    }
    Ok(())
}

#[test]
fn baseline_branch_behind_diverged_empty_and_gh_failure_fail() -> TestResult {
    let repo = green()?.repo()?;
    let replies = [
        Ok("behind".to_owned()),
        Ok("diverged".to_owned()),
        Ok(String::new()),
        Err("gh: HTTP 404".to_owned()),
    ];
    for reply in replies {
        let failure = baseline_branch(&repo, &Scripted(reply.clone()))
            .expect_err("a rev that is not on the branch must fail");
        let message = failure.to_string();
        assert!(message.contains(REV), "{message}");
        if let Err(reason) = reply {
            assert!(message.contains(&reason), "{message}");
        }
    }
    Ok(())
}
