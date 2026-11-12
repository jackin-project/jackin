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
            .plan(
                setup
                    .request(1)?
                    .tuple(Agent::MinimaxCode, Provider::Minimax),
            )
            .err()
            .ok_or("expected error")?;
        assert!(
            matches!(
                &err,
                PlanError::TupleNotSupported {
                    agent: Agent::MinimaxCode,
                    provider: Provider::Minimax,
                    reason,
                } if !reason.is_empty()
            ),
            "{err}"
        );
        let plan = setup.plan(setup.request(2)?)?;
        assert_eq!(plan.account().account.as_str(), "work");
        Ok(())
    })
    .await
}

#[tokio::test]
async fn unavailable_native_lead_checks_credential_before_availability() -> TestResult {
    guard(async {
        let mut spec = Spec::new(RoleFixture::new("sentinel")?);
        spec.accounts = "[accounts.kimi]\nprovider = \"kimi\"\ncredential = { source = \"file\", path = \"/tmp/kimi-static\" }\n".to_owned();
        let setup = spec.build().await?;
        let error = setup
            .plan(setup.request(1)?.tuple(Agent::KimiCode, Provider::Kimi))
            .err()
            .ok_or("static native source was accepted")?;
        assert!(matches!(
            error,
            PlanError::Config(jackin_core::ConfigError::CredentialNotEligible {
                agent: Agent::KimiCode,
                ..
            })
        ));

        let mut spec = Spec::new(RoleFixture::new("sentinel")?);
        spec.accounts = "[accounts.kimi]\nprovider = \"kimi\"\ncredential = { source = \"host-login\", path = \"/tmp/kimi-native\" }\n".to_owned();
        let setup = spec.build().await?;
        let error = setup
            .plan(setup.request(2)?.tuple(Agent::KimiCode, Provider::Kimi))
            .err()
            .ok_or("T08 became supported")?;
        assert!(matches!(
            error,
            PlanError::TupleNotSupported { ref reason, .. }
                if reason == "native refresh and final live proof pending"
        ));
        Ok(())
    })
    .await
}
