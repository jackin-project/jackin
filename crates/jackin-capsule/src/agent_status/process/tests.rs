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
        Some((Some(Agent::Codex), 300))
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
        Some((Some(Agent::Claude), 300))
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
        Some((None, 100))
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
