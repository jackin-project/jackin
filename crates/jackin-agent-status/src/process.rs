// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Linux /proc-based foreground process identity detection.
//!
//! Uses the `procfs` crate to read process metadata and determine which
//! agent binary owns the terminal's foreground process group. Called from
//! the 1Hz ticker in `daemon.rs` to validate hook authority and provide
//! a fallback detection signal.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::Instant;

use jackin_core::Agent;

use crate::policy::CPU_SAMPLE_WINDOW;

#[cfg(not(target_os = "linux"))]
mod procfs {
    use std::path::PathBuf;

    pub(crate) mod process {
        use super::PathBuf;

        #[derive(Debug, Clone)]
        pub(crate) struct Process;

        #[derive(Debug, Clone)]
        pub(crate) struct Stat {
            pub(crate) pid: i32,
            pub(crate) ppid: i32,
            pub(crate) pgrp: i32,
            pub(crate) tpgid: i32,
            pub(crate) comm: String,
            pub(crate) utime: u64,
            pub(crate) stime: u64,
        }

        impl Process {
            pub(crate) fn new(_pid: i32) -> Result<Self, ()> {
                Err(())
            }

            // Stub mirrors the Linux `procfs` Process API surface so call sites stay shared.
            #[expect(
                clippy::unused_self,
                reason = "non-Linux stub mirrors procfs Process method receivers used on Linux"
            )]
            pub(crate) fn stat(&self) -> Result<Stat, ()> {
                Err(())
            }

            #[expect(
                clippy::unused_self,
                reason = "non-Linux stub mirrors procfs Process method receivers used on Linux"
            )]
            pub(crate) fn exe(&self) -> Result<PathBuf, ()> {
                Err(())
            }

            #[expect(
                clippy::unused_self,
                reason = "non-Linux stub mirrors procfs Process method receivers used on Linux"
            )]
            pub(crate) fn cmdline(&self) -> Result<Vec<String>, ()> {
                Err(())
            }
        }

        pub(crate) fn all_processes() -> Result<std::vec::IntoIter<Result<Process, ()>>, ()> {
            Ok(Vec::new().into_iter())
        }
    }
}

/// Information about a single process read from /proc.
#[derive(Debug, Clone)]
pub struct ProcessInfo {
    pub pid: u32,
    /// Process group ID.
    pub pgid: u32,
    /// Terminal foreground process group ID.
    pub tpgid: i32,
    /// Command line arguments, split on NUL bytes.
    pub cmdline: Vec<String>,
    /// Resolved exe symlink path.
    pub exe_path: Option<PathBuf>,
    /// comm field (capped at 15 chars by kernel).
    pub comm: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProcessCpuSample {
    pub total_jiffies: u64,
    pub sampled_at: Instant,
}

/// Whether `/proc` physics can be sampled on this platform. Only Linux exposes
/// the `/proc` fields the detectors read; elsewhere the `procfs` shim returns
/// nothing. Callers use this to distinguish "physics unavailable" (no evidence)
/// from "process gone" (a real exit) — the watchdog must never demote on the
/// former.
pub const fn physics_available() -> bool {
    cfg!(target_os = "linux")
}

/// Reads process info for `pid` from /proc. Returns `None` when the
/// process doesn't exist or required fields are unreadable.
pub fn read_process_info(pid: u32) -> Option<ProcessInfo> {
    let process = procfs::process::Process::new(pid as i32).ok()?;
    let stat = process.stat().ok()?;
    let pgid = u32::try_from(stat.pgrp).unwrap_or(0);
    let tpgid = stat.tpgid;
    let comm = stat.comm;
    let exe_path = process.exe().ok();
    let cmdline = process.cmdline().unwrap_or_default();
    Some(ProcessInfo {
        pid,
        pgid,
        tpgid,
        cmdline,
        exe_path,
        comm,
    })
}

pub fn read_process_cpu_jiffies(pid: u32) -> Option<u64> {
    let process = procfs::process::Process::new(pid as i32).ok()?;
    let stat = process.stat().ok()?;
    Some(stat.utime.saturating_add(stat.stime))
}

pub fn sample_cpu_jiffies_delta(
    pid: u32,
    previous: &mut Option<ProcessCpuSample>,
    now: Instant,
) -> u64 {
    sample_cpu_jiffies_delta_from_total(read_process_cpu_jiffies(pid), previous, now)
}

pub trait ProcessSampler {
    fn physics_available(&self) -> bool;
    fn read_process_info(&self, pid: u32) -> Option<ProcessInfo>;
    fn foreground_group(&self, root_info: &ProcessInfo) -> ForegroundGroup;
    fn descendant_process_count(&self, root_pid: u32) -> u32;
    fn sample_cpu_jiffies_delta(
        &mut self,
        pid: u32,
        previous: &mut Option<ProcessCpuSample>,
        now: Instant,
    ) -> u64;
}

#[derive(Debug, Default)]
pub struct ProcfsProcessSampler;

impl ProcessSampler for ProcfsProcessSampler {
    fn physics_available(&self) -> bool {
        physics_available()
    }

