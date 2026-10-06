// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! oh-my-pi multi-provider client (`omp`) adapter.

use crate::auth::AuthForwardMode;

use crate::agent::runtime::{
    AgentRuntime, AgentStatePaths, FolderVar, FolderVarKind, looks_like_version,
    render_fallback_install_block,
};

// Pinned npm release (bin `omp` verified via registry metadata;
// version observed 2026-09-17 — re-pin when the installer lane probes
// the registry). Requires node/npm on image PATH.
const FALLBACK_INSTALL_COMMAND: &str = "npm install -g @oh-my-pi/pi-coding-agent@18.2.4";

/// [`crate::agent::runtime::AgentRuntime`] adapter for omp.
#[derive(Debug)]
pub(crate) struct OmpRuntime;

impl crate::agent::runtime::private::Sealed for OmpRuntime {}

impl AgentRuntime for OmpRuntime {
    fn slug(&self) -> &'static str {
        "omp"
    }

    fn label(&self) -> &'static str {
        "omp"
    }

    fn install_block(&self, source: &str) -> String {
        format!(
            "\
USER agent
COPY --link --chown=agent:0 --chmod=0755 {source} /home/agent/.omp/bin/omp
ENV PATH=\"/home/agent/.omp/bin:/home/agent/.local/bin:${{PATH}}\"
RUN set -euxo pipefail && \\
    omp --version
"
        )
    }

    fn container_binary_paths(&self) -> &'static [&'static str] {
        &["/home/agent/.omp/bin/omp"]
    }

    fn fallback_install_block(&self) -> String {
        render_fallback_install_block(
            "/home/agent/.omp/bin:/home/agent/.local/bin",
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
            credential_dir: ".omp",
            config_dir: None, // agent.db + profiles both under ~/.omp
            // SQLite store (NOT JSON): provisioning copies the file;
            // discovery treats file presence as evidence without parsing.
            credential_file: Some(".omp/agent/agent.db"),
            // OMP_PROFILE selects a named profile within the dir.
            folder_env_var: Some(FolderVar {
                name: "PI_CODING_AGENT_DIR",
                kind: FolderVarKind::Dir,
            }),
        }
    }

    fn parse_version<'a>(&self, raw: &'a str) -> Option<&'a str> {
        raw.split_whitespace()
            .find(|token| looks_like_version(token))
    }
}
