# Plan 005: Versioned JSONL adapter — canonical keys, prohibited-key negative tests

> **Executor instructions**: Follow step by step; verify each step; STOP conditions binding. Update status row in `plans/codebase-health/README.md` when done.
>
> **Drift check (run first)**: `git diff --stat 846038946..HEAD -- crates/jackin-diagnostics/src/run.rs crates/jackin-diagnostics/src/summary.rs crates/jackin-diagnostics/src/observability.rs`
> Mismatch with "Current state" = STOP. Requires plan 001 (canonical spellings) landed.

## Status

- **Priority**: P1
- **Effort**: M
- **Risk**: MED
- **Depends on**: plans/codebase-health/001-telemetry-event-registry.md
- **Category**: tech-debt (telemetry contract)
- **Planned at**: commit `846038946`, 2026-07-14

## Why this matters

The contract's migration clause: "Migrate existing JSONL deliberately through a versioned adapter: map `kind`, `stage`, `detail`, `error_type`, and `log.category` to the canonical schema, then test that OTLP and canonical-schema JSONL records contain none of those prohibited keys." Today `JsonEvent` still serializes top-level `kind`, `stage`, `detail`, `run_id` beside the canonical fields, the schema has no version marker, and the existing tests assert prohibited keys are PRESENT — so a regression reintroducing them can never be caught, and downstream consumers (summaries, pty-fixture extraction) key on legacy names indefinitely.

## Current state

- `crates/jackin-diagnostics/src/run.rs:131-159` — `JsonEvent` struct serializes `kind`, `stage`, `detail`, `run_id` plus canonical `event.name`/`event.outcome`/`jackin.*` fields; no `schema` version field. (Read the full struct + its serde attrs before editing; ~50KB file.)
- Consumers that parse the JSONL: `crates/jackin-diagnostics/src/summary.rs` (`summarize_run_file`/`summarize_reader`), `crates/jackin-xtask/src/pty_fixture.rs` (extracts session bytes from run JSONL), plus test fixtures. Grep before editing: `grep -rn '"kind"' crates --include='*.rs' | grep -v target` to enumerate every reader.
- Historical contract: `correlation_ids` (`observability.rs:1511-1519`) documents run-id-as-trace-id fallback "for offline file-only mode and historical fixtures" — the adapter must keep reading old files.
- Tests currently asserting prohibited keys present: `observability/otlp/tests.rs:220-224` (post-001 these may already be partially updated — reconcile with live code).

## Commands you will need

| Purpose | Command | Expected |
|---|---|---|
| Diagnostics | `cargo nextest run -p jackin-diagnostics --all-features` | pass |
| xtask (pty fixture) | `cargo nextest run -p jackin-xtask` | pass |
| Lint | `cargo clippy -p jackin-diagnostics -p jackin-xtask --all-targets --all-features -- -D warnings` | exit 0 |
| Cross-crate | `cargo xtask ci --fast` | exit 0 |

## Scope

**In scope**: `crates/jackin-diagnostics/src/run.rs` (+ tests), `summary.rs` (+ tests), `observability.rs` emit path where it still forwards legacy keys, new `run/jsonl_adapter.rs` (or similarly named self-named module) + tests, `crates/jackin-xtask/src/pty_fixture.rs` reader update, test fixtures under the diagnostics crate.

**Out of scope**: OTLP export attributes (001/003 own those), capsule file format (004), UI artifact paths.

## Git workflow

Branch `refactor/jsonl-versioned-adapter`; Conventional Commits; `git commit -s`; push per commit.

## Steps

### Step 1: Version the schema

Add `schema: u32` (serde field `"schema"`) to `JsonEvent`; current canonical shape = `2`. Absent field ⇒ version 1 (legacy). Writer always writes 2.

**Verify**: `cargo nextest run -p jackin-diagnostics -E 'test(/run::tests/)'` → pass with updated fixtures.

### Step 2: Canonicalize the written record

Version-2 records serialize canonical keys only: `event.name`, `event.outcome`, `jackin.component`, `jackin.operation`, `jackin.category`, `jackin.stage`, `error.type`, `parallax.run.id`, `session.id` where applicable, plus timestamp/severity/body/trace fields. Remove top-level `kind`, `stage`, `detail`, `error_type`, `run_id` from the v2 writer (`run_id` value now lives in `parallax.run.id`; `kind`'s information lives in `event.name`).

**Verify**: write one run file in a test, parse as JSON, assert none of the five prohibited keys present and `schema == 2`.

### Step 3: Reader adapter

New module `run/jsonl_adapter.rs`: `fn canonicalize(line: &serde_json::Value) -> CanonicalEvent` mapping v1 → v2 (`kind` → dotted `event.name` via the plan-001 registry's kind→def map; `stage`→`jackin.stage`; `detail` folded into a bounded evidence attr; `error_type`→`error.type`; `run_id`→`parallax.run.id`; `expected_shutdown`→`expected_close`). Route `summary.rs` and `pty_fixture.rs` reads through it so historical fixtures keep working.

**Verify**: `cargo nextest run -p jackin-diagnostics -p jackin-xtask` → pass, including existing historical-fixture tests unmodified.

### Step 4: Negative tests both sinks

Add: (a) JSONL v2 writer emits none of `kind|stage|detail|error_type|log.category|run_id` as top-level keys (parse + assert); (b) OTLP captured records contain none of those as attribute keys (extend the plan-001 exporter sweep). Flip any remaining test asserting presence.

**Verify**: `cargo nextest run -p jackin-diagnostics --all-features` → pass; `cargo xtask ci --fast` → exit 0.

## Test plan

Adapter unit tests (v1 fixture lines → canonical), writer negative test, OTLP negative sweep, summary/pty-fixture regression via existing suites. Model fixture handling on existing tests in `run/tests.rs` and `summary/tests.rs`.

## Done criteria

- [x] v2 JSONL contains no prohibited keys (test-proven) and carries `schema: 2`
- [x] v1 fixtures still summarize/extract correctly through the adapter
- [x] OTLP negative sweep green
- [x] `cargo xtask ci --fast` exits 0; status row updated

## STOP conditions

- Drift vs excerpts; or `JsonEvent` shape differs materially from `run.rs:131-159` description.
- An external consumer outside this repo demonstrably parses the v1 keys from live run files (search docs for a documented JSONL contract; `docs/content/docs/reference/` telemetry pages) — surface the compatibility question instead of silently breaking it.
- Registry lacks a def for some legacy `kind` — extend plan-001 registry first (in-scope for 001, not here) and STOP if that means editing files this plan doesn't own.

## Maintenance notes

- The adapter is the ONLY place legacy spellings may appear; reviewers reject new writers of `kind`/`error_type`.
- Plan 009 matrix asserts prohibited-key absence continuously.
- When pre-1.0 fixture corpora are eventually regenerated to v2, the v1 arm can be considered for deletion (pre-release rules allow breaking, but the roadmap explicitly wants the versioned adapter step).

## Execution notes

- Writer schema=2 with jackin.detail; OTLP emit no longer stamps `kind`.
