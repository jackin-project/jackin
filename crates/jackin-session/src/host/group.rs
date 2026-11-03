// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin_core::GroupFault;
use rustix::io::Errno;
use rustix::process::{Pid, Signal, kill_process_group};

#[cfg(target_os = "linux")]
mod stat;

pub(super) enum Sweep {
    Empty,
    Signalled,
}

pub(super) fn strike(group: Pid) {
    kill_process_group(group, Signal::KILL).unwrap_or_default();
}

pub(super) fn sweep(group: Pid) -> Result<Sweep, GroupFault> {
    match kill_process_group(group, Signal::KILL) {
        Ok(()) => survivors(group),
        Err(Errno::SRCH) => Ok(Sweep::Empty),
        Err(Errno::PERM) => denied(group),
        Err(errno) => Err(cannot_stop(format!("signalling it failed: {errno}"))),
    }
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
    if live_members(group)?.is_empty() {
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
pub(super) fn by_member(group: Pid) -> Result<Sweep, GroupFault> {
    let members = live_members(group)?;
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
fn live_members(group: Pid) -> Result<Vec<Pid>, GroupFault> {
    members(group).map_err(|error| cannot_stop(format!("listing its members failed: {error}")))
}

#[cfg(target_os = "linux")]
fn members(group: Pid) -> std::io::Result<Vec<Pid>> {
    let mut found = Vec::new();
    for entry in std::fs::read_dir("/proc")? {
        let entry = entry?;
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
            Err(error) => return Err(error),
        };
        if stat::is_live_member(&stat, group) {
            found.push(pid);
        }
    }
    Ok(found)
}

#[cfg(target_os = "linux")]
fn vanished(error: &std::io::Error) -> bool {
    error.kind() == std::io::ErrorKind::NotFound
        || error.raw_os_error() == Some(Errno::SRCH.raw_os_error())
}
