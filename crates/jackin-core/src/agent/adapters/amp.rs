//! Amp adapter.

use crate::auth::AuthForwardMode;

use crate::agent::runtime::{
    AgentRuntime, AgentStatePaths, looks_like_version, render_fallback_install_block,
};

const FALLBACK_INSTALL_COMMAND: &str = "curl -fsSL https://ampcode.com/install.sh | bash";

#[derive(Debug)]
pub struct AmpRuntime;

impl crate::agent::runtime::private::Sealed for AmpRuntime {}

impl AgentRuntime for AmpRuntime {
    fn slug(&self) -> &'static str {
        "amp"
    }

    fn label(&self) -> &'static str {
        "Amp"
    }

    fn install_block(&self, source: &str) -> String {
        format!(
            "\
USER agent
COPY --link --chown=agent:agent --chmod=0755 {source} /home/agent/.amp/bin/amp
ENV PATH=\"/home/agent/.local/bin:/home/agent/.amp/bin:${{PATH}}\"
RUN set -euxo pipefail && \\
    mkdir -p \"${{HOME}}/.local/bin\" && \\
    ln -sf \"${{HOME}}/.amp/bin/amp\" \"${{HOME}}/.local/bin/amp\"
"
        )
    }

    fn fallback_install_block(&self) -> String {
        render_fallback_install_block(
            "/home/agent/.local/bin:/home/agent/.amp/bin",
            FALLBACK_INSTALL_COMMAND,
            self.slug(),
        )
    }

    fn fallback_install_command(&self) -> &'static str {
        FALLBACK_INSTALL_COMMAND
    }

    fn required_env_var(&self, mode: AuthForwardMode) -> Option<&'static str> {
        match mode {
            AuthForwardMode::ApiKey => Some(crate::env_model::AMP_API_KEY_ENV_NAME),
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
            credential_dir: ".local/share/amp",
            credential_file: Some(".local/share/amp/secrets.json"),
            folder_env_var: Some("XDG_DATA_HOME"),
        }
    }

    fn parse_version<'a>(&self, raw: &'a str) -> Option<&'a str> {
        raw.split_whitespace()
            .find(|token| looks_like_version(token))
    }
}
