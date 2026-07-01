use clap::Args;
use owo_colors::OwoColorize as _;

use crate::cli::{BANNER, HELP_STYLES};
use jackin_config::AppConfig;
use jackin_core::Agent;
use jackin_core::JackinPaths;
use jackin_core::RoleSelector;
use jackin_docker::ShellRunner;
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
    #[command(flatten)]
    pub flags: PrewarmFlags,

    /// Role selector whose repo cache and/or derived image(s) should be prewarmed.
    #[arg(long, conflicts_with_all = ["workspace"])]
    pub role: Option<String>,
    /// Saved workspace whose default role repo and/or agent image should be prewarmed.
    #[arg(long, conflicts_with_all = ["role"])]
    pub workspace: Option<String>,

    /// Role git URL override for role/image prewarm. Defaults to configured role source.
    #[arg(long, requires = "role", conflicts_with_all = ["workspace"])]
    pub role_git: Option<String>,
    /// Role branch to prewarm. Uses branch-scoped image tags.
    #[arg(long, requires = "image")]
    pub role_branch: Option<String>,
}

/// Flags for `jackin prewarm` (flattened into `PrewarmArgs` for CLI ergonomics).
#[expect(
    clippy::struct_excessive_bools,
    reason = "tracked in codebase-health-enforcement"
)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, clap::Args)]
pub struct PrewarmFlags {
    #[arg(long)]
    pub image: bool,
    #[arg(long)]
    pub daemon: bool,
    #[arg(long)]
    pub roles: bool,
    #[arg(long)]
    pub sidecar: bool,
    #[arg(long)]
    pub sidecar_container: bool,
    #[arg(long)]
    pub keep_sidecar_container: bool,
    #[arg(long, conflicts_with_all = ["workspace"])]
    pub all_workspaces: bool,
    #[arg(long)]
    pub all_roles: bool,
}

fn parse_agent(s: &str) -> Result<Agent, String> {
    s.parse()
        .map_err(|e: jackin_core::ParseAgentError| e.to_string())
}

