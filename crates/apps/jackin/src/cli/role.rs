// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use clap::{Args, Subcommand};
use std::path::PathBuf;

use super::{BANNER, HELP_STYLES};

#[derive(Debug, Args, PartialEq, Eq)]
#[command(
    before_help = BANNER,
    styles = HELP_STYLES,
    after_long_help = "\
Examples:
  jackin load                                          # use workspace + last role for cwd
  jackin load --rebuild                                # same, with fresh agent install
  jackin load agent-smith
  jackin load agent-smith ~/Projects/my-app
  jackin load agent-smith ~/Projects/my-app:/app
  jackin load agent-smith big-monorepo
  jackin load agent-smith big-monorepo --mount ~/extra-data
  jackin load agent-smith ~/app --mount ~/cache:/cache:ro
  jackin load the-architect --agent codex --model gpt-6-luna --effort max
  jackin load the-architect --role-branch feat/my-pr   # build + test a PR branch locally"
)]
#[command(
    about = "Jack a role into an isolated container",
    long_about = "Jack a role into an isolated container

TARGET can be a path (~/Projects/my-app), a path with container destination (~/Projects/my-app:/app), or a saved workspace name. When omitted, the current directory is used."
)]
pub struct LoadArgs {
    #[arg(
        help = "Role class selector (e.g. `agent-smith`, `chainargos/agent-brown`). When omitted, uses the last-used or default role for the workspace.",
        long_help = "Role class selector (e.g. `agent-smith`, `chainargos/agent-brown`). When omitted, uses the last-used or default role for the workspace."
    )]
    pub selector: Option<String>,
    #[arg(
        help = "Path, `path:container-dest`, or saved workspace name",
        long_help = "Path, `path:container-dest`, or saved workspace name",
        value_name = "TARGET"
    )]
    pub target: Option<String>,
    #[arg(
        help = "Additional bind-mount spec as `path[:ro]` or `src:dst[:ro]` (repeatable)",
        long_help = "Additional bind-mount spec as `path[:ro]` or `src:dst[:ro]` (repeatable)",
        long = "mount"
    )]
    pub mounts: Vec<String>,
    #[arg(
        help = "Force rebuild the Docker image and refresh agent CLI install layers",
        long_help = "Force rebuild the Docker image and refresh agent CLI install layers",
        long,
        default_value_t = false
    )]
    pub rebuild: bool,
    #[arg(
        help = "Acknowledge a dirty host working tree for isolated mounts.",
        long_help = "Acknowledge a dirty host working tree for isolated mounts.",
        long
    )]
    pub force: bool,
    #[arg(help = "Agent to launch under (claude, codex, amp, kimi, opencode, grok, antigravity, gemini, cursor, muse, omp, or hermes). Overrides the workspace's `default_agent` field for this launch only. When neither is set, defaults to claude.", long_help = "Agent to launch under (claude, codex, amp, kimi, opencode, grok, antigravity, gemini, cursor, muse, omp, or hermes). Overrides the workspace's `default_agent` field for this launch only. When neither is set, defaults to claude.", long, value_parser = parse_agent)]
    pub agent: Option<jackin_core::Agent>,
    #[arg(
        help = "Registered account ID assigned to this workspace; overrides its agent binding",
        long_help = "Registered account ID assigned to this workspace; overrides its agent binding",
        long
    )]
    pub account: Option<String>,
    #[arg(help = "Model ID for this load only; overrides the selected agent's role and account defaults.", long_help = "Model ID for this load only; overrides the selected agent's role and account defaults.", long, value_name = "MODEL", value_parser = parse_model)]
    pub model: Option<String>,
    #[arg(help = "Reasoning effort for this load only (low, medium, high, or max).", long_help = "Reasoning effort for this load only (low, medium, high, or max).", long, value_name = "EFFORT", value_parser = parse_reasoning_effort)]
    pub effort: Option<jackin_core::ReasoningEffort>,
    #[arg(
        help = "Check out a specific branch of the role repository for local testing. The published image is ignored and the image is built from the branch's Dockerfile using Docker's layer cache. Useful for verifying a PR before it merges to the default branch.",
        long_help = "Check out a specific branch of the role repository for local testing. The published image is ignored and the image is built from the branch's Dockerfile using Docker's layer cache. Useful for verifying a PR before it merges to the default branch.",
        long
    )]
    pub role_branch: Option<String>,
    #[arg(help = "Docker security profile for this launch.", long_help = "Docker security profile for this launch.", long, value_name = "PROFILE", value_parser = parse_docker_profile)]
    pub docker_profile: Option<jackin_runtime::runtime::DockerSecurityProfile>,
    #[arg(
        help = "Print the resolved launch plan (workspace, role, mounts, auth decisions, derived image) and exit without spawning any containers.",
        long_help = "Print the resolved launch plan (workspace, role, mounts, auth decisions, derived image) and exit without spawning any containers.",
        long
    )]
    pub dry_run: bool,
    #[arg(
        long,
        value_name = "FORMAT",
        default_value = "human",
        requires = "dry_run"
    )]
    #[arg(
        help = "Output format for `--dry-run` (`human` or `json`)",
        long_help = "Output format for `--dry-run` (`human` or `json`)"
    )]
    pub format: String,
}