    fn read_process_info(&self, pid: u32) -> Option<ProcessInfo> {
        read_process_info(pid)
    }

    fn foreground_group(&self, root_info: &ProcessInfo) -> ForegroundGroup {
        detect_foreground_agent(root_info)
    }

    fn descendant_process_count(&self, root_pid: u32) -> u32 {
        descendant_process_count(root_pid)
    }

    fn sample_cpu_jiffies_delta(
        &mut self,
        pid: u32,
        previous: &mut Option<ProcessCpuSample>,
        now: Instant,
    ) -> u64 {
        sample_cpu_jiffies_delta(pid, previous, now)
    }
}

fn sample_cpu_jiffies_delta_from_total(
    total_jiffies: Option<u64>,
    previous: &mut Option<ProcessCpuSample>,
    now: Instant,
) -> u64 {
    let Some(total_jiffies) = total_jiffies else {
        *previous = None;
        return 0;
    };
    let Some(prior) = previous else {
        *previous = Some(ProcessCpuSample {
            total_jiffies,
            sampled_at: now,
        });
        return 0;
    };
    if now.duration_since(prior.sampled_at) < CPU_SAMPLE_WINDOW {
        return 0;
    }
    let delta = total_jiffies.saturating_sub(prior.total_jiffies);
    *previous = Some(ProcessCpuSample {
        total_jiffies,
        sampled_at: now,
    });
    delta
}

pub fn descendant_process_count(root_pid: u32) -> u32 {
    let Ok(iter) = procfs::process::all_processes() else {
        return 0;
    };
    // Feed (pid, ppid) pairs straight from /proc; the helper owns the single
    // parent->children map build so it stays unit-testable with synthetic input.
    let parents = iter.filter_map(|proc_result| {
        let stat = proc_result.ok()?.stat().ok()?;
        (stat.pid > 0 && stat.ppid > 0).then_some((
            u32::try_from(stat.pid).unwrap_or(0),
            u32::try_from(stat.ppid).unwrap_or(0),
        ))
    });
    descendant_process_count_from_parents(root_pid, parents)
}

fn descendant_process_count_from_parents(
    root_pid: u32,
    processes: impl IntoIterator<Item = (u32, u32)>,
) -> u32 {
    let mut children_by_parent: HashMap<u32, Vec<u32>> = HashMap::new();
    for (pid, ppid) in processes {
        children_by_parent.entry(ppid).or_default().push(pid);
    }
    let mut count = 0u32;
    let mut stack = children_by_parent.remove(&root_pid).unwrap_or_default();
    while let Some(pid) = stack.pop() {
        count = count.saturating_add(1);
        if let Some(children) = children_by_parent.remove(&pid) {
            stack.extend(children);
        }
    }
    count
}

/// Scan all processes in `/proc` and return those with `pgrp == target_pgid`.
pub fn pids_in_pgrp(target_pgid: u32) -> Vec<u32> {
    let Ok(iter) = procfs::process::all_processes() else {
        return Vec::new();
    };
    let mut pids = Vec::new();
    for proc_result in iter {
        let Ok(process) = proc_result else { continue };
        let Ok(stat) = process.stat() else { continue };
        if stat.pgrp == target_pgid as i32 {
            pids.push(u32::try_from(stat.pid).unwrap_or(0));
        }
    }
    pids
}

/// Map a process basename to the canonical agent slug enum, or `None` when it
/// is not a recognized agent binary.
fn agent_from_name(name: &str) -> Option<Agent> {
    // `claude-code` is the npm package's binary name; the canonical slug is
    // `claude`. Everything else maps by `Agent`'s own slug parser.
    let slug = if name == "claude-code" {
        "claude"
    } else {
        name
    };
    Agent::from_slug(slug)
}

fn basename(value: &str) -> &str {
    value
        .rsplit(['/', '\\'])
        .next()
        .filter(|part| !part.is_empty())
        .unwrap_or(value)
}

fn strip_script_extension(name: &str) -> &str {
    Path::new(name)
        .file_stem()
        .and_then(std::ffi::OsStr::to_str)
        .unwrap_or(name)
}

fn agent_from_wrapped_path(path: &str) -> Option<Agent> {
    if path.contains("@anthropic-ai/claude-code") || path.contains("claude-code") {
        return Some(Agent::Claude);
    }

    path.split(['/', '\\']).rev().find_map(|component| {
        if component.is_empty() || component.starts_with('@') {
            return None;
        }
        agent_from_name(strip_script_extension(component))
    })
}

fn is_argv0(exe_name: &str, arg: &str) -> bool {
    basename(arg) == exe_name
}

fn first_node_script_arg<'a>(exe_name: &str, cmdline: &'a [String]) -> Option<&'a str> {
    let mut skip_eval_operand = false;
    for arg in cmdline {
        if is_argv0(exe_name, arg) {
            continue;
        }
        if skip_eval_operand {
            skip_eval_operand = false;
            continue;
        }
        match arg.as_str() {
            "-e" | "-p" | "--eval" | "--print" => {
                skip_eval_operand = true;
            }
            "--" => {}
            flag if flag.starts_with("--eval=") || flag.starts_with("--print=") => {}
            flag if flag.starts_with('-') => {}
            script => return Some(script),
        }
    }
    None
}

