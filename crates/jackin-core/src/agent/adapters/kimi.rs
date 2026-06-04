//! Kimi Code adapter.

use crate::auth::AuthForwardMode;

use crate::agent::runtime::{AgentRuntime, AgentStatePaths};

pub struct KimiRuntime;

impl crate::agent::runtime::private::Sealed for KimiRuntime {}

impl AgentRuntime for KimiRuntime {
    fn slug(&self) -> &'static str {
        "kimi"
    }

    fn label(&self) -> &'static str {
        "Kimi"
    }

    fn install_block(&self, source: &str) -> String {
        format!(
            "\
USER agent
ARG JACKIN_CACHE_BUST=0
RUN mkdir -p /home/agent/.kimi-code/bin
COPY --chown=agent:agent {source} /home/agent/.kimi-code/bin/kimi
ENV PATH=\"/home/agent/.kimi-code/bin:/home/agent/.local/bin:${{PATH}}\"
RUN set -euxo pipefail && \\
    : \"${{JACKIN_CACHE_BUST}}\" && \\
    chmod 0755 \"${{HOME}}/.kimi-code/bin/kimi\" && \\
    kimi --version
"
        )
    }

    fn required_env_var(&self, mode: AuthForwardMode) -> Option<&'static str> {
        match mode {
            // Kimi uses KIMI_CODE_API_KEY (renamed from KIMI_API_KEY in #523).
            AuthForwardMode::ApiKey => Some("KIMI_API_KEY"),
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
            // Kimi stores credentials in ~/.kimi-code/ as a directory.
            credential_dir: ".kimi-code",
            credential_file: None, // directory-based provisioning
            folder_env_var: None,  // no standard folder env var
        }
    }
}
