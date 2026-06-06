//! Amp adapter.

use crate::auth::AuthForwardMode;

use crate::agent::runtime::{AgentRuntime, AgentStatePaths};

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
ARG JACKIN_CACHE_BUST=0
RUN mkdir -p /home/agent/.amp/bin
COPY --chown=agent:agent {source} /home/agent/.amp/bin/amp
ENV PATH=\"/home/agent/.local/bin:/home/agent/.amp/bin:${{PATH}}\"
RUN set -euxo pipefail && \\
    : \"${{JACKIN_CACHE_BUST}}\" && \\
    chmod 0755 \"${{HOME}}/.amp/bin/amp\" && \\
    mkdir -p \"${{HOME}}/.local/bin\" && \\
    ln -sf \"${{HOME}}/.amp/bin/amp\" \"${{HOME}}/.local/bin/amp\" && \\
    amp --version
"
        )
    }

    fn required_env_var(&self, mode: AuthForwardMode) -> Option<&'static str> {
        match mode {
            AuthForwardMode::ApiKey => Some("AMP_API_KEY"),
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
            folder_env_var: Some("AMP_DATA_HOME"),
        }
    }

    fn parse_version<'a>(&self, raw: &'a str) -> Option<&'a str> {
        raw.split_whitespace().find(|token| {
            token.split('.').count() >= 2 && token.starts_with(|c: char| c.is_ascii_digit())
        })
    }
}
