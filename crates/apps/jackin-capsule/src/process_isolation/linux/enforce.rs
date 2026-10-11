// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! Privilege drop and Landlock enforcement.

use super::{
    ACCESS_EXECUTE, ACCESS_READ_FILE, ACCESS_RESOLVE_UNIX, ACCESS_TRUNCATE, ACCESS_WRITE_FILE,
    CAP_DAC_OVERRIDE, CAP_VERSION_3, CapUserData, CapUserHeader, FULL, LANDLOCK_ABI_RESOLVE_UNIX,
    LANDLOCK_CREATE_RULESET_VERSION, LANDLOCK_RULE_TYPE_PATH_BENEATH, PR_CAP_AMBIENT,
    PR_CAP_AMBIENT_RAISE, PR_SET_KEEPCAPS, PR_SET_NO_NEW_PRIVS, PathBeneathAttr, Rule, RulesetAttr,
    access_for_abi,
};
use anyhow::{Context, Result, bail};
use jackin_protocol::SessionIdentity;
use std::ffi::CString;
use std::mem::size_of;

pub(crate) fn install_landlock(rules: &[Rule]) -> Result<()> {
    #[expect(unsafe_code, reason = "audited Landlock isolation-boundary syscall")]
    // SAFETY: Landlock's version query is defined as a null attribute
    // pointer with zero size and a version-query flag.
    let abi = unsafe {
        libc::syscall(
            libc::SYS_landlock_create_ruleset,
            std::ptr::null::<RulesetAttr>(),
            0usize,
            LANDLOCK_CREATE_RULESET_VERSION,
        )
    };
    if abi < 3 {
        bail!("Landlock ABI 3 is required for credential isolation; kernel reported {abi}");
    }
    let handled = RulesetAttr {
        handled_access_fs: FULL
            | if abi >= LANDLOCK_ABI_RESOLVE_UNIX {
                ACCESS_RESOLVE_UNIX
            } else {
                0
            },
    };
    #[expect(unsafe_code, reason = "audited Landlock isolation-boundary syscall")]
    // SAFETY: `handled` is a valid, initialized ruleset attribute and its
    // size matches the ABI structure passed to the kernel.
    let ruleset = unsafe {
        libc::syscall(
            libc::SYS_landlock_create_ruleset,
            &handled,
            size_of::<RulesetAttr>(),
            0usize,
        )
    };
    if ruleset < 0 {
        return Err(std::io::Error::last_os_error())
            .context("create required Landlock credential boundary");
    }
    let ruleset = i32::try_from(ruleset).context("Landlock ruleset fd out of range")?;
    for rule in rules {
        if !rule.path.exists() {
            if rule.required {
                close_fd(ruleset);
                bail!(
                    "required isolated path does not exist: {}",
                    rule.path.display()
                );
            }
            continue;
        }
        let path = CString::new(rule.path.as_os_str().as_encoded_bytes())
            .with_context(|| format!("invalid isolated path {}", rule.path.display()))?;
        #[expect(unsafe_code, reason = "audited Landlock isolation-boundary syscall")]
        // SAFETY: `path` is a NUL-terminated path owned for this call;
        // O_PATH|O_CLOEXEC requests only a kernel path handle.
        let parent = unsafe { libc::open(path.as_ptr(), libc::O_PATH | libc::O_CLOEXEC) };
        if parent < 0 {
            close_fd(ruleset);
            return Err(std::io::Error::last_os_error())
                .with_context(|| format!("open isolated path {}", rule.path.display()));
        }
        let beneath = PathBeneathAttr {
            // File-only rules cannot carry directory creation/removal or
            // READ_DIR rights. Most rules are directories, but selected
            // auth mounts and runtime files are exact regular files.
            allowed_access: if rule.path.is_dir() {
                access_for_abi(rule.access, abi)
            } else {
                access_for_abi(rule.access, abi)
                    & (ACCESS_EXECUTE
                        | ACCESS_WRITE_FILE
                        | ACCESS_READ_FILE
                        | ACCESS_TRUNCATE
                        | ACCESS_RESOLVE_UNIX)
            },
            parent_fd: parent,
        };
        #[expect(unsafe_code, reason = "audited Landlock isolation-boundary syscall")]
        // SAFETY: `beneath` is a valid path-beneath attribute whose fd is
        // open for the duration of the syscall.
        let result = unsafe {
            libc::syscall(
                libc::SYS_landlock_add_rule,
                ruleset,
                LANDLOCK_RULE_TYPE_PATH_BENEATH,
                &beneath,
                0u32,
            )
        };
        close_fd(parent);
        if result < 0 {
            close_fd(ruleset);
            return Err(std::io::Error::last_os_error())
                .with_context(|| format!("install isolated path rule {}", rule.path.display()));
        }
    }
    #[expect(unsafe_code, reason = "audited Landlock isolation-boundary syscall")]
    // SAFETY: `prctl` changes only the calling process's no-new-privs
    // attribute and receives no pointer arguments.
    if unsafe { libc::prctl(PR_SET_NO_NEW_PRIVS, 1, 0, 0, 0) } != 0 {
        close_fd(ruleset);
        return Err(std::io::Error::last_os_error())
            .context("enable no-new-privileges for Landlock");
    }
    #[expect(unsafe_code, reason = "audited Landlock isolation-boundary syscall")]
    // SAFETY: `ruleset` is the valid fd returned by Landlock and remains
    // open until this syscall completes.
    let result = unsafe { libc::syscall(libc::SYS_landlock_restrict_self, ruleset, 0u32) };
    close_fd(ruleset);
    if result < 0 {
        return Err(std::io::Error::last_os_error())
            .context("activate required Landlock credential boundary");
    }
    Ok(())
}

