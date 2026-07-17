# Plan 005: Async ownership helpers — joined/detached/cycle/stream spawn wrappers and the spawn lint

> **Executor instructions**: Follow this plan step by step. Run every
> verification command and confirm the expected result before moving to the
> next step. If anything in the "STOP conditions" section occurs, stop and
> report — do not improvise. When done, update the status row for this plan
> in `plans/unified-otel-observability/README.md`.
>
> **Drift check (run first)**: `git diff --stat fa8194882..HEAD -- crates/jackin-telemetry crates/jackin-runtime/src/runtime/prewarm_trigger.rs crates/jackin-capsule/src/daemon crates/jackin-capsule/src/session.rs crates/jackin-xtask/src`
> If any in-scope file changed since this plan was written, compare the
> "Current state" excerpts against the live code before proceeding; on a
> mismatch, treat it as a STOP condition.

## Status

- **Priority**: P1
- **Effort**: M
- **Risk**: MED (touches every production spawn site; wrappers are behavior-preserving pass-throughs plus context handling)
- **Depends on**: plans/unified-otel-observability/004-telemetry-facade-api.md
- **Roadmap item**: [Unified OpenTelemetry observability](../../docs/content/docs/roadmap/unified-otel-observability.mdx) (`docs/content/docs/roadmap/unified-otel-observability.mdx`) — this plan implements "Async and cross-process context" (in-process half: helpers + ownership lint; the wire half is plan 006); the roadmap item is the binding contract and overrides this plan on any conflict.
- **Planned at**: commit `fa8194882`, 2026-07-15

## Why this matters

The contract: no `Span::enter()`/`.entered()`/OTel `ContextGuard` across `.await`; joined tasks use an instrumented child with `or_current()`; detached work creates a root with a link; periodic cycles and streams declare ownership explicitly; helpers cover Tokio spawn/handles/`JoinSet`, blocking/local tasks, threads, TUI subscriptions, socket handlers, timers, and streams; architecture lint requires each production spawn to declare joined, detached, cycle, or stream ownership. Today the workspace has 11 production `tokio::spawn`, 33 `spawn_blocking`, 5 `JoinSet`, ~17 `std::thread::spawn` sites with no shared context discipline — detached tasks re-read a global (`jackin_diagnostics::active_run()`) inside the task, and pipe-reader threads run span-less. This plan builds the helper family and the lint; it migrates the spawn sites mechanically (ownership classification only — richer instrumentation of what runs inside them comes in plans 008–010).

## Current state

(verified at planning commit; full inventory in the table below)

- Positive baseline: **no production span guard is held across `.await` today.** The two `.enter()` sites are synchronous (`crates/jackin-diagnostics/src/run.rs:470` wraps sync JSONL emission; `crates/jackin-diagnostics/src/observability.rs:770` is a tokio-runtime enter, not a span). The documented convention exists at `crates/jackin-diagnostics/src/screen.rs:17`.
- The one correct async attach: `crates/jackin-diagnostics/src/screen.rs:277` — `fut.instrument(span).await`.
- Detached-context seam (the bug class to fix): e.g. `crates/jackin-runtime/src/runtime/prewarm_trigger.rs:112` —

  ```rust
  tokio::spawn(async move {
      if let Some(run) = jackin_diagnostics::active_run() {
          run.stage("background_image_prewarm_started", …);
      }
      …
  });
  ```

  Context is re-fetched from a global inside the task instead of captured/linked at spawn.
- Existing shared wrapper to preserve/extend: `jackin_tui::runtime::spawn_blocking_subscription` (`crates/jackin-tui/src/runtime.rs:78-104`) — TUI subscription offload; and `Multiplexer::spawn_context_lookup` (`crates/jackin-capsule/src/daemon/context_mgmt.rs:143-174`).
- Production spawn inventory (classification target; regenerate with the grep in step 4):

