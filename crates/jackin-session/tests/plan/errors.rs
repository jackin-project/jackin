// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin_core::{Agent, ConfigError, GatewayName, Provider, RoleName};
use jackin_session::{LaunchRequest, PlanError, plan_launch};
use jackin_testkit::{RepoFixture, RoleFixture};

use super::support::{Spec, TestResult, guard, launch_id};

#[tokio::test]
async fn plan_errors_are_typed_for_unavailable_tuples_unknown_gateways_and_unregistered_projects()
-> TestResult {
    guard(async {
        let mut spec = Spec::new(RoleFixture::new("sentinel")?);
        spec.accounts = "[accounts.work]\nprovider = \"codex\"\ncredential = { source = \"env\", var = \"OPENAI_API_KEY\" }\n".to_owned();
        let setup = spec.build().await?;
        let result = setup.plan(
            setup
                .request(1)?
                .tuple(Agent::Codex, Provider::Codex)
                .model(Some("gpt-6.1-sol".to_owned())),
        );
        assert!(
            matches!(&result, Err(PlanError::TupleNotSupported { agent: Agent::Codex, provider: Provider::Codex, reason }) if reason == "final live proof pending after host re-login"),
            "{:?}",
            result.err()
        );

        let setup = Spec::new(RoleFixture::new("sentinel")?).build().await?;
        let unknown = LaunchRequest::new(
            GatewayName::parse("nowhere")?,
            setup.project.clone(),
            RoleName::parse("sentinel")?,
            launch_id(1),
        );
        let result = plan_launch(&setup.config, &setup.role, unknown);
        assert!(
            matches!(&result, Err(PlanError::Config(ConfigError::UnknownGateway(name))) if name.as_str() == "nowhere"),
            "{:?}",
            result.err()
        );

        let mismatched = LaunchRequest::new(
            setup.gateway.clone(),
            setup.project.clone(),
            RoleName::parse("other")?,
            launch_id(1),
        );
        let result = plan_launch(&setup.config, &setup.role, mismatched);
        assert!(
            matches!(&result, Err(PlanError::RoleMismatch { requested, loaded }) if requested.as_str() == "other" && loaded.as_str() == "sentinel"),
            "{:?}",
            result.err()
        );

        let stranger = RepoFixture::new().await?;
        let request = LaunchRequest::new(
            setup.gateway.clone(),
            stranger.project().await?,
            RoleName::parse("sentinel")?,
            launch_id(1),
        );
        let result = plan_launch(&setup.config, &setup.role, request);
        assert!(
            matches!(&result, Err(PlanError::Config(ConfigError::NoWorkspaceForPath(_)))),
            "{:?}",
            result.err()
        );
        Ok(())
    })
    .await
}
