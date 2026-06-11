use clap::Args;
use owo_colors::OwoColorize as _;

use crate::agent::Agent;
use crate::cli::{BANNER, HELP_STYLES};
use crate::config::AppConfig;
use crate::docker::ShellRunner;
use crate::paths::JackinPaths;
use crate::selector::RoleSelector;
use jackin_docker::docker_client::{BollardDockerClient, DockerApi};

/// `jackin prewarm` — fill jackin-owned runtime caches before launch.
#[derive(Debug, Args, PartialEq, Eq)]
#[command(
    before_help = BANNER,
    styles = HELP_STYLES,
    about = "Prewarm jackin-owned runtime caches before launch"
)]
pub struct PrewarmArgs {
    /// Agent runtime binary to prewarm. Repeat to choose several. Defaults to all agents.
    #[arg(long = "agent", value_parser = parse_agent)]
    pub agents: Vec<Agent>,
    /// Also prewarm derived Docker image(s) for a role.
    #[arg(long)]
    pub image: bool,
    /// Also prefetch/update every configured role repo cache.
    #[arg(long)]
    pub roles: bool,
    /// Also prewarm the Docker-in-Docker sidecar image used by fresh launches.
    #[arg(long)]
    pub sidecar: bool,
    /// Role selector whose repo cache and/or derived image(s) should be prewarmed.
    #[arg(long, conflicts_with_all = ["workspace", "all_workspaces"])]
    pub role: Option<String>,
    /// Saved workspace whose default role repo and/or agent image should be prewarmed.
    #[arg(long, conflicts_with_all = ["role", "all_workspaces"])]
    pub workspace: Option<String>,
    /// Prewarm targets for every saved workspace with a default role.
    #[arg(long, conflicts_with_all = ["role", "workspace", "role_git"])]
    pub all_workspaces: bool,
    /// Role git URL override for role/image prewarm. Defaults to configured role source.
    #[arg(long, requires = "role", conflicts_with_all = ["workspace", "all_workspaces"])]
    pub role_git: Option<String>,
    /// Role branch to prewarm. Uses branch-scoped image tags.
    #[arg(long, requires = "image")]
    pub role_branch: Option<String>,
}

fn parse_agent(s: &str) -> Result<Agent, String> {
    s.parse()
        .map_err(|e: crate::agent::ParseAgentError| e.to_string())
}

pub async fn run(
    args: &PrewarmArgs,
    paths: &JackinPaths,
    config: &AppConfig,
    debug: bool,
) -> anyhow::Result<()> {
    let image_targets = if args.image {
        PrewarmImageTarget::resolve(args, config)?
    } else {
        Vec::new()
    };
    let agents = binary_prewarm_agents(args, &image_targets);

    print!("{BANNER}");
    println!("prewarm\n");

    let capsule = crate::capsule_binary::ensure_available(paths);
    let agents_result = prewarm_agents(paths, &agents);
    let (capsule_result, agent_results) = tokio::join!(capsule, agents_result);

    match capsule_result {
        Ok(path) => println!("  {}  capsule  {}", "✓".green(), path.display()),
        Err(error) => {
            println!("  {}  capsule  {error:#}", "✗".red().bold());
            return Err(error.into());
        }
    }

    let mut failed = Vec::new();
    for result in agent_results {
        match result {
            Ok(row) => {
                let version = row.version.unwrap_or_else(|| "version unknown".to_owned());
                println!(
                    "  {}  {:<8} {}  {}",
                    "✓".green(),
                    row.agent.slug(),
                    version,
                    row.path.display()
                );
            }
            Err(row) => {
                println!("  {}  {:<8} {}", "!".yellow(), row.agent.slug(), row.error);
                failed.push(row.agent);
            }
        }
    }

    for target in image_targets {
        prewarm_images(args, paths, target, debug).await?;
    }

    if args.sidecar {
        prewarm_sidecar_image().await?;
    }

    if args.roles {
        let targets = PrewarmRoleTarget::resolve(args, config)?;
        prewarm_role_repos(paths, targets, debug).await?;
    }

    if failed.is_empty() {
        println!();
        println!("{}", "✓  runtime cache prewarmed".green());
    } else {
        println!();
        println!(
            "{}  {} agent binary prewarm(s) failed; Docker builds can still use fallback installers",
            "!".yellow(),
            failed.len()
        );
    }
    Ok(())
}

