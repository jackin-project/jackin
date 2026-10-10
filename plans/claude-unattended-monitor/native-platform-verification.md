# Native macOS and OrbStack verification packet

This packet records the remaining operator-run platform evidence for the Claude
usage monitor. It does not report a host run. The historical code checkpoint
used to prepare it was `40b579a0ce599c805cc5e79fe0f9f53010282540` (tree
`fb53218a5c24b69cab250cdf151f927c3f24f6ae`). The documentation commit changes
the repository HEAD, so that historical SHA is not an execution pin. Before
running, the release owner must replace `REPLACE_WITH_FINAL_REVIEWED_SHA` below
with the exact final reviewed source SHA. If source code has advanced, regenerate
and review the source-bound output first; do not use a moving branch. The
placeholder intentionally makes the command fail until the final SHA is pinned.

No operator host matching the required platform has been recorded yet. This
gate remains pending until an operator supplies a macOS 26 arm64 host with
Xcode 26.6, macOS SDK 26.5, Swift 6.3, and a running OrbStack Docker engine.
Hosted macOS CI or a different Docker engine is not a substitute for this
OrbStack gate.

## Required host and tool facts

The host capture must show all of the following before `--run`:

| Fact | Required value |
| --- | --- |
| Source | Exact reviewed SHA above; clean checkout |
| Host | macOS 26, `arm64` |
| Xcode | Xcode 26.6 selected; macOS SDK 26.5 |
| Swift | Swift 6.3 |
| Container engine | OrbStack running; selected Docker context and server facts identify that OrbStack engine, Linux arm64 |
| Rust execution | Absolute MBX 1.23.0; `+1.99.0` selected by the commands below |
| Mise | Absolute Mise 2026.10.7 |
| Concurrency | `CARGO_BUILD_JOBS=2`, `NEXTEST_TEST_THREADS=2`; native Swift tasks use `--jobs 2` |

The checked-in host script is
[`native-host-gate.sh`](native-host-gate.sh), SHA-256
`2041fcc8372583f55bd463c198055770b2a2b3a66514dc8eee51b315fb4b0cf2`. It is a
byte-for-byte copy of the reviewed operator script. Verify that hash before
using it. It records the source SHA, worktree status, tool versions and hashes,
macOS/Xcode/SDK/Swift facts, OrbStack version, Docker context, and Docker server
facts. It does not enforce the required host tuple or compare the source SHA to
an expected value; the operator must check those facts against this packet
before proceeding.

Use an evidence directory outside the checkout. Keep it private because logs
can contain local paths and test diagnostics. Set absolute paths for MBX and
Mise on the supplied host; do not invoke Cargo directly or change Cargo/Rust
home directories to work around a tool-resolution failure.
Run the command blocks from the clean repository root.

```bash
set -euo pipefail

SOURCE_SHA='REPLACE_WITH_FINAL_REVIEWED_SHA'
EVIDENCE_DIR="${TMPDIR:-/private/tmp}/jackin-native-${SOURCE_SHA}"
MBX_BIN='/absolute/path/to/mbx-1.23.0'
MISE_BIN='/absolute/path/to/mise-2026.10.7'
DEVELOPER_DIR='/Applications/Xcode_26.6.app/Contents/Developer'
SCRIPT="$PWD/plans/claude-unattended-monitor/native-host-gate.sh"

[[ "$SOURCE_SHA" =~ ^[0-9a-f]{40}$ ]]
test "$(git rev-parse HEAD)" = "$SOURCE_SHA"
test -z "$(git status --porcelain=v1 --untracked-files=all)"
test "$(shasum -a 256 "$SCRIPT" | awk '{print $1}')" = \
  '2041fcc8372583f55bd463c198055770b2a2b3a66514dc8eee51b315fb4b0cf2'

export MBX_BIN MISE_BIN DEVELOPER_DIR
export CARGO_BUILD_JOBS=2 NEXTEST_TEST_THREADS=2

"$SCRIPT" --capture-only "$EVIDENCE_DIR"

{
  printf 'source_sha=%s\n' "$SOURCE_SHA"
  printf 'cargo_build_jobs=%s\n' "$CARGO_BUILD_JOBS"
  printf 'nextest_test_threads=%s\n' "$NEXTEST_TEST_THREADS"
  printf 'developer_dir=%s\n' "$DEVELOPER_DIR"
  printf 'mbx_path=%s\nmise_path=%s\n' "$MBX_BIN" "$MISE_BIN"
} > "$EVIDENCE_DIR/operator-context.txt"
chmod 600 "$EVIDENCE_DIR/operator-context.txt"
```

