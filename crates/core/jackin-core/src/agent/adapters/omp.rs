// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use crate::auth::AuthForwardMode;

use crate::agent::runtime::{
    AgentRuntime, AgentStatePaths, FolderVar, FolderVarKind, looks_like_version,
    render_fallback_install_block,
};

const FALLBACK_INSTALL_COMMAND: &str = "npm install -g @oh-my-pi/pi-coding-agent@18.2.4";

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
            config_dir: None,

            credential_file: Some(".omp/agent/agent.db"),
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
