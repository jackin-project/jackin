// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! Single-file-credential agent provisioners (Grok, Antigravity, Gemini, Cursor, Muse).

use crate::{AuthProvisionOutcome, RoleState};

use jackin_config::AuthForwardMode;

use std::path::{Path, PathBuf};

use crate::auth::provision_single_file_credential;

impl RoleState {
    /// Provision Grok's host-side `~/.grok/auth.json` per the chosen mode.
    ///
    /// The auth.json carries OAuth / OIDC tokens (from `grok login`) and is
    /// the handoff for the browser-based login flow. `GROK_DEPLOYMENT_KEY` or
    /// `XAI_API_KEY` in the env take precedence inside the CLI (per install
    /// script and docs); when present we still allow a Sync mount so any
    /// supplementary config or prior tokens are available, but ApiKey/Ignore
    /// correctly suppress the file to force env-only auth.
    pub(crate) fn provision_grok_auth(
        auth_json: &Path,
        mode: AuthForwardMode,
        host_home: &Path,
    ) -> anyhow::Result<(AuthProvisionOutcome, Option<PathBuf>)> {
        Self::provision_grok_auth_from_path(auth_json, mode, &host_home.join(".grok/auth.json"))
    }

    pub(crate) fn provision_grok_auth_from_source_dir(
        auth_json: &Path,
        mode: AuthForwardMode,
        source_dir: &Path,
    ) -> anyhow::Result<(AuthProvisionOutcome, Option<PathBuf>)> {
        Self::provision_grok_auth_from_path(auth_json, mode, &source_dir.join("auth.json"))
    }

    pub(crate) fn provision_grok_auth_from_path(
        auth_json: &Path,
        mode: AuthForwardMode,
        host_auth_json: &Path,
    ) -> anyhow::Result<(AuthProvisionOutcome, Option<PathBuf>)> {
        provision_single_file_credential(
            auth_json,
            host_auth_json,
            mode,
            "Grok auth.json",
            "Grok",
            true,
            true,
            true,
        )
    }
}

impl RoleState {
    /// Provision Antigravity's host-side `settings.json` per the chosen mode.
    ///
    /// Source: `~/.gemini/antigravity-cli/settings.json`. Prefs only — the
    /// OAuth grant lives in the host Keychain singleton and cannot be
    /// synced, so Sync mode forwards preferences while real auth comes
    /// from `GEMINI_API_KEY` (`ApiKey` mode) or in-container login.
    pub(crate) fn provision_antigravity_auth(
        settings_json: &Path,
        mode: AuthForwardMode,
        host_home: &Path,
    ) -> anyhow::Result<(AuthProvisionOutcome, Option<PathBuf>)> {
        Self::provision_antigravity_auth_from_path(
            settings_json,
            mode,
            &host_home.join(".gemini/antigravity-cli/settings.json"),
        )
    }

    pub(crate) fn provision_antigravity_auth_from_source_dir(
        settings_json: &Path,
        mode: AuthForwardMode,
        source_dir: &Path,
    ) -> anyhow::Result<(AuthProvisionOutcome, Option<PathBuf>)> {
        Self::provision_antigravity_auth_from_path(
            settings_json,
            mode,
            &source_dir.join("settings.json"),
        )
    }

    pub(crate) fn provision_antigravity_auth_from_path(
        settings_json: &Path,
        mode: AuthForwardMode,
        host_settings_json: &Path,
    ) -> anyhow::Result<(AuthProvisionOutcome, Option<PathBuf>)> {
        provision_single_file_credential(
            settings_json,
            host_settings_json,
            mode,
            "Antigravity settings.json",
            "Antigravity",
            true,
            true,
            true,
        )
    }
}

impl RoleState {
    /// Provision Gemini CLI's host-side `~/.gemini/oauth_creds.json` per the
    /// chosen mode. Follows the same semantics as `provision_grok_auth`.
    pub(crate) fn provision_gemini_auth(
        oauth_creds: &Path,
        mode: AuthForwardMode,
        host_home: &Path,
    ) -> anyhow::Result<(AuthProvisionOutcome, Option<PathBuf>)> {
        Self::provision_gemini_auth_from_path(
            oauth_creds,
            mode,
            &host_home.join(".gemini/oauth_creds.json"),
        )
    }

