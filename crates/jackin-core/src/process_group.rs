// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::time::Duration;

use rustix::io::Errno;
use rustix::process::{Pid, Signal, kill_process_group};

use crate::GroupFault;

#[cfg(target_os = "linux")]
mod stat;
#[cfg(all(test, target_os = "linux"))]
mod tests;

const GROUP_GRACE: Duration = Duration::from_secs(5);
const GROUP_POLL: Duration = Duration::from_millis(2);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Sweep {
    Empty,
    Signalled,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ProcessGroup(Pid);

impl ProcessGroup {
    #[must_use]
    pub fn of_leader(id: u32) -> Option<ProcessGroup> {
        i32::try_from(id)
            .ok()
            .and_then(Pid::from_raw)
            .map(ProcessGroup)
    }

    pub fn strike(self) {
        kill_process_group(self.0, Signal::KILL).unwrap_or_default();
    }

    pub fn sweep(self) -> Result<Sweep, GroupFault> {
        match kill_process_group(self.0, Signal::KILL) {
            Ok(()) => survivors(self.0),
            Err(Errno::SRCH) => Ok(Sweep::Empty),
            Err(Errno::PERM) => denied(self.0),
            Err(errno) => Err(cannot_stop(format!("signalling it failed: {errno}"))),
        }
    }

    pub async fn stop(self) -> Result<(), GroupFault> {
        let emptied = async {
            loop {
                match swept(self).await? {
                    Sweep::Empty => return Ok(()),
                    Sweep::Signalled => tokio::time::sleep(GROUP_POLL).await,
                }
            }
        };
        tokio::time::timeout(GROUP_GRACE, emptied)
            .await
            .unwrap_or(Err(GroupFault::Alive { after: GROUP_GRACE }))
    }
}

async fn swept(group: ProcessGroup) -> Result<Sweep, GroupFault> {
    tokio::task::spawn_blocking(move || group.sweep())
        .await
        .unwrap_or_else(|error| Err(cannot_stop(format!("the sweep did not finish: {error}"))))
}

fn cannot_stop(message: String) -> GroupFault {
    GroupFault::Unreachable { message }
}

#[cfg(not(target_os = "linux"))]
fn survivors(_group: Pid) -> Result<Sweep, GroupFault> {
    Ok(Sweep::Signalled)
}

#[cfg(not(target_os = "linux"))]
fn denied(_group: Pid) -> Result<Sweep, GroupFault> {
    Ok(Sweep::Signalled)
}

#[cfg(target_os = "linux")]
fn survivors(group: Pid) -> Result<Sweep, GroupFault> {
    if members(group)?.is_empty() {
        Ok(Sweep::Empty)
    } else {
        Ok(Sweep::Signalled)
    }
}

#[cfg(target_os = "linux")]
fn denied(group: Pid) -> Result<Sweep, GroupFault> {
    by_member(group)
}

#[cfg(target_os = "linux")]
fn by_member(group: Pid) -> Result<Sweep, GroupFault> {
    let members = members(group)?;
    if members.is_empty() {
        return Ok(Sweep::Empty);
    }
    for member in members {
        match rustix::process::kill_process(member, Signal::KILL) {
            Ok(()) | Err(Errno::SRCH) => {}
            Err(errno) => {
                return Err(cannot_stop(format!(
                    "signalling process {} failed: {errno}",
                    member.as_raw_pid()
                )));
            }
        }
    }
    Ok(Sweep::Signalled)
}

#[cfg(target_os = "linux")]
fn members(group: Pid) -> Result<Vec<Pid>, GroupFault> {
    let listing =
        |error: std::io::Error| cannot_stop(format!("listing its members failed: {error}"));
    let mut found = Vec::new();
    for entry in std::fs::read_dir("/proc").map_err(listing)? {
        let entry = entry.map_err(listing)?;
        let Some(pid) = entry
            .file_name()
            .to_str()
            .and_then(|name| name.parse::<i32>().ok())
            .and_then(Pid::from_raw)
        else {
            continue;
        };
        let stat = match std::fs::read(entry.path().join("stat")) {
            Ok(stat) => stat,
            Err(error) if vanished(&error) => continue,
            Err(error) => {
                return Err(cannot_stop(format!(
                    "reading the state of process {} failed: {error}",
                    pid.as_raw_pid()
                )));
            }
        };
        match stat::is_live_member(&stat, group) {
            Ok(true) => found.push(pid),
            Ok(false) => {}
            Err(stat::Unparsable) => {
                return Err(cannot_stop(format!(
                    "the state of process {} is not a /proc stat line",
                    pid.as_raw_pid()
                )));
            }
        }
    }
    Ok(found)
}

#[cfg(target_os = "linux")]
fn vanished(error: &std::io::Error) -> bool {
    error.kind() == std::io::ErrorKind::NotFound
        || error.raw_os_error() == Some(Errno::SRCH.raw_os_error())
}