| Class | Sites |
|---|---|
| stream | capsule `socket.rs:127` (accept loop), `daemon.rs:1289` (attach client task), runtime `exec_host.rs:63` (host.sock accept); PTY pumps `session.rs:468,521,604` (spawn_blocking); thread pipe-readers: usage `codex.rs:744`, `grok.rs:387`, `format.rs:270-271`, env `op_cli.rs:330,335,443,448`, `host_claude.rs:254`, capsule `pr_context.rs:375`, `util.rs:121`, launch-tui `tui/input.rs:36`, capsule `pid1.rs:95`, `git_context.rs:197` |
| detached | capsule `daemon.rs:1122` (handshake), `daemon/input_dispatch.rs:96`; runtime `image.rs:439,544,675,777`, `prewarm_trigger.rs:112,187`; image `agent_binary.rs:180`; capsule `daemon/context_mgmt.rs:163,222` (fire-and-forget spawn_blocking) |
| cycle | launch-tui `tui/run.rs:92` (33 ms input poll); capsule tickers are select-arms not spawns (instrumented in plan 010) |
| joined | all `JoinSet` sites (`cli/prewarm.rs:306,478,658`, `image.rs:614`, `image/prewarm.rs:77`); awaited `spawn_blocking` (image `capsule_binary.rs:306`, `agent_binary.rs:758-1032` ×8; host `caffeinate.rs:136`, `host_clipboard.rs:191,226`; `jackin` `app.rs:164`, console services/effects ×6; runtime `launch_runtime.rs:173,840`, `launch_pipeline.rs:135,970,988`, `orchestrate.rs:699`; capsule `resource_metrics.rs:40`, `multiplexer_utils.rs:296` — the last two are polled-joined); threads: usage `refresh.rs:60` (worker pool, re-establishes span in-thread), env `op_cli.rs:182`, runtime `git_pull.rs:66`, `jackin` `role_claude_plugins.rs:57`, capsule `runtime_setup.rs:130` |

- OS threads inherit neither the tracing dispatcher nor its current span. Joined helpers must capture both at spawn, install the dispatcher in the new thread, and enter the captured span around the closure so operations created inside the thread are exported and parented correctly.

## Target helper family (in `crates/jackin-telemetry/src/spawn.rs`)

```rust
pub fn spawn_joined<F: Future>(name: &'static str, fut: F) -> JoinHandle<F::Output>;
pub fn spawn_detached<F: Future>(def: &'static SpanDef, fut: F) -> JoinHandle<F::Output>;
pub fn spawn_cycle<F: Future>(…) -> JoinHandle<…>;   // declares cycle ownership; cycle spans come later
pub fn spawn_stream<F: Future>(…) -> JoinHandle<…>;  // declares stream ownership; no lifetime span ever
// plus: joined_blocking / detached_blocking / stream_blocking (spawn_blocking variants),
// thread_joined / thread_stream (std::thread variants), and a JoinSet extension trait
// (spawn_joined_on(&mut JoinSet, …)).
```

Semantics:
- **joined**: wrap the future/closure in `tracing::Span::current().or_current()` semantics — concretely `fut.instrument(Span::current())`. For OS threads, capture both `tracing::Dispatch` and the current span, install the dispatcher, then enter the span around the moved work. Inherits the owning operation; creates no new span itself.
- **detached**: capture `Span::current().context()` at the spawn site, create a new ROOT span from `def` inside the task with a **span link** to the captured context (≤ 8 links, plan 004 guard API), so detached work is a linked root, never a child.
- **cycle** / **stream**: no lifetime span; they only declare ownership (a `&'static str` name recorded as a field on an optional DEBUG event) and give plans 008–010 the hook point where per-cycle `background.cycle` roots and per-attempt `connection.attempt` roots get created. Streams must never create a span per frame/byte.
- All helpers are no-op-cheap when telemetry is disabled (delegate straight to `tokio::spawn` etc.).

## Commands you will need

| Purpose | Command | Expected on success |
|---|---|---|
| Crate tests | `cargo nextest run -p jackin-telemetry --locked` | all pass |
| Workspace | `cargo nextest run --workspace --all-features --locked` | all pass |
| Spawn lint | `cargo xtask telemetry-registry` | exit 0 |
| Lint lane | `cargo xtask ci --only lint` | exit 0 |

## Scope

