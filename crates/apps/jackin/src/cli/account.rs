// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use clap::{Args, Subcommand};
use jackin_core::Agent;
use std::path::PathBuf;

use super::{BANNER, HELP_STYLES};

#[derive(Debug, Subcommand, PartialEq, Eq)]
pub enum AccountCommand {
    #[command(about = "List registered accounts without credential values", long_about = "List registered accounts without credential values", before_help = BANNER, styles = HELP_STYLES)]
    List,
    #[command(about = "Import authentication from default agent directories and provider key environment variables", long_about = "Import authentication from default agent directories and provider key environment variables", before_help = BANNER, styles = HELP_STYLES)]
    Scan,
    #[command(before_help = BANNER, styles = HELP_STYLES, after_long_help = "Examples:
  jackin account add claude-work --agent claude --directory ~/.claude-work
  jackin account add openai-work --provider openai --api-key
  jackin account add openai-ci --provider openai --api-key --secret-ref '$OPENAI_WORK_KEY'
  jackin account add kimi-work --provider moonshot --api-key --model PROVIDER_MODEL_ID
  jackin workspace account assign my-app claude-work
  jackin workspace account select my-app claude-work --agent claude")]
    #[command(
        about = "Register a profile directory, API key, or Claude OAuth token",
        long_about = "Register a profile directory, API key, or Claude OAuth token"
    )]
    Add(AddAccountArgs),
    #[command(about = "Remove an account and all workspace assignments and bindings", long_about = "Remove an account and all workspace assignments and bindings", before_help = BANNER, styles = HELP_STYLES)]
    Remove { id: String },
    #[command(about = "Allow this account to authenticate launches", long_about = "Allow this account to authenticate launches", before_help = BANNER, styles = HELP_STYLES)]
    Enable { id: String },
    #[command(about = "Prevent this account from authenticating launches", long_about = "Prevent this account from authenticating launches", before_help = BANNER, styles = HELP_STYLES)]
    Disable { id: String },
    #[command(about = "Choose the default account for an agent (workspace access remains explicit)", long_about = "Choose the default account for an agent (workspace access remains explicit)", before_help = BANNER, styles = HELP_STYLES)]
    Default {
        id: String,
        #[arg(long, value_parser = parse_agent)]
        agent: Agent,
    },
}

#[derive(Debug, Args, PartialEq, Eq)]
#[command(group(clap::ArgGroup::new("source").required(true).args(["directory", "api_key", "oauth_token"])))]
pub struct AddAccountArgs {
    #[arg(
        help = "Stable account ID (lowercase letters, digits, and hyphens)",
        long_help = "Stable account ID (lowercase letters, digits, and hyphens)"
    )]
    pub id: String,
    #[arg(
        help = "Human-readable label (defaults to the ID)",
        long_help = "Human-readable label (defaults to the ID)",
        long
    )]
    pub name: Option<String>,
    #[arg(
        help = "AI provider: anthropic, openai, amp, xai, opencode, moonshot, zai, minimax (inferred for native agent profiles)",
        long_help = "AI provider: anthropic, openai, amp, xai, opencode, moonshot, zai, minimax (inferred for native agent profiles)",
        long
    )]
    pub provider: Option<String>,
    #[arg(help = "Coding agent owning the profile or OAuth token", long_help = "Coding agent owning the profile or OAuth token", long, value_parser = parse_agent)]
    pub agent: Option<Agent>,
    #[arg(help = "Existing agent authentication directory, such as ~/.claude-work", long_help = "Existing agent authentication directory, such as ~/.claude-work", long, requires = "agent", conflicts_with_all = ["stdin", "secret_ref", "base_url", "model"])]
    pub directory: Option<PathBuf>,
    #[arg(
        help = "Add an API key; masked prompt unless --stdin or --secret-ref is given",
        long_help = "Add an API key; masked prompt unless --stdin or --secret-ref is given",
        long,
        requires = "provider",
        conflicts_with = "agent"
    )]
    pub api_key: bool,
    #[arg(help = "Add a Claude OAuth token; masked prompt unless --stdin or --secret-ref", long_help = "Add a Claude OAuth token; masked prompt unless --stdin or --secret-ref", long, requires = "agent", conflicts_with_all = ["base_url", "model"])]
    pub oauth_token: bool,
    #[arg(
        help = "Read the key/token from standard input",
        long_help = "Read the key/token from standard input",
        long,
        conflicts_with = "secret_ref"
    )]
    pub stdin: bool,
    #[arg(
        help = "Reference a secret using $VAR, ${VAR}, or op://... (never a literal key)",
        long_help = "Reference a secret using $VAR, ${VAR}, or op://... (never a literal key)",
        long
    )]
    pub secret_ref: Option<String>,
    #[arg(
        help = "Provider API base URL",
        long_help = "Provider API base URL",
        long
    )]
    pub base_url: Option<String>,
    #[arg(
        help = "Provider model ID (required for cross-provider Claude/Codex accounts)",
        long_help = "Provider model ID (required for cross-provider Claude/Codex accounts)",
        long
    )]
    pub model: Option<String>,
}

#[derive(Debug, Subcommand, PartialEq, Eq)]
pub enum WorkspaceAccountCommand {
    #[command(about = "List this workspace's assigned accounts and agent bindings", long_about = "List this workspace's assigned accounts and agent bindings", before_help = BANNER, styles = HELP_STYLES)]
    List { workspace: String },
    #[command(about = "Allow this workspace to use an account", long_about = "Allow this workspace to use an account", before_help = BANNER, styles = HELP_STYLES)]
    Assign { workspace: String, account: String },
    #[command(about = "Revoke an account and clear bindings using it in this workspace", long_about = "Revoke an account and clear bindings using it in this workspace", before_help = BANNER, styles = HELP_STYLES)]
    Unassign { workspace: String, account: String },
    #[command(about = "Choose an assigned account for an agent, optionally for one role", long_about = "Choose an assigned account for an agent, optionally for one role", before_help = BANNER, styles = HELP_STYLES)]
    Select {
        workspace: String,
        #[arg(
            help = "Omit with --clear to remove the binding",
            long_help = "Omit with --clear to remove the binding",
            required_unless_present = "clear",
            conflicts_with = "clear"
        )]
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