pub async fn run(
    args: &PrewarmArgs,
    paths: &JackinPaths,
    config: &AppConfig,
    debug: bool,
) -> anyhow::Result<()> {
    let image_targets = if args.flags.image {
        PrewarmImageTarget::resolve(args, config)?
    } else {
        Vec::new()
    };
    let agents = binary_prewarm_agents(args, &image_targets);

    print!("{BANNER}");
    println!("prewarm\n");

    let capsule = jackin_image::capsule_binary::ensure_available(paths);
    let agents_result = prewarm_agents(paths, &agents);
    let sidecar_needed = should_prewarm_sidecar_image(args);
    let sidecar_result = async {
        if sidecar_needed {
            Some(prewarm_sidecar_image_status().await)
        } else {
            None
        }
    };
    let sidecar_container_needed = should_prewarm_sidecar_container(args);
    if args.flags.daemon {
        emit_daemon_prewarm_plan();
    }
    let sidecar_container_result = async {
        if sidecar_container_needed {
            Some(prewarm_sidecar_container_status(should_keep_sidecar_container(args)).await)
        } else {
            None
        }
    };
    let (capsule_result, agent_results, sidecar_result, sidecar_container_result) = tokio::join!(
        capsule,
        agents_result,
        sidecar_result,
        sidecar_container_result
    );

    match capsule_result {
        Ok(path) => println!("  {}  capsule  {}", "✓".green(), path.display()),
        Err(error) => {
            println!("  {}  capsule  {error:#}", "✗".red().bold());
            return Err(error);
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

    if let Some(result) = sidecar_result {
        print_sidecar_image_result(result)?;
    }
    if let Some(result) = sidecar_container_result {
        if let Ok(row) = result.as_ref()
            && row.kept
        {
            jackin_runtime::runtime::write_prewarmed_dind_state(paths, row)?;
        }
        print_sidecar_container_result(result)?;
    }

    if !image_targets.is_empty() {
        prewarm_images(args, paths, image_targets, debug).await?;
    }

    if args.flags.roles {
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

fn should_prewarm_sidecar_image(args: &PrewarmArgs) -> bool {
    args.flags.sidecar || args.flags.image
}

fn should_prewarm_sidecar_container(args: &PrewarmArgs) -> bool {
    args.flags.sidecar_container || args.flags.daemon
}

fn should_keep_sidecar_container(args: &PrewarmArgs) -> bool {
    args.flags.keep_sidecar_container || args.flags.daemon
}

fn emit_daemon_prewarm_plan() {
    let detail = r#"{"plan":"PrewarmOnly","reason":"daemon_prewarm:kept_sidecar","container":null,"skipped":["standalone_sidecar_image_prewarm"]}"#;
    if let Some(run) = jackin_diagnostics::active_run() {
        run.stage(
            "launch_plan",
            "sidecar",
            "selected launch plan PrewarmOnly",
            Some(detail),
        );
    }
}

enum SidecarImagePrewarmStatus {
    Present,
    Pulled,
}

async fn prewarm_sidecar_image_status() -> anyhow::Result<SidecarImagePrewarmStatus> {
    let docker = BollardDockerClient::connect()?;
    let image = jackin_runtime::runtime::DIND_IMAGE;
    let tags = docker.list_image_tags(image).await?;
    if tags.is_empty() {
        docker.pull_image(image).await?;
        Ok(SidecarImagePrewarmStatus::Pulled)
    } else {
        Ok(SidecarImagePrewarmStatus::Present)
    }
}

async fn prewarm_sidecar_container_status(
    keep: bool,
) -> anyhow::Result<jackin_runtime::runtime::DindSidecarPrewarm> {
    let docker = BollardDockerClient::connect()?;
    jackin_runtime::runtime::prewarm_dind_sidecar_container(&docker, keep).await
}

fn print_sidecar_image_result(
    result: anyhow::Result<SidecarImagePrewarmStatus>,
) -> anyhow::Result<()> {
    println!();
    println!("sidecar");
    let image = jackin_runtime::runtime::DIND_IMAGE;
    match result {
        Ok(SidecarImagePrewarmStatus::Pulled) => {
            println!("  {}  {:<8} pulled", "✓".green(), image);
            Ok(())
        }
        Ok(SidecarImagePrewarmStatus::Present) => {
            println!("  {}  {:<8} present", "✓".green(), image);
            Ok(())
        }
        Err(error) => {
            println!("  {}  {:<8} {error:#}", "✗".red().bold(), image);
            Err(error)
        }
    }
}

fn print_sidecar_container_result(
    result: anyhow::Result<jackin_runtime::runtime::DindSidecarPrewarm>,
) -> anyhow::Result<()> {
    println!();
    println!("sidecar container");
    match result {
        Ok(row) => {
            println!(
                "  {}  {:<8} {:<13} {}ms  {}",
                "✓".green(),
                jackin_runtime::runtime::DIND_IMAGE,
                if row.kept {
                    "ready+kept"
                } else {
                    "ready+removed"
                },
                row.ready_ms,
                row.dind
            );
            Ok(())
        }
        Err(error) => {
            println!(
                "  {}  {:<8} {error:#}",
                "✗".red().bold(),
                jackin_runtime::runtime::DIND_IMAGE
            );
            Err(error)
        }
    }
}

async fn prewarm_role_repos(
    paths: &JackinPaths,
    targets: Vec<PrewarmRoleTarget>,
    debug: bool,
) -> anyhow::Result<()> {
    println!();
    println!("role repos");

    let mut tasks = tokio::task::JoinSet::new();
    for (index, target) in targets.into_iter().enumerate() {
        let paths = paths.clone();
        tasks.spawn(async move {
            let mut runner = ShellRunner { debug };
            let selector = target.selector;
            let result = jackin_runtime::runtime::register_agent_repo(
                &paths,
                &selector,
                &target.role_git,
                &mut runner,
                debug,
            )
            .await
            .map(|(cached_repo, _validated_repo)| cached_repo.repo_dir);
            (index, selector, result)
        });
    }

    let mut results = Vec::new();
    while let Some(result) = tasks.join_next().await {
        results.push(result?);
    }
    results.sort_by_key(|(index, _, _)| *index);

    let mut failed = Vec::new();
    for (_index, selector, result) in results {
        match result {
            Ok(repo_dir) => println!(
                "  {}  {:<24} {}",
                "✓".green(),
                selector.key(),
                repo_dir.display()
            ),
            Err(error) => {
                println!("  {}  {:<24} {error:#}", "✗".red().bold(), selector.key());
                failed.push(selector.key());
            }
        }
    }

    if failed.is_empty() {
        Ok(())
    } else {
        anyhow::bail!("{} role repo prewarm(s) failed", failed.len())
    }
}

/// Resolve a role's git source: prefer an explicit `--role-git` override, else
/// look it up in `config.roles[selector]`. On miss, the error suggests
/// `--role-git` only when `suggest_role_git_flag` is set — the flag conflicts
/// with `--workspace`/`--all-workspaces`, so the workspace path must not hint it.
fn resolve_role_git(
    config: &AppConfig,
    selector: &RoleSelector,
    override_git: Option<&str>,
    suggest_role_git_flag: bool,
) -> anyhow::Result<String> {
    override_git
        .map(str::to_owned)
        .or_else(|| {
            config
                .roles
                .get(&selector.key())
                .map(|source| source.git.clone())
        })
        .ok_or_else(|| {
            let hint = if suggest_role_git_flag {
                "; pass `--role-git <url>`"
            } else {
                ""
            };
            anyhow::anyhow!("no git source configured for role `{selector}`{hint}")
        })
}

struct PrewarmRoleTarget {
    selector: RoleSelector,
    role_git: String,
}

impl PrewarmRoleTarget {
    fn resolve(args: &PrewarmArgs, config: &AppConfig) -> anyhow::Result<Vec<Self>> {
        if args.flags.all_workspaces {
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
            let role_git = resolve_role_git(config, &selector, args.role_git.as_deref(), true)?;
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
        targets.sort_by_key(|a| a.selector.key());
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
        let role_git = resolve_role_git(config, &selector, None, false)?;
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
    targets: Vec<PrewarmImageTarget>,
    debug: bool,
) -> anyhow::Result<()> {
    let mut tasks = tokio::task::JoinSet::new();
    for (index, target) in targets.into_iter().enumerate() {
        let paths = paths.clone();
        let role_branch = args.role_branch.clone();
        tasks.spawn(async move {
            let PrewarmImageTarget {
                selector,
                role_git,
                agents,
                label,
                is_agent_narrowed: _,
            } = target;
            let rows = jackin_runtime::runtime::prewarm_role_images(
                &paths,
                &selector,
                &role_git,
                role_branch.as_deref(),
                &agents,
                debug,
            )
            .await;
            (index, label, rows)
        });
    }

    let mut results = Vec::new();
    while let Some(result) = tasks.join_next().await {
        results.push(result?);
    }
    results.sort_by_key(|(index, _, _)| *index);

    for (_index, label, rows) in results {
        println!();
        println!("images for {label}");
        print_image_prewarm_rows(rows?)?;
    }
    Ok(())
}

fn print_image_prewarm_rows(
    rows: Vec<jackin_runtime::runtime::RoleImagePrewarmRow>,
) -> anyhow::Result<()> {
    for row in rows {
        let status = match row.status {
            jackin_runtime::runtime::ImagePrewarmStatus::Reused => "reused",
            jackin_runtime::runtime::ImagePrewarmStatus::Built => "built",
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
        if args.flags.all_workspaces {
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

        if args.flags.all_roles {
            if config.roles.is_empty() {
                anyhow::bail!("no configured roles to image-prewarm");
            }
            let mut targets = config
                .roles
                .iter()
                .map(|(key, source)| {
                    let selector = RoleSelector::parse(key)?;
                    Ok(Self {
                        label: selector.to_string(),
                        selector,
                        role_git: source.git.clone(),
                        agents: args.agents.clone(),
                        is_agent_narrowed: !args.agents.is_empty(),
                    })
                })
                .collect::<anyhow::Result<Vec<_>>>()?;
            targets.sort_by(|a, b| a.label.cmp(&b.label));
            return Ok(targets);
        }

        if let Some(role) = args.role.as_deref() {
            let selector = RoleSelector::parse(role)?;
            let role_git = resolve_role_git(config, &selector, args.role_git.as_deref(), true)?;
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
                "`jackin prewarm --image` requires `--role <selector>`, `--workspace <name>`, `--all-workspaces`, or `--all-roles`"
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
        let role_git = resolve_role_git(config, &selector, None, false)?;
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
            let result = jackin_image::agent_binary::ensure_available(&paths, agent)
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
            flags: PrewarmFlags {
                image: true,
                daemon: false,
                roles: false,
                sidecar: false,
                sidecar_container: false,
                keep_sidecar_container: false,
                all_workspaces: false,
                all_roles: false,
            },
            role: None,
            workspace: Some("jackin".to_owned()),
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
            flags: PrewarmFlags {
                image: true,
                daemon: false,
                roles: false,
                sidecar: false,
                sidecar_container: false,
                keep_sidecar_container: false,
                all_workspaces: false,
                all_roles: false,
            },
            role: Some("agent-smith".to_owned()),
            workspace: None,
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
            flags: PrewarmFlags {
                image: true,
                daemon: false,
                roles: false,
                sidecar: false,
                sidecar_container: false,
                keep_sidecar_container: false,
                all_workspaces: true,
                all_roles: false,
            },
            role: None,
            workspace: None,
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
    fn image_all_roles_expands_configured_roles_without_agent_narrowing() {
        let mut config = config_with_workspace_default(Some(Agent::Codex));
        config.roles.insert(
            "the-architect".to_owned(),
            jackin_config::RoleSource {
                git: "https://example.invalid/the-architect.git".to_owned(),
                trusted: true,
                env: std::collections::BTreeMap::new(),
            },
        );
        let args = PrewarmArgs {
            agents: Vec::new(),
            flags: PrewarmFlags {
                image: true,
                daemon: false,
                roles: false,
                sidecar: false,
                sidecar_container: false,
                keep_sidecar_container: false,
                all_workspaces: false,
                all_roles: true,
            },
            role: None,
            workspace: None,
            role_git: None,
            role_branch: None,
        };

        let targets = PrewarmImageTarget::resolve(&args, &config).unwrap();

        assert_eq!(targets.len(), 2);
        assert_eq!(targets[0].selector.key(), "agent-smith");
        assert_eq!(targets[1].selector.key(), "the-architect");
        assert!(targets.iter().all(|target| target.agents.is_empty()));
        assert_eq!(binary_prewarm_agents(&args, &targets), Agent::ALL.to_vec());
    }

    #[test]
    fn image_all_roles_respects_explicit_agent_filter() {
        let config = config_with_workspace_default(Some(Agent::Codex));
        let args = PrewarmArgs {
            agents: vec![Agent::Kimi],
            flags: PrewarmFlags {
                image: true,
                daemon: false,
                roles: false,
                sidecar: false,
                sidecar_container: false,
                keep_sidecar_container: false,
                all_workspaces: false,
                all_roles: true,
            },
            role: None,
            workspace: None,
            role_git: None,
            role_branch: None,
        };

        let targets = PrewarmImageTarget::resolve(&args, &config).unwrap();

        assert_eq!(targets.len(), 1);
        assert_eq!(targets[0].agents, vec![Agent::Kimi]);
        assert_eq!(binary_prewarm_agents(&args, &targets), vec![Agent::Kimi]);
    }

    #[test]
    fn roles_prewarm_does_not_require_image_targets() {
        let config = config_with_workspace_default(Some(Agent::Codex));
        let args = PrewarmArgs {
            agents: Vec::new(),
            flags: PrewarmFlags {
                image: false,
                daemon: false,
                roles: true,
                sidecar: false,
                sidecar_container: false,
                keep_sidecar_container: false,
                all_workspaces: false,
                all_roles: false,
            },
            role: None,
            workspace: None,
            role_git: None,
            role_branch: None,
        };

        assert!(PrewarmImageTarget::resolve(&args, &config).is_err());
        assert_eq!(binary_prewarm_agents(&args, &[]), Agent::ALL.to_vec());
        assert!(!should_prewarm_sidecar_image(&args));
    }

    #[test]
    fn image_prewarm_also_prewarms_sidecar_image() {
        let args = PrewarmArgs {
            agents: Vec::new(),
            flags: PrewarmFlags {
                image: true,
                daemon: false,
                roles: false,
                sidecar: false,
                sidecar_container: false,
                keep_sidecar_container: false,
                all_workspaces: false,
                all_roles: false,
            },
            role: Some("agent-smith".to_owned()),
            workspace: None,
            role_git: None,
            role_branch: None,
        };

        assert!(should_prewarm_sidecar_image(&args));
    }

    #[test]
    fn sidecar_prewarm_can_run_without_image_targets() {
        let args = PrewarmArgs {
            agents: Vec::new(),
            flags: PrewarmFlags {
                image: false,
                daemon: false,
                roles: false,
                sidecar: true,
                sidecar_container: false,
                keep_sidecar_container: false,
                all_workspaces: false,
                all_roles: false,
            },
            role: None,
            workspace: None,
            role_git: None,
            role_branch: None,
        };

        assert!(should_prewarm_sidecar_image(&args));
    }

    #[test]
    fn sidecar_container_prewarm_uses_container_path_image_lookup() {
        let args = PrewarmArgs {
            agents: Vec::new(),
            flags: PrewarmFlags {
                image: false,
                daemon: false,
                roles: false,
                sidecar: false,
                sidecar_container: true,
                keep_sidecar_container: false,
                all_workspaces: false,
                all_roles: false,
            },
            role: None,
            workspace: None,
            role_git: None,
            role_branch: None,
        };

        assert!(!should_prewarm_sidecar_image(&args));
        assert!(should_prewarm_sidecar_container(&args));
        assert!(!should_keep_sidecar_container(&args));
    }

    #[test]
    fn daemon_prewarm_keeps_sidecar_without_duplicate_image_lookup() {
        let args = PrewarmArgs {
            agents: Vec::new(),
            flags: PrewarmFlags {
                image: false,
                daemon: true,
                roles: false,
                sidecar: false,
                sidecar_container: false,
                keep_sidecar_container: true,
                all_workspaces: false,
                all_roles: false,
            },
            role: None,
            workspace: None,
            role_git: None,
            role_branch: None,
        };

        assert!(!should_prewarm_sidecar_image(&args));
        assert!(should_prewarm_sidecar_container(&args));
        assert!(should_keep_sidecar_container(&args));
    }

    #[test]
    fn daemon_prewarm_records_plan_and_skipped_work() {
        let temp = tempfile::tempdir().unwrap();
        let paths = JackinPaths::for_tests(temp.path());
        let run = jackin_diagnostics::RunDiagnostics::start(&paths, false, "prewarm").unwrap();
        let _guard = run.activate();

        emit_daemon_prewarm_plan();

        let diagnostics = std::fs::read_to_string(run.path()).unwrap();
        assert!(
            diagnostics.contains("\"kind\":\"launch_plan\""),
            "{diagnostics}"
        );
        assert!(diagnostics.contains("PrewarmOnly"), "{diagnostics}");
        assert!(
            diagnostics.contains("daemon_prewarm:kept_sidecar"),
            "{diagnostics}"
        );
        assert!(
            diagnostics.contains("standalone_sidecar_image_prewarm"),
            "{diagnostics}"
        );
    }

    #[test]
    fn roles_prewarm_can_target_one_role_without_image() {
        let config = config_with_workspace_default(Some(Agent::Codex));
        let args = PrewarmArgs {
            agents: Vec::new(),
            flags: PrewarmFlags {
                image: false,
                daemon: false,
                roles: true,
                sidecar: false,
                sidecar_container: false,
                keep_sidecar_container: false,
                all_workspaces: false,
                all_roles: false,
            },
            role: Some("agent-smith".to_owned()),
            workspace: None,
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
            flags: PrewarmFlags {
                image: false,
                daemon: false,
                roles: true,
                sidecar: false,
                sidecar_container: false,
                keep_sidecar_container: false,
                all_workspaces: false,
                all_roles: false,
            },
            role: None,
            workspace: Some("jackin".to_owned()),
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
            flags: PrewarmFlags {
                image: false,
                daemon: false,
                roles: true,
                sidecar: false,
                sidecar_container: false,
                keep_sidecar_container: false,
                all_workspaces: true,
                all_roles: false,
            },
            role: None,
            workspace: None,
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
            flags: PrewarmFlags {
                image: false,
                daemon: false,
                roles: true,
                sidecar: false,
                sidecar_container: false,
                keep_sidecar_container: false,
                all_workspaces: false,
                all_roles: false,
            },
            role: Some("agent-smith".to_owned()),
            workspace: None,
            role_git: Some("https://example.invalid/custom.git".to_owned()),
            role_branch: None,
        };
        let targets = PrewarmRoleTarget::resolve(&args, &config).unwrap();

        assert_eq!(targets.len(), 1);
        assert_eq!(targets[0].role_git, "https://example.invalid/custom.git");
    }
}
