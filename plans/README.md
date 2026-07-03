# Telemetry Ecosystem Program Archive

This directory is retained as the concise archive for the temporary telemetry implementation program added during PR 717. The detailed numbered execution plans were retired after their dependency-valid work either landed or hit a documented STOP condition; historical details remain in git history starting at `8a4cd329d`.

The findings record that motivated the program remains at `docs/content/docs/reference/research/agent-telemetry/parallax-observability-findings.mdx`. Durable behavior and contributor guidance now live in canonical docs such as `docs/content/docs/reference/runtime/diagnostics.mdx`, `docs/content/docs/(public)/guides/run-telemetry.mdx`, `docs/content/docs/(public)/guides/environment-variables.mdx`, `TESTING.md`, `ENGINEERING.md`, and `AGENTS.md`.

## Status

| Plan | Outcome | Evidence |
|---|---|---|
| 001 In-memory OTLP export seam | Done | `5faf10324 test(diagnostics): add in-memory OTLP export seam` |
| 002 OTLP target allowlist | Done | `942657a5e fix(diagnostics): allowlist OTLP export targets` |
| 003 Truthful severity and error typing | Done | `14ad8c43b fix(diagnostics): export failures at error severity` |
| 004 Route direct events through tracing | Done | `fe08cbf2d fix(diagnostics): route direct events through tracing` |
| 005 Payload containment and redaction boundary | Blocked | STOP: `jackin-xtask pty-fixture` reads `session feed_pty bytes` from host run JSONL, so removing/localizing that payload stream would break fixture extraction without a replacement contract. |
| 006 Structured event taxonomy | Blocked | Depends on the blocked 005 payload boundary. |
| 007 Real launch spans and subprocess coverage | Done | `d45e9559c feat(diagnostics): make launch spans cover work` |
| 008 Telemetry level and category controls | Blocked | Depends on blocked 005/006 telemetry structure. |
| 009 Honest diagnostics-file operator contract | Done | `18c9e6622 fix(diagnostics): hide nonpersisted run paths` |
| 010 `[telemetry]` config schema | Blocked | Depends on blocked 008 and its telemetry-level model. |
| 011 Telemetry hygiene batch | Done | `a14c138ec`, `a93fe1e63`, `48cf955f5`, `fc1d01793`, `72d26c86d`, `afeb05713`, `f71b99bbf` |
| 012 Domain metrics and turso reuse | Blocked | Depends on blocked 005/008. |
| 013 Docs truth sync | Done | `12efe33df docs: sync telemetry diagnostics contract` |

## Remaining Blocker

The open blocker is the fixture extraction contract: `crates/jackin-xtask/src/pty_fixture.rs` consumes raw `session feed_pty bytes` records from host run JSONL. Plan 005 cannot safely move PTY/frame/keystroke payloads to local-only capsule logs until fixture extraction has an alternate source, likely the capsule `multiplexer.log` path already recorded in run diagnostics.

Because 006, 008, 010, and 012 were designed to build on 005's payload boundary, they are intentionally not implemented in this PR. Treat any future continuation as a new scoped design starting from the fixture-contract blocker, not by reviving the retired step-by-step plans.
