// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin_core::{RoleName, WorkspaceName};
use jackin_session::PlanError;
use jackin_testkit::RoleFixture;

use super::support::{Spec, TestResult, guard};

async fn setup(keys: &str) -> Result<super::support::Setup, Box<dyn std::error::Error>> {
    let mut spec = Spec::new(RoleFixture::new("sentinel")?);
    spec.workspace_keys = keys.to_owned();
    spec.tables = "\n[roles.other]\nsource = { path = \"/somewhere/else\" }\n".to_owned();
    spec.build().await
}

#[tokio::test]
async fn a_workspace_without_a_roles_list_allows_every_role() -> TestResult {
    guard(async {
        let setup = setup("").await?;
        setup.plan(setup.request(1)?)?;
        Ok(())
    })
    .await
}

#[tokio::test]
async fn a_role_inside_the_allowed_roles_launches() -> TestResult {
    guard(async {
        let setup = setup("roles = [\"sentinel\", \"other\"]\n").await?;
        setup.plan(setup.request(1)?)?;
        Ok(())
    })
    .await
}

#[tokio::test]
async fn a_role_outside_the_allowed_roles_is_refused_naming_the_workspace_and_the_role()
-> TestResult {
    guard(async {
        let setup = setup("roles = [\"other\"]\n").await?;
        let refused = setup.plan(setup.request(1)?);
        assert!(
            matches!(
                &refused,
                Err(PlanError::RoleNotAllowed { workspace, role })
                    if *workspace == WorkspaceName::parse("platform-backend").unwrap()
                        && *role == RoleName::parse("sentinel").unwrap()
            ),
            "{:?}",
            refused.err()
        );
        let text = refused.err().map(|error| error.to_string());
        assert_eq!(
            text.as_deref(),
            Some("workspace platform-backend does not allow role sentinel")
        );
        Ok(())
    })
    .await
}

#[tokio::test]
async fn a_refused_role_wins_over_an_account_that_cannot_launch() -> TestResult {
    guard(async {
        let unusable = [
            ("no account at all", String::new()),
            (
                "a key account",
                "[accounts.work]\nprovider = \"codex\"\ncredential = { source = \"env\", var = \"OPENAI_KEY\" }\n".to_owned(),
            ),
        ];
        for (label, accounts) in unusable {
            let mut refused = Spec::new(RoleFixture::new("sentinel")?);
            refused.accounts.clone_from(&accounts);
            refused.workspace_keys = "roles = [\"other\"]\n".to_owned();
            refused.tables = "\n[roles.other]\nsource = { path = \"/somewhere/else\" }\n".to_owned();
            let setup = refused.build().await?;
            let result = setup.plan(setup.request(1)?);
            assert!(
                matches!(&result, Err(PlanError::RoleNotAllowed { .. })),
                "{label}: {:?}",
                result.err()
            );

            let mut allowed = Spec::new(RoleFixture::new("sentinel")?);
            allowed.accounts = accounts;
            let setup = allowed.build().await?;
            let result = setup.plan(setup.request(1)?);
            assert!(
                matches!(
                    &result,
                    Err(PlanError::Selection(_) | PlanError::Unsupported { .. })
                ),
                "{label}: {:?}",
                result.err()
            );
        }
        Ok(())
    })
    .await
}
