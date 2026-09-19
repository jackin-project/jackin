// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Google Gemini CLI (`gemini`) adapter.

use crate::auth::AuthForwardMode;

use crate::agent::runtime::{
    AgentRuntime, AgentStatePaths, FolderVar, FolderVarKind, looks_like_version,
    render_fallback_install_block,
};

// Documented upstream install (`npm i -g @google/gemini-cli`; Homebrew
// `gemini-cli` is the macOS equivalent). Requires node/npm on image PATH;
// the smoke check below fails the build loudly when they are absent.
const FALLBACK_INSTALL_COMMAND: &str = "npm install -g @google/gemini-cli";

/// [`crate::agent::runtime::AgentRuntime`] adapter for Gemini CLI.
#[derive(Debug)]
pub(crate) struct GeminiRuntime;

impl crate::agent::runtime::private::Sealed for GeminiRuntime {}

impl AgentRuntime for GeminiRuntime {
    fn slug(&self) -> &'static str {
        "gemini"
    }

    fn label(&self) -> &'static str {
        "Gemini"
    }

    fn install_block(&self, source: &str) -> String {
        format!(
            "\
USER agent
COPY --link --chown=agent:0 --chmod=0755 {source} /home/agent/.gemini-cli/bin/gemini
ENV PATH=\"/home/agent/.gemini-cli/bin:/home/agent/.local/bin:${{PATH}}\"
RUN set -euxo pipefail && \\
    gemini --version
"
        )
    }

    fn container_binary_paths(&self) -> &'static [&'static str] {
        &["/home/agent/.gemini-cli/bin/gemini"]
    }

    fn fallback_install_block(&self) -> String {
        render_fallback_install_block(
            "/home/agent/.gemini-cli/bin:/home/agent/.local/bin",
            FALLBACK_INSTALL_COMMAND,
            self.slug(),
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
            credential_dir: ".gemini",
            config_dir: None, // all durable state under ~/.gemini
            credential_file: Some(".gemini/oauth_creds.json"),
            // NOTE: GEMINI_CLI_HOME names the *parent* to which `.gemini`
            // is appended (it is not the config dir itself).
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
