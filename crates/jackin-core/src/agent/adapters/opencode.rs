//! `OpenCode` adapter.

use crate::auth::AuthForwardMode;

use crate::agent::runtime::{AgentRuntime, AgentStatePaths};

pub struct OpencodeRuntime;

impl crate::agent::runtime::private::Sealed for OpencodeRuntime {}

impl AgentRuntime for OpencodeRuntime {
    fn slug(&self) -> &'static str {
        "opencode"
    }

    fn label(&self) -> &'static str {
        "OpenCode"
    }

    fn install_block(&self, source: &str) -> String {
        format!(
            "\
USER agent
ARG JACKIN_CACHE_BUST=0
RUN mkdir -p /home/agent/.opencode/bin
COPY --chown=agent:agent {source} /home/agent/.opencode/bin/opencode
ENV PATH=\"/home/agent/.opencode/bin:${{PATH}}\"
RUN set -euxo pipefail && \\
    : \"${{JACKIN_CACHE_BUST}}\" && \\
    chmod 0755 \"${{HOME}}/.opencode/bin/opencode\" && \\
    opencode --version
"
        )
    }

    fn required_env_var(&self, mode: AuthForwardMode) -> Option<&'static str> {
        match mode {
            AuthForwardMode::ApiKey => Some("OPENCODE_API_KEY"),
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
            credential_dir: ".local/share/opencode",
            credential_file: Some(".local/share/opencode/auth.json"),
            folder_env_var: Some("XDG_DATA_HOME"),
        }
    }
}
