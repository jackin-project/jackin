//! Explicit role-image prewarm entrypoint and result rows.

use jackin_core::Agent;
#[cfg(not(test))]
use jackin_core::{JackinPaths, RoleSelector};

/// Result status for one explicit role-image prewarm request.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImagePrewarmStatus {
    /// Existing local image labels already match the current recipe.
    Reused,
    /// Local image was missing or invalid, so prewarm rebuilt it.
    Built,
}

/// One row from explicit `jackin prewarm --image` role-image preparation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RoleImagePrewarmRow {
    /// Agent runtime whose image was checked or built.
    pub agent: Agent,
    /// Derived image tag checked or built for this agent.
    pub image: String,
    /// Whether prewarm reused or built the image.
    pub status: ImagePrewarmStatus,
}

/// Resolve a role repo and prewarm derived images for selected agents.
///
/// This writes only jackin-owned role cache, binary cache, build-context, and
/// Docker image state. It does not touch host repos, host git config, shell
/// config, `gh` config, or agent configs.
#[cfg(not(test))]
pub async fn prewarm_role_images(
    paths: &JackinPaths,
    selector: &RoleSelector,
    role_git: &str,
    branch_override: Option<&str>,
    agents: &[Agent],
    debug: bool,
) -> anyhow::Result<Vec<RoleImagePrewarmRow>> {
    use crate::runtime::repo_cache::{RepoResolveOptions, resolve_agent_repo_with};
    use jackin_docker::ShellRunner;

    let mut resolve_runner = ShellRunner { debug };
    let (_cached_repo, validated_repo, repo_lock) = resolve_agent_repo_with(
        paths,
        selector,
        role_git,
        &mut resolve_runner,
        RepoResolveOptions::non_interactive()
            .with_branch(branch_override)
            .with_refresh_ttl(std::time::Duration::ZERO),
        || Ok(false),
    )
    .await?;
    let supported = validated_repo.manifest.supported_agents();
    drop(repo_lock);
    let requested = if agents.is_empty() {
        supported
    } else {
        for agent in agents {
            if !supported.contains(agent) {
                anyhow::bail!(
                    "role {selector} does not support {}; supported agents: {}",
                    agent.slug(),
                    supported
                        .iter()
                        .map(|agent| agent.slug())
                        .collect::<Vec<_>>()
                        .join(", ")
                );
            }
        }
        agents.to_vec()
    };

    let mut tasks = tokio::task::JoinSet::new();
    for (index, agent) in requested.into_iter().enumerate() {
        let paths = paths.clone();
        let selector = selector.clone();
        let role_git = role_git.to_owned();
        let branch_override = branch_override.map(str::to_owned);
        tasks.spawn(async move {
            let row = super::prewarm_agent_image(
                &paths,
                &selector,
                &role_git,
                branch_override.as_deref(),
                agent,
                debug,
            )
            .await;
            (index, row)
        });
    }

    let mut rows = Vec::new();
    while let Some(result) = tasks.join_next().await {
        let (index, row) = result?;
        rows.push((index, row?));
    }
    rows.sort_by_key(|(index, _)| *index);
    let rows = rows.into_iter().map(|(_, row)| row).collect();
    Ok(rows)
}
