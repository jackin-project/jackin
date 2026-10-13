// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! Codex credential provisioning.

use crate::{AuthProvisionOutcome, RoleState};

use jackin_config::AuthForwardMode;

use std::path::{Path, PathBuf};

use crate::auth::provision_single_file_credential;

impl RoleState {
    /// Provision Codex auth state. Runtime policy is passed as CLI
    /// flags by the entrypoint rather than generated into
    /// `~/.codex/config.toml`.
    ///
    /// `auth.json` semantics mirror Claude's `.credentials.json` for
    /// the file-mount surface; in-container `codex login` writes are
    /// only persisted across container removal when a sync mount
    /// already exists at launch (a host file at `~/.codex/auth.json`).
    ///   * **Sync** + host file present → copy with `0600` perms,
    ///     return `Synced`.
    ///   * **Sync** + host file absent → leave any existing role-state
    ///     `auth.json` untouched (it may survive from a prior synced
    ///     run), return `HostMissing`.
    ///   * **`ApiKey`** → wipe the role-state `auth.json` (the agent
    ///     authenticates via `OPENAI_API_KEY`; a forwarded auth.json
    ///     would let it silently fall back to OAuth credentials the
    ///     operator chose to bypass), return `TokenMode`.
    ///   * **`OAuthToken`** → unreachable in production: parser-rejected
    ///     for Codex. Defensive arm returns `TokenMode` without
    ///     touching role-state files.
    ///   * **Ignore** → delete the role-state `auth.json` if present,
    ///     return `Skipped`.
    ///
    /// Returns `(outcome, mounted_auth_json)` where `mounted_auth_json` is
    /// the role-state `auth.json` path when it should be bind-mounted into
    /// the container (file exists post-call), or `None` when the mount must
    /// be skipped (Ignore wiped it / Sync host-missing with no prior file /
    /// Token mode with no prior file). Centralising the decision here means
    /// `RoleState::prepare` does not need to re-stat the file or reason
    /// about which outcome implies which mount state.
    pub(crate) fn provision_codex_auth(
        auth_json: &Path,
        mode: AuthForwardMode,
        host_home: &Path,
    ) -> anyhow::Result<(AuthProvisionOutcome, Option<PathBuf>)> {
        Self::provision_codex_auth_from_path(auth_json, mode, &host_home.join(".codex/auth.json"))
    }

    pub(crate) fn provision_codex_auth_from_source_dir(
        auth_json: &Path,
        mode: AuthForwardMode,
        source_dir: &Path,
    ) -> anyhow::Result<(AuthProvisionOutcome, Option<PathBuf>)> {
        Self::provision_codex_auth_from_path(auth_json, mode, &source_dir.join("auth.json"))
    }

    pub(crate) fn provision_codex_auth_from_path(
        auth_json: &Path,
        mode: AuthForwardMode,
        host_auth_json: &Path,
    ) -> anyhow::Result<(AuthProvisionOutcome, Option<PathBuf>)> {
        // OAuthToken is parser-rejected for Codex (unreachable in production),
        // so no warning is needed. Empty/whitespace auth is not a usable
        // credential and must agree with source-folder validation by staying
        // out of the role-state mount surface.
        provision_single_file_credential(
            auth_json,
            host_auth_json,
            mode,
            "Codex auth.json",
            "Codex",
            true,
            false,
            false,
        )
    }
}