pub(crate) fn drop_privileges(identity: SessionIdentity) -> Result<()> {
    anyhow::ensure!(
        identity.uid > 0 && identity.gid > 0,
        "session identity cannot be root"
    );
    #[expect(unsafe_code, reason = "audited privilege-drop boundary syscall")]
    // SAFETY: `prctl` changes only this process's keep-caps flag and
    // receives no pointer arguments.
    if unsafe { libc::prctl(PR_SET_KEEPCAPS, 1, 0, 0, 0) } != 0 {
        return Err(std::io::Error::last_os_error()).context("retain session DAC capabilities");
    }
    #[expect(unsafe_code, reason = "audited privilege-drop boundary syscall")]
    // SAFETY: a zero count with a null group list is the documented way
    // to clear supplementary groups for this process.
    if unsafe { libc::setgroups(0, std::ptr::null()) } != 0 {
        return Err(std::io::Error::last_os_error())
            .context("clear supervisor supplementary groups");
    }
    #[expect(unsafe_code, reason = "audited privilege-drop boundary syscall")]
    // SAFETY: all three gid values are the validated non-root session gid.
    if unsafe { libc::setresgid(identity.gid, identity.gid, identity.gid) } != 0 {
        return Err(std::io::Error::last_os_error()).context("drop session gid");
    }
    #[expect(unsafe_code, reason = "audited privilege-drop boundary syscall")]
    // SAFETY: all three uid values are the validated non-root session uid.
    if unsafe { libc::setresuid(identity.uid, identity.uid, identity.uid) } != 0 {
        return Err(std::io::Error::last_os_error()).context("drop session uid");
    }

    let mask = retained_capability_mask();
    let header = CapUserHeader {
        version: CAP_VERSION_3,
        pid: 0,
    };
    let mut data = [
        CapUserData {
            effective: mask,
            permitted: mask,
            inheritable: mask,
        },
        CapUserData {
            effective: 0,
            permitted: 0,
            inheritable: 0,
        },
    ];
    #[expect(unsafe_code, reason = "audited privilege-drop boundary syscall")]
    // SAFETY: `header` and the two-element capability data array are
    // initialized to the kernel's documented capset ABI.
    if unsafe { libc::syscall(libc::SYS_capset, &header, data.as_mut_ptr()) } != 0 {
        return Err(std::io::Error::last_os_error()).context("retain session DAC capabilities");
    }
    for capability in [CAP_DAC_OVERRIDE] {
        #[expect(unsafe_code, reason = "audited privilege-drop boundary syscall")]
        // SAFETY: this raises the one capability just installed in the
        // calling process's ambient set.
        if unsafe { libc::prctl(PR_CAP_AMBIENT, PR_CAP_AMBIENT_RAISE, capability, 0, 0) } != 0 {
            return Err(std::io::Error::last_os_error())
                .context("make session DAC boundary capabilities survive exec");
        }
    }
    #[expect(unsafe_code, reason = "audited privilege-drop boundary syscall")]
    // SAFETY: `geteuid` has no pointer arguments and reports this process.
    let effective_uid = unsafe { libc::geteuid() };
    anyhow::ensure!(
        effective_uid == identity.uid,
        "session uid drop did not stick"
    );
    #[expect(unsafe_code, reason = "audited privilege-drop boundary syscall")]
    // SAFETY: `getegid` has no pointer arguments and reports this process.
    let effective_gid = unsafe { libc::getegid() };
    anyhow::ensure!(
        effective_gid == identity.gid,
        "session gid drop did not stick"
    );
    Ok(())
}

/// DAC override is retained only because host bind mounts can be owned by
/// a different numeric UID than the per-session identity. Landlock remains
/// the path boundary. `CAP_FOWNER` is deliberately not retained: no session
/// operation needs to bypass ownership checks for chmod/chown/signal-like
/// ownership actions.
pub(crate) const fn retained_capability_mask() -> u32 {
    1u32 << CAP_DAC_OVERRIDE
}

#[expect(unsafe_code, reason = "audited Landlock isolation-boundary syscall")]
pub(crate) fn close_fd(fd: libc::c_int) {
    // SAFETY: callers pass file descriptors returned by the kernel and no
    // longer use them after this close.
    unsafe {
        libc::close(fd);
    }
}
