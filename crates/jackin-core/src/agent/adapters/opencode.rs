// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! `OpenCode` adapter.

use crate::auth::AuthForwardMode;

use crate::agent::runtime::{
    AgentRuntime, AgentStatePaths, bounded_fallback_curl, looks_like_version,
    render_fallback_install_block,
};

const FALLBACK_INSTALL_COMMAND: &str =
    bounded_fallback_curl!("https://opencode.ai/install", " | bash");

/// [`crate::agent::runtime::AgentRuntime`] adapter for `OpenCode`.
#[derive(Debug)]
pub(crate) struct OpencodeRuntime;

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
COPY --link --chown=agent:0 --chmod=0755 {source} /home/agent/.opencode/bin/opencode
ENV PATH=\"/home/agent/.opencode/bin:${{PATH}}\"
RUN set -euxo pipefail && \\
    opencode --version
"
        )
    }

    fn container_binary_paths(&self) -> &'static [&'static str] {
        &["/home/agent/.opencode/bin/opencode"]
    }

    fn fallback_install_block(&self) -> String {
        render_fallback_install_block(
            "/home/agent/.opencode/bin:/home/agent/.local/bin",
            FALLBACK_INSTALL_COMMAND,
            self.slug(),
        )
    }

    fn fallback_install_command(&self) -> &'static str {
        FALLBACK_INSTALL_COMMAND
    }

    fn required_env_var(&self, mode: AuthForwardMode) -> Option<&'static str> {
        match mode {
            AuthForwardMode::ApiKey => Some(crate::env_model::OPENCODE_API_KEY_ENV_NAME),
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
            config_dir: Some(".config/opencode"),
            credential_file: Some(".local/share/opencode/auth.json"),
            folder_env_var: Some("XDG_DATA_HOME"),
            home_files: &[],
        }
    }

    fn default_home_exclude_paths(&self) -> &'static [&'static str] {
        &[".config/opencode/opencode.json"]
    }

    fn parse_version<'a>(&self, raw: &'a str) -> Option<&'a str> {
        // `opencode --version` returns e.g. "1.14.48" or "v1.14.48".
        let trimmed = raw.trim();
        let token = trimmed.strip_prefix('v').unwrap_or(trimmed);
        if !looks_like_version(token) {
            return None;
        }
        Some(token)
    }
}
