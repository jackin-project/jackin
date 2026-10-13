// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Nous Hermes multi-provider client (`hermes`) adapter.

use crate::auth::AuthForwardMode;

use crate::agent::runtime::{
    AgentRuntime, AgentStatePaths, FolderVar, FolderVarKind, bounded_fallback_curl,
    looks_like_version, render_fallback_install_block,
};

// Official installer (verified 2026-09-17: script header documents this
// exact curl-pipe-bash invocation; uses uv, falls back to venv+pip).
// NOTE: the Hermes TUI needs Node >= 20 plus the Python agent runtime
// and a PTY (`HERMES_TUI=1` or `--tui`); `HERMES_TUI_DIR` selects a
// prebuilt frontend. The base image must provide node + python or the
// smoke check below fails the build loudly.
const FALLBACK_INSTALL_COMMAND: &str = bounded_fallback_curl!(
    "https://hermes-agent.nousresearch.com/install.sh",
    " | bash"
);

/// [`crate::agent::runtime::AgentRuntime`] adapter for Hermes.
#[derive(Debug)]
pub(crate) struct HermesRuntime;

impl crate::agent::runtime::private::Sealed for HermesRuntime {}

impl AgentRuntime for HermesRuntime {
    fn slug(&self) -> &'static str {
        "hermes"
    }

    fn label(&self) -> &'static str {
        "Hermes"
    }

    fn install_block(&self, source: &str) -> String {
        format!(
            "\
USER agent
COPY --link --chown=agent:0 --chmod=0755 {source} /home/agent/.hermes/bin/hermes
ENV PATH=\"/home/agent/.hermes/bin:/home/agent/.local/bin:${{PATH}}\"
RUN set -euxo pipefail && \\
    hermes --version
"
        )
    }

    fn container_binary_paths(&self) -> &'static [&'static str] {
        &["/home/agent/.hermes/bin/hermes"]
    }

    fn fallback_install_block(&self) -> String {
        render_fallback_install_block(
            "/home/agent/.hermes/bin:/home/agent/.local/bin",
            FALLBACK_INSTALL_COMMAND,
            self.slug(),
        )
    }

    fn fallback_install_command(&self) -> &'static str {
        FALLBACK_INSTALL_COMMAND
    }

    fn required_env_var(&self, mode: AuthForwardMode) -> Option<&'static str> {
        // Pure multi-provider client with no native billing: even ApiKey
        // mode has no single agent-level variable; the account layer
        // selects the provider's variable per account.
        let _ = mode;
        None
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
            credential_dir: ".hermes",
            config_dir: None,      // config.yaml + profiles both under ~/.hermes
            credential_file: None, // multi-file profiles dir, not one file
            folder_env_var: Some(FolderVar {
                name: "HERMES_HOME",
                kind: FolderVarKind::Dir,
            }),
        }
    }

    fn parse_version<'a>(&self, raw: &'a str) -> Option<&'a str> {
        raw.split_whitespace()
            .find(|token| looks_like_version(token))
    }
}
