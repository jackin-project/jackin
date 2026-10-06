// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! Discovery scope and credential root matrix.

use super::ForwardedUsageAccount;

use std::path::PathBuf;

/// Discovery boundary: Desktop may scan host config; Capsule sees capabilities only.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UsageDiscoveryScope {
    /// Host-wide Desktop inventory rooted at explicit operator paths.
    HostDesktop {
        /// Directory containing `config.toml` and `workspaces/`.
        config_root: PathBuf,
        /// Operator home used for default and tilde-relative credential roots.
        operator_home: PathBuf,
    },
    /// Container inventory restricted to explicitly forwarded accounts.
    Capsule {
        /// Broker/runtime-issued capabilities available inside this Capsule.
        forwarded_accounts: Vec<ForwardedUsageAccount>,
    },
}

/// Credential-root inventory for docs and debug (no secrets read).
#[must_use]
pub fn host_credential_root_matrix() -> Vec<HostCredentialRootRow> {
    use jackin_core::container_paths;
    vec![
        HostCredentialRootRow {
            surface: "claude",
            host_paths: "~/.claude/.credentials.json, ~/.claude.json, $CLAUDE_CONFIG_DIR",
            env_vars: "ANTHROPIC_API_KEY, ANTHROPIC_AUTH_TOKEN",
            container_handoff: container_paths::CLAUDE_CREDENTIALS,
        },
        HostCredentialRootRow {
            surface: "codex",
            host_paths: "$CODEX_HOME/auth.json, ~/.codex/auth.json",
            env_vars: "",
            container_handoff: container_paths::CODEX_AUTH,
        },
        HostCredentialRootRow {
            surface: "amp",
            host_paths: "Amp home secrets loaders",
            env_vars: "",
            container_handoff: container_paths::AMP_SECRETS,
        },
        HostCredentialRootRow {
            surface: "grok",
            host_paths: "~/.grok (auth + bin)",
            env_vars: "",
            container_handoff: container_paths::GROK_AUTH,
        },
        HostCredentialRootRow {
            surface: "kimi",
            host_paths: "~/.kimi-code, ~/.kimi",
            env_vars: "KIMI_AUTH_TOKEN, KIMI_CODE_API_KEY, kimi_auth_token",
            container_handoff: container_paths::KIMI_CODE_DIR,
        },
        HostCredentialRootRow {
            surface: "opencode",
            host_paths: "$XDG_DATA_HOME/opencode/auth.json or ~/.local/share/opencode/auth.json",
            env_vars: "",
            container_handoff: "",
        },
        HostCredentialRootRow {
            surface: "zai",
            host_paths: "",
            env_vars: "ZAI_API_KEY, ZHIPU_API_KEY, Z_AI_API_KEY",
            container_handoff: "",
        },
        HostCredentialRootRow {
            surface: "minimax",
            host_paths: "",
            env_vars: "MINIMAX_CODING_API_KEY, MINIMAX_API_KEY",
            container_handoff: "",
        },
        HostCredentialRootRow {
            surface: "google",
            host_paths: "~/.gemini/antigravity-cli, ~/.gemini, $GEMINI_CLI_HOME",
            env_vars: "GEMINI_API_KEY, GOOGLE_API_KEY",
            container_handoff: container_paths::GEMINI_AUTH,
        },
        HostCredentialRootRow {
            surface: "cursor",
            host_paths: "~/.cursor, $CURSOR_CONFIG_DIR",
            env_vars: "CURSOR_API_KEY",
            container_handoff: container_paths::CURSOR_AUTH,
        },
        HostCredentialRootRow {
            surface: "meta",
            host_paths: "~/.config/muse",
            env_vars: "META_API_KEY",
            container_handoff: container_paths::MUSE_AUTH,
        },
        HostCredentialRootRow {
            surface: "openrouter",
            host_paths: "",
            env_vars: "OPENROUTER_API_KEY",
            container_handoff: "",
        },
    ]
}

/// One row of the host credential matrix.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HostCredentialRootRow {
    /// Surface id.
    pub surface: &'static str,
    /// Host path roots.
    pub host_paths: &'static str,
    /// Environment variables.
    pub env_vars: &'static str,
    /// Container handoff fallback.
    pub container_handoff: &'static str,
}