fn parse_agent(s: &str) -> Result<jackin_core::Agent, String> {
    s.parse()
        .map_err(|e: jackin_core::ParseAgentError| e.to_string())
}

fn parse_model(s: &str) -> Result<String, String> {
    let model = s.trim();
    if model.is_empty() {
        return Err("model ID cannot be empty".to_owned());
    }
    Ok(model.to_owned())
}

fn parse_reasoning_effort(s: &str) -> Result<jackin_core::ReasoningEffort, String> {
    s.parse::<jackin_core::ReasoningEffort>()
        .map_err(|error| error.to_string())
}

fn parse_docker_profile(s: &str) -> Result<jackin_runtime::runtime::DockerSecurityProfile, String> {
    s.parse()
        .map_err(|e: jackin_runtime::runtime::docker_profile::ParseProfileError| e.to_string())
}

#[derive(Debug, Args, PartialEq, Eq)]
#[command(
    before_help = BANNER,
    styles = HELP_STYLES,
    after_long_help = "\
Examples:
  jackin hardline                              # auto-detect workspace + running role for cwd
  jackin hardline --inspect                    # inspect detected instance state without attaching
  jackin hardline --new                        # start another agent session in the detected instance
  jackin hardline --new --agent codex          # start a specific runtime in the selected instance
  jackin hardline --shell                      # open a one-shot zsh shell in the detected instance
  jackin hardline agent-smith
  jackin hardline --inspect k7p9m2xq
  jackin hardline chainargos/the-architect
  jackin hardline k7p9m2xq
  jackin hardline jk-k7p9m2xq-agentsmith"
)]
#[command(
    about = "Reattach to a running role's session",
    long_about = "Reattach to a running role's session

When omitted, finds the saved workspace for the current directory and reconnects to a running role container belonging to it."
)]
pub struct HardlineArgs {
    #[arg(
        help = "Role class selector, instance ID, or container name to reconnect to. When omitted, uses the running role in the workspace for the current directory.",
        long_help = "Role class selector, instance ID, or container name to reconnect to. When omitted, uses the running role in the workspace for the current directory."
    )]
    pub selector: Option<String>,
    #[arg(
        help = "Print manifest, Docker, `DinD`, and mount state without attaching or restarting.",
        long_help = "Print manifest, Docker, `DinD`, and mount state without attaching or restarting.",
        long
    )]
    pub inspect: bool,
    #[arg(
        help = "Start a new foreground agent process inside the selected running instance.",
        long_help = "Start a new foreground agent process inside the selected running instance.",
        long,
        conflicts_with = "inspect"
    )]
    pub new: bool,
    #[arg(help = "Agent runtime for `--new` (claude, codex, amp, kimi, opencode, grok, antigravity, gemini, cursor, muse, omp, or hermes). Defaults to the instance manifest.", long_help = "Agent runtime for `--new` (claude, codex, amp, kimi, opencode, grok, antigravity, gemini, cursor, muse, omp, or hermes). Defaults to the instance manifest.", long, value_parser = parse_agent, requires = "new")]
    pub agent: Option<jackin_core::Agent>,
    #[arg(help = "Open a zsh shell in the selected running instance without an agent slug.", long_help = "Open a zsh shell in the selected running instance without an agent slug.", long, conflicts_with_all = ["inspect", "new"])]
    pub shell: bool,
}

