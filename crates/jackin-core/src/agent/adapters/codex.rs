//! Codex adapter.

use crate::auth::AuthForwardMode;

use crate::agent::runtime::{
    AgentRuntime, AgentStatePaths, bounded_fallback_curl, looks_like_version,
    render_fallback_install_block,
};

const FALLBACK_INSTALL_COMMAND: &str = bounded_fallback_curl!(
    "https://chatgpt.com/codex/install.sh",
    " | CODEX_NON_INTERACTIVE=1 bash"
);

/// [`crate::agent::runtime::AgentRuntime`] adapter for Codex.
#[derive(Debug)]
pub struct CodexRuntime;

impl crate::agent::runtime::private::Sealed for CodexRuntime {}

impl AgentRuntime for CodexRuntime {
    fn slug(&self) -> &'static str {
        "codex"
    }

    fn label(&self) -> &'static str {
        "Codex"
    }

    fn install_block(&self, source: &str) -> String {
        format!(
            "\
USER agent
COPY --link --chown=agent:0 --chmod=0755 {source} /home/agent/.local/bin/codex
ENV PATH=\"/home/agent/.local/bin:${{PATH}}\"
RUN set -euxo pipefail && \\
    codex --version
"
        )
    }

    fn container_binary_paths(&self) -> &'static [&'static str] {
        &["/home/agent/.local/bin/codex"]
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
            AuthForwardMode::ApiKey => Some(crate::env_model::OPENAI_API_KEY_ENV_NAME),
            AuthForwardMode::Sync | AuthForwardMode::Ignore | AuthForwardMode::OAuthToken => None,
        }
    }

    fn supported_modes(&self) -> &'static [AuthForwardMode] {
        &[
            AuthForwardMode::Sync,
            AuthForwardMode::ApiKey,
            AuthForwardMode::Ignore,
        ]
    }

    fn state_paths(&self) -> AgentStatePaths {
        AgentStatePaths {
            credential_dir: ".codex",
            config_dir: None, // all durable state under ~/.codex
            credential_file: Some(".codex/auth.json"),
            folder_env_var: Some("CODEX_HOME"),
            home_files: &[],
        }
    }

    fn default_home_exclude_paths(&self) -> &'static [&'static str] {
        &[".codex/tmp"]
    }

    fn parse_version<'a>(&self, raw: &'a str) -> Option<&'a str> {
        raw.split_whitespace()
            .find(|token| looks_like_version(token))
    }
}