Review `host-provenance.txt` and stop if any required host/tool fact differs.
In particular, a successful `orb version` is insufficient by itself: the Docker
context and server facts must identify the running OrbStack Linux arm64 engine.
Record the exported concurrency settings in a private `operator-context.txt`
next to the host capture.

If the preflight matches, run both required gates from the same clean source
checkout and evidence directory:

```bash
test "$(git rev-parse HEAD)" = "$SOURCE_SHA"
test -z "$(git status --porcelain=v1 --untracked-files=all)"
set +e
"$SCRIPT" --run "$EVIDENCE_DIR"
gate_status=$?
set -e

: > "$EVIDENCE_DIR/native-reports.sha256"
chmod 600 "$EVIDENCE_DIR/native-reports.sha256"
for report in \
  native/.build/swift-unit-tests.xml \
  native/.build/swift-unit-tests-swift-testing.xml \
  native/.build/swift-unit-tests.log; do
  if [[ -f "$report" && ! -L "$report" ]]; then
    cp "$report" "$EVIDENCE_DIR/$(basename "$report")"
    chmod 600 "$EVIDENCE_DIR/$(basename "$report")"
    shasum -a 256 "$EVIDENCE_DIR/$(basename "$report")" >> \
      "$EVIDENCE_DIR/native-reports.sha256"
  else
    printf 'missing_or_unsafe_report=%s\n' "$report" >> \
      "$EVIDENCE_DIR/native-reports-missing.txt"
  fi
done
if [[ -f "$EVIDENCE_DIR/native-reports-missing.txt" ]]; then
  chmod 600 "$EVIDENCE_DIR/native-reports-missing.txt"
fi

test "$(git rev-parse HEAD)" = "$SOURCE_SHA"
git status --porcelain=v1 --untracked-files=all > "$EVIDENCE_DIR/source-status-after.txt"
chmod 600 "$EVIDENCE_DIR/source-status-after.txt"
test "$gate_status" -eq 0
```

The script records separate checksummed logs for the OrbStack E2E and native
desktop gates. `--run` fails if either gate fails. The surrounding commands
still copy Swift reports and capture the resulting checkout status before
returning that failure. The script does not clean or reset generated files;
preserve and report any post-run worktree changes.

## Rust and OrbStack acceptance

The script runs `mbx +1.99.0 xtask ci --e2e`. With no `--only` selection this
also runs the regular CI partitions and feature powerset, then the Docker E2E
partition. That partition checks `docker info`, exports the capsule binary, and
runs the `docker-e2e` nextest profile. The profile selects these five test
binaries: `dind_e2e`, `session_send_e2e`, `usage_broker_e2e`,
`load_options_e2e`, and `multi_account_tabs_e2e` (`.config/nextest.toml:8-16`).

Acceptance requires a nonzero executed-test count for the selected E2E suite,
nonzero passing cases in each of the five selected binaries, and zero failures
or flaky/retried cases. The copied JUnit file must be present, regular, and
non-symlinked; retain its checksum and the E2E log checksum. Confirm in the
report that the usage broker cases ran, including:

- `usage_broker_twenty_host_processes_make_one_provider_call`
- `usage_broker_killed_owner_recovers_once_without_a_herd`
- `usage_broker_desktop_and_twenty_docker_capsules_make_one_provider_call`
- `usage_broker_docker_capsule_cannot_access_another_account_or_global_tree`
- `usage_broker_failure_and_rate_deadline_are_identical_for_all_waiters`
- `usage_broker_unavailable_state_makes_zero_provider_calls`

