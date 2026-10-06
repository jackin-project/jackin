//! jackin-instance-agents: per-agent credential provisioning.
//!
//! Agent provisioners, auth-source snapshot/capture, slot layout, and the
//! subprocess-telemetry helper used while provisioning. Depends on
//! `jackin-instance-credentials`; orchestrated by `jackin-instance-roles`.

mod account_sources;
mod agent_auth;
mod agent_slots;
mod amp;
mod capture;
mod capture_unixless;
mod claude;
mod codex;
mod github;
mod hermes;
mod ignore;
mod kimi;
mod mounts;
mod omp;
mod opencode;
mod process_telemetry;
mod single_file;
mod single_file_agents;
mod single_file_slots;
mod slots;
mod snapshot;
mod validation;

pub use account_sources::{capture_selected_account_sources, validate_selected_account_sources};
pub use agent_auth::{
    AgentAuthProvision, AgentRuntimeState, DEFAULT_ACCOUNT_ID, InstanceAuthBinding,
    ProvisionedAuth, ProvisionedInstanceAuth, emit_agent_auth_provision,
};
pub use agent_slots::{
    provision_amp_slot, provision_claude_slot, provision_codex_slot, provision_hermes_slot,
    provision_kimi_slot, provision_omp_slot, provision_opencode_slot,
};
pub use ignore::{
    agent_ignore_can_skip_state_prepare, github_ignore_can_skip_state_prepare,
    skipped_ignore_instance_auth,
};
pub use mounts::admit_auth_mounts;
pub use process_telemetry::exec_sync;
pub use single_file_slots::{
    provision_antigravity_slot, provision_cursor_slot, provision_gemini_slot, provision_grok_slot,
    provision_muse_slot,
};
pub use slots::{
    SlotLayout, agent_slot_dirs, slot_home_and_target, slot_home_rel, slot_layout, slot_store_rel,
    slot_suffixes, xdg_cache_rel, xdg_root_agent,
};
pub use snapshot::{
    AuthSourceDescriptor, SelectedAuthSourceSnapshot, SelectedAuthSourceSnapshotInner,
    SelectedSourceDirectory, capture_selected_source, claude_source_missing_error,
    finish_source_snapshot, validate_sync_source_dir, validate_sync_source_dir_for_provider,
    validate_sync_source_dir_for_selection,
};

pub use amp::{provision_amp_auth, provision_amp_auth_from_source_dir};
pub use claude::{provision_claude_auth, provision_claude_auth_from_config_dir};
pub use codex::{provision_codex_auth, provision_codex_auth_from_source_dir};
pub use github::provision_github_auth;
pub use hermes::{provision_hermes_auth, provision_hermes_auth_from_source_dir};
pub use kimi::{provision_kimi_auth, provision_kimi_auth_from_source_dir};
pub use omp::{provision_omp_auth, provision_omp_auth_from_source_dir};
pub use opencode::{provision_opencode_auth, provision_opencode_auth_from_source_dir};
pub use single_file_agents::{
    provision_antigravity_auth, provision_antigravity_auth_from_source_dir, provision_cursor_auth,
    provision_cursor_auth_from_source_dir, provision_gemini_auth,
    provision_gemini_auth_from_source_dir, provision_grok_auth,
    provision_grok_auth_from_source_dir, provision_muse_auth, provision_muse_auth_from_source_dir,
};

#[cfg(unix)]
pub use amp::lock_amp_source_dir;
#[cfg(not(unix))]
pub use amp::{amp_credentials_dir, require_credential_file};
#[cfg(unix)]
pub use capture::capture_locked_source;
pub use capture::create_source_snapshot_dir;
pub use capture_unixless::{
    SnapshotHashBudget, hash_snapshot_tree, snapshot_content_revision, write_snapshot_bytes,
};
#[cfg(not(unix))]
pub use capture_unixless::{
    capture_unixless_single_file_source, capture_unixless_source, copy_unixless_source_tree,
    copy_unixless_source_tree_inner,
};
pub use claude::copy_host_claude_json;
#[cfg(unix)]
pub use claude::locked_claude_credentials;
#[cfg(target_os = "macos")]
pub use claude::read_claude_keychain;
#[cfg(not(unix))]
pub use claude::read_host_credentials_from_claude_config_dir;
pub use github::{HostGhAuth, host_home_is_real, parse_gh_hosts_yml, wipe_file_if_present};
#[cfg(unix)]
pub use kimi::validate_kimi_locked_source;
#[cfg(not(unix))]
pub use kimi::validate_kimi_source_dir_unixless;
#[cfg(not(unix))]
pub use omp::capture_omp_database_snapshot_from_paths;
#[cfg(unix)]
pub use omp::{capture_omp_database_snapshot, validate_omp_source_selection};
pub use omp::{private_file_exists, read_source_bytes, read_source_text};
pub use single_file::{
    provision_single_blob_credential, provision_single_blob_credential_from_content,
    provision_single_file_credential, provision_single_file_credential_with_content,
    wipe_agent_file_state, wipe_kimi_state,
};
#[cfg(unix)]
pub use validation::validate_locked_sync_source_dir;
#[cfg(not(unix))]
pub use validation::validate_opencode_source_dir;
pub use validation::{select_opencode_auth_entry, validate_store_source_dir};
