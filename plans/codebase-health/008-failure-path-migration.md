# Plan 008: Migrate failure-prone HTTP / Docker / attach / cleanup / process paths to typed telemetry

> **Executor instructions**: Follow step by step; verify each step; STOP conditions binding. Update status row in `plans/codebase-health/README.md` when done.
>
> **Drift check (run first)**: `git diff --stat 846038946..HEAD -- crates/jackin-capsule/src/attach_protocol.rs crates/jackin-usage/src/usage/ crates/jackin-docker/src/ crates/jackin-image/src/agent_binary.rs`
> Mismatch with "Current state" = STOP. Requires plans 001 (registry, `operation_error` signature) and 004 (capsule bridge) landed.

## Status

- **Priority**: P1
- **Effort**: L
- **Risk**: MED
- **Depends on**: plans/codebase-health/001-telemetry-event-registry.md, plans/codebase-health/004-capsule-bridge-prefix-free.md
- **Category**: tech-debt (telemetry contract)
- **Planned at**: commit `846038946`, 2026-07-14

## Why this matters

Roadmap Telemetry item 6: "Migrate failure-prone HTTP, Docker, attach, cleanup, and process paths from free-text macro logging; use stable error types that exclude volatile identifiers from grouping." Today attach failures are free-text `cerror!` strings, provider HTTP calls carry no operation span and log failures only via `cdebug!`, and the typed facade is adopted at exactly one choke point (`ShellRunner`). Failures therefore have no `error.type`, no `event.outcome=failure`, no failure metric, and no stable fingerprint — run/container/path variation splits what should be one error class. This is the largest call-site migration of the telemetry program; it is deliberately scoped to the failure-prone paths the roadmap enumerates, not all ~250 breadcrumb sites.

## Current state

- Sole facade adoption: `crates/jackin-docker/src/shell_runner.rs:98,259,394` (`enter_operation` / `operation_error` / `operation_record_exit_code`); span name `process.execute`.
- Attach failures free-text: `crates/jackin-capsule/src/attach_protocol.rs:302-333` — `cerror!("attach client: socket read failed: {e}")`, EOF and write-failure siblings.
- Provider HTTP without instrumentation: `crates/jackin-usage/src/usage/codex.rs:793` (`.inspect_err(|error| crate::cdebug!("codex account/read RPC failed: {error}"))`); `crates/jackin-usage/src/usage.rs:1252-1275`, `usage/amp.rs:216`, `usage/grok.rs:452` build `reqwest` clients with no operation span. `crates/jackin-image/src/agent_binary.rs` downloads over HTTP (find sites: `grep -n "reqwest\|http" crates/jackin-image/src/agent_binary.rs | head`).
- Docker lifecycle rides the JSONL taxonomy path (`run.container_started`/`container_exited`/`docker_build_step` kinds through `run.rs`), not the facade.
- Facade API after plan 001: `operation_error(event_name, error_type, body, attrs)` sets `error.type`, span status ERROR, failure metric (`incr_errors` carries `error.type` dimension — `metrics.rs`).
- Contract fingerprint rule: dynamic values (paths, container ids, URLs, session numbers) are evidence attributes only — never in `error.type`, event name, or body-fingerprint inputs.

## Commands you will need

| Purpose | Command | Expected |
|---|---|---|
| Per-crate tests | `cargo nextest run -p jackin-capsule -p jackin-usage -p jackin-docker -p jackin-image --all-features` | pass |
| Lint | `cargo clippy --workspace --all-targets --all-features --locked -- -D warnings` | exit 0 |
| Cross-crate | `cargo xtask ci --fast` | exit 0 |

## Scope

**In scope**: attach protocol failure sites (`attach_protocol.rs`), provider HTTP request paths in `jackin-usage` (`usage.rs`, `usage/codex.rs`, `usage/amp.rs`, `usage/grok.rs`, plus any peer provider modules found by `grep -rln "reqwest" crates/jackin-usage/src`), agent-binary download path (`jackin-image/src/agent_binary.rs`), Docker lifecycle emit sites in `run.rs` consumers (launch runtime container start/exit/inspect calls — enumerate via `grep -rn "container_started\|container_exited\|docker_build_step" crates --include='*.rs' | grep -v tests`), cleanup/teardown error paths in `crates/jackin-runtime/src/runtime/launch/load_cleanup.rs`. New registry defs for each error class.

**Out of scope**: success-path breadcrumb migration (stays on macros per the two-tier contract); the host-to-capsule integration test (plan 009); shared HTTP transport design (plan 018 covers command transport only; HTTP stays per-crate for now).

## Git workflow

Branch `refactor/telemetry-failure-paths`; Conventional Commits; `git commit -s`; push per commit. This plan is large — commit per area (attach, usage-HTTP, image-HTTP, docker-lifecycle, cleanup) so review is sliceable.

## Steps

### Step 1: Error-type census + registry defs

