#!/usr/bin/env python3
"""Read pinned Jackin source and execute only its extracted launch selection case.

No agent, container, hook installer, provider request or credential lookup runs.
The case probe deliberately omits runtime setup, role hooks and final exec.
"""
import argparse
import hashlib
import json
from pathlib import Path
import re
import shutil
import subprocess
from datetime import datetime, timezone


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--repo", type=Path, required=True)
    parser.add_argument("--commit", required=True)
    args = parser.parse_args()
    if args.commit != "6e266406b49da2ab3c55355f3b7d33d1643ee6cd":
        parser.error("This executable case audit is limited to the reviewed source commit.")

    def source(path):
        return subprocess.check_output(
            ["git", "-C", str(args.repo), "show", f"{args.commit}:{path}"]
        ).decode()

    paths = [
        "Cargo.toml",
        "crates/core/jackin-core/src/agent.rs",
        "crates/core/jackin-core/src/manifest.rs",
        "crates/core/jackin-core/src/container_paths.rs",
        "crates/core/jackin-protocol/src/lib.rs",
        "crates/services/jackin-runtime-launch-capsule-setup/src/capsule_setup.rs",
        "crates/adapters/jackin-image/src/derived_image.rs",
        "crates/apps/jackin-capsule/src/session/agent_cmd.rs",
        "crates/apps/jackin-capsule/src/agent_status/hook_installer.rs",
        "crates/apps/jackin-capsule/src/agent_status/hook_installer/claude.rs",
        "crates/apps/jackin-capsule/src/agent_status/hook_installer/codex.rs",
        "crates/apps/jackin-capsule/src/runtime_setup/agent_auth.rs",
        "crates/apps/jackin-capsule/src/daemon/control/auth.rs",
        "crates/apps/jackin-capsule/src/process_isolation.rs",
        "crates/apps/jackin-capsule/src/process_isolation/linux.rs",
        "crates/apps/jackin-capsule/src/process_isolation/linux/rules.rs",
        "crates/apps/jackin-capsule/src/process_isolation/linux/paths.rs",
        "crates/apps/jackin-capsule/src/exec.rs",
        "crates/apps/jackin-capsule/src/mcp_server.rs",
        "crates/services/jackin-usage-token-monitor/src/session.rs",
        "crates/services/jackin-usage-token-monitor/src/monitor.rs",
        "crates/services/jackin-usage-token-monitor/src/totals.rs",
        "crates/services/jackin-usage-token-monitor/src/claude.rs",
        "crates/services/jackin-usage-token-monitor/src/codex.rs",
        "docker/runtime/entrypoint.sh",
        "docker/runtime/agent-status/hooks/codex/report-hook.sh",
    ]
    contents = {path: source(path) for path in paths}
    agent_code = contents["crates/core/jackin-core/src/agent.rs"]
    all_block = agent_code.split("pub const ALL:", 1)[1].split("];", 1)[0]
    variants = re.findall(r"Self::(\w+)", all_block)
    slug_block = agent_code.split("pub const fn slug", 1)[1].split("pub const fn label", 1)[0]
    slug_map = dict(re.findall(r'Self::(\w+) => "([a-z]+)"', slug_block))
    agents = [slug_map[variant] for variant in variants]

    entrypoint = contents["docker/runtime/entrypoint.sh"]
    start = entrypoint.index('case "${JACKIN_AGENT:?JACKIN_AGENT must be set}" in')
    end = entrypoint.index("\nesac", start) + len("\nesac")
    selected_case = entrypoint[start:end]
    # This whitelist is a safety assertion about the exact reviewed shell fragment.
    executable_lines = "\n".join(line for line in selected_case.splitlines() if not line.lstrip().startswith("#"))
    assert "$(" not in executable_lines and "`" not in executable_lines
    assert "exec " not in executable_lines and "source " not in executable_lines
    script = "set -euo pipefail\n" + selected_case + '\nprintf "%s\\0" "${LAUNCH[@]}"\n'
    results = []
    for agent in agents:
        result = subprocess.run(
            ["bash", "--noprofile", "--norc", "-c", script],
            env={"PATH": "/usr/bin:/bin", "JACKIN_AGENT": agent},
            capture_output=True,
            timeout=5,
            check=False,
        )
        results.append({
            "agent": agent,
            "exit_code": result.returncode,
            "launch_argv": result.stdout.decode().rstrip("\0").split("\0") if result.stdout else [],
            "stderr": result.stderr.decode(),
        })

    codex = contents["crates/apps/jackin-capsule/src/agent_status/hook_installer/codex.rs"]
    codex_events = re.findall(r'"(\w+)"', codex.split("CODEX_HOOK_EVENTS:", 1)[1].split("];", 1)[0])
    claude = contents["crates/apps/jackin-capsule/src/agent_status/hook_installer/claude.rs"]
    claude_events = re.findall(r'\("(\w+)", (true|false)\)', claude.split("CLAUDE_HOOK_EVENTS:", 1)[1].split("];", 1)[0])
    poll = contents["crates/services/jackin-usage-token-monitor/src/session.rs"]
    active_pollers = re.findall(r"Agent::(\w+) => (\w+)::poll_session", poll)
    protocol = contents["crates/core/jackin-protocol/src/lib.rs"]
    identity = protocol.split("pub struct SessionIdentity", 1)[1].split("}", 1)[0] + "}"
    print(json.dumps({
        "executed_at_utc": datetime.now(timezone.utc).isoformat(),
        "commit": args.commit,
        "repo": str(args.repo),
        "evidence_class": "pinned source inspection plus isolated shell case selection",
        "not_executed": ["full entrypoint", "container launch", "native agent", "hook replacement", "provider request", "security runtime"],
        "source_files": [{"path": p, "sha256": hashlib.sha256(c.encode()).hexdigest(), "url": f"https://github.com/jackin-project/jackin/blob/{args.commit}/{p}"} for p, c in contents.items()],
        "agent_registry": agents,
        "entrypoint_case_sha256": hashlib.sha256(selected_case.encode()).hexdigest(),
        "entrypoint_selection": results,
        "codex_status_hook_events": codex_events,
        "claude_status_hook_events": [{"event": e, "async": a == "true"} for e, a in claude_events],
        "active_usage_poller_branches": active_pollers,
        "session_identity_source": "pub struct SessionIdentity" + identity,
        "host_binary_paths_on_current_path": {a: shutil.which(a) for a in ("claude", "codex")},
    }, indent=2))


if __name__ == "__main__":
    main()
