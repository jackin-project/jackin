// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use rustix::io::Errno;
use rustix::process::{Pid, Signal, kill_process_group};

pub(super) enum Sweep {
    Empty,
    Signalled,
}

pub(super) struct Fault(pub(super) String);

pub(super) fn sweep(group: Pid) -> Result<Sweep, Fault> {
    match kill_process_group(group, Signal::KILL) {
        Ok(()) => Ok(Sweep::Signalled),
        Err(Errno::SRCH) => Ok(Sweep::Empty),
        Err(Errno::PERM) => denied(group),
        Err(errno) => Err(Fault(format!("signalling it failed: {errno}"))),
    }
}

#[cfg(not(target_os = "linux"))]
fn denied(_group: Pid) -> Result<Sweep, Fault> {
    Ok(Sweep::Empty)
}

#[cfg(target_os = "linux")]
fn denied(group: Pid) -> Result<Sweep, Fault> {
    let members =
        members(group).map_err(|error| Fault(format!("listing its members failed: {error}")))?;
    if members.is_empty() {
        return Ok(Sweep::Empty);
    }
    for member in members {
        match rustix::process::kill_process(member, Signal::KILL) {
            Ok(()) | Err(Errno::SRCH) => {}
            Err(errno) => {
                return Err(Fault(format!(
                    "signalling process {} failed: {errno}",
                    member.as_raw_pid()
                )));
            }
        }
    }
    Ok(Sweep::Signalled)
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
        let stat = match std::fs::read_to_string(entry.path().join("stat")) {
            Ok(stat) => stat,
            Err(error) if vanished(&error) => continue,
            Err(error) => return Err(error),
        };
        if lives_in(&stat, group) {
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

#[cfg(target_os = "linux")]
fn lives_in(stat: &str, group: Pid) -> bool {
    let Some((_, after_name)) = stat.rsplit_once(')') else {
        return false;
    };
    let mut fields = after_name.split_whitespace();
    let state = fields.next();
    let pgrp = fields.nth(1).and_then(|field| field.parse::<i32>().ok());
    !matches!(state, Some("Z" | "X")) && pgrp == Some(group.as_raw_pid())
}