For each area, define stable `error.type` consts in the plan-001 registry (low-cardinality machine classes): attach — `attach_socket_eof`, `attach_socket_read_failed`, `attach_socket_write_failed`; usage HTTP — `usage_http_request_failed`, `usage_http_status`, `usage_rpc_failed`; image download — `agent_binary_download_failed`, `agent_binary_checksum_mismatch` (match actual failure modes read from the code, don't invent); docker lifecycle — `docker_run_failed`, `docker_inspect_failed`, `docker_wait_failed`; cleanup — `cleanup_teardown_failed`. Registry defs declare fingerprint inputs = `error.type` + operation only.

**Verify**: registry tests pass with new defs; `cargo nextest run -p jackin-diagnostics --all-features` → pass.

### Step 2: Attach path

Wrap the attach client/server session I/O in an operation span (`capsule.attach`); on failure call `operation_error` with the registered class; keep the human breadcrumb via the (post-004) render-only macro so operator-visible file text is unchanged. Expected detach is NOT an error: route it through the `expected_close` outcome event (registry def from plan 001).

**Verify**: `cargo nextest run -p jackin-capsule` → pass; new test asserts a forced socket-read failure yields exactly one ERROR record with `error.type = attach_socket_eof`-class value and span status ERROR, and message-text variation (different session ids) does not change the exported `error.type`/event name.

### Step 3: Usage HTTP

Wrap each provider fetch (`usage.rs:1252` client region, `codex.rs`, `amp.rs`, `grok.rs`) in `enter_operation` with a registered per-provider-agnostic operation (`usage.refresh` — provider goes in the plan-007 feature-decision dimension or a bounded `usage.provider` evidence attr IF the registry privacy review allows; if plan 007 removed provider identity from generic telemetry, use the bounded evidence attr form it defined — check the registry). Failures → `operation_error` with registered class; URLs never exported (evidence rule: host-only or omitted).

**Verify**: `cargo nextest run -p jackin-usage --all-features` → pass; test asserts failure record carries `error.type` and no URL/query text in body or attrs.

### Step 4: Image download + Docker lifecycle + cleanup

Same pattern: operation span + registered `error.type` on failure; Docker lifecycle events move from raw JSONL kinds to registry-validated emissions (the kinds already exist — ensure they now flow with outcome/error.type through the plan-001 path); `load_cleanup.rs` teardown `?` failures emit `cleanup_teardown_failed` before propagating.

**Verify**: `cargo nextest run -p jackin-image -p jackin-runtime -p jackin-docker` → pass.

### Step 5: Gates + fingerprint stability test

Add one cross-area test (diagnostics crate): same error class emitted twice with different volatile evidence (different container name/path) produces identical `event.name` + `error.type` pairs. Run full gates.

**Verify**: `cargo clippy --workspace --all-targets --all-features --locked -- -D warnings` → exit 0; `cargo xtask ci --fast` → exit 0.

## Test plan

Per-area failure-injection tests (attach socket, HTTP error status via existing test servers/fixtures in jackin-usage tests — find the current HTTP test pattern with `grep -rn "mock\|test.server\|wiremock" crates/jackin-usage/src --include='*.rs' | head`), fingerprint-stability test, expected-detach non-error test.

## Done criteria

- [x] `grep -rn "cerror!" crates/jackin-capsule/src/attach_protocol.rs` → remaining sites are render-only breadcrumbs paired with `operation_error` (or gone)
- [x] Every enumerated failure path exports registered `error.type` + `event.outcome=failure` + span status ERROR (tests prove per area)
- [x] Volatile-identifier variation does not split fingerprints (test-proven)
- [x] Expected detach → `expected_close`, no error.type
- [x] `cargo xtask ci --fast` exits 0; status row updated

## STOP conditions

- Drift vs excerpts; or plan 001/004 signatures differ from what this plan assumes (re-read their landed shape first — they are authoritative over this plan's sketches).
- An area has no existing test seam to force failures (e.g. no mockable HTTP in a provider module) — building a new mock layer is a scope call; report instead of improvising one.
- Migration surfaces >2 new arch-tier violations (`cargo xtask lint arch --strict`) — the dependency shape needs operator review.

## Maintenance notes

- New failure paths must land with a registered error class; reviewers reject free-text-only failure logging.
- Plan 009's matrix asserts the one-ERROR-per-failure and fingerprint-stability invariants continuously.
- Success-path breadcrumb migration (the remaining ~200 macro sites) is deliberately deferred; revisit only after the conformance matrix is green and stable.

## Execution notes

- Attach socket/decode/write failures emit operation_error with stable error.types; expected EOF detach stays non-failure.
- Usage HTTP/RPC, image resolution/download/checksum, Docker run/inspect/wait, cleanup teardown, process spawn, and attach I/O classes are covered by one exporter-backed failure census.

**Completed 2026-07-15**: all enumerated failure-prone boundaries emit registered, stable error classifications; expected detach remains non-failure and exporter tests cover fingerprint stability. Fast CI passes and the index reflects the completed state.