These E2E tests exercise process and container coordination with fixture
providers and quota data. They are not live Claude quota or authentication
proof. The host gate is bounded to two Cargo build jobs and two nextest test
threads ([nextest concurrency environment](https://nexte.st/docs/configuration/env-vars/));
the repository serializes Docker E2E tests to one thread.

## Native desktop acceptance

The script invokes the pinned Mise executable through MBX, then
`mise -C native run ci`. That task requires all of these steps to pass:

1. `mbx +1.99.0 xtask desktop bindings-check`.
2. `xcodegen generate --spec native/project.yml`.
3. Strict `swift-format lint` on handwritten Swift sources.
4. Strict `swiftlint lint --config native/.swiftlint.yml`.
5. `mbx +1.99.0 xtask desktop test`.
6. `mbx +1.99.0 xtask desktop build`.
7. `mbx +1.99.0 xtask desktop test-swift --jobs 2`.
8. `mbx +1.99.0 xtask desktop verify`.

`desktop test-swift` must report nonzero XCTest and Swift Testing counts with
zero failures; its implementation rejects missing or empty reports. Preserve
the checksummed native log and, outside the checkout, copies of
`native/.build/swift-unit-tests.xml`,
`native/.build/swift-unit-tests-swift-testing.xml`, and
`native/.build/swift-unit-tests.log`. The explicit Xcode selection must remain
Xcode 26.6 throughout the run.

## Real-input observation and stop checks

The Rust E2E suite uses fixtures. Separately, real-input evidence is acceptable
only when an operator-authorized, ordinary Claude Code session emits a genuine
statusline callback through the exact reviewed installation. Do not feed
handwritten JSON to `statusline ingest`, replay a fixture, edit evidence
timestamps, or run a timer to make evidence look fresh. Do not put raw callback
payloads, settings files, credentials, account labels, or session IDs in the
repository or PR. This packet does not authorize `auth prepare`, Keychain access,
binding or collector approval, dispatch-policy approval, or settings writes. If
the real callback integration is not already installed with operator approval,
leave the live-input portion pending; do not install it as part of this gate.

The operator's private evidence summary should record the callback's source,
receipt/evidence times, age and freshness, with account and session identifiers
redacted. Record the five-hour and seven-day quota fields independently: their
used values, reset hints, field freshness, and the monitor's readiness result.
Record the windows the callback actually reports; do not wait for a five-hour
or seven-day period to elapse or infer a reset from elapsed time. The callback
does not authenticate the remote provider account, and reset time alone does
not prove a quota reset.

If an operator-approved monitor already exists, capture its status and stop
result with the documented read/stop commands below, using its existing
identifier. Do not create a monitor or approve a binding/collector to satisfy
this packet:

```bash
jackin usage status --monitor "$MONITOR_ID" --format json
jackin usage monitor stop --monitor "$MONITOR_ID" --format json
jackin usage status --monitor "$MONITOR_ID" --format json
```

Redact the monitor identifier and any account/session data from the shared
summary. If there is no already-approved monitor, leave this live stop check
pending and rely only on the deterministic source regression listed below.

Use the existing contract expectations as the pass criteria:

- An observation-only monitor reports `dispatch: not_authorized` and
  `runnable: false`; do not create or approve a dispatch policy for this check.
- Missing or stale quota evidence blocks readiness. After more than 300 seconds
  without new evidence, verify the status reports stale/unknown fields and
  `runnable: false`. A repeated identical callback does not renew those fields'
  evidence age; a changed sibling does not refresh an unchanged field.
- Stop the exact monitor and verify its returned status is `lifecycle: stopped`
  and `runnable: false`. The collector admission regression requires no new
  provider admission after Stop returns. It permits a request admitted before
  Stop to finish; do not describe Stop as cancelling in-flight work.
- Never infer replenishment from a timer or a reset hint. If real new evidence
  does not arrive, preserve the stale/blocked result and report it.

The existing deterministic source regressions include
`identical_statusline_does_not_refresh_evidence_age_or_decision_sequence`,
`missing_and_stale_quota_fields_remain_unknown_independently`,
`stop_waits_for_collector_admission_and_later_snapshots_exclude_it`,
`ticker_collector_preserves_minimum_attempt_floor_and_retry_after`, and
`a_stale_high_session_does_not_resume_until_lower_usage_confirms_a_new_reset`.
The collector tests use a fake provider; they prove authorization, timing, and
Stop linearization, not real account behavior.

## Evidence handoff and current blocker

Keep the complete evidence directory outside the source checkout with mode
`0700`. The review packet should contain only the exact source SHA/tree, host
tuple, tool versions and binary hashes, commands, start/end times, exit codes,
per-binary E2E counts, native test counts, and hashes of the private logs and
reports. Redact identity and local path data from any shared summary. Do not
claim live-account proof from the fixture gates or publish unredacted evidence.

The prior available host snapshot was macOS 27 with Xcode unavailable, so it
does not meet this gate. Until an operator confirms access to macOS 26 arm64,
Xcode 26.6 / SDK 26.5 / Swift 6.3, and OrbStack, this platform verification is
pending. No native or OrbStack gate is claimed by this packet.

The 300-second freshness contract is intentionally fail-closed: the monitor
may become stale and non-runnable while waiting for a later genuine callback.
This packet does not promise continuous `Current` status or automatic resume.
The corresponding contract says unknown/stale evidence blocks runnable status
and reset hints do not establish replenishment
([usage command contract](../../docs/content/%28public%29/commands/usage.mdx#decisions-and-resets),
[`bootstrap-contract.md`](bootstrap-contract.md#experimental-collector-and-evidence)).