fn first_python_script_arg<'a>(exe_name: &str, cmdline: &'a [String]) -> Option<&'a str> {
    for arg in cmdline {
        if is_argv0(exe_name, arg) {
            continue;
        }
        match arg.as_str() {
            "-c" | "-m" => return None,
            flag if flag.starts_with('-') => {}
            script => return Some(script),
        }
    }
    None
}

fn first_shell_script_arg<'a>(exe_name: &str, cmdline: &'a [String]) -> Option<&'a str> {
    for arg in cmdline {
        if is_argv0(exe_name, arg) {
            continue;
        }
        match arg.as_str() {
            "-c" => return None,
            flag if flag.starts_with('-') => {}
            script => return Some(script),
        }
    }
    None
}

fn wrapped_agent_from_argv(exe_name: &str, cmdline: &[String]) -> Option<Agent> {
    let script = match exe_name {
        "node" | "bun" | "deno" => first_node_script_arg(exe_name, cmdline),
        "python" | "python3" => first_python_script_arg(exe_name, cmdline),
        "sh" | "bash" | "zsh" | "fish" => first_shell_script_arg(exe_name, cmdline),
        _ => None,
    }?;
    agent_from_wrapped_path(script)
}

/// Identify the agent running in `proc`. Returns `None` when no recognized
/// agent is found.
pub fn identify_agent(info: &ProcessInfo) -> Option<Agent> {
    // Prefer the exe basename: it is the full binary name, unlike `comm` which
    // the kernel truncates to 15 chars (so a longer agent name would be missed).
    if let Some(ref exe) = info.exe_path {
        let exe_name = exe.file_name()?.to_string_lossy();
        if let Some(agent) = agent_from_name(exe_name.as_ref()) {
            return Some(agent);
        }
        if matches!(
            exe_name.as_ref(),
            "node" | "bun" | "deno" | "python" | "python3" | "sh" | "bash" | "zsh" | "fish"
        ) {
            return wrapped_agent_from_argv(exe_name.as_ref(), &info.cmdline);
        }
    }

    // Fall back to the (15-char-truncated) comm when the exe path is unreadable.
    agent_from_name(info.comm.as_str())
}

/// What owns the terminal's foreground process group. A `pgid` is present
/// exactly when a group exists — an invariant the previous
/// `Option<(Option<Agent>, u32)>` left to the caller to decode.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ForegroundGroup {
    /// No foreground process group (`tpgid <= 0`).
    None,
    /// A foreground group exists but holds no recognized agent (e.g. a shell).
    Unrecognized { pgid: u32 },
    /// A recognized agent owns the foreground group.
    Agent { agent: Agent, pgid: u32 },
}

impl ForegroundGroup {
    /// A recognized agent owns the foreground.
    pub fn is_agent(self) -> bool {
        matches!(self, Self::Agent { .. })
    }

    /// A foreground process group exists at all (agent or not).
    pub fn has_group(self) -> bool {
        !matches!(self, Self::None)
    }

    /// The foreground process group id, when one exists.
    pub fn pgid(self) -> Option<u32> {
        match self {
            Self::None => None,
            Self::Unrecognized { pgid } | Self::Agent { pgid, .. } => Some(pgid),
        }
    }
}

/// Given the child PID of a session's root process, determine what agent
/// currently owns the terminal's foreground process group. `root_info` is the
/// already-read `/proc` info for the child PID, so the caller (which read it for
/// its own physics sample) does not pay a second stat+exe+cmdline read here.
pub fn detect_foreground_agent(root_info: &ProcessInfo) -> ForegroundGroup {
    if root_info.tpgid <= 0 {
        return ForegroundGroup::None;
    }
    let Ok(fg_pgid) = u32::try_from(root_info.tpgid) else {
        return ForegroundGroup::None;
    };
    let process_group: Vec<_> = pids_in_pgrp(fg_pgid)
        .into_iter()
        .filter_map(read_process_info)
        .collect();
    foreground_group_from_process_infos(fg_pgid, &process_group)
}

fn foreground_group_from_process_infos(
    fg_pgid: u32,
    process_group: &[ProcessInfo],
) -> ForegroundGroup {
    for proc_info in process_group {
        if let Some(agent) = identify_agent(proc_info) {
            return ForegroundGroup::Agent {
                agent,
                pgid: fg_pgid,
            };
        }
    }
    ForegroundGroup::Unrecognized { pgid: fg_pgid }
}

#[cfg(test)]
mod tests;
