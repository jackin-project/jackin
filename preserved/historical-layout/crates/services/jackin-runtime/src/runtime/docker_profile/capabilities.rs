// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! Valid/minimum Linux capability name tables.

/// All recognized Linux capability names (without the `CAP_` prefix,
/// uppercase). Used to validate `capabilities_add` entries at launch time.
pub const VALID_CAPABILITIES: &[&str] = &[
    "AUDIT_CONTROL",
    "AUDIT_READ",
    "AUDIT_WRITE",
    "BPF",
    "BLOCK_SUSPEND",
    "CHECKPOINT_RESTORE",
    "CHOWN",
    "DAC_OVERRIDE",
    "DAC_READ_SEARCH",
    "FOWNER",
    "FSETID",
    "IPC_LOCK",
    "IPC_OWNER",
    "KILL",
    "LEASE",
    "LINUX_IMMUTABLE",
    "MAC_ADMIN",
    "MAC_OVERRIDE",
    "MKNOD",
    "NET_ADMIN",
    "NET_BIND_SERVICE",
    "NET_BROADCAST",
    "NET_RAW",
    "PERFMON",
    "SETFCAP",
    "SETGID",
    "SETPCAP",
    "SETUID",
    "SYS_ADMIN",
    "SYS_BOOT",
    "SYS_CHROOT",
    "SYS_MODULE",
    "SYS_NICE",
    "SYS_PACCT",
    "SYS_PTRACE",
    "SYS_RAWIO",
    "SYS_RESOURCE",
    "SYS_TIME",
    "SYS_TTY_CONFIG",
    "WAKE_ALARM",
];

/// The 8-cap minimum set applied under `hardened` and `locked`.
///
/// Applied regardless of `DinD` status (with `DinD` active the caps can be
/// circumvented via `docker run --privileged` against the sidecar, but they are
/// still emitted for defense in depth). Derived from common role workflows
/// (package managers, build tools, process supervisors). Everything else is
/// dropped from Docker's 14-cap default.
pub const MINIMUM_CAPABILITIES: &[&str] = &[
    "CHOWN",
    "DAC_OVERRIDE",
    "FOWNER",
    "FSETID",
    "SETUID",
    "SETGID",
    "SETFCAP",
    "KILL",
];
