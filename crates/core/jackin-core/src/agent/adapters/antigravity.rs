// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use crate::auth::AuthForwardMode;

use crate::agent::runtime::{
    AgentRuntime, AgentStatePaths, FolderVar, FolderVarKind, bounded_fallback_curl,
    looks_like_version, render_fallback_install_block,
};

const FALLBACK_INSTALL_COMMAND: &str =
    bounded_fallback_curl!("https://antigravity.google/cli/install.sh", " | bash");

#[derive(Debug)]
pub(crate) struct AntigravityRuntime;

impl crate::agent::runtime::private::Sealed for AntigravityRuntime {}

impl AgentRuntime for AntigravityRuntime {
    fn slug(&self) -> &'static str {
        "antigravity"
    }

    fn label(&self) -> &'static str {
        "Antigravity"
    }

    fn install_block(&self, source: &str) -> String {
        format!(
            "\
USER agent
COPY --link --chown=agent:0 --chmod=0755 {source} /home/agent/.antigravity/bin/agy
ENV PATH=\"/home/agent/.antigravity/bin:/home/agent/.local/bin:${{PATH}}\"
RUN set -euxo pipefail && \\
    agy --version
"
        )
    }

    fn container_binary_paths(&self) -> &'static [&'static str] {
        &["/home/agent/.antigravity/bin/agy"]
    }

    fn fallback_install_block(&self) -> String {
        render_fallback_install_block(
            "/home/agent/.antigravity/bin:/home/agent/.local/bin",
            FALLBACK_INSTALL_COMMAND,
            "agy",
        )
    }

    fn fallback_install_command(&self) -> &'static str {
        FALLBACK_INSTALL_COMMAND
    }

    fn required_env_var(&self, mode: AuthForwardMode) -> Option<&'static str> {
        match mode {
            AuthForwardMode::ApiKey => Some(crate::env_model::GEMINI_API_KEY_ENV_NAME),
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
            credential_dir: ".gemini/antigravity-cli",
            config_dir: None,
            credential_file: None,

            folder_env_var: Some(FolderVar {
                name: "GEMINI_CLI_HOME",
                kind: FolderVarKind::Parent,
            }),
        }
    }

    fn parse_version<'a>(&self, raw: &'a str) -> Option<&'a str> {
        raw.split_whitespace()
            .find(|token| looks_like_version(token))
    }
}
