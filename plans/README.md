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
| 005 Payload containment and redaction boundary | Done | `pty-fixture` now follows the `container_started.detail.capsule_log` pointer to raw `multiplexer.log` bytes; raw PTY/frame/input debug lines use local-only capsule logging, and exported diagnostics text is redacted/capped at the host boundary. |
| 006 Structured event taxonomy | Done | Diagnostics JSONL and OTLP records now carry `event.name`, `event.outcome`, `jackin.component`, `jackin.operation`, and `jackin.category` beside the compatibility fields. |
| 007 Real launch spans and subprocess coverage | Done | `d45e9559c feat(diagnostics): make launch spans cover work` |
| 008 Telemetry level and category controls | Done | `JACKIN_TELEMETRY_LEVEL=info|debug|trace` now drives host/capsule OTLP verbosity and debug capture; `JACKIN_TELEMETRY_CATEGORIES` filters debug categories before JSONL/OTLP export. |
| 009 Honest diagnostics-file operator contract | Done | `18c9e6622 fix(diagnostics): hide nonpersisted run paths` |
| 010 `[telemetry]` config schema | Done | `config.toml` now accepts `[telemetry].level` and `[telemetry].categories`, applied before diagnostics startup with env vars taking precedence. |
| 011 Telemetry hygiene batch | Done | `a14c138ec`, `a93fe1e63`, `48cf955f5`, `fc1d01793`, `72d26c86d`, `afeb05713`, `f71b99bbf` |
| 012 Domain metrics and turso reuse | Pending | Depends on a scoped domain-metrics pass. |
| 013 Docs truth sync | Done | `12efe33df docs: sync telemetry diagnostics contract` |

## Remaining Items

The fixture extraction blocker was removed by teaching `crates/jackin-xtask/src/pty_fixture.rs` to read raw `session feed_pty bytes` records from the capsule `multiplexer.log` path already recorded in host run diagnostics. Raw payload debug lines remain available locally for fixture replay, but are no longer bridged into host JSONL or OTLP.

Plan 012 remains pending and should be implemented on this same branch/PR if the telemetry program continues. Treat this archive as the status source of truth; do not revive the retired step-by-step plan files.
