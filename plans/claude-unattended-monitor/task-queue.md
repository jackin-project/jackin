# Claude unattended usage monitor

## Safety boundary

Work starts read-only. Implementation and verification use isolated state and
fixtures. Do not access live provider endpoints or Keychain, invoke Claude,
modify Claude auth/settings, kill sessions, or mutate their worktrees.
All delegated work uses GPT-6-Luna at max reasoning.

## Queue

- [done] Research shipped provider/auth, broker/coordinator, CLI/consumers,
  and official statusline contracts independently.
- [done] Freeze command names, observation/policy/spend contracts, and file
  ownership before parallel implementation.
- [in progress] Implement broker-owned noninteractive auth and bounded optional HTTP.
- [in progress] Implement bounded statusline ingress and explicit composition setup.
- [in progress] Implement durable observations, monitors, decisions, spend baselines,
  reset waits, and passive status/readiness APIs.
- [in progress] Migrate CLI and affected consumers; remove superseded paths.
- [pending] Run deterministic offline fake clock/Keychain/HTTP coverage and
  consumer regressions; independent security/rate-limit reviews.
- [pending] Build an isolated binary, verify exact commands and JSON/exit schema,
  document evidence and limitations, commit and push checkpoints.
- [pending] Produce ready-to-paste Claude Code handoff from verified commands.

## Acceptance evidence

Record checks, request counts, no-dialog evidence, commit SHAs, and actual binary
paths here as they become available. Historical plans are not implementation proof.

## Implementation boundary and contract

Implementation checkout: `/tmp/jackin-claude-monitor` (branch
`claude-unattended-monitor`, base `ff9eb01f`). The original checkout is clean;
a read-only process check found Claude running, so all edits/builds remain in
the isolated checkout. No live auth, Keychain or provider checks are authorized.

Frozen commands: `usage auth prepare`, `usage service start|stop|status`,
`usage doctor --provider claude --unattended`, `usage monitor start|stop`,
`usage status`, `usage refresh`, `usage watch`, `usage wait --until runnable`,
`usage statusline ingest|compose`, `usage spend record`. Monitoring commands
select a stable monitor ID; start selects account and goal IDs. Usage-level
`--data-dir` provides isolated state. JSON status/readiness and JSONL watch
are separate formats. Superseded host snapshot/projection commands are removed.

Independent OAuth refresh is disabled for durable monitors. `refresh` is
local reconciliation, never a forced HTTP call. Existing broker provider work
is hardened with a persisted Claude attempt floor of 300 seconds and positive
backoff; this does not guarantee avoidance of provider bans.

Official Claude Code docs: <https://code.claude.com/docs/en/statusline>.
`rate_limits.five_hour` and `.seven_day` each optionally carry
`used_percentage` and `resets_at`. Introduced in official v2.1.80 changelog.
These fields do not identify the account or provide an observation timestamp.
Integer basis points in the monitor protocol have explicit names (9000=90%).
Repeated identical callbacks retain first evidence receipt age. Missing/stale
fields remain unknown. Session list-price cost is not billing evidence.

## Ownership

- broker_research: new monitor protocol DTOs/tests and module export.
- provider_research: Claude provider/auth and narrow discovery/facade callers.
- coordinator_rate_limits: coordinator admission/backoff/recovery/tests.
- monitor_engine: new broker monitor store/engine, delegates spend/policy modules.
- broker_integration: socket/lifecycle/client integration and relay rejection.
- consumer_tests_research: CLI, bootstrap bypass, local-only broker binary.
- statusline_docs: composition helper and official input contract documentation.
- security_review: independent read-only security review.

## Verified so far

- `cargo check --offline -p jackin-protocol`: passed.
- Protocol monitor contract fixtures: 4 tests passed (agent report).
- Original worktree status after moving our newly created files: clean.
