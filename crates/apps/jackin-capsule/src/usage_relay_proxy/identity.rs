// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! Peer and supervisor identity for the usage relay proxy.

use anyhow::{Context as _, Result};
use jackin_protocol::SessionIdentity;
use tokio::net::UnixStream;

pub(crate) const DEFAULT_CAPSULE_SUPERVISOR_PID: u32 = 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct PeerIdentity {
    pub(crate) pid: Option<u32>,
    pub(crate) start_time: Option<u64>,
    pub(crate) uid: u32,
    pub(crate) gid: u32,
}

/// Supervisor binding pinned at relay startup.
///
/// PID alone is forgeable through PID reuse: if the supervisor exits, another
/// root process can inherit its PID number and pass a `peer.pid ==
/// supervisor_pid` check. The kernel process start time cannot be recycled
/// the same way, so the supervisor peer must match `(pid, start_time)`.
/// A token cannot replace this: any root process can read another process's
/// environment or root-only files, while `SO_PEERCRED` pid + `/proc` start
/// time are kernel-supplied and unforgeable by the peer. Matching fails
/// closed: an unknown start time on either side denies the supervisor path.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct SupervisorIdentity {
    pub(crate) pid: u32,
    pub(crate) start_time: Option<u64>,
}

impl SupervisorIdentity {
    pub(crate) fn matches(self, peer: PeerIdentity) -> bool {
        peer.uid == 0
            && peer.gid == 0
            && peer.pid == Some(self.pid)
            && self.start_time.is_some()
            && peer.start_time == self.start_time
    }
}

/// Kernel process start time (`/proc/<pid>/stat` field 22, clock ticks since
/// boot) used to disambiguate PID reuse. Returns `None` when the start time
/// cannot be verified (missing process, unreadable `/proc`, non-Linux
/// platform); callers must treat `None` as "not the supervisor".
pub(crate) fn process_start_time(pid: u32) -> Option<u64> {
    #[cfg(target_os = "linux")]
    {
        let stat = std::fs::read_to_string(format!("/proc/{pid}/stat")).ok()?;
        parse_proc_stat_start_time(&stat)
    }
    #[cfg(not(target_os = "linux"))]
    {
        let _ = pid;
        None
    }
}

#[cfg(target_os = "linux")]
pub(crate) fn parse_proc_stat_start_time(stat: &str) -> Option<u64> {
    // `comm` (field 2) may contain spaces and ')', so split after its
    // closing paren; remaining fields start at field 3 (state).
    let after_comm = stat.rsplit_once(')')?.1;
    let mut fields = after_comm.split_ascii_whitespace();
    // Field 22 (starttime) is the 20th field after `comm`.
    fields.nth(19)?.parse::<u64>().ok()
}

impl From<SessionIdentity> for PeerIdentity {
    fn from(identity: SessionIdentity) -> Self {
        Self {
            pid: None,
            start_time: None,
            uid: identity.uid,
            gid: identity.gid,
        }
    }
}

pub(crate) fn load_supervisor_identity() -> Result<SupervisorIdentity> {
    let pid = parse_supervisor_pid(std::env::var(jackin_protocol::CAPSULE_SUPERVISOR_PID_ENV))?;
    // Pin (pid, start_time) now: a later root process reusing this PID gets a
    // different start time and fails `matches`. If the start time is
    // unverifiable the binding carries `None` and the supervisor path denies
    // while session peers keep working.
    Ok(SupervisorIdentity {
        pid,
        start_time: process_start_time(pid),
    })
}

pub(crate) fn parse_supervisor_pid(variable: Result<String, std::env::VarError>) -> Result<u32> {
    let supervisor_pid = match variable {
        Ok(value) => value.parse::<u32>().with_context(|| {
            format!(
                "invalid {} value {value:?}",
                jackin_protocol::CAPSULE_SUPERVISOR_PID_ENV
            )
        })?,
        Err(std::env::VarError::NotPresent) => DEFAULT_CAPSULE_SUPERVISOR_PID,
        Err(error) => return Err(error.into()),
    };
    anyhow::ensure!(
        supervisor_pid > 0,
        "Capsule supervisor PID must be positive"
    );
    Ok(supervisor_pid)
}

pub(crate) fn supervisor_peer_allows(
    supervisor: SupervisorIdentity,
    peer: Option<PeerIdentity>,
) -> bool {
    let Some(peer) = peer else {
        return false;
    };
    if peer.uid == 0 || peer.gid == 0 {
        return supervisor.matches(peer);
    }
    true
}

pub(crate) fn peer_identity(stream: &UnixStream) -> Option<PeerIdentity> {
    stream.peer_cred().ok().map(|credentials| {
        let pid = credentials.pid().and_then(|pid| u32::try_from(pid).ok());
        PeerIdentity {
            pid,
            start_time: pid.and_then(process_start_time),
            uid: credentials.uid(),
            gid: credentials.gid(),
        }
    })
}
