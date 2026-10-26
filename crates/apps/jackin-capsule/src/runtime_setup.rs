// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin_core::container_paths;
use std::fs;
use std::os::unix::fs::PermissionsExt as _;
use std::path::Path;

mod agent_auth;
mod agents;
mod claude;
mod commands;
mod forward;
mod git;
mod paths;
mod run;
mod seed;

pub use run::{run, run_prepare_commit_msg_hook};

pub(crate) use agent_auth::{AuthMaterialization, AuthMode, run_agent_setup};
pub(crate) use agents::{
    setup_amp, setup_antigravity, setup_codex, setup_cursor, setup_gemini, setup_grok,
    setup_hermes, setup_kimi, setup_muse, setup_omp, setup_opencode,
};
pub(crate) use claude::setup_claude;
pub(crate) use commands::{
    gh_auth_status_ok, run_command, run_optional_command, runtime_setup_output,
};
pub(crate) use forward::{
    ForwardedCredential, apply_forwarded_credential, seed_forwarded_credential,
};

pub(crate) use git::{
    cache_dco_identity_if_needed, coauthor_trailer_for_agent, ensure_git_config_multivalue,
    ensure_message_trailer, git_config_value, git_trailer_hook_ready, read_cached_dco_identity,
    record_recovered_degradation,
};
pub(crate) use paths::{
    AGENT_HOME, GROK_AUTH_PATH, MUSE_AUTH_PATH, amp_secrets_path, antigravity_settings_path,
    claude_account_path, claude_config_dir, claude_credentials_path, codex_auth_path, codex_home,
    cursor_auth_path, cursor_home, forwarded_dir, forwarded_file, gemini_home,
    gemini_oauth_creds_path, hermes_home, omp_agent_db_path, omp_home, opencode_auth_path,
    xdg_data_home,
};

pub(crate) use run::session_state_path;

pub(crate) use seed::{
    copy_dir_contents, copy_file_with_mode, dir_nonempty, remove_file_if_exists,
    seed_agent_home_from_enum,
};

const CAPSULE_RUNTIME_BIN: &str = container_paths::CAPSULE_BIN;

fn nonempty_env(name: &str) -> Option<String> {
    std::env::var(name).ok().filter(|value| !value.is_empty())
}

fn env_is_one(name: &str) -> bool {
    std::env::var(name).as_deref() == Ok("1")
}

fn is_executable(path: impl AsRef<Path>) -> bool {
    fs::metadata(path).is_ok_and(|metadata| metadata.permissions().mode() & 0o111 != 0)
}
