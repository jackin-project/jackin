// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::sync::Arc;

use jackin_core::{Config, RoleName, StdFileSystem, WorkspaceName};
use jackin_session::{RoleCatalog, RoleError};
use jackin_testkit::{RoleFixture, TestRunner};

use super::roles_origin::{Origin, URL, cache};
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
    allowing(names, default_role, None).await
}

async fn allowing(
    names: &[&str],
    default_role: Option<&str>,
    allowed: Option<&[&str]>,
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
    if let Some(allowed) = allowed {
        let quoted: Vec<String> = allowed.iter().map(|name| format!("\"{name}\"")).collect();
        text.push_str(&format!("roles = [{}]\n", quoted.join(", ")));
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

#[tokio::test]
async fn the_only_role_fallback_counts_only_the_roles_the_workspace_allows() -> TestResult {
    guard(async {
        let narrowed = allowing(&["first", "second"], None, Some(&["second"])).await?;
        assert_eq!(narrowed.load(None).await?.ok(), Some("second".to_owned()));
        assert_eq!(
            narrowed.load(Some("second")).await?.ok(),
            Some("second".to_owned())
        );

        let open = allowing(&["first", "second"], None, None).await?;
        assert!(matches!(open.load(None).await?, Err(RoleError::NoRole)));
        let both = allowing(&["first", "second"], None, Some(&["first", "second"])).await?;
        assert!(matches!(both.load(None).await?, Err(RoleError::NoRole)));
        Ok(())
    })
    .await
}

#[tokio::test]
async fn a_role_outside_the_allowed_roles_is_refused_by_name() -> TestResult {
    guard(async {
        let narrowed = allowing(&["first", "second"], None, Some(&["second"])).await?;
        let refused = narrowed.load(Some("first")).await?;
        assert!(
            matches!(&refused, Err(RoleError::NotAllowed(name)) if name.as_str() == "first"),
            "{refused:?}"
        );
        Ok(())
    })
    .await
}

#[tokio::test]
async fn a_git_role_outside_the_allowed_roles_is_refused_before_anything_is_checked_out()
-> TestResult {
    guard(async {
        let origin = Origin::new().await?;
        let (cache, keep) = cache()?;
        let second = RoleFixture::new("second")?;
        second.commit().await?;
        let config = Config::parse(&format!(
            "[roles.first]\nsource = {{ git = \"{URL}\" }}\n\n[roles.second]\nsource = {{ path = \"{}\" }}\n\n[workspaces.w]\npath = \"{}\"\nroles = [\"second\"]\n",
            second.root().as_path().display(),
            origin.home.as_path().display()
        ))?;
        let catalog = RoleCatalog::new(
            Arc::new(StdFileSystem::default()),
            Arc::new(TestRunner::with_home(&origin.home)),
            cache,
        );
        let workspace = config
            .workspace(&WorkspaceName::parse("w")?)
            .ok_or("no workspace")?;
        let first = RoleName::parse("first")?;
        let refused = catalog.load(&config, Some(&first), workspace).await;
        assert_eq!(std::fs::read_dir(keep.path())?.count(), 0);
        assert!(
            matches!(&refused, Err(RoleError::NotAllowed(name)) if *name == first),
            "{:?}",
            refused.err()
        );
        Ok(())
    })
    .await
}
