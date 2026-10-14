// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin_instance_credentials::AuthProvisionOutcome;

use jackin_config::AuthForwardMode;

use std::path::{Path, PathBuf};

use crate::provision_single_file_credential;

pub fn provision_codex_auth(
    auth_json: &Path,
    mode: AuthForwardMode,
    host_home: &Path,
) -> anyhow::Result<(AuthProvisionOutcome, Option<PathBuf>)> {
    provision_codex_auth_from_path(auth_json, mode, &host_home.join(".codex/auth.json"))
}

pub fn provision_codex_auth_from_source_dir(
    auth_json: &Path,
    mode: AuthForwardMode,
    source_dir: &Path,
) -> anyhow::Result<(AuthProvisionOutcome, Option<PathBuf>)> {
    provision_codex_auth_from_path(auth_json, mode, &source_dir.join("auth.json"))
}

fn provision_codex_auth_from_path(
    auth_json: &Path,
    mode: AuthForwardMode,
    host_auth_json: &Path,
) -> anyhow::Result<(AuthProvisionOutcome, Option<PathBuf>)> {
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
