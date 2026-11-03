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
