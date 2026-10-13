// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! `tmpfs` paths and readonly-root flags.

use super::{
    DockerSecurityProfile, EffectiveGrants, TMPFS_PATHS_HARDENED_EXTRA, TMPFS_PATHS_MINIMAL,
};

/// Resolved tmpfs path set for a read-only-root profile.
///
/// `locked` uses the minimal set (apt is unsupported); `hardened` adds
/// package-manager paths. Single source of truth for `--tmpfs` flags and the
/// session contract's "writable tmpfs" line so the two never drift.
pub fn tmpfs_paths(profile: DockerSecurityProfile) -> Vec<&'static str> {
    let extra: &[&str] = if matches!(profile, DockerSecurityProfile::Locked) {
        &[]
    } else {
        TMPFS_PATHS_HARDENED_EXTRA
    };
    TMPFS_PATHS_MINIMAL.iter().chain(extra).copied().collect()
}

/// Container env that redirects tools writing under `$HOME` onto a writable
/// location when the profile's root filesystem is read-only. Empty for
/// writable-root profiles.
///
/// The env-redirect arm of the read-only-root `$HOME` story (the in-place
/// writable-path arm is [`tmpfs_paths`]). `git config --global` can't be fixed
/// with a tmpfs/bind on `~/.gitconfig` alone because it writes a `.gitconfig.lock`
/// in the read-only home dir, so it is pointed at the already-writable
/// `/jackin/state` bind mount instead. (The full `$HOME` audit is tracked on the
/// Docker hardening roadmap item.)
pub fn readonly_home_env(grants: &EffectiveGrants) -> Vec<String> {
    if grants.system_writes {
        return Vec::new();
    }
    vec!["GIT_CONFIG_GLOBAL=/jackin/state/gitconfig".to_owned()]
}

/// Emit `--read-only` plus a `--tmpfs <path>:rw,nosuid,nodev` pair for every
/// [`tmpfs_paths`] entry. Empty for writable-root profiles.
///
/// When sudo is granted on a read-only-root profile (`hardened`/`locked` with an
/// explicit `sudo = true`), a narrow tmpfs is also mounted over `/etc/sudoers.d`
/// so the post-run `sudo-provision` step can write `/etc/sudoers.d/agent` — `/etc`
/// is otherwise read-only and the write would EROFS-fail the launch. The mount is
/// `mode=0755` (root-owned), so only the root `docker exec` provision step can
/// write it; an agent-owned file would be ignored by sudo regardless, and the
/// agent already has passwordless root via the grant, so this is no new escalation.
pub fn readonly_root_flags(
    profile: DockerSecurityProfile,
    grants: &EffectiveGrants,
) -> Vec<String> {
    if grants.system_writes {
        return Vec::new();
    }
    let mut flags = vec!["--read-only".to_owned()];
    for path in tmpfs_paths(profile) {
        flags.push("--tmpfs".to_owned());
        flags.push(format!("{path}:rw,nosuid,nodev"));
    }
    if grants.sudo {
        flags.push("--tmpfs".to_owned());
        flags.push("/etc/sudoers.d:rw,nosuid,nodev,mode=0755".to_owned());
    }
    flags
}
