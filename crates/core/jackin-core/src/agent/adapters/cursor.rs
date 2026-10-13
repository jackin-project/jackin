// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Cursor agent CLI (`cursor-agent`, alias `agent`) adapter.

use crate::auth::AuthForwardMode;

use crate::agent::runtime::{
    AgentRuntime, AgentStatePaths, FolderVar, FolderVarKind, bounded_fallback_curl,
    looks_like_version, render_fallback_install_block,
};

// Official installer (verified 2026-09-17: serves `cursor-agent-installer.sh`).
const FALLBACK_INSTALL_COMMAND: &str =
    bounded_fallback_curl!("https://cursor.com/install", " | bash");

/// [`crate::agent::runtime::AgentRuntime`] adapter for Cursor.
#[derive(Debug)]
pub(crate) struct CursorRuntime;

impl crate::agent::runtime::private::Sealed for CursorRuntime {}

impl AgentRuntime for CursorRuntime {
    fn slug(&self) -> &'static str {
        "cursor"
    }

    fn label(&self) -> &'static str {
        "Cursor"
    }

    fn install_block(&self, source: &str) -> String {
        format!(
            "\
USER agent
COPY --link --chown=agent:0 --chmod=0755 {source} /home/agent/.cursor-agent/bin/agent
COPY --link --chown=agent:0 --chmod=0755 {source} /home/agent/.cursor-agent/bin/cursor-agent
ENV PATH=\"/home/agent/.cursor-agent/bin:/home/agent/.local/bin:${{PATH}}\"
RUN set -euxo pipefail && \\
    cursor-agent --version
"
        )
    }

    fn container_binary_paths(&self) -> &'static [&'static str] {
        // Both names: the official installer defaults to `agent` with
        // `cursor-agent` as the alias. NOTE: Grok Build also ships an
        // `agent` symlink, so host resolvers must probe absolute paths
        // plus `--version` output and never resolve the bare `agent` name.
        &[
            "/home/agent/.cursor-agent/bin/agent",
            "/home/agent/.cursor-agent/bin/cursor-agent",
        ]
    }

    fn fallback_install_block(&self) -> String {
        render_fallback_install_block(
            "/home/agent/.cursor-agent/bin:/home/agent/.local/bin",
            FALLBACK_INSTALL_COMMAND,
            // Smoke-check `cursor-agent`, never bare `agent` (Grok collision).
            "cursor-agent",
        )
    }

    fn fallback_install_command(&self) -> &'static str {
        FALLBACK_INSTALL_COMMAND
    }

    fn required_env_var(&self, mode: AuthForwardMode) -> Option<&'static str> {
        match mode {
            AuthForwardMode::ApiKey => Some(crate::env_model::CURSOR_API_KEY_ENV_NAME),
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
            credential_dir: ".cursor",
            config_dir: None, // auth.json + cli-config.json both under ~/.cursor
            credential_file: Some(".cursor/auth.json"),
            folder_env_var: Some(FolderVar {
                name: "CURSOR_CONFIG_DIR",
                kind: FolderVarKind::Dir,
            }),
        }
    }

    fn parse_version<'a>(&self, raw: &'a str) -> Option<&'a str> {
        // `cursor-agent --version` returns e.g. "2026.09.10-fd3934a".
        raw.split_whitespace()
            .find(|token| looks_like_version(token))
    }
}
