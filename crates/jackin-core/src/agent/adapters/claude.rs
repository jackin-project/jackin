//! Claude Code adapter.

use crate::auth::AuthForwardMode;
use crate::constants::CLAUDE_OAUTH_TOKEN_ENV;

use crate::agent::runtime::{AgentRuntime, AgentStatePaths, looks_like_version};

const FALLBACK_INSTALL_COMMAND: &str = "curl -fsSL https://claude.ai/install.sh | bash";

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
RUN mkdir -p /tmp/jackin-agent-binaries
COPY --chown=agent:agent {source} /tmp/jackin-agent-binaries/claude
RUN set -euxo pipefail && \\
    : \"${{JACKIN_CACHE_BUST}}\" && \\
    chmod 0755 /tmp/jackin-agent-binaries/claude && \\
    /tmp/jackin-agent-binaries/claude install && \\
    claude --version
"
        )
    }

    fn fallback_install_block(&self) -> String {
        format!(
            "\
USER agent
ARG JACKIN_CACHE_BUST=0
ENV PATH=\"/home/agent/.local/bin:${{PATH}}\"
RUN set -euxo pipefail && \\
    : \"${{JACKIN_CACHE_BUST}}\" && \\
    {FALLBACK_INSTALL_COMMAND} && \\
    claude --version
"
        )
    }

    fn fallback_install_command(&self) -> &'static str {
        FALLBACK_INSTALL_COMMAND
    }

    fn required_env_var(&self, mode: AuthForwardMode) -> Option<&'static str> {
        match mode {
            AuthForwardMode::ApiKey => Some("ANTHROPIC_API_KEY"),
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
            credential_file: None, // directory-based: .credentials.json + ~/.claude.json
            folder_env_var: Some("CLAUDE_CONFIG_DIR"),
        }
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
