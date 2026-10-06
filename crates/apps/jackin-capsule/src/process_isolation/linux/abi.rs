// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! Landlock ABI constants, capability constants, and syscall structs.

use std::path::PathBuf;

pub(crate) const CAP_DAC_OVERRIDE: u32 = 1;
pub(crate) const CAP_VERSION_3: u32 = 0x2008_0522;
pub(crate) const PR_SET_KEEPCAPS: libc::c_int = 8;
pub(crate) const PR_SET_NO_NEW_PRIVS: libc::c_int = 38;
pub(crate) const PR_CAP_AMBIENT: libc::c_int = 47;
pub(crate) const PR_CAP_AMBIENT_RAISE: libc::c_ulong = 2;
pub(crate) const LANDLOCK_CREATE_RULESET_VERSION: libc::c_uint = 1;
pub(crate) const LANDLOCK_RULE_TYPE_PATH_BENEATH: libc::c_uint = 1;

pub(crate) const ACCESS_EXECUTE: u64 = 1 << 0;
pub(crate) const ACCESS_WRITE_FILE: u64 = 1 << 1;
pub(crate) const ACCESS_READ_FILE: u64 = 1 << 2;
pub(crate) const ACCESS_READ_DIR: u64 = 1 << 3;
pub(crate) const ACCESS_REMOVE_DIR: u64 = 1 << 4;
pub(crate) const ACCESS_REMOVE_FILE: u64 = 1 << 5;
pub(crate) const ACCESS_MAKE_CHAR: u64 = 1 << 6;
pub(crate) const ACCESS_MAKE_DIR: u64 = 1 << 7;
pub(crate) const ACCESS_MAKE_REG: u64 = 1 << 8;
pub(crate) const ACCESS_MAKE_SOCK: u64 = 1 << 9;
pub(crate) const ACCESS_MAKE_FIFO: u64 = 1 << 10;
pub(crate) const ACCESS_MAKE_BLOCK: u64 = 1 << 11;
pub(crate) const ACCESS_MAKE_SYM: u64 = 1 << 12;
pub(crate) const ACCESS_REFER: u64 = 1 << 13;
pub(crate) const ACCESS_TRUNCATE: u64 = 1 << 14;
pub(crate) const ACCESS_RESOLVE_UNIX: u64 = 1 << 16;
pub(crate) const LANDLOCK_ABI_RESOLVE_UNIX: libc::c_long = 9;

pub(crate) const TRAVERSE: u64 = ACCESS_EXECUTE;
pub(crate) const READ_FILE_ONLY: u64 = ACCESS_EXECUTE | ACCESS_READ_FILE;
pub(crate) const READ_ONLY: u64 = READ_FILE_ONLY | ACCESS_READ_DIR;
// `std::process::Stdio::null()` opens the null device read/write even
// when it is used for stdin. Keep the device tree read-only and grant
// only this exact character device the file I/O needed by that primitive.
pub(crate) const NULL_DEVICE: u64 = READ_FILE_ONLY | ACCESS_WRITE_FILE;
pub(crate) const WRITABLE: u64 = ACCESS_WRITE_FILE
    | ACCESS_REMOVE_DIR
    | ACCESS_REMOVE_FILE
    | ACCESS_MAKE_CHAR
    | ACCESS_MAKE_DIR
    | ACCESS_MAKE_REG
    | ACCESS_MAKE_SOCK
    | ACCESS_MAKE_FIFO
    | ACCESS_MAKE_BLOCK
    | ACCESS_MAKE_SYM
    | ACCESS_REFER
    | ACCESS_TRUNCATE;
pub(crate) const FULL: u64 = READ_ONLY | WRITABLE;
pub(crate) const FULL_WITH_UNIX: u64 = FULL | ACCESS_RESOLVE_UNIX;
pub(crate) const READ_ONLY_WITH_UNIX: u64 = READ_ONLY | ACCESS_RESOLVE_UNIX;

// Pass only the ABI-1 prefix to create_ruleset. ABI 3 accepts this
// prefix, and this boundary does not use the later network/scoped fields;
// passing the full newer struct would make ABI-3 support depend on the
// kernel accepting fields introduced after the advertised ABI.
#[repr(C)]
pub(crate) struct RulesetAttr {
    pub(crate) handled_access_fs: u64,
}

#[repr(C, packed)]
pub(crate) struct PathBeneathAttr {
    pub(crate) allowed_access: u64,
    pub(crate) parent_fd: libc::c_int,
}

#[repr(C)]
pub(crate) struct CapUserHeader {
    pub(crate) version: u32,
    pub(crate) pid: libc::pid_t,
}

#[repr(C)]
pub(crate) struct CapUserData {
    pub(crate) effective: u32,
    pub(crate) permitted: u32,
    pub(crate) inheritable: u32,
}

#[derive(Debug, Clone)]
pub(crate) struct Rule {
    pub(crate) path: PathBuf,
    pub(crate) access: u64,
    pub(crate) required: bool,
}

pub(crate) fn access_for_abi(access: u64, abi: libc::c_long) -> u64 {
    if abi >= LANDLOCK_ABI_RESOLVE_UNIX {
        access
    } else {
        access & !ACCESS_RESOLVE_UNIX
    }
}

// Keep production constants/functions private. The Linux-only unit tests
// live beside, rather than inside, this implementation module, so expose
// a test-only view instead of widening the production API.
pub(crate) mod support {
    #[cfg(test)]
    pub(crate) const ACCESS_RESOLVE_UNIX: u64 = super::ACCESS_RESOLVE_UNIX;
    #[cfg(test)]
    pub(crate) const FULL_WITH_UNIX: u64 = super::FULL_WITH_UNIX;
    #[cfg(test)]
    pub(crate) const NULL_DEVICE: u64 = super::NULL_DEVICE;
    #[cfg(test)]
    pub(crate) const READ_ONLY: u64 = super::READ_ONLY;
    #[cfg(test)]
    pub(crate) const READ_ONLY_WITH_UNIX: u64 = super::READ_ONLY_WITH_UNIX;
    #[cfg(test)]
    pub(crate) const WRITABLE: u64 = super::WRITABLE;

    #[cfg(test)]
    pub(crate) fn access_for_abi(access: u64, abi: libc::c_long) -> u64 {
        super::access_for_abi(access, abi)
    }
}
