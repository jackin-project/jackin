//! Grok Build adapter.

use crate::auth::AuthForwardMode;

use crate::agent::runtime::{AgentRuntime, AgentStatePaths, looks_like_version};

#[derive(Debug)]
pub struct GrokRuntime;

impl crate::agent::runtime::private::Sealed for GrokRuntime {}

impl AgentRuntime for GrokRuntime {
    fn slug(&self) -> &'static str {
        "grok"
    }

    fn label(&self) -> &'static str {
        "Grok"
    }

    fn install_block(&self, source: &str) -> String {
        format!(
            "\
USER agent
ARG JACKIN_CACHE_BUST=0
RUN mkdir -p /home/agent/.grok/bin /home/agent/.local/bin
COPY --chown=agent:agent {source} /home/agent/.grok/bin/grok
ENV PATH=\"/home/agent/.grok/bin:/home/agent/.local/bin:${{PATH}}\"
RUN set -euxo pipefail && \\
    : \"${{JACKIN_CACHE_BUST}}\" && \\
    chmod 0755 \"${{HOME}}/.grok/bin/grok\" && \\
    ln -sf \"${{HOME}}/.grok/bin/grok\" \"${{HOME}}/.grok/bin/agent\" && \\
    ln -sf \"${{HOME}}/.grok/bin/grok\" \"${{HOME}}/.local/bin/grok\" && \\
    ln -sf \"${{HOME}}/.grok/bin/grok\" \"${{HOME}}/.local/bin/agent\" && \\
    grok --version
"
        )
    }

    fn required_env_var(&self, mode: AuthForwardMode) -> Option<&'static str> {
        match mode {
            AuthForwardMode::ApiKey => Some("XAI_API_KEY"),
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
            credential_dir: ".grok",
            credential_file: Some(".grok/auth.json"),
            folder_env_var: None,
        }
    }

    fn parse_version<'a>(&self, raw: &'a str) -> Option<&'a str> {
        raw.split_whitespace()
            .find(|token| looks_like_version(token))
    }
}
