//! Linux /proc-based foreground process identity detection.
//!
//! Uses the `procfs` crate to read process metadata and determine which
//! agent binary owns the terminal's foreground process group. Called from
//! the 1Hz ticker in `daemon.rs` to validate hook authority and provide
//! a fallback detection signal.

use std::collections::HashMap;
use std::path::PathBuf;
use std::time::Instant;

use crate::agent_status::policy::CPU_SAMPLE_WINDOW;

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

            pub(crate) fn stat(&self) -> Result<Stat, ()> {
                Err(())
            }

            pub(crate) fn exe(&self) -> Result<PathBuf, ()> {
                Err(())
            }

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

/// Reads the `tpgid` (terminal foreground process group) for `pid`.
///
/// Returns `None` when the process doesn't exist or the field is unparseable.
pub fn read_tpgid(pid: u32) -> Option<i32> {
    let process = procfs::process::Process::new(pid as i32).ok()?;
    let stat = process.stat().ok()?;
    Some(stat.tpgid)
}

/// Reads process info for `pid` from /proc. Returns `None` when the
/// process doesn't exist or required fields are unreadable.
pub fn read_process_info(pid: u32) -> Option<ProcessInfo> {
    let process = procfs::process::Process::new(pid as i32).ok()?;
    let stat = process.stat().ok()?;
    let pgid = stat.pgrp as u32;
    let tpgid = stat.tpgid;
    let comm = stat.comm.clone();
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
    let mut children_by_parent: HashMap<u32, Vec<u32>> = HashMap::new();
    for proc_result in iter {
        let Ok(process) = proc_result else { continue };
        let Ok(stat) = process.stat() else { continue };
        if stat.pid <= 0 || stat.ppid <= 0 {
            continue;
        }
        children_by_parent
            .entry(stat.ppid as u32)
            .or_default()
            .push(stat.pid as u32);
    }
    descendant_process_count_from_parents(
        root_pid,
        children_by_parent
            .into_iter()
            .flat_map(|(ppid, pids)| pids.into_iter().map(move |pid| (pid, ppid))),
    )
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
            pids.push(stat.pid as u32);
        }
    }
    pids
}

/// Agent kinds that jackin' recognises.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AgentKind {
    ClaudeCode,
    Codex,
    Amp,
    Kimi,
    OpenCode,
    Unknown,
}

impl AgentKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::ClaudeCode => "claude",
            Self::Codex => "codex",
            Self::Amp => "amp",
            Self::Kimi => "kimi",
            Self::OpenCode => "opencode",
            Self::Unknown => "unknown",
        }
    }
}

fn agent_kind_from_name(name: &str) -> Option<AgentKind> {
    match name {
        "codex" => Some(AgentKind::Codex),
        "amp" => Some(AgentKind::Amp),
        "kimi" => Some(AgentKind::Kimi),
        "opencode" => Some(AgentKind::OpenCode),
        "claude" | "claude-code" => Some(AgentKind::ClaudeCode),
        _ => None,
    }
}

/// Identify the agent running in `proc`. Returns `None` when no known agent
/// is found.
pub fn identify_agent(info: &ProcessInfo) -> Option<AgentKind> {
    // Primary: exe basename
    if let Some(ref exe) = info.exe_path {
        let exe_name = exe.file_name()?.to_string_lossy();
        if let Some(kind) = agent_kind_from_name(exe_name.as_ref()) {
            return Some(kind);
        }
        // Node-wrapped agents: inspect argv[1] for the JS entry point
        if matches!(exe_name.as_ref(), "node" | "bun" | "deno") {
            if let Some(script) = info.cmdline.get(1)
                && (script.contains("@anthropic-ai/claude-code") || script.contains("claude-code"))
            {
                return Some(AgentKind::ClaudeCode);
            }
            return Some(AgentKind::Unknown);
        }
    }

    // Fallback: comm field (capped at 15 chars)
    agent_kind_from_name(info.comm.as_str())
}

