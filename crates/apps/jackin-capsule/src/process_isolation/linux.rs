// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! Linux isolation entry: wrapper orchestration over rules and enforcement.

use super::SessionIdentity;
use anyhow::{Context, Result};
use jackin_protocol::CapsuleConfig;
use std::os::unix::process::CommandExt as _;

mod abi;
mod enforce;
mod paths;
mod rules;

#[cfg(test)]
pub(super) use abi::support;
pub(super) use abi::{
    ACCESS_EXECUTE, ACCESS_READ_FILE, ACCESS_RESOLVE_UNIX, ACCESS_TRUNCATE, ACCESS_WRITE_FILE,
    CAP_DAC_OVERRIDE, CAP_VERSION_3, CapUserData, CapUserHeader, FULL, FULL_WITH_UNIX,
    LANDLOCK_ABI_RESOLVE_UNIX, LANDLOCK_CREATE_RULESET_VERSION, LANDLOCK_RULE_TYPE_PATH_BENEATH,
    NULL_DEVICE, PR_CAP_AMBIENT, PR_CAP_AMBIENT_RAISE, PR_SET_KEEPCAPS, PR_SET_NO_NEW_PRIVS,
    PathBeneathAttr, READ_FILE_ONLY, READ_ONLY, READ_ONLY_WITH_UNIX, Rule, RulesetAttr, TRAVERSE,
    access_for_abi,
};
#[cfg(test)]
pub(super) use enforce::retained_capability_mask;
pub(super) use enforce::{drop_privileges, install_landlock};
#[cfg(test)]
pub(super) use paths::add_execute_only_ancestors;
pub(super) use paths::{
    normalize_existing_path, optional_exact_rule, pane_homes_parent, prepare_pane_homes_parent,
    prepare_session_root, required_exact_rule, session_root_path, validate_cwd_boundary,
    validate_workspace_mount_boundary, validate_worktree_git_target,
};
pub(super) use rules::rules_for;
#[cfg(test)]
pub(super) use rules::rules_for_test;

pub(crate) fn run(
    config: &CapsuleConfig,
    instance: Option<&str>,
    identity: SessionIdentity,
    program: &str,
    args: &[String],
) -> Result<()> {
    #[expect(unsafe_code, reason = "audited privilege-drop boundary syscall")]
    // SAFETY: `geteuid` has no pointer arguments and only returns the
    // effective uid of the calling process.
    let effective_uid = unsafe { libc::geteuid() };
    anyhow::ensure!(
        effective_uid == 0,
        "capsule isolation wrapper must start as root"
    );
    let session_id = std::env::var(jackin_protocol::ISOLATION_SESSION_ID_ENV)
        .context("isolated session wrapper requires JACKIN_ISOLATION_SESSION_ID")?
        .parse::<u64>()
        .context("isolated session wrapper has invalid JACKIN_ISOLATION_SESSION_ID")?;
    anyhow::ensure!(session_id > 0, "isolated session id cannot be zero");
    let session_root = session_root_path(session_id);
    prepare_session_root(&session_root)?;
    prepare_pane_homes_parent(config, instance)?;
    let cwd = std::env::current_dir().context("resolve isolated session cwd")?;
    let rules = rules_for(config, instance, &cwd, &session_root)?;
    drop_privileges(identity)?;
    install_landlock(&rules)?;

    let error = std::process::Command::new(program).args(args).exec();
    Err(error).with_context(|| format!("exec isolated session program {program}"))
}