**In scope:**
- `crates/jackin-telemetry/src/spawn.rs` (+ `spawn/tests.rs`)
- `crates/jackin-xtask/src/telemetry_registry.rs` — spawn-ownership lint (below)
- Mechanical migration of every production spawn site in the inventory table to the matching helper (crates: `jackin`, `jackin-runtime`, `jackin-capsule`, `jackin-image`, `jackin-host`, `jackin-usage`, `jackin-env`, `jackin-launch-tui`, `jackin-console`). `jackin-tui`'s `spawn_blocking_subscription` internals switch to the helper; its public API stays (it already IS a declared-joined wrapper). Add `jackin-telemetry` as a dependency to each of these crates' `Cargo.toml`.
- `crates/jackin-xtask/src/arch.rs` — no change needed (jackin-telemetry is T0; all these crates are higher tiers).

**Out of scope:**
- Adding cycle/attempt/job spans inside the spawned work (plans 008–010).
- The wire propagation carrier (plan 006).
- `jackin-dev`, `jackin-xtask`, `jackin-pr-trailers`, lookbook, test files, benches.

## Git workflow

- Branch: `feature/unified-otel-observability` — single branch, single PR for the whole roadmap item (plans 001–015 together); no per-plan branch or separate PR. Conventional Commits, e.g. `feat(telemetry): spawn ownership helpers and lint`. Sign `-s`, push after every commit (repo hard rule). Suggest one commit for the helpers + lint, then one per crate migrated, so the single-PR history stays tractable.

## Steps

### Step 1: Implement the helpers

Write `spawn.rs` per the semantics above. Runtime notes that matter: the capsule runs a **current-thread** tokio main (`crates/jackin-capsule/src/main.rs:24`), so helpers must not require a multi-thread runtime; `spawn_blocking` variants must work from both runtime flavors; thread variants must not assume a runtime exists (mirror the fallback in `context_mgmt.rs:166-171`).

**Verify**: `cargo nextest run -p jackin-telemetry --locked -E 'test(spawn)'` → tests pass (see Test plan).

### Step 2: Spawn-ownership lint

Extend `cargo xtask telemetry-registry`: scan production sources for raw `tokio::spawn(`, `tokio::task::spawn_blocking(`, `Handle::spawn_blocking`, `tokio::task::spawn_local(`, `std::thread::spawn(`, `JoinSet::spawn` — fail unless the call is inside `crates/jackin-telemetry/src/spawn.rs` (or a seeded shrink-only allowlist row). Seed the allowlist with the full inventory table above, then drain it in steps 3–4 to empty (target: allowlist empty at plan completion — leave rows only where a genuine third-party constraint exists, each with a reason string).

**Verify**: `cargo xtask telemetry-registry` → exit 0 with seeded allowlist; a synthetic raw `tokio::spawn` in a production file fails the lane; revert.

### Step 3: Migrate detached + cycle + stream sites

Swap each inventoried site to its helper. For today's detached sites, the linked-root `def` should be a placeholder DEBUG-level span def per site family (e.g. `background.work` seed def in the schema registry) — plans 008–010 replace these with the real `background.cycle`/job/attempt defs; the point here is ownership + link correctness, not final naming. Keep exact runtime behavior: same spawn flavor, same handles returned, same abort semantics (e.g. `daemon.rs:1289` stores the handle in `client_registry.attached_task` and aborts it — the helper must return a compatible `JoinHandle`).

**Verify**: `cargo nextest run --workspace --all-features --locked` → all pass; `cargo xtask telemetry-registry` allowlist shrunk accordingly.

### Step 4: Migrate joined sites + census check

Swap joined sites (`spawn_joined`/`joined_blocking`/JoinSet trait). Then regenerate the census and confirm zero raw spawns remain outside the helper module:

```
grep -rn "tokio::spawn(\|spawn_blocking(\|thread::spawn(" crates/ --include='*.rs' \
  | grep -v "jackin-telemetry/src/spawn\|/tests.rs\|/benches/\|jackin-xtask\|jackin-dev\|jackin-pr-trailers\|jackin-tui-lookbook\|jackin-lints"
```

**Verify**: the grep returns no matches (or only reasoned allowlist rows that also appear in the lint config); workspace tests pass.

