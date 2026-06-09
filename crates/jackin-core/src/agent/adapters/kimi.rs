//! Kimi Code adapter.

use crate::auth::AuthForwardMode;

use crate::agent::runtime::{AgentRuntime, AgentStatePaths, looks_like_version};

#[derive(Debug)]
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

    fn parse_version<'a>(&self, raw: &'a str) -> Option<&'a str> {
        // `kimi --version` returns e.g. "kimi 1.2.3"; try first token, then second.
        let mut tokens = raw.split_whitespace();
        let first = tokens.next()?;
        if looks_like_version(first) {
            return Some(first);
        }
        let second = tokens.next()?;
        if looks_like_version(second) {
            return Some(second);
        }
        None
    }
}
