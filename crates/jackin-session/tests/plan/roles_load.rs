// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin_core::{Config, RoleName, WorkspaceName};
use jackin_session::RoleError;
use jackin_testkit::RoleFixture;

use super::support::{TestResult, catalog, guard};

struct Roles {
    config: Config,
    cache: jackin_core::HostPath,
    _cache: tempfile::TempDir,
    _fixtures: Vec<RoleFixture>,
}

async fn roles(
    names: &[&str],
    default_role: Option<&str>,
) -> Result<Roles, Box<dyn std::error::Error>> {
    let mut text = String::new();
    let mut fixtures = Vec::new();
    for name in names {
        let role = RoleFixture::new(name)?;
        role.commit().await?;
        text.push_str(&format!(
            "[roles.{name}]\nsource = {{ path = \"{}\" }}\n\n",
            role.root().as_path().display()
        ));
        fixtures.push(role);
    }
    let anchor = fixtures
        .first()
        .map(|role| role.root().as_path().display().to_string())
        .unwrap_or_default();
    text.push_str(&format!("[workspaces.w]\npath = \"{anchor}\"\n"));
    if let Some(default_role) = default_role {
        text.push_str(&format!("default_role = \"{default_role}\"\n"));
    }
    let cache_dir = tempfile::tempdir()?;
    let cache = jackin_core::HostPath::parse(&std::fs::canonicalize(cache_dir.path())?)?;
    Ok(Roles {
        config: Config::parse(&text)?,
        cache,
        _cache: cache_dir,
        _fixtures: fixtures,
    })
}

impl Roles {
    async fn load(
        &self,
        role: Option<&str>,
    ) -> Result<Result<String, RoleError>, Box<dyn std::error::Error>> {
        let (catalog, _runner) = catalog(&self.cache);
        let workspace = self
            .config
            .workspace(&WorkspaceName::parse("w")?)
            .ok_or("no workspace")?;
        let role = role.map(RoleName::parse).transpose()?;
        Ok(catalog
            .load(&self.config, role.as_ref(), workspace)
            .await
            .map(|loaded| loaded.name().as_str().to_owned()))
    }
}

#[tokio::test]
async fn load_picks_the_explicit_role_then_the_workspace_default_then_the_only_role() -> TestResult
{
    guard(async {
        let two = roles(&["alpha", "beta"], Some("alpha")).await?;
        assert_eq!(two.load(Some("beta")).await?.ok(), Some("beta".to_owned()));
        assert_eq!(two.load(None).await?.ok(), Some("alpha".to_owned()));
        let only = roles(&["alpha"], None).await?;
        assert_eq!(only.load(None).await?.ok(), Some("alpha".to_owned()));
        Ok(())
    })
    .await
}

#[tokio::test]
async fn load_refuses_an_ambiguous_or_unknown_role_selection() -> TestResult {
    guard(async {
        let two = roles(&["alpha", "beta"], None).await?;
        assert!(matches!(two.load(None).await?, Err(RoleError::NoRole)));
        assert!(matches!(
            two.load(Some("gamma")).await?,
            Err(RoleError::UnknownRole(name)) if name.as_str() == "gamma"
        ));
        Ok(())
    })
    .await
}
