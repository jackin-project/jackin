// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use clap::{Args, Subcommand};
use jackin_core::Agent;
use std::path::PathBuf;

use super::{BANNER, HELP_STYLES};

#[derive(Debug, Subcommand, PartialEq, Eq)]
pub enum AccountCommand {
    #[command(before_help = BANNER, styles = HELP_STYLES)]
    List,

    #[command(before_help = BANNER, styles = HELP_STYLES)]
    Scan,

    #[command(before_help = BANNER, styles = HELP_STYLES, after_long_help = "Examples:
  jackin account add claude-work --agent claude --directory ~/.claude-work
  jackin account add openai-work --provider openai --api-key
  jackin account add openai-ci --provider openai --api-key --secret-ref '$OPENAI_WORK_KEY'
  jackin account add kimi-work --provider moonshot --api-key --model PROVIDER_MODEL_ID
  jackin workspace account assign my-app claude-work
  jackin workspace account select my-app claude-work --agent claude")]
    Add(AddAccountArgs),

    #[command(before_help = BANNER, styles = HELP_STYLES)]
    Remove { id: String },

    #[command(before_help = BANNER, styles = HELP_STYLES)]
    Enable { id: String },

    #[command(before_help = BANNER, styles = HELP_STYLES)]
    Disable { id: String },

    #[command(before_help = BANNER, styles = HELP_STYLES)]
    Default {
        id: String,
        #[arg(long, value_parser = parse_agent)]
        agent: Agent,
    },
}

#[derive(Debug, Args, PartialEq, Eq)]
#[command(group(clap::ArgGroup::new("source").required(true).args(["directory", "api_key", "oauth_token"])))]
pub struct AddAccountArgs {
    pub id: String,

    #[arg(long)]
    pub name: Option<String>,

    #[arg(long)]
    pub provider: Option<String>,

    #[arg(long, value_parser = parse_agent)]
    pub agent: Option<Agent>,

    #[arg(long, requires = "agent", conflicts_with_all = ["stdin", "secret_ref", "base_url", "model"])]
    pub directory: Option<PathBuf>,

    #[arg(long, requires = "provider", conflicts_with = "agent")]
    pub api_key: bool,

    #[arg(long, requires = "agent", conflicts_with_all = ["base_url", "model"])]
    pub oauth_token: bool,

    #[arg(long, conflicts_with = "secret_ref")]
    pub stdin: bool,

    #[arg(long)]
    pub secret_ref: Option<String>,

    #[arg(long)]
    pub base_url: Option<String>,

    #[arg(long)]
    pub model: Option<String>,
}

#[derive(Debug, Subcommand, PartialEq, Eq)]
pub enum WorkspaceAccountCommand {
    #[command(before_help = BANNER, styles = HELP_STYLES)]
    List { workspace: String },

    #[command(before_help = BANNER, styles = HELP_STYLES)]
    Assign { workspace: String, account: String },

    #[command(before_help = BANNER, styles = HELP_STYLES)]
    Unassign { workspace: String, account: String },

    #[command(before_help = BANNER, styles = HELP_STYLES)]
    Select {
        workspace: String,

        #[arg(required_unless_present = "clear", conflicts_with = "clear")]
        account: Option<String>,
        #[arg(long, value_parser = parse_agent)]
        agent: Agent,
        #[arg(long)]
        role: Option<String>,
        #[arg(long)]
        clear: bool,
    },
}

fn parse_agent(s: &str) -> Result<Agent, String> {
    s.parse()
        .map_err(|error: jackin_core::ParseAgentError| error.to_string())
}

#[cfg(test)]
mod tests;