async fn prewarm_sidecar_image() -> anyhow::Result<()> {
    println!();
    println!("sidecar");
    let docker = BollardDockerClient::connect()?;
    let image = crate::runtime::DIND_IMAGE;
    let tags = docker.list_image_tags(image).await?;
    if tags.is_empty() {
        docker.pull_image(image).await?;
        println!("  {}  {:<8} pulled", "✓".green(), image);
    } else {
        println!("  {}  {:<8} present", "✓".green(), image);
    }
    Ok(())
}

async fn prewarm_role_repos(
    paths: &JackinPaths,
    targets: Vec<PrewarmRoleTarget>,
    debug: bool,
) -> anyhow::Result<()> {
    println!();
    println!("role repos");

    let mut failed = Vec::new();
    for target in targets {
        let mut runner = ShellRunner { debug };
        match crate::runtime::register_agent_repo(
            paths,
            &target.selector,
            &target.role_git,
            &mut runner,
            debug,
        )
        .await
        {
            Ok((cached_repo, _validated_repo)) => println!(
                "  {}  {:<24} {}",
                "✓".green(),
                target.selector.key(),
                cached_repo.repo_dir.display()
            ),
            Err(error) => {
                println!(
                    "  {}  {:<24} {error:#}",
                    "✗".red().bold(),
                    target.selector.key()
                );
                failed.push(target.selector.key());
            }
        }
    }

    if failed.is_empty() {
        Ok(())
    } else {
        anyhow::bail!("{} role repo prewarm(s) failed", failed.len())
    }
}

struct PrewarmRoleTarget {
    selector: RoleSelector,
    role_git: String,
}

impl PrewarmRoleTarget {
    fn resolve(args: &PrewarmArgs, config: &AppConfig) -> anyhow::Result<Vec<Self>> {
        if args.all_workspaces {
            let mut targets = std::collections::BTreeMap::new();
            for workspace_name in config.workspaces.keys() {
                if let Some(target) = Self::resolve_workspace(config, workspace_name)? {
                    targets.insert(target.selector.key(), target);
                }
            }
            if targets.is_empty() {
                anyhow::bail!("no saved workspaces have a default role to role-prewarm");
            }
            return Ok(targets.into_values().collect());
        }

        if let Some(workspace_name) = args.workspace.as_deref() {
            return Self::resolve_workspace(config, workspace_name)?
                .map(|target| vec![target])
                .ok_or_else(|| {
                    anyhow::anyhow!(
                        "workspace `{workspace_name}` has no default role; pass `--role <selector>` or set a workspace default role"
                    )
                });
        }

        if let Some(role) = args.role.as_deref() {
            let selector = RoleSelector::parse(role)?;
            let role_git = args
                .role_git
                .clone()
                .or_else(|| {
                    config
                        .roles
                        .get(&selector.key())
                        .map(|source| source.git.clone())
                })
                .ok_or_else(|| {
                    anyhow::anyhow!(
                        "no git source configured for role `{selector}`; pass `--role-git <url>`"
                    )
                })?;
            return Ok(vec![Self { selector, role_git }]);
        }

        if config.roles.is_empty() {
            anyhow::bail!("no configured roles to prewarm");
        }

        let mut targets = Vec::new();
        for (key, source) in &config.roles {
            targets.push(Self {
                selector: RoleSelector::parse(key)?,
                role_git: source.git.clone(),
            });
        }
        targets.sort_by(|a, b| a.selector.key().cmp(&b.selector.key()));
        Ok(targets)
    }

    fn resolve_workspace(config: &AppConfig, workspace_name: &str) -> anyhow::Result<Option<Self>> {
        let workspace = config
            .workspaces
            .get(workspace_name)
            .ok_or_else(|| anyhow::anyhow!("workspace `{workspace_name}` is not configured"))?;
        let Some(role) = workspace.default_role.as_deref() else {
            return Ok(None);
        };
        let selector = RoleSelector::parse(role)?;
        let role_git = config
            .roles
            .get(&selector.key())
            .map(|source| source.git.clone())
            .ok_or_else(|| anyhow::anyhow!("no git source configured for role `{selector}`"))?;
        Ok(Some(Self { selector, role_git }))
    }
}

