use clap::Args;
use owo_colors::OwoColorize as _;

use crate::agent::Agent;
use crate::cli::{BANNER, HELP_STYLES};
use crate::config::AppConfig;
use crate::paths::JackinPaths;
use crate::selector::RoleSelector;

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
    /// Role selector whose derived image(s) should be prewarmed.
    #[arg(long, requires = "image")]
    pub role: Option<String>,
    /// Role git URL override for image prewarm. Defaults to configured role source.
    #[arg(long, requires = "image")]
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
    let agents = if args.agents.is_empty() {
        Agent::ALL.to_vec()
    } else {
        args.agents.clone()
    };

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

    if args.image {
        prewarm_images(args, paths, config, debug).await?;
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

async fn prewarm_images(
    args: &PrewarmArgs,
    paths: &JackinPaths,
    config: &AppConfig,
    debug: bool,
) -> anyhow::Result<()> {
    let role = args
        .role
        .as_deref()
        .ok_or_else(|| anyhow::anyhow!("`jackin prewarm --image` requires `--role <selector>`"))?;
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

    println!();
    println!("images for {selector}");
    let rows = crate::runtime::prewarm_role_images(
        paths,
        &selector,
        &role_git,
        args.role_branch.as_deref(),
        &args.agents,
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
