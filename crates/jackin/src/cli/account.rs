// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Account command grammar. Secrets never enter command arguments.

use clap::{Args, Subcommand};
use jackin_core::Agent;
use std::path::PathBuf;

use super::{BANNER, HELP_STYLES};

#[derive(Debug, Subcommand, PartialEq, Eq)]
pub enum AccountCommand {
    /// List registered accounts without credential values
    #[command(before_help = BANNER, styles = HELP_STYLES)]
    List,
    /// Import authentication from default agent directories and provider key environment variables
    #[command(before_help = BANNER, styles = HELP_STYLES)]
    Scan,
    /// Register a profile directory, API key, or Claude OAuth token
    #[command(before_help = BANNER, styles = HELP_STYLES, after_long_help = "Examples:
  jackin account add claude-work --agent claude --directory ~/.claude-work
  jackin account add openai-work --provider openai --api-key
  jackin account add openai-ci --provider openai --api-key --secret-ref '$OPENAI_WORK_KEY'
  jackin account add kimi-work --provider moonshot --api-key --model PROVIDER_MODEL_ID
  jackin workspace account assign my-app claude-work
  jackin workspace account select my-app claude-work --agent claude")]
    Add(AddAccountArgs),
    /// Remove an account and all workspace assignments and bindings
    #[command(before_help = BANNER, styles = HELP_STYLES)]
    Remove { id: String },
    /// Allow this account to authenticate launches
    #[command(before_help = BANNER, styles = HELP_STYLES)]
    Enable { id: String },
    /// Prevent this account from authenticating launches
    #[command(before_help = BANNER, styles = HELP_STYLES)]
    Disable { id: String },
    /// Choose the default account for an agent (workspace access remains explicit)
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
    /// Stable account ID (lowercase letters, digits, and hyphens)
    pub id: String,
    /// Human-readable label (defaults to the ID)
    #[arg(long)]
    pub name: Option<String>,
    /// AI provider: anthropic, openai, amp, xai, opencode, moonshot, zai, minimax
    /// (inferred for native agent profiles)
    #[arg(long)]
    pub provider: Option<String>,
    /// Coding agent owning the profile or OAuth token
    #[arg(long, value_parser = parse_agent)]
    pub agent: Option<Agent>,
    /// Existing agent authentication directory, such as ~/.claude-work
    #[arg(long, requires = "agent", conflicts_with_all = ["stdin", "secret_ref", "base_url", "model"])]
    pub directory: Option<PathBuf>,
    /// Add an API key; masked prompt unless --stdin or --secret-ref is given
    #[arg(long, requires = "provider", conflicts_with = "agent")]
    pub api_key: bool,
    /// Add a Claude OAuth token; masked prompt unless --stdin or --secret-ref
    #[arg(long, requires = "agent", conflicts_with_all = ["base_url", "model"])]
    pub oauth_token: bool,
    /// Read the key/token from standard input
    #[arg(long, conflicts_with = "secret_ref")]
    pub stdin: bool,
    /// Reference a secret using $VAR, ${VAR}, or op://... (never a literal key)
    #[arg(long)]
    pub secret_ref: Option<String>,
    /// Provider API base URL
    #[arg(long)]
    pub base_url: Option<String>,
    /// Provider model ID (required for cross-provider Claude/Codex accounts)
    #[arg(long)]
    pub model: Option<String>,
}

#[derive(Debug, Subcommand, PartialEq, Eq)]
pub enum WorkspaceAccountCommand {
    /// List this workspace's assigned accounts and agent bindings
    #[command(before_help = BANNER, styles = HELP_STYLES)]
    List { workspace: String },
    /// Allow this workspace to use an account
    #[command(before_help = BANNER, styles = HELP_STYLES)]
    Assign { workspace: String, account: String },
    /// Revoke an account and clear bindings using it in this workspace
    #[command(before_help = BANNER, styles = HELP_STYLES)]
    Unassign { workspace: String, account: String },
    /// Choose an assigned account for an agent, optionally for one role
    #[command(before_help = BANNER, styles = HELP_STYLES)]
    Select {
        workspace: String,
        /// Omit with --clear to remove the binding
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