fn binary_prewarm_agents(args: &PrewarmArgs, image_targets: &[PrewarmImageTarget]) -> Vec<Agent> {
    if !args.agents.is_empty() {
        return args.agents.clone();
    }
    if !image_targets.is_empty() {
        let narrowed = image_targets
            .iter()
            .all(|target| target.is_agent_narrowed && !target.agents.is_empty());
        if narrowed {
            let mut agents = image_targets
                .iter()
                .flat_map(|target| target.agents.iter().copied())
                .collect::<Vec<_>>();
            agents.sort();
            agents.dedup();
            return agents;
        }
    }
    if args.agents.is_empty() {
        Agent::ALL.to_vec()
    } else {
        args.agents.clone()
    }
}

async fn prewarm_images(
    args: &PrewarmArgs,
    paths: &JackinPaths,
    target: PrewarmImageTarget,
    debug: bool,
) -> anyhow::Result<()> {
    println!();
    println!("images for {}", target.label);
    let rows = crate::runtime::prewarm_role_images(
        paths,
        &target.selector,
        &target.role_git,
        args.role_branch.as_deref(),
        &target.agents,
        debug,
    )
    .await?;
    for row in rows {
        let status = match row.status {
            crate::runtime::ImagePrewarmStatus::Reused => "reused",
            crate::runtime::ImagePrewarmStatus::Built => "built",
        };
        println!(
            "  {}  {:<8} {:<6} {}",
            "✓".green(),
            row.agent.slug(),
            status,
            row.image
        );
    }
    Ok(())
}

struct PrewarmImageTarget {
    selector: RoleSelector,
    role_git: String,
    agents: Vec<Agent>,
    label: String,
    is_agent_narrowed: bool,
}

impl PrewarmImageTarget {
    fn resolve(args: &PrewarmArgs, config: &AppConfig) -> anyhow::Result<Vec<Self>> {
        if args.all_workspaces {
            let mut targets = config
                .workspaces
                .keys()
                .filter_map(|workspace_name| {
                    Self::resolve_workspace(args, config, workspace_name).transpose()
                })
                .collect::<anyhow::Result<Vec<_>>>()?;
            targets.sort_by(|a, b| a.label.cmp(&b.label));
            if targets.is_empty() {
                anyhow::bail!("no saved workspaces have a default role to image-prewarm");
            }
            return Ok(targets);
        }

        if let Some(role) = args.role.as_deref() {
            let selector = RoleSelector::parse(role)?;
            let role_git = args
                .role_git
                .clone()
                .or_else(|| {
                    config
                        .roles
                        .get(&selector.key())
                        .map(|source| source.git.clone())
                })
                .ok_or_else(|| {
                    anyhow::anyhow!(
                        "no git source configured for role `{selector}`; pass `--role-git <url>`"
                    )
                })?;
            return Ok(vec![Self {
                label: selector.to_string(),
                selector,
                role_git,
                agents: args.agents.clone(),
                is_agent_narrowed: !args.agents.is_empty(),
            }]);
        }

        let workspace_name = args.workspace.as_deref().ok_or_else(|| {
            anyhow::anyhow!(
                "`jackin prewarm --image` requires `--role <selector>`, `--workspace <name>`, or `--all-workspaces`"
            )
        })?;
        let target = Self::resolve_workspace(args, config, workspace_name)?.ok_or_else(|| {
            anyhow::anyhow!(
                "workspace `{workspace_name}` has no default role; pass `--role <selector>` or set a workspace default role"
            )
        })?;
        Ok(vec![target])
    }

    fn resolve_workspace(
        args: &PrewarmArgs,
        config: &AppConfig,
        workspace_name: &str,
    ) -> anyhow::Result<Option<Self>> {
        let workspace = config
            .workspaces
            .get(workspace_name)
            .ok_or_else(|| anyhow::anyhow!("workspace `{workspace_name}` is not configured"))?;
        let Some(role) = workspace.default_role.as_deref() else {
            return Ok(None);
        };
        let selector = RoleSelector::parse(role)?;
        let role_git = config
            .roles
            .get(&selector.key())
            .map(|source| source.git.clone())
            .ok_or_else(|| anyhow::anyhow!("no git source configured for role `{selector}`"))?;
        let (agents, is_agent_narrowed) = if args.agents.is_empty() {
            workspace
                .default_agent
                .map_or_else(|| (Vec::new(), false), |agent| (vec![agent], true))
        } else {
            (args.agents.clone(), true)
        };

        Ok(Some(Self {
            label: format!("workspace {workspace_name} ({selector})"),
            selector,
            role_git,
            agents,
            is_agent_narrowed,
        }))
    }
}

