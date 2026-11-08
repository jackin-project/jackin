// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin_core::{Agent, Provider};
use jackin_session::PlanError;
use jackin_testkit::RoleFixture;

use super::support::{Spec, TestResult, guard};

#[tokio::test]
async fn unavailable_tuple_is_rejected_before_image_and_codex_succeeds() -> TestResult {
    guard(async {
        let setup = Spec::new(RoleFixture::new("sentinel")?).build().await?;
        let err = setup
            .plan(setup.request(1)?.tuple(Agent::ClaudeCode, Provider::Zai))
            .err()
            .ok_or("expected error")?;
        assert!(matches!(err, PlanError::TupleNotSupported { .. }), "{err}");
        let plan = setup.plan(setup.request(2)?)?;
        assert_eq!(plan.account().account.as_str(), "work");
        Ok(())
    })
    .await
}