    pub(crate) fn provision_gemini_auth_from_source_dir(
        oauth_creds: &Path,
        mode: AuthForwardMode,
        source_dir: &Path,
    ) -> anyhow::Result<(AuthProvisionOutcome, Option<PathBuf>)> {
        Self::provision_gemini_auth_from_path(
            oauth_creds,
            mode,
            &source_dir.join("oauth_creds.json"),
        )
    }

    pub(crate) fn provision_gemini_auth_from_path(
        oauth_creds: &Path,
        mode: AuthForwardMode,
        host_oauth_creds: &Path,
    ) -> anyhow::Result<(AuthProvisionOutcome, Option<PathBuf>)> {
        provision_single_file_credential(
            oauth_creds,
            host_oauth_creds,
            mode,
            "Gemini oauth_creds.json",
            "Gemini",
            true,
            true,
            true,
        )
    }
}

impl RoleState {
    /// Provision Cursor's host-side `~/.cursor/auth.json` per the chosen mode.
    /// Follows the same semantics as `provision_grok_auth`.
    pub(crate) fn provision_cursor_auth(
        auth_json: &Path,
        mode: AuthForwardMode,
        host_home: &Path,
    ) -> anyhow::Result<(AuthProvisionOutcome, Option<PathBuf>)> {
        Self::provision_cursor_auth_from_path(auth_json, mode, &host_home.join(".cursor/auth.json"))
    }

    pub(crate) fn provision_cursor_auth_from_source_dir(
        auth_json: &Path,
        mode: AuthForwardMode,
        source_dir: &Path,
    ) -> anyhow::Result<(AuthProvisionOutcome, Option<PathBuf>)> {
        Self::provision_cursor_auth_from_path(auth_json, mode, &source_dir.join("auth.json"))
    }

    pub(crate) fn provision_cursor_auth_from_path(
        auth_json: &Path,
        mode: AuthForwardMode,
        host_auth_json: &Path,
    ) -> anyhow::Result<(AuthProvisionOutcome, Option<PathBuf>)> {
        provision_single_file_credential(
            auth_json,
            host_auth_json,
            mode,
            "Cursor auth.json",
            "Cursor",
            true,
            true,
            true,
        )
    }
}

impl RoleState {
    /// Provision Muse's host-side `~/.config/muse/auth.json` per the chosen
    /// mode. Follows the same semantics as `provision_grok_auth`. The file
    /// carries identity fields; the secret itself stays in the host
    /// Keychain, so a synced file alone may still require in-container
    /// login or `META_API_KEY`.
    pub(crate) fn provision_muse_auth(
        auth_json: &Path,
        mode: AuthForwardMode,
        host_home: &Path,
    ) -> anyhow::Result<(AuthProvisionOutcome, Option<PathBuf>)> {
        Self::provision_muse_auth_from_path(
            auth_json,
            mode,
            &host_home.join(".config/muse/auth.json"),
        )
    }

    pub(crate) fn provision_muse_auth_from_source_dir(
        auth_json: &Path,
        mode: AuthForwardMode,
        source_dir: &Path,
    ) -> anyhow::Result<(AuthProvisionOutcome, Option<PathBuf>)> {
        Self::provision_muse_auth_from_path(auth_json, mode, &source_dir.join("auth.json"))
    }

    pub(crate) fn provision_muse_auth_from_path(
        auth_json: &Path,
        mode: AuthForwardMode,
        host_auth_json: &Path,
    ) -> anyhow::Result<(AuthProvisionOutcome, Option<PathBuf>)> {
        provision_single_file_credential(
            auth_json,
            host_auth_json,
            mode,
            "Muse auth.json",
            "Muse",
            true,
            true,
            true,
        )
    }
}
