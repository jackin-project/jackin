// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! Shared provider timeouts, handoff paths, and OAuth constants.

use jackin_core::container_paths;

use std::time::Duration;

pub(crate) const PROVIDER_HTTP_TIMEOUT: Duration = Duration::from_secs(10);
pub(crate) const PROVIDER_CLI_TIMEOUT: Duration = Duration::from_secs(10);
pub(crate) const CODEX_RPC_INIT_TIMEOUT: Duration = Duration::from_secs(8);
pub(crate) const CODEX_RPC_REQUEST_TIMEOUT: Duration = Duration::from_secs(3);
pub(crate) const CODEX_RPC_LAUNCH_COOLDOWN: Duration = Duration::from_mins(30);
pub(crate) const CLAUDE_VERSION_TIMEOUT: Duration = Duration::from_secs(2);
pub(crate) const CLAUDE_CODE_USER_AGENT_FALLBACK: &str = "claude-code/2.1.0";
pub(crate) const GROK_RPC_INIT_TIMEOUT: Duration = Duration::from_secs(8);
pub(crate) const GROK_RPC_REQUEST_TIMEOUT: Duration = Duration::from_secs(12);
pub(crate) const MATERIALIZED_USAGE_ACCOUNTS_PATH: &str = container_paths::USAGE_ACCOUNTS;
pub(crate) const CODEX_HANDOFF_AUTH_PATH: &str = container_paths::CODEX_AUTH;
pub(crate) const AMP_HANDOFF_SECRETS_PATH: &str = container_paths::AMP_SECRETS;
pub(crate) const KIMI_HANDOFF_HOME: &str = container_paths::KIMI_CODE_DIR;
pub(crate) const GROK_HANDOFF_AUTH_PATH: &str = container_paths::GROK_AUTH;
pub(crate) const CLAUDE_HANDOFF_CREDENTIALS_PATH: &str = container_paths::CLAUDE_CREDENTIALS;
pub const USAGE_SNAPSHOT_STORE_PATH: &str = container_paths::USAGE_SNAPSHOT_STORE;

/// `OpenAI` OAuth token endpoint and the Codex CLI's public client id (the same
/// values the CLI uses for its own refresh grant — neither is a secret).
pub(crate) const CODEX_OAUTH_TOKEN_URL: &str = "https://auth.openai.com/oauth/token";
pub(crate) const CODEX_OAUTH_CLIENT_ID: &str = "app_EMoamEEZ73f0CkXaXp7hrann";
