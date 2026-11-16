#!/usr/bin/env python3
"""Reply to and resolve the two open review threads on jackin PR #1282."""

import json
import subprocess

REPLY_1 = """Fixed and pushed in `9b023c8dc`.

The judge no longer resolves the recorded paths on the host that runs xtask. `crates/xtask/src/e2e/steps/j07.rs:112-119` now compares the recorded `exe` with the recorded `interpreter_resolved` directly, and the `same_physical_path` helper (the `std::fs::canonicalize` call) is deleted, so no host filesystem is consulted at all.

Why the direct comparison is the sandbox comparison: both recorded values are already resolved inside the sandbox, so the comparison is made on the sandbox filesystem by construction. The probe records the executable from the sandbox's own `/proc/<pid>/exe` (jackin-sentinel PR #160, `probe/jackin-sentinel-probe:469` `record_exe=$(readlink "/proc/$record_pid/exe" 2>/dev/null)`) and resolves the interpreter with the in-sandbox `physical_path` helper (`probe/jackin-sentinel-probe:364-380`, used at `:468` `record_resolved=$(physical_path "$record_interpreter")`). The probe itself compares the two inside the sandbox (`:477-478` `record_exe_path=$(physical_path "$record_exe")` / `[ "$record_exe_path" = "$record_resolved" ] || return 1`). The kernel path and `physical_path` resolve the same way, so the recorded strings are equal exactly when the two sandbox paths are the same file.

The host resolution could only ever add or remove decisions, never add evidence: it accepted two distinct sandbox paths that the host aliases to one file, and it rejected a sandbox alias the host does not know. The new comparison is stricter in the aliasing case and identical in every other case, so no assertion is weakened.

Contract test: `crates/xtask/tests/e2e/k_identity.rs:228` `the_launcher_paths_are_compared_without_the_host_filesystem` — a temp directory holds a symlink alias that the host resolves to the same inode as the real file; the judge rejects the record — red before (the host canonicalization accepted it), green after.

The wire format is unchanged: `rewrite/probe-schema.json` `$defs/LauncherIdentity` and the serde structs in `crates/xtask/src/e2e/probe.rs` are untouched, so the flat JSON stays byte-identical for the sentinel contract of PR #160.

Gates: `cargo nextest run -p jackin-xtask` 287/287, `cargo fmt --check`, `cargo clippy -p jackin-xtask --all-targets` (0 warnings), `cargo xtask ci --only comments,layers,pins,containers,errors` green.

Resolving."""

REPLY_2 = """Fixed and pushed in `9b023c8dc`.

`crates/xtask/src/e2e/steps/j07.rs:164-170` now compares the complete `(pid, start time)` tuples of the pre-launch and the pre-restart snapshot instead of the start time alone, so a snapshot of another process created in the same clock tick no longer passes. The pid is not discarded any more.

The legitimate record stays valid: the pre-launch snapshot and the later snapshot are the stat of the same launcher pid (jackin-sentinel PR #160, `probe/jackin-sentinel-probe:2285` `launcher_stat_pre=$(proc_stat "$launcher_pid")` and `:481` `record_before=$(proc_stat "$record_pid")` for that same pid), and `exec` keeps the pid and the start time, so the tuples are equal.

Contract test: `crates/xtask/tests/e2e/k_identity.rs:213` `a_pre_launch_snapshot_from_another_pid_is_rejected` — a `stat_pre` that describes pid 999 with the same start time is rejected — red before (only the start time was compared), green after. The existing `crates/xtask/tests/e2e/k_identity.rs:172` `the_pre_launch_start_time_is_read_from_the_start_time_field` still proves that the process state is not mistaken for the start time: a different state with the same pid and start time is accepted.

Gates: `cargo nextest run -p jackin-xtask` 287/287, `cargo fmt --check`, `cargo clippy -p jackin-xtask --all-targets` (0 warnings), `cargo xtask ci --only comments,layers,pins,containers,errors` green.

Resolving."""

THREADS = [
    ("PRRT_kwDOR2N26c63Rf3p", REPLY_1),
    ("PRRT_kwDOR2N26c63Rf3x", REPLY_2),
]

REPLY_MUTATION = """
mutation($threadId: ID!, $body: String!) {
  addPullRequestReviewThreadReply(input: {pullRequestReviewThreadId: $threadId, body: $body}) {
    comment { url }
  }
}
"""

RESOLVE_MUTATION = """
mutation($threadId: ID!) {
  resolveReviewThread(input: {threadId: $threadId}) {
    thread { id isResolved }
  }
}
"""


def run(query, variables):
    command = ["gh", "api", "graphql", "-f", f"query={query}"]
    for name, value in variables.items():
        command.extend(["-f", f"{name}={value}"])
    result = subprocess.run(command, capture_output=True, text=True, check=True)
    payload = json.loads(result.stdout)
    if "errors" in payload:
        raise SystemExit(f"graphql errors: {payload['errors']}")
    return payload["data"]


for thread_id, body in THREADS:
    data = run(REPLY_MUTATION, {"threadId": thread_id, "body": body})
    print(thread_id, "reply:", data["addPullRequestReviewThreadReply"]["comment"]["url"])
    data = run(RESOLVE_MUTATION, {"threadId": thread_id})
    print(thread_id, "resolved:", data["resolveReviewThread"]["thread"])
