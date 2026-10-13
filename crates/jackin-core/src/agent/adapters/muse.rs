// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Meta Muse CLI (`muse`) adapter.

use crate::auth::AuthForwardMode;

use crate::agent::runtime::{AgentRuntime, AgentStatePaths, looks_like_version};

// No verified standalone installer exists for Muse (it ships via host
// package managers with no documented container-side URL), so the
// fallback fails closed with an actionable error instead of inventing
// a download URL. Host-side binary prefetch is the only install path.
const FALLBACK_INSTALL_COMMAND: &str = "echo 'muse: no verified standalone installer; install Meta Muse on the host so binary prefetch can find it' >&2; exit 1";

/// [`crate::agent::runtime::AgentRuntime`] adapter for Muse.
#[derive(Debug)]
pub(crate) struct MuseRuntime;

impl crate::agent::runtime::private::Sealed for MuseRuntime {}

impl AgentRuntime for MuseRuntime {
    fn slug(&self) -> &'static str {
        "muse"
    }

    fn label(&self) -> &'static str {
        "Muse"
    }

    fn install_block(&self, source: &str) -> String {
        format!(
            "\
USER agent
COPY --link --chown=agent:0 --chmod=0755 {source} /home/agent/.muse/bin/muse
ENV PATH=\"/home/agent/.muse/bin:/home/agent/.local/bin:${{PATH}}\"
RUN set -euxo pipefail && \\
    muse --version
"
        )
    }

    fn container_binary_paths(&self) -> &'static [&'static str] {
        &["/home/agent/.muse/bin/muse"]
    }

    fn fallback_install_block(&self) -> String {
        // Deliberately not the shared retry-loop block: retrying a
        // fail-closed error would only burn ~15s of sleeps.
        format!(
            "\
USER agent
RUN {FALLBACK_INSTALL_COMMAND}
"
        )
    }

    fn fallback_install_command(&self) -> &'static str {
        FALLBACK_INSTALL_COMMAND
    }

    fn required_env_var(&self, mode: AuthForwardMode) -> Option<&'static str> {
        match mode {
            // Verified in `muse login --help`: META_API_KEY overrides login.
            AuthForwardMode::ApiKey => Some(crate::env_model::META_API_KEY_ENV_NAME),
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
            // XDG config layout (NOT ~/.muse): auth.json with
            // schema_version 2 holds identity fields; the secret itself
            // lives in the Keychain (service `ai.meta.dev.credentials`,
            // account `meta`).
            credential_dir: ".config/muse",
            config_dir: None, // auth.json + settings.json both under .config/muse
            credential_file: Some(".config/muse/auth.json"),
            folder_env_var: None, // no MUSE_HOME observed
        }
    }

    fn parse_version<'a>(&self, raw: &'a str) -> Option<&'a str> {
        // `muse --version` returns e.g. "1.3.0".
        raw.split_whitespace()
            .find(|token| looks_like_version(token))
    }
}