/// Given the child PID of a session's root process, determine what agent
/// currently owns the terminal's foreground process group.
///
/// Returns `(agent_kind, foreground_pgid)` or `None` when detection fails.
pub fn detect_foreground_agent(child_pid: u32) -> Option<(AgentKind, u32)> {
    let info = read_process_info(child_pid)?;
    if info.tpgid <= 0 {
        return None;
    }
    let fg_pgid = u32::try_from(info.tpgid).ok()?;
    let process_group: Vec<_> = pids_in_pgrp(fg_pgid)
        .into_iter()
        .filter_map(read_process_info)
        .collect();
    detect_foreground_agent_from_process_infos(&info, &process_group)
}

fn detect_foreground_agent_from_process_infos(
    root_info: &ProcessInfo,
    process_group: &[ProcessInfo],
) -> Option<(AgentKind, u32)> {
    if root_info.tpgid <= 0 {
        return None;
    }
    let fg_pgid = u32::try_from(root_info.tpgid).ok()?;
    for proc_info in process_group {
        if let Some(kind) = identify_agent(proc_info)
            && kind != AgentKind::Unknown
        {
            return Some((kind, fg_pgid));
        }
    }
    // Process group exists but no recognized agent binary found.
    Some((AgentKind::Unknown, fg_pgid))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    fn proc_info(
        pid: u32,
        pgid: u32,
        tpgid: i32,
        exe_path: Option<&str>,
        comm: &str,
        cmdline: &[&str],
    ) -> ProcessInfo {
        ProcessInfo {
            pid,
            pgid,
            tpgid,
            cmdline: cmdline.iter().map(|part| (*part).to_owned()).collect(),
            exe_path: exe_path.map(PathBuf::from),
            comm: comm.to_owned(),
        }
    }

    #[test]
    fn identify_agent_node_wrapped_claude_from_cmdline() {
        let info = proc_info(
            100,
            100,
            100,
            Some("/usr/bin/node"),
            "node",
            &[
                "node",
                "/usr/local/lib/node_modules/@anthropic-ai/claude-code/cli.js",
            ],
        );
        assert_eq!(identify_agent(&info), Some(AgentKind::ClaudeCode));
    }

    #[test]
    fn identify_agent_native_codex_binary() {
        let info = proc_info(
            200,
            200,
            200,
            Some("/usr/local/bin/codex"),
            "codex",
            &["codex"],
        );
        assert_eq!(identify_agent(&info), Some(AgentKind::Codex));
    }

    #[test]
    fn identify_agent_native_amp_binary() {
        let info = proc_info(
            300,
            300,
            300,
            Some("/usr/local/bin/amp"),
            "amp",
            &["amp", "--dangerously-allow-all"],
        );
        assert_eq!(identify_agent(&info), Some(AgentKind::Amp));
    }

    #[test]
    fn identify_agent_stat_comm_truncation_falls_back_to_exe() {
        let info = proc_info(
            400,
            400,
            400,
            Some("/usr/bin/node"),
            "node",
            &["node", "/path/to/@anthropic-ai/claude-code/cli.js"],
        );
        assert_eq!(identify_agent(&info), Some(AgentKind::ClaudeCode));
    }

    #[test]
    fn cpu_sample_waits_for_window_then_reports_saturating_delta() {
        let now = Instant::now();
        let mut previous = None;

        assert_eq!(
            sample_cpu_jiffies_delta_from_total(Some(100), &mut previous, now),
            0
        );
        assert_eq!(
            previous,
            Some(ProcessCpuSample {
                total_jiffies: 100,
                sampled_at: now
            })
        );

        let before_window = (now + CPU_SAMPLE_WINDOW)
            .checked_sub(Duration::from_millis(1))
            .unwrap();
        assert_eq!(
            sample_cpu_jiffies_delta_from_total(Some(125), &mut previous, before_window),
            0
        );
        assert_eq!(
            previous,
            Some(ProcessCpuSample {
                total_jiffies: 100,
                sampled_at: now
            })
        );

        let after_window = now + CPU_SAMPLE_WINDOW + Duration::from_millis(1);
        assert_eq!(
            sample_cpu_jiffies_delta_from_total(Some(140), &mut previous, after_window),
            40
        );
        assert_eq!(
            previous,
            Some(ProcessCpuSample {
                total_jiffies: 140,
                sampled_at: after_window
            })
        );

        let after_reset = after_window + CPU_SAMPLE_WINDOW + Duration::from_millis(1);
        assert_eq!(
            sample_cpu_jiffies_delta_from_total(Some(10), &mut previous, after_reset),
            0
        );
    }

    #[test]
    fn cpu_sample_missing_process_clears_prior_sample() {
        let now = Instant::now();
        let mut previous = Some(ProcessCpuSample {
            total_jiffies: 100,
            sampled_at: now,
        });

        assert_eq!(
            sample_cpu_jiffies_delta_from_total(None, &mut previous, now),
            0
        );
        assert_eq!(previous, None);
    }

    #[test]
    fn descendant_count_fixture_counts_full_tree_only_under_root() {
        let processes = [(2, 1), (3, 1), (4, 2), (5, 4), (6, 99), (7, 6)];

        assert_eq!(descendant_process_count_from_parents(1, processes), 4);
        assert_eq!(descendant_process_count_from_parents(99, processes), 2);
        assert_eq!(descendant_process_count_from_parents(42, processes), 0);
    }

    #[test]
    fn foreground_agent_fixture_detects_direct_binary() {
        let root = proc_info(100, 100, 300, Some("/bin/zsh"), "zsh", &["zsh"]);
        let foreground = [
            proc_info(
                300,
                300,
                300,
                Some("/usr/local/bin/codex"),
                "codex",
                &["codex"],
            ),
            proc_info(301, 300, 300, Some("/usr/bin/node"), "node", &["node"]),
        ];

        assert_eq!(
            detect_foreground_agent_from_process_infos(&root, &foreground),
            Some((AgentKind::Codex, 300))
        );
    }

    #[test]
    fn foreground_agent_fixture_detects_node_wrapped_claude() {
        let root = proc_info(100, 100, 300, Some("/bin/zsh"), "zsh", &["zsh"]);
        let foreground = [proc_info(
            300,
            300,
            300,
            Some("/usr/bin/node"),
            "node",
            &["node", "/app/node_modules/@anthropic-ai/claude-code/cli.js"],
        )];

        assert_eq!(
            detect_foreground_agent_from_process_infos(&root, &foreground),
            Some((AgentKind::ClaudeCode, 300))
        );
    }

    #[test]
    fn foreground_agent_fixture_reports_unknown_shell_handoff() {
        let root = proc_info(100, 100, 100, Some("/bin/bash"), "bash", &["bash"]);
        let foreground = [
            proc_info(100, 100, 100, Some("/bin/bash"), "bash", &["bash"]),
            proc_info(
                101,
                100,
                100,
                Some("/usr/bin/starship"),
                "starship",
                &["starship"],
            ),
        ];

        assert_eq!(
            detect_foreground_agent_from_process_infos(&root, &foreground),
            Some((AgentKind::Unknown, 100))
        );
    }

    #[test]
    fn foreground_agent_fixture_rejects_missing_foreground_group() {
        let root = proc_info(100, 100, 0, Some("/bin/bash"), "bash", &["bash"]);
        let foreground = [proc_info(
            100,
            100,
            0,
            Some("/usr/local/bin/codex"),
            "codex",
            &["codex"],
        )];

        assert_eq!(
            detect_foreground_agent_from_process_infos(&root, &foreground),
            None
        );
    }

    #[test]
    fn dead_process_returns_none() {
        let info = read_process_info(99999999);
        assert!(info.is_none());
    }
}
