// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Kimi Code adapter.

use crate::auth::AuthForwardMode;

use crate::agent::runtime::{
    AgentRuntime, AgentStatePaths, FolderVar, FolderVarKind, bounded_fallback_curl,
    looks_like_version, render_fallback_install_block,
};

const FALLBACK_INSTALL_COMMAND: &str =
    bounded_fallback_curl!("https://code.kimi.com/kimi-code/install.sh", " | bash");

/// [`crate::agent::runtime::AgentRuntime`] adapter for Kimi Code.
#[derive(Debug)]
pub(crate) struct KimiRuntime;

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
ARG JACKIN_EXPECTED_KIMI_VERSION
COPY --link --chown=agent:0 --chmod=0755 {source} /home/agent/.kimi-code/bin/kimi
ENV PATH=\"/home/agent/.kimi-code/bin:/home/agent/.local/bin:${{PATH}}\"
RUN set -euxo pipefail && \\
    test -n \"$JACKIN_EXPECTED_KIMI_VERSION\" && \\
    test \"$(kimi --version)\" = \"kimi $JACKIN_EXPECTED_KIMI_VERSION\"
ENV JACKIN_KIMI_CLI_VERSION=$JACKIN_EXPECTED_KIMI_VERSION
"
        )
    }

    fn container_binary_paths(&self) -> &'static [&'static str] {
        &["/home/agent/.kimi-code/bin/kimi"]
    }

    fn fallback_install_block(&self) -> String {
        let mut block = render_fallback_install_block(
            "/home/agent/.kimi-code/bin:/home/agent/.local/bin",
            FALLBACK_INSTALL_COMMAND,
            self.slug(),
        );
        block.push_str(
            "ARG JACKIN_EXPECTED_KIMI_VERSION\n\
RUN set -euxo pipefail && \\
    test -n \"$JACKIN_EXPECTED_KIMI_VERSION\" && \\
    test \"$(kimi --version)\" = \"kimi $JACKIN_EXPECTED_KIMI_VERSION\"\n\
ENV JACKIN_KIMI_CLI_VERSION=$JACKIN_EXPECTED_KIMI_VERSION\n",
        );
        block
    }

    fn fallback_install_command(&self) -> &'static str {
        FALLBACK_INSTALL_COMMAND
    }

    fn required_env_var(&self, mode: AuthForwardMode) -> Option<&'static str> {
        match mode {
            AuthForwardMode::ApiKey => Some(crate::env_model::KIMI_API_KEY_ENV_NAME),
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
            config_dir: None,      // all durable state under ~/.kimi-code
            credential_file: None, // directory-based provisioning
            folder_env_var: Some(FolderVar {
                name: "KIMI_CODE_HOME",
                kind: FolderVarKind::Dir,
            }),
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
