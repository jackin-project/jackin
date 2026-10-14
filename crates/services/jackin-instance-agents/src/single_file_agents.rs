// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin_instance_credentials::AuthProvisionOutcome;

use jackin_config::AuthForwardMode;

use std::path::{Path, PathBuf};

use crate::provision_single_file_credential;

pub fn provision_grok_auth(
    auth_json: &Path,
    mode: AuthForwardMode,
    host_home: &Path,
) -> anyhow::Result<(AuthProvisionOutcome, Option<PathBuf>)> {
    provision_grok_auth_from_path(auth_json, mode, &host_home.join(".grok/auth.json"))
}

pub fn provision_grok_auth_from_source_dir(
    auth_json: &Path,
    mode: AuthForwardMode,
    source_dir: &Path,
) -> anyhow::Result<(AuthProvisionOutcome, Option<PathBuf>)> {
    provision_grok_auth_from_path(auth_json, mode, &source_dir.join("auth.json"))
}

fn provision_grok_auth_from_path(
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

pub fn provision_antigravity_auth(
    settings_json: &Path,
    mode: AuthForwardMode,
    host_home: &Path,
) -> anyhow::Result<(AuthProvisionOutcome, Option<PathBuf>)> {
    provision_antigravity_auth_from_path(
        settings_json,
        mode,
        &host_home.join(".gemini/antigravity-cli/settings.json"),
    )
}

pub fn provision_antigravity_auth_from_source_dir(
    settings_json: &Path,
    mode: AuthForwardMode,
    source_dir: &Path,
) -> anyhow::Result<(AuthProvisionOutcome, Option<PathBuf>)> {
    provision_antigravity_auth_from_path(settings_json, mode, &source_dir.join("settings.json"))
}

fn provision_antigravity_auth_from_path(
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

pub fn provision_gemini_auth(
    oauth_creds: &Path,
    mode: AuthForwardMode,
    host_home: &Path,
) -> anyhow::Result<(AuthProvisionOutcome, Option<PathBuf>)> {
    provision_gemini_auth_from_path(
        oauth_creds,
        mode,
        &host_home.join(".gemini/oauth_creds.json"),
    )
}

pub fn provision_gemini_auth_from_source_dir(
    oauth_creds: &Path,
    mode: AuthForwardMode,
    source_dir: &Path,
) -> anyhow::Result<(AuthProvisionOutcome, Option<PathBuf>)> {
    provision_gemini_auth_from_path(oauth_creds, mode, &source_dir.join("oauth_creds.json"))
}

fn provision_gemini_auth_from_path(
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

pub fn provision_cursor_auth(
    auth_json: &Path,
    mode: AuthForwardMode,
    host_home: &Path,
) -> anyhow::Result<(AuthProvisionOutcome, Option<PathBuf>)> {
    provision_cursor_auth_from_path(auth_json, mode, &host_home.join(".cursor/auth.json"))
}

pub fn provision_cursor_auth_from_source_dir(
    auth_json: &Path,
    mode: AuthForwardMode,
    source_dir: &Path,
) -> anyhow::Result<(AuthProvisionOutcome, Option<PathBuf>)> {
    provision_cursor_auth_from_path(auth_json, mode, &source_dir.join("auth.json"))
}

fn provision_cursor_auth_from_path(
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

pub fn provision_muse_auth(
    auth_json: &Path,
    mode: AuthForwardMode,
    host_home: &Path,
) -> anyhow::Result<(AuthProvisionOutcome, Option<PathBuf>)> {
    provision_muse_auth_from_path(auth_json, mode, &host_home.join(".config/muse/auth.json"))
}

pub fn provision_muse_auth_from_source_dir(
    auth_json: &Path,
    mode: AuthForwardMode,
    source_dir: &Path,
) -> anyhow::Result<(AuthProvisionOutcome, Option<PathBuf>)> {
    provision_muse_auth_from_path(auth_json, mode, &source_dir.join("auth.json"))
}

fn provision_muse_auth_from_path(
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
