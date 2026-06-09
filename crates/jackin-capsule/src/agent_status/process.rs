//! Linux /proc-based foreground process identity detection.
//!
//! Uses the `procfs` crate to read process metadata and determine which
//! agent binary owns the terminal's foreground process group. Called from
//! the 1Hz ticker in `daemon.rs` to validate hook authority and provide
//! a fallback detection signal.

use std::path::PathBuf;

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
            if let Some(script) = info.cmdline.get(1) {
                if script.contains("@anthropic-ai/claude-code") || script.contains("claude-code") {
                    return Some(AgentKind::ClaudeCode);
                }
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
    let tpgid = info.tpgid;
    if tpgid <= 0 {
        return None;
    }
    let fg_pgid = tpgid as u32;
    // Scan the process group for a recognisable agent binary.
    for pid in pids_in_pgrp(fg_pgid) {
        if let Some(proc_info) = read_process_info(pid) {
            if let Some(kind) = identify_agent(&proc_info) {
                if kind != AgentKind::Unknown {
                    return Some((kind, fg_pgid));
                }
            }
        }
    }
    // Process group exists but no recognised agent binary found.
    Some((AgentKind::Unknown, fg_pgid))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identify_agent_node_wrapped_claude_from_cmdline() {
        let info = ProcessInfo {
            pid: 100,
            pgid: 100,
            tpgid: 100,
            cmdline: vec![
                "node".to_string(),
                "/usr/local/lib/node_modules/@anthropic-ai/claude-code/cli.js".to_string(),
            ],
            exe_path: Some(PathBuf::from("/usr/bin/node")),
            comm: "node".to_string(),
        };
        assert_eq!(identify_agent(&info), Some(AgentKind::ClaudeCode));
    }

    #[test]
    fn identify_agent_native_codex_binary() {
        let info = ProcessInfo {
            pid: 200,
            pgid: 200,
            tpgid: 200,
            cmdline: vec!["codex".to_string()],
            exe_path: Some(PathBuf::from("/usr/local/bin/codex")),
            comm: "codex".to_string(),
        };
        assert_eq!(identify_agent(&info), Some(AgentKind::Codex));
    }

    #[test]
    fn identify_agent_native_amp_binary() {
        let info = ProcessInfo {
            pid: 300,
            pgid: 300,
            tpgid: 300,
            cmdline: vec!["amp".to_string(), "--dangerously-allow-all".to_string()],
            exe_path: Some(PathBuf::from("/usr/local/bin/amp")),
            comm: "amp".to_string(),
        };
        assert_eq!(identify_agent(&info), Some(AgentKind::Amp));
    }

    #[test]
    fn identify_agent_stat_comm_truncation_falls_back_to_exe() {
        let info = ProcessInfo {
            pid: 400,
            pgid: 400,
            tpgid: 400,
            cmdline: vec![
                "node".to_string(),
                "/path/to/@anthropic-ai/claude-code/cli.js".to_string(),
            ],
            exe_path: Some(PathBuf::from("/usr/bin/node")),
            comm: "node".to_string(),
        };
        assert_eq!(identify_agent(&info), Some(AgentKind::ClaudeCode));
    }

    #[test]
    fn dead_process_returns_none() {
        let info = read_process_info(99999999);
        assert!(info.is_none());
    }
}