#[derive(Debug)]
struct AgentPrewarmRow {
    agent: Agent,
    path: std::path::PathBuf,
    version: Option<String>,
}

#[derive(Debug)]
struct AgentPrewarmError {
    agent: Agent,
    error: anyhow::Error,
}

async fn prewarm_agents(
    paths: &JackinPaths,
    agents: &[Agent],
) -> Vec<Result<AgentPrewarmRow, AgentPrewarmError>> {
    let mut tasks = tokio::task::JoinSet::new();
    for agent in agents.iter().copied() {
        let paths = paths.clone();
        tasks.spawn(async move {
            let result = crate::agent_binary::ensure_available(&paths, agent)
                .await
                .map(|binary| AgentPrewarmRow {
                    agent: binary.agent,
                    path: binary.path,
                    version: binary.version,
                })
                .map_err(|error| AgentPrewarmError { agent, error });
            (agent, result)
        });
    }

    let mut rows = Vec::new();
    while let Some(result) = tasks.join_next().await {
        match result {
            Ok((_agent, row)) => rows.push(row),
            Err(error) => rows.push(Err(AgentPrewarmError {
                agent: Agent::Claude,
                error: error.into(),
            })),
        }
    }
    rows.sort_by_key(|row| match row {
        Ok(row) => row.agent,
        Err(row) => row.agent,
    });
    rows
}

#[cfg(test)]
mod tests {
    use super::*;

    fn config_with_workspace_default(agent: Option<Agent>) -> AppConfig {
        let mut config = AppConfig::default();
        config.roles.insert(
            "agent-smith".to_owned(),
            jackin_config::RoleSource {
                git: "https://example.invalid/agent-smith.git".to_owned(),
                trusted: true,
                env: std::collections::BTreeMap::new(),
            },
        );
        config.workspaces.insert(
            "jackin".to_owned(),
            jackin_config::WorkspaceConfig {
                workdir: "/workspace".to_owned(),
                default_role: Some("agent-smith".to_owned()),
                default_agent: agent,
                ..jackin_config::WorkspaceConfig::default()
            },
        );
        config
    }

    #[test]
    fn image_workspace_default_agent_narrows_binary_prewarm() {
        let config = config_with_workspace_default(Some(Agent::Codex));
        let args = PrewarmArgs {
            agents: Vec::new(),
            image: true,
            roles: false,
            sidecar: false,
            role: None,
            workspace: Some("jackin".to_owned()),
            all_workspaces: false,
            role_git: None,
            role_branch: None,
        };
        let target = PrewarmImageTarget::resolve(&args, &config)
            .unwrap()
            .pop()
            .unwrap();

        assert_eq!(target.agents, vec![Agent::Codex]);
        assert_eq!(binary_prewarm_agents(&args, &[target]), vec![Agent::Codex]);
    }

    #[test]
    fn image_role_without_agent_keeps_all_binary_prewarm() {
        let config = config_with_workspace_default(Some(Agent::Codex));
        let args = PrewarmArgs {
            agents: Vec::new(),
            image: true,
            roles: false,
            sidecar: false,
            role: Some("agent-smith".to_owned()),
            workspace: None,
            all_workspaces: false,
            role_git: None,
            role_branch: None,
        };
        let target = PrewarmImageTarget::resolve(&args, &config)
            .unwrap()
            .pop()
            .unwrap();

        assert!(target.agents.is_empty());
        assert_eq!(binary_prewarm_agents(&args, &[target]), Agent::ALL.to_vec());
    }