### Step 5: Guard-across-await lint (cheap regression net)

Add to the lint lane a textual heuristic: flag `.enter()`/`.entered()` appearing in any `async fn`/`async move` block within production crates (except `jackin-diagnostics/src/run.rs:470` region until plan 013 removes it — allowlist it). This is a coarse net — the real protection is the helper API — but it catches the classic regression cheaply.

**Verify**: synthetic `let _g = span.enter();` inside an async fn in a product crate fails the lane; revert.

## Reopened audit additions (2026-07-16)

- Cycle/stream helpers declare ownership only and must not instrument the loop/stream/thread lifetime with the caller span; bounded attempt/cycle spans live inside task bodies.
- Replace the CLI command `Span::enter()` held across awaited dispatch. Add a syntax-aware async-scope lint for `.enter()`, `.entered()`, and OpenTelemetry context guards, with explicit safe synchronous/runtime-guard cases.
- Complete the executor matrix for runtime `Handle`, blocking/local tasks, `LocalSet`, joined/detached `JoinSet`, named/scoped OS threads, subscriptions, timers, cycles, and streams. Classify and migrate all scoped-thread sites or record a narrow tier-architecture exemption.
- Replace substring spawn linting with syntax-aware coverage for Tokio/task/local/Handle/JoinSet, thread builder/scoped/imported/aliased forms and whitespace variants, while excluding subprocess `Command::spawn`.
- Detached helpers are outcome-aware for success/failure/error/timeout, panic, and abort instead of completing every returned future as success. Preserve names and generic outputs.
- Detached completion now automatically emits one bodyless `error.typed` event for failure/error/timeout and panic while completing the owning span with the same bounded `error.type`; callers do not repeat error-recording code.
- Tests prove parent identity, detached root plus one link, unsampled/invalid/disabled behavior, no lifetime retention, all helper families, panic/abort outcomes, and disabled-path allocation behavior.
- A production Git-pull exporter test proved that span-only OS-thread propagation disabled spans created inside the worker. Joined named and unnamed thread helpers now propagate both dispatcher and span; the subprocess failure is exported with its owning context and without repository/program paths.

## Test plan

In `crates/jackin-telemetry/src/spawn/tests.rs` (tokio multi-thread AND current-thread variants where relevant):
- joined: task body sees the caller's span as current (`Span::current()` id equality via a test subscriber).
- detached: task span is a root (no parent) and carries exactly one link to the spawner's span context.
- disabled: helpers still execute the work and return handles (functional no-op path).
- thread variants: span captured across the OS-thread boundary.
- Abort compatibility: `JoinHandle::abort` works through the wrapper.
- Existing behavior nets: the workspace suite (notably `jackin-capsule` daemon/session tests and `jackin-runtime` launch tests) must stay green — they are the regression harness for the mechanical swap.

## Done criteria

- [ ] `cargo nextest run --workspace --all-features --locked` exits 0
- [ ] Census grep (step 4) returns no unmanaged spawn sites
- [ ] `cargo xtask telemetry-registry` exits 0; spawn-ownership + guard-across-await checks active
- [ ] Detached-link test passes
- [ ] `plans/unified-otel-observability/README.md` status row updated

## STOP conditions

Stop and report back (do not improvise) if:
- Any spawn site's behavior depends on NOT inheriting context (e.g. a deliberate detach from a cancelled scope) in a way the helper breaks — report the site instead of special-casing silently.
- The current-thread capsule runtime deadlocks under an instrumented wrapper (would indicate an accidental block_on in the helper).
- A crate would need a dependency edge that violates the arch tiers (all listed crates are ≥ T3 and jackin-telemetry is T0 — if `cargo xtask lint --strict` disagrees, stop).

## Maintenance notes

- Every future spawn goes through these helpers; the lint makes that structural. Reviewer focus: the detached-link capture happens at the SPAWN SITE (before `move`), not inside the task.
- Plans 008–010 replace placeholder defs with real span defs — search `background.work` placeholders.
- The `spawn_cycle`/`spawn_stream` declarations are also the inventory for plan 014's "no lifetime trace" conformance sweep.
