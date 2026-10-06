// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

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
    assert_eq!(identify_agent(&info), Some(Agent::Claude));
}

#[test]
fn identify_agent_node_wrapped_opencode_from_script_path() {
    let info = proc_info(
        110,
        110,
        110,
        Some("/usr/bin/node"),
        "node",
        &["node", "/usr/lib/node_modules/opencode/bin/opencode.js"],
    );
    assert_eq!(identify_agent(&info), Some(Agent::Opencode));
}

#[test]
fn identify_agent_node_wrapped_codex_from_scoped_package_path() {
    let info = proc_info(
        120,
        120,
        120,
        Some("/usr/bin/node"),
        "node",
        &["node", "/usr/lib/node_modules/@openai/codex/bin/cli.mjs"],
    );
    assert_eq!(identify_agent(&info), Some(Agent::Codex));
}

#[test]
fn identify_agent_bun_wrapped_amp_from_scoped_package_path() {
    let info = proc_info(
        130,
        130,
        130,
        Some("/usr/bin/bun"),
        "bun",
        &["bun", "/usr/lib/node_modules/@sourcegraph/amp/dist/amp.cjs"],
    );
    assert_eq!(identify_agent(&info), Some(Agent::Amp));
}

#[test]
fn identify_agent_node_wrapped_kimi_from_script_path() {
    let info = proc_info(
        140,
        140,
        140,
        Some("/usr/bin/node"),
        "node",
        &["node", "/usr/lib/node_modules/kimi/bin/kimi.js"],
    );
    assert_eq!(identify_agent(&info), Some(Agent::Kimi));
}

#[test]
fn identify_agent_node_wrapped_grok_from_script_path() {
    let info = proc_info(
        150,
        150,
        150,
        Some("/usr/bin/node"),
        "node",
        &["node", "/usr/lib/node_modules/grok/bin/grok.js"],
    );
    assert_eq!(identify_agent(&info), Some(Agent::Grok));
}

#[test]
fn identify_agent_node_eval_does_not_match_agent_mentions() {
    let info = proc_info(
        160,
        160,
        160,
        Some("/usr/bin/node"),
        "node",
        &["node", "-e", "console.log('amp')"],
    );
    assert_eq!(identify_agent(&info), None);
}

#[test]
fn identify_agent_python_module_does_not_match_agent_mentions() {
    let info = proc_info(
        170,
        170,
        170,
        Some("/usr/bin/python"),
        "python",
        &["python", "-m", "http.server"],
    );
    assert_eq!(identify_agent(&info), None);
}

#[test]
fn identify_agent_python_script_matches_agent_basename() {
    let info = proc_info(
        180,
        180,
        180,
        Some("/usr/bin/python3"),
        "python3",
        &["python3", "/opt/agents/kimi.py"],
    );
    assert_eq!(identify_agent(&info), Some(Agent::Kimi));
}

#[test]
fn identify_agent_shell_inline_command_does_not_match_agent_mentions() {
    let info = proc_info(
        190,
        190,
        190,
        Some("/bin/bash"),
        "bash",
        &["bash", "-c", "grok --help"],
    );
    assert_eq!(identify_agent(&info), None);
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
    assert_eq!(identify_agent(&info), Some(Agent::Codex));
}

#[test]
fn identify_agent_native_opencode_comm_fallback() {
    let info = proc_info(250, 250, 250, None, "opencode", &["opencode"]);
    assert_eq!(identify_agent(&info), Some(Agent::Opencode));
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
    assert_eq!(identify_agent(&info), Some(Agent::Amp));
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
    assert_eq!(identify_agent(&info), Some(Agent::Claude));
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
        foreground_group_from_process_infos(300, &foreground),
        ForegroundGroup::Agent {
            agent: Agent::Codex,
            pgid: 300
        }
    );
}

#[test]
fn foreground_agent_fixture_detects_node_wrapped_claude() {
    let foreground = [proc_info(
        300,
        300,
        300,
        Some("/usr/bin/node"),
        "node",
        &["node", "/app/node_modules/@anthropic-ai/claude-code/cli.js"],
    )];

    assert_eq!(
        foreground_group_from_process_infos(300, &foreground),
        ForegroundGroup::Agent {
            agent: Agent::Claude,
            pgid: 300
        }
    );
}

#[test]
fn foreground_agent_fixture_reports_unknown_shell_handoff() {
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
        foreground_group_from_process_infos(100, &foreground),
        ForegroundGroup::Unrecognized { pgid: 100 }
    );
}

#[test]
fn foreground_agent_fixture_rejects_missing_foreground_group() {
    // tpgid <= 0 means no foreground group; the public entry point short-circuits
    // before touching /proc, so the guard is testable from a fixture.
    let root = proc_info(100, 100, 0, Some("/bin/bash"), "bash", &["bash"]);
    assert_eq!(detect_foreground_agent(&root), ForegroundGroup::None);
}

#[test]
fn dead_process_returns_none() {
    let info = read_process_info(99999999);
    assert!(info.is_none());
}
