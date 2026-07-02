//! Claude Code adapter.

use crate::auth::AuthForwardMode;
use crate::constants::CLAUDE_OAUTH_TOKEN_ENV;
use crate::env_model;

use crate::agent::runtime::{
    AgentRuntime, AgentStatePaths, bounded_fallback_curl, looks_like_version,
    render_fallback_install_block,
};

const FALLBACK_INSTALL_COMMAND: &str =
    bounded_fallback_curl!("https://claude.ai/install.sh", " | bash");

#[derive(Debug)]
pub struct ClaudeRuntime;

impl crate::agent::runtime::private::Sealed for ClaudeRuntime {}

impl AgentRuntime for ClaudeRuntime {
    fn slug(&self) -> &'static str {
        "claude"
    }

    fn label(&self) -> &'static str {
        "Claude"
    }

    fn install_block(&self, source: &str) -> String {
        format!(
            "\
USER agent
ARG JACKIN_CACHE_BUST=0
ENV XDG_CACHE_HOME=\"/home/agent/.cache\"
COPY --link --chown=agent:0 --chmod=0755 {source} /jackin/agent-binaries/claude
RUN --mount=type=cache,id=jackin-agent-prefetch-claude,target=/home/agent/.cache,uid=1000,gid=1000,sharing=locked \\
    set -euxo pipefail && \\
    : \"${{JACKIN_CACHE_BUST}}\" && \\
    /jackin/agent-binaries/claude install && \\
    claude --version
"
        )
    }

    fn container_binary_paths(&self) -> &'static [&'static str] {
        &["/home/agent/.local/bin/claude"]
    }

    fn fallback_install_block(&self) -> String {
        render_fallback_install_block(
            "/home/agent/.local/bin",
            FALLBACK_INSTALL_COMMAND,
            self.slug(),
        )
    }

    fn fallback_install_command(&self) -> &'static str {
        FALLBACK_INSTALL_COMMAND
    }

    fn required_env_var(&self, mode: AuthForwardMode) -> Option<&'static str> {
        match mode {
            AuthForwardMode::ApiKey => Some(env_model::ANTHROPIC_API_KEY_ENV_NAME),
            AuthForwardMode::OAuthToken => Some(CLAUDE_OAUTH_TOKEN_ENV),
            AuthForwardMode::Sync | AuthForwardMode::Ignore => None,
        }
    }

    fn supported_modes(&self) -> &'static [AuthForwardMode] {
        &[
            AuthForwardMode::Sync,
            AuthForwardMode::ApiKey,
            AuthForwardMode::OAuthToken,
            AuthForwardMode::Ignore,
        ]
    }

    fn state_paths(&self) -> AgentStatePaths {
        AgentStatePaths {
            // Claude stores credentials in ~/.claude/ (directory) + ~/.claude.json.
            credential_dir: ".claude",
            config_dir: None,      // all durable state under ~/.claude
            credential_file: None, // directory-based: .credentials.json + ~/.claude.json
            folder_env_var: Some("CLAUDE_CONFIG_DIR"),
            home_files: &[".claude.json"],
        }
    }

    fn default_home_exclude_paths(&self) -> &'static [&'static str] {
        &[".claude/backups"]
    }

    fn parse_version<'a>(&self, raw: &'a str) -> Option<&'a str> {
        // `claude --version` returns e.g. "2.1.96 (Claude Code)"; take the first token.
        let token = raw.split_whitespace().next()?;
        if !looks_like_version(token) {
            return None;
        }
        Some(token)
    }
}