#[derive(Debug, Args, PartialEq, Eq, Default, Clone)]
#[command(about = "Open the operator console to manage workspaces, launch roles, and more", long_about = "Open the operator console to manage workspaces, launch roles, and more

Running `jackin` with no subcommand on an interactive terminal opens the same console. The operator console is always the full experience — rich TUI, intro rain on entry, outro rain on the last container's exit. There is nothing to disable, so this carries no flags.", styles = HELP_STYLES)]
pub struct ConsoleArgs {}

#[derive(Debug, Subcommand, PartialEq, Eq)]
pub enum RoleCommand {
    #[command(about = "Validate a role repository's manifest, Dockerfile, hooks, and env declarations", long_about = "Validate a role repository's manifest, Dockerfile, hooks, and env declarations", before_help = BANNER, styles = HELP_STYLES)]
    Validate(RoleRepoPathArgs),
    #[command(about = "Migrate a role manifest to the current schema version, then validate it", long_about = "Migrate a role manifest to the current schema version, then validate it", before_help = BANNER, styles = HELP_STYLES)]
    Migrate(RoleRepoPathArgs),
    #[command(about = "Create a new role repository scaffold", long_about = "Create a new role repository scaffold", before_help = BANNER, styles = HELP_STYLES)]
    Create(RoleCreateArgs),
    #[command(about = "Print the construct image version tag pinned in the role Dockerfile", long_about = "Print the construct image version tag pinned in the role Dockerfile", before_help = BANNER, styles = HELP_STYLES)]
    ConstructVersion(RoleRepoPathArgs),
    #[command(about = "Print the published Docker image declared in the role manifest", long_about = "Print the published Docker image declared in the role manifest", before_help = BANNER, styles = HELP_STYLES)]
    PublishedImage(RoleRepoPathArgs),
    #[command(about = "Print the published Docker image repository without tag or digest", long_about = "Print the published Docker image repository without tag or digest", before_help = BANNER, styles = HELP_STYLES)]
    PublishedImageRepository(RoleRepoPathArgs),
    #[command(about = "Print Docker labels for publishing the role image", long_about = "Print Docker labels for publishing the role image", before_help = BANNER, styles = HELP_STYLES)]
    PublishLabels(RolePublishLabelsArgs),
}

#[derive(Debug, Args, PartialEq, Eq)]
#[command(
    about = "Role repository path argument shared by `validate` and `migrate`.",
    long_about = "Role repository path argument shared by `validate` and `migrate`."
)]
pub struct RoleRepoPathArgs {
    #[arg(
        help = "Role repository path. Defaults to the current directory.",
        long_help = "Role repository path. Defaults to the current directory.",
        value_name = "ROLE_REPO_PATH"
    )]
    pub path: Option<PathBuf>,
}

#[derive(Debug, Args, PartialEq, Eq)]
#[command(
    about = "Arguments for `jackin role publish-labels`.",
    long_about = "Arguments for `jackin role publish-labels`."
)]
pub struct RolePublishLabelsArgs {
    #[arg(
        help = "Git commit SHA for the role repository image being published.",
        long_help = "Git commit SHA for the role repository image being published.",
        long
    )]
    pub role_git_sha: String,
    #[arg(
        help = "Role repository path. Defaults to the current directory.",
        long_help = "Role repository path. Defaults to the current directory.",
        value_name = "ROLE_REPO_PATH"
    )]
    pub path: Option<PathBuf>,
}

#[derive(Debug, Args, PartialEq, Eq)]
#[command(
    about = "Arguments for `jackin role create`.",
    long_about = "Arguments for `jackin role create`."
)]
pub struct RoleCreateArgs {
    #[arg(
        help = "Role name or namespace/name selector, e.g. `docs-writer` or `chainargos/backend-engineer`",
        long_help = "Role name or namespace/name selector, e.g. `docs-writer` or `chainargos/backend-engineer`"
    )]
    pub role: String,
    #[arg(
        help = "Projects directory where the role repo should be created. Defaults to `JACKIN_PROJECTS_DIR` or ~/Projects.",
        long_help = "Projects directory where the role repo should be created. Defaults to `JACKIN_PROJECTS_DIR` or ~/Projects.",
        value_name = "PROJECTS_DIR"
    )]
    pub projects_dir: Option<PathBuf>,
}