    #[test]
    fn image_all_workspaces_unions_default_agents_for_binary_prewarm() {
        let mut config = config_with_workspace_default(Some(Agent::Codex));
        config.roles.insert(
            "the-architect".to_owned(),
            jackin_config::RoleSource {
                git: "https://example.invalid/the-architect.git".to_owned(),
                trusted: true,
                env: std::collections::BTreeMap::new(),
            },
        );
        config.workspaces.insert(
            "docs".to_owned(),
            jackin_config::WorkspaceConfig {
                workdir: "/docs".to_owned(),
                default_role: Some("the-architect".to_owned()),
                default_agent: Some(Agent::Claude),
                ..jackin_config::WorkspaceConfig::default()
            },
        );
        let args = PrewarmArgs {
            agents: Vec::new(),
            image: true,
            roles: false,
            sidecar: false,
            role: None,
            workspace: None,
            all_workspaces: true,
            role_git: None,
            role_branch: None,
        };
        let targets = PrewarmImageTarget::resolve(&args, &config).unwrap();

        assert_eq!(targets.len(), 2);
        assert_eq!(
            binary_prewarm_agents(&args, &targets),
            vec![Agent::Claude, Agent::Codex]
        );
    }

    #[test]
    fn roles_prewarm_does_not_require_image_targets() {
        let config = config_with_workspace_default(Some(Agent::Codex));
        let args = PrewarmArgs {
            agents: Vec::new(),
            image: false,
            roles: true,
            sidecar: false,
            role: None,
            workspace: None,
            all_workspaces: false,
            role_git: None,
            role_branch: None,
        };

        assert!(PrewarmImageTarget::resolve(&args, &config).is_err());
        assert_eq!(binary_prewarm_agents(&args, &[]), Agent::ALL.to_vec());
    }

    #[test]
    fn roles_prewarm_can_target_one_role_without_image() {
        let config = config_with_workspace_default(Some(Agent::Codex));
        let args = PrewarmArgs {
            agents: Vec::new(),
            image: false,
            roles: true,
            sidecar: false,
            role: Some("agent-smith".to_owned()),
            workspace: None,
            all_workspaces: false,
            role_git: None,
            role_branch: None,
        };
        let targets = PrewarmRoleTarget::resolve(&args, &config).unwrap();

        assert_eq!(targets.len(), 1);
        assert_eq!(targets[0].selector.key(), "agent-smith");
        assert_eq!(
            targets[0].role_git,
            "https://example.invalid/agent-smith.git"
        );
    }

    #[test]
    fn roles_prewarm_can_target_workspace_default_role_without_image() {
        let config = config_with_workspace_default(Some(Agent::Codex));
        let args = PrewarmArgs {
            agents: Vec::new(),
            image: false,
            roles: true,
            sidecar: false,
            role: None,
            workspace: Some("jackin".to_owned()),
            all_workspaces: false,
            role_git: None,
            role_branch: None,
        };
        let targets = PrewarmRoleTarget::resolve(&args, &config).unwrap();

        assert_eq!(targets.len(), 1);
        assert_eq!(targets[0].selector.key(), "agent-smith");
        assert_eq!(
            targets[0].role_git,
            "https://example.invalid/agent-smith.git"
        );
    }

    #[test]
    fn roles_prewarm_all_workspaces_deduplicates_default_roles_without_image() {
        let mut config = config_with_workspace_default(Some(Agent::Codex));
        config.workspaces.insert(
            "docs".to_owned(),
            jackin_config::WorkspaceConfig {
                workdir: "/docs".to_owned(),
                default_role: Some("agent-smith".to_owned()),
                default_agent: Some(Agent::Claude),
                ..jackin_config::WorkspaceConfig::default()
            },
        );
        let args = PrewarmArgs {
            agents: Vec::new(),
            image: false,
            roles: true,
            sidecar: false,
            role: None,
            workspace: None,
            all_workspaces: true,
            role_git: None,
            role_branch: None,
        };
        let targets = PrewarmRoleTarget::resolve(&args, &config).unwrap();

        assert_eq!(targets.len(), 1);
        assert_eq!(targets[0].selector.key(), "agent-smith");
    }

    #[test]
    fn roles_prewarm_can_use_role_git_override_without_image() {
        let config = AppConfig::default();
        let args = PrewarmArgs {
            agents: Vec::new(),
            image: false,
            roles: true,
            sidecar: false,
            role: Some("agent-smith".to_owned()),
            workspace: None,
            all_workspaces: false,
            role_git: Some("https://example.invalid/custom.git".to_owned()),
            role_branch: None,
        };
        let targets = PrewarmRoleTarget::resolve(&args, &config).unwrap();

        assert_eq!(targets.len(), 1);
        assert_eq!(targets[0].role_git, "https://example.invalid/custom.git");
    }
}
