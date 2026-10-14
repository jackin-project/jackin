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
pub struct LoadArgs {
    pub selector: Option<String>,

    #[arg(value_name = "TARGET")]
    pub target: Option<String>,

    #[arg(long = "mount")]
    pub mounts: Vec<String>,

    #[arg(long, default_value_t = false)]
    pub rebuild: bool,

    #[arg(long)]
    pub force: bool,

    #[arg(long, value_parser = parse_agent)]
    pub agent: Option<jackin_core::Agent>,

    #[arg(long)]
    pub account: Option<String>,

    #[arg(long, value_name = "MODEL", value_parser = parse_model)]
    pub model: Option<String>,

    #[arg(long, value_name = "EFFORT", value_parser = parse_reasoning_effort)]
    pub effort: Option<jackin_core::ReasoningEffort>,

    #[arg(long)]
    pub role_branch: Option<String>,

    #[arg(long, value_name = "PROFILE", value_parser = parse_docker_profile)]
    pub docker_profile: Option<jackin_runtime::runtime::DockerSecurityProfile>,

    #[arg(long)]
    pub dry_run: bool,

    #[arg(
        long,
        value_name = "FORMAT",
        default_value = "human",
        requires = "dry_run"
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
pub struct HardlineArgs {
    pub selector: Option<String>,

    #[arg(long)]
    pub inspect: bool,

    #[arg(long, conflicts_with = "inspect")]
    pub new: bool,

    #[arg(long, value_parser = parse_agent, requires = "new")]
    pub agent: Option<jackin_core::Agent>,

    #[arg(long, conflicts_with_all = ["inspect", "new"])]
    pub shell: bool,
}

#[derive(Debug, Args, PartialEq, Eq, Default, Clone)]
#[command(styles = HELP_STYLES)]
pub struct ConsoleArgs {}

#[derive(Debug, Subcommand, PartialEq, Eq)]
pub enum RoleCommand {
    #[command(before_help = BANNER, styles = HELP_STYLES)]
    Validate(RoleRepoPathArgs),

    #[command(before_help = BANNER, styles = HELP_STYLES)]
    Migrate(RoleRepoPathArgs),

    #[command(before_help = BANNER, styles = HELP_STYLES)]
    Create(RoleCreateArgs),

    #[command(before_help = BANNER, styles = HELP_STYLES)]
    ConstructVersion(RoleRepoPathArgs),

    #[command(before_help = BANNER, styles = HELP_STYLES)]
    PublishedImage(RoleRepoPathArgs),

    #[command(before_help = BANNER, styles = HELP_STYLES)]
    PublishedImageRepository(RoleRepoPathArgs),

    #[command(before_help = BANNER, styles = HELP_STYLES)]
    PublishLabels(RolePublishLabelsArgs),
}

#[derive(Debug, Args, PartialEq, Eq)]
pub struct RoleRepoPathArgs {
    #[arg(value_name = "ROLE_REPO_PATH")]
    pub path: Option<PathBuf>,
}

#[derive(Debug, Args, PartialEq, Eq)]
pub struct RolePublishLabelsArgs {
    #[arg(long)]
    pub role_git_sha: String,

    #[arg(value_name = "ROLE_REPO_PATH")]
    pub path: Option<PathBuf>,
}

#[derive(Debug, Args, PartialEq, Eq)]
pub struct RoleCreateArgs {
    pub role: String,

    #[arg(value_name = "PROJECTS_DIR")]
    pub projects_dir: Option<PathBuf>,
}

#[cfg(test)]
mod tests;
