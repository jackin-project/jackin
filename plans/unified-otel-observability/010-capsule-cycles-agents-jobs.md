# Plan 010: Capsule and background work — cycles, agent state, PTY lifecycle, streams, prewarm jobs

> **Executor instructions**: Follow this plan step by step. Run every
> verification command and confirm the expected result before moving to the
> next step. If anything in the "STOP conditions" section occurs, stop and
> report — do not improvise. When done, update the status row for this plan
> in `plans/unified-otel-observability/README.md`.
>
> **Drift check (run first)**: `git diff --stat fa8194882..HEAD -- crates/jackin-capsule/src crates/jackin-runtime/src/runtime/prewarm_trigger.rs crates/jackin-agent-status/src crates/jackin-usage/src/telemetry.rs`
> If any in-scope file changed since this plan was written, compare the
> "Current state" excerpts against the live code before proceeding; on a
> mismatch, treat it as a STOP condition.

## Status

- **Priority**: P1
- **Effort**: L
- **Risk**: MED
- **Depends on**: plans/unified-otel-observability/005-async-spawn-helpers.md, 006-cross-process-propagation.md, 007-identity-lifecycle-roots.md, 009-tui-screens-actions.md (widget event defs)
- **Roadmap item**: [Unified OpenTelemetry observability](../../docs/content/docs/roadmap/unified-otel-observability.mdx) (`docs/content/docs/roadmap/unified-otel-observability.mdx`) — this plan implements the "Product state and health" case table (agent-state transitions, auth provisioning, PTY lifecycle, periodic Capsule work, joined async work, runtime/stream health, token usage), the detached prewarm job rows ("Identity and correlation", `PRODUCER`/`CONSUMER` trace design), Capsule widget navigation, and the Capsule-export-safety paragraph of "Direct OTLP runtime contract"; the roadmap item is the binding contract and overrides this plan on any conflict.
- **Planned at**: commit `fa8194882`, 2026-07-15

## Why this matters

The capsule daemon is the busiest telemetry producer: PTY-hosted agents, periodic reconciliation cycles, byte streams, agent-status arbitration. Today its signals are `clog!`-style prefix lines plus a session-lifetime correlation. The contract turns this into: one `background.cycle` trace per SUBSTANTIVE cycle (no-op ticks metric-only, cycles never get `job.id`), named agent-state transition events only on effective change, auth-provisioning outcome events, PTY spawn/exit events with `gen_ai.conversation.id` + `pty.exit.reason`, throughput/liveness metrics for streams (never per-frame spans), widget focus events for tabs/panes, and `PRODUCER`/`CONSUMER` linked traces with durable `job.id` for the two detached prewarm jobs.

## Current state

(verified at planning commit)

- **Daemon loop** (`crates/jackin-capsule/src/daemon.rs`): startup `run_daemon` `:961` (telemetry init `:977`, logging init `:980`); tickers created `:1010-1012`, `tokio::select!` loop `:1063-1507`. Cycle cadences (constants in `src/tui/subscriptions.rs`): branch context 1 s (`:9`, arm `daemon.rs:1491` → `maybe_spawn_git_branch_context_lookup`, `daemon/context_mgmt.rs:23,31`); state tick 1 s (`:20`, arm `:1332` → `handle_state_tick` `:824-946` containing: resource metrics `:825` (debug-gated, `daemon/resource_metrics.rs:73-74`), PR context `:826` (self-gated 60 s, `subscriptions.rs:15`, runs `gh pr list`), token/provider poll `:865,869` (self-throttled 30/60 s), per-session agent status `:885` with transition `clog!`s `:890-902`); usage account refresh 30 s (`:25`, arm `:1497`, `daemon/multiplexer_utils.rs:276,303`). **`instance_refresh` is host-side** — there is no capsule instance-refresh cycle; the host's periodic instance work lives in the console services (`crates/jackin/src/console/services.rs` subscription refreshes) — instrument the HOST cycle where it actually ticks.
- **Cycle→contract mapping**: `branch_context`, `pr_context`, `usage_account`, `provider_probe` (the token/provider poll), `agent_status` in the capsule; `instance_refresh` on the host console.
- **Agent identity**: `jackin_core::Agent` (`crates/jackin-core/src/agent.rs:23-36`): `Claude, Codex, Amp, Kimi, Opencode, Grok`; slugs `:51-60`. Session stores the slug (`session.rs:135` `agent: Option<String>`; `None` = shell pane).
- **Agent status**: computed in `Session::advance_status_with_process_sampler` (`crates/jackin-capsule/src/session.rs:995-1079`): evidence snapshot `:1038-1048`, `arbitrate()` (`jackin-agent-status/src/arbitrate.rs:18`), `stuck` derived from `EvidenceNote::WatchdogDemoted` `:1052-1055`, debounce `:1061`, `publish_raw` `:1066`. Wire enums: `AgentRawState` (`jackin-protocol/src/agent_status.rs:22`), `AgentStatusConfidence` (`:39`), `AgentStatusSource` (`:53` — `Reported{source_id}`: the source_id is PROHIBITED in telemetry). Contract event: one named event ONLY on effective state change; fields `gen_ai.agent.name`, `agent.state` (`working|blocked|done|idle|unknown`), `agent.status.source` (`none|visible_screen|shell_integration|foreground_process|reported`), `agent.status.confidence`, `agent.status.stuck`; NEVER reporter source id, screen evidence, pgid, tab label, captured grid.
- **PTY lifecycle**: spawn at `Session::spawn` (`session.rs:421` currently calls `record_capsule_activity` — plan 009 stubbed/removed it), sid `:439`, env injection `:440/:1578-1592`; pumps `:468,521,604`; exit classification `child_exit_reason` (`session.rs:1471-1483`): `Ok(success)`→None, signal→"exited after signal {n}", code→"exited with code {n}", `Err`→"wait failed"; consumed `daemon.rs:1378-1420`. Contract mapping to `pty.exit.reason`: clean | signal | nonzero_exit | wait_failed | cancelled (cancelled = operator-driven kill/detach teardown paths). Spawn/exit events carry `gen_ai.agent.name` (when agent pane), `gen_ai.conversation.id`, `pty.exit.reason`, `process.exit.code` when present, `error.type` on failure. `gen_ai.conversation.id`: mint per PTY-hosted agent lifetime at spawn.
- **Streams / throughput today**: `incr_terminal_bytes_received` at `session.rs:1127` (PTY output); `record_frame(bytes, cursor_moves, painted_cells)` at `client_writer.rs:133`; `record_render(elapsed_us, 0)` at `daemon/compositor.rs:86`. Gaps: no input-direction byte counter; no capsule mouse counter. Contract instruments: PTY/terminal bytes (with `stream.direction` = input|output), frame/cursor/mouse counts, render duration/painted cells, connection state, queue depth, process uptime/CPU/memory, Tokio runtime instruments. Payload/identity fields excluded.
- **Capsule widgets**: focus chokepoint `synthesise_focus_swap` (`daemon/pane_layout.rs:416-439` — every tab/pane focus change routes through it); palette open `daemon/multiplexer_utils.rs:30-33`, commands `daemon/input_dispatch.rs:940-1060`. Contract: `app.screen.id=capsule`; `app.widget.name` ∈ {`capsule.tab`, `capsule.pane`, `capsule.command_palette`}; omit dynamic tab labels and numeric pane/session ids; focus = events + duration metrics, never spans.
- **Prewarm jobs** (`crates/jackin-runtime/src/runtime/prewarm_trigger.rs`): image `spawn_background_image_prewarm` `:87` (spawn `:112`, per-target `prewarm_role_images` `:122`, no retry, errors swallowed to stages); sidecar `spawn_background_sidecar_prewarm` `:170` (spawn `:187` → `background_sidecar_prewarm_once` `:218`, dedup via lock `:222` + live-state check `:226`); post-attach replenish `launch_runtime.rs:1186-1191`. Contract: opaque durable `job.id`; `job.type` ∈ {image_prewarm, sidecar_prewarm}; `PRODUCER` scheduling span + separate linked `CONSUMER` attempt trace; shared invocation/session correlation when one exists; skips are `outcome=skip`; periodic cycles never mint job ids.
- **Auth provisioning**: agent auth setup in `crates/jackin-instance/src/auth.rs` (has `#[instrument(skip_all)]` sites at `lib.rs:420,448,627`) and capsule `runtime_setup.rs`. Contract: named outcome event per agent and mode; `gen_ai.agent.name`, `auth.mode` (`sync|api_key|oauth_token|ignore`), `credential.source.type` (`environment|agent_home|onepassword|github_cli|oauth_store|none`), `outcome`, stable `error.type`; no credential name/value/vault id/env value.
- **Token usage**: contract standard `gen_ai.client.token.usage` histogram with `gen_ai.provider.name` + standard token-type attrs when counts are exported; the usage DB stays application state.
- **Capsule export safety**: capsule export requires endpoint/auth classified Capsule-safe by the Parallax launch env; otherwise host-only telemetry + typed-health coverage gap; `network.mode=none` ALWAYS prevents capsule telemetry egress; host headers/client keys never auto-copied into agent-visible capsule env. Today's injection (`launch_runtime.rs:750-766`) copies the endpoint + adds the OTLP host to the firewall allowlist (`:672-678`) unconditionally when set.

## Commands you will need

| Purpose | Command | Expected on success |
|---|---|---|
| Capsule tests | `cargo nextest run -p jackin-capsule --all-features --locked` | all pass |
| Runtime tests | `cargo nextest run -p jackin-runtime --locked` | all pass |
| Conformance | `cargo nextest run -p jackin-diagnostics -p jackin-capsule --all-features --locked -E 'test(/conformance/)'` | all pass |
| Capsule benches | `cargo bench -p jackin-capsule -- --quick` | completes |
| Workspace | `cargo nextest run --workspace --all-features --locked` | all pass |

## Scope

**In scope:**
- `crates/jackin-capsule/src/daemon.rs`, `daemon/context_mgmt.rs`, `daemon/multiplexer_utils.rs`, `daemon/resource_metrics.rs`, `daemon/pane_layout.rs`, `daemon/input_dispatch.rs`, `src/session.rs`, `src/exit_assess.rs` (exit-reason mapping only)
- `crates/jackin/src/console/services.rs` (host `instance_refresh` cycle)
- `crates/jackin-runtime/src/runtime/prewarm_trigger.rs` (+ the consumer entry points `runtime/image.rs` prewarm paths, `image/prewarm.rs`)
- `crates/jackin-instance/src/auth.rs` + capsule `runtime_setup.rs` (auth events)
- `crates/jackin-usage` (token-usage instrument wiring where counts already exist; `telemetry.rs` capsule-safe gating)
- `crates/jackin-runtime/src/runtime/launch/launch_runtime.rs:672-678,750-766` (endpoint injection gating: skip injection when `network.mode=none`)
- Schema registry additions: `background.cycle` span def, agent-state/auth/PTY event defs, capsule widget names, job defs, stream instruments

**Out of scope:**
- Deleting `clog!`/`cdebug!` macros or multiplexer.log (plans 011/013). The cycle/PTY/status `clog!` lines keep firing alongside until plan 011 migrates them.
- Parallax-side classification of "Capsule-safe" endpoints (backend-owned); this plan implements the jackin❯ gating switch only.
- Render/compositor metrics beyond what exists (already wired; plan 009 owns drive_frame).

## Git workflow

- Branch: `feature/unified-otel-observability` — single branch, single PR for the whole roadmap item (plans 001–015 together); no per-plan branch or separate PR. Conventional Commits per surface (e.g. `feat(capsule): background.cycle traces and agent-state events`). Sign `-s`, push after every commit.

## Steps

### Step 1: Background cycles

Wrap SUBSTANTIVE cycle work in a `background.cycle` root (facade guard + `spawn_cycle`/detached helper where the work is offloaded): capsule — branch-context lookup actually spawned (not every 1 s tick; the tick that decides "nothing to do" increments a tick counter metric only), PR-context lookup when it actually runs `gh pr list`, usage-account refresh when a refresh actually starts, provider probe when `poll_due_sessions` actually polls, agent-status reconciliation only when a session's evidence pass runs AND produces work (state change or demotion — a quiet pass is metric-only); host — instance refresh in console services. Attributes: `background.cycle.name`, `outcome`, `error.type`; cycle count/duration metrics. NO `job.id` on cycles. The persistent daemon must not attach any stale invocation/session context to autonomous cycles (plan 006 rule) — capsule cycles DO carry the capsule's own `session.id` (they belong to the capsule session) but never a caller's invocation id.

**Verify**: capsule test — a no-op state tick exports zero spans and bumps the tick metric; a real branch-context lookup exports one `background.cycle` root with `background.cycle.name=branch_context`.

### Step 2: Agent-state transition events

In `handle_state_tick`'s transition block (`daemon.rs:885-909` region, where `StatusTick{transition, stuck}` is observed): emit the named event only on effective-state change with the five contract fields (map `AgentRawState`+derived-done → `agent.state`; `AgentStatusSource::Reported{..}` → `reported` WITHOUT the source id; confidence 1:1; `stuck` from the watchdog note). Transition/stuck/flap metrics per the Metrics table (`gen_ai.agent.name`, `agent.state`, bounded source/confidence dims).

**Verify**: unit test on the emission function: same-state tick emits nothing; change emits one event; `reported` carries no source_id field (assert absence).

### Step 3: PTY lifecycle + conversation id

`Session::spawn`: mint `gen_ai.conversation.id` (uuid) for agent panes, store on `Session`; emit spawn event (`gen_ai.agent.name` from slug via `Agent::from_slug`, `gen_ai.conversation.id`). Exit path (`daemon.rs:1378-1420` consumption): map `child_exit_reason` output to `pty.exit.reason` (`None`→`clean`; "exited after signal"→`signal`; "exited with code"→`nonzero_exit`; "wait failed"→`wait_failed`; operator kill/teardown paths→`cancelled` — thread a flag from `KillPane`/shutdown paths) — implement the mapping as a function beside `child_exit_reason` in `session.rs` returning the schema enum, and emit the exit event with `process.exit.code` when present. PTY content never logged (the event has no body). Shell panes (agent=None) emit the events without `gen_ai.*` fields.

**Verify**: session tests — spawn/exit event pair per session; each `pty.exit.reason` arm covered (signal, code, wait-failed via mock; cancelled via kill path); no PTY bytes in any event field.

### Step 4: Streams + widgets

- Add the input-direction byte counter (attach input write path — `Session::send_input`/writer pump feed) and capsule mouse-event counter (`daemon/mouse_input.rs` dispatch); convert existing recorders to the new facade instruments with `stream.direction` where applicable (old `jackin.*` instruments keep working in parallel until plan 013 — new instruments are additive).
- Widget focus: hook `synthesise_focus_swap` (`pane_layout.rs:416-439`) to emit `ui.widget.focused`/`ui.widget.unfocused` (widget name `capsule.pane`; tab switches at the `session_lifecycle.rs` callers emit `capsule.tab`; palette open/close `capsule.command_palette`) + focus-duration metric — plan 009's tracker, capsule instance. Omit tab labels and numeric ids (assert in test).

**Verify**: focus-swap test emits event pair with `app.widget.name=capsule.pane` and no label/id fields; byte counters move in both directions in a pump test.

### Step 5: Prewarm PRODUCER/CONSUMER jobs

`prewarm_trigger.rs`: at scheduling time mint durable `job.id` (uuid) per job; open a `PRODUCER` span (SpanKind::Producer via facade span def) named for the scheduling decision, attrs `job.id`, `job.type`, inheriting current invocation/session correlation; capture its context into the plan 006 `TelemetryContext{job_id, …}` and move it into the spawned task; inside the task each attempt opens a separate `CONSUMER` root trace linked to the producer context, attrs `job.id`, `job.type`, `outcome` (dedup-skip paths `:222,:226` ⇒ `outcome=skip`), `error.type` on failure. Same for image prewarm per-target attempts (`:121-129` loop — one CONSUMER per target attempt) and the post-attach replenish trigger. The JoinSet-based foreground prewarm fan-outs (`cli/prewarm.rs`) are JOINED work under the `cli.command` root — no job ids there (contract: joined async work does not receive a detached-job identity).

**Verify**: runtime test — scheduling exports one PRODUCER span; a (test-mode) attempt exports a CONSUMER root with a link whose linked SpanContext equals the producer's, sharing `job.id`; skip path exports `outcome=skip`.

### Step 6: Auth events + token usage + capsule-safe gating

- Auth provisioning outcome events at the per-agent/mode resolution points in `jackin-instance/src/auth.rs` (and capsule-side `runtime_setup.rs` where agent auth materializes): contract fields only; add a privacy negative test (no env value / vault id / credential name in fields).
- Capsule panic hook → standard `app.crash`: the capsule daemon's panic hook (today in `jackin-usage/src/logging.rs:164-182` — writes PANIC+backtrace to the log and bridges an Error record) is upgraded to emit the standard `app.crash` event (`app.crash.id` uuid, `session.id`, redacted `exception.type`/`exception.message` ≤ 4 KiB) through the facade and force-flush — the same shape plan 009 gives the host hook. The stderr backtrace print may remain as operator output.
- Token usage: where product token counts already exist (token monitor summaries — `jackin-usage` token_monitor), record `gen_ai.client.token.usage` histogram with `gen_ai.provider.name` + standard token-type attribute; DB stays application state.
- Capsule-safe export gating: in `launch_runtime.rs`, skip OTLP endpoint + TRACEPARENT injection AND the firewall-allowlist addition when the effective `network.mode` is `none` (fail closed); add a launch-env classification hook (a function deciding "endpoint is Capsule-safe" — for now: endpoint env value explicitly marked by Parallax launch env variable `JACKIN_CAPSULE_OTLP_SAFE=1`, absent ⇒ host-only telemetry) and report the coverage gap through typed health (plan 002's `TelemetryHealth` gains a `capsule_export: enabled|disabled_network_none|disabled_unclassified` field). Never copy host headers/client keys into capsule env (verify none are today: `grep -n "OTEL_EXPORTER_OTLP_HEADERS" crates/jackin-runtime/src/`).

**Verify**: launch-env test — `network.mode=none` produces NO OTEL/TRACEPARENT env and no firewall OTLP host; auth event privacy negatives pass.

## Reopened audit additions (2026-07-16)

- The shared generated cycle contract requires bounded `background.cycle.name`, accepts stable `error.type`, and automatically records count/duration/outcome/error metrics without job IDs at guard completion. Branch and PR workers now use distinct validated cycle names; a delivered PR lookup failure is a recovered degradation, a closed result channel is `rpc_error`, and thread creation failure is `process_spawn_error`. Remaining: instrument usage-account, provider-probe, instance-refresh, and substantive agent-status work; clear stale caller correlation on autonomous cycles; and keep no-op ticks metric-only.
- Govern agent-transition event/metric attribute contracts, map the authoritative status source, record flap behavior, and prove same-state silence plus reporter-ID/privacy absence.
- PTY exit events now classify wait failures as `io_error`, signal/non-zero exits as `process_exit_nonzero`, and operator termination as `cancelled`, without formatting the underlying error or PTY content. Remaining: add exporter-backed paired spawn/exit privacy coverage.
- Replace Capsule pane action spans with `WidgetFocusTracker` lifecycle for pane, tab, and command palette focus plus bounded duration.
- Prewarm scheduling/attempt paths emit count, active, and duration metrics keyed only by `job.type`, outcome, and stable error type. Connection state, queue depth, standard process health, and established Tokio runtime instruments are also wired to cheap snapshots.
- Create one linked consumer attempt per prewarm target with truthful skip/failure/error outcomes; producer and consumer never default to success, and no `job.id` enters metrics.
- Add governed auth-provision events and standard `gen_ai.client.token.usage`, with auth/token privacy negatives. Emit a schema-valid complete Capsule `app.crash` event (UUID, session and bounded exception fields) and final flush.
- Define an explicit Capsule-safe endpoint and authentication carrier contract. Prove classified credentials are Capsule-safe, host headers/client keys are never copied, unclassified auth reports a coverage gap, and `network.mode=none` injects nothing and opens no egress.

## Test plan

- Per-step tests as above, in the owning crates' sibling `tests.rs` files; capsule daemon tests model on the existing echo-harness style (`crates/jackin-capsule/src/daemon/tests.rs`).
- Conformance additions: `conformance_no_stream_lifetime_spans` (a session with pumps + N frames exports zero stream/session-lifetime spans), producer/consumer link check, cycle-no-job-id check.
- Export-volume regen after counts change: `cargo nextest run -p jackin-diagnostics --all-features -E 'test(conformance_export_volume)'` + `cargo xtask lint ratchet --print export-volume` → update `ratchet.toml`.
- Perf: `cargo bench -p jackin-capsule -- --quick` (pane_body, scrollback_snapshot) completes without obvious regression; plan 014 enforces the 5% budget.

## Done criteria

- [ ] `cargo nextest run --workspace --all-features --locked` exits 0
- [ ] Producer/consumer link + shared `job.id` test passes; cycles carry no `job.id`
- [ ] Agent-state event fires only on change; `reported` has no source id
- [ ] `pty.exit.reason` mapping covers all five values with tests
- [ ] `network.mode=none` launch injects no telemetry env (test)
- [ ] `cargo xtask lint --strict` exits 0
- [ ] `plans/unified-otel-observability/README.md` status row updated

## STOP conditions

Stop and report back (do not improvise) if:
- The capsule's current-thread runtime + `spawn_blocking` pumps make direction-tagged byte counting contended (counters must be atomics — if you find yourself adding a lock on the byte path, stop).
- `job.id` durability across the sidecar lock/live-state dedup implies persistence the code doesn't have (the contract says durable job id; the lock file at `try_lock_prewarmed_dind` may be the natural place — if extending that file format is required, report the design choice first).
- The "Capsule-safe" classification needs Parallax-side information that doesn't exist in the launch env — implement the conservative default (host-only + typed gap) and note it; do NOT weaken `network.mode=none`.
- Emitting agent-state events inside the 1 s state tick measurably regresses the tick (bench guard).

## Maintenance notes

- Plan 011 migrates the surrounding `clog!` narrations at these sites; the structured events added here are what those lines collapse into.
- The `instance_refresh` cycle lives host-side — future contributors will look for it in the capsule; the schema description should say "host console instance refresh". NOTE: this is a deliberate, code-verified divergence from the roadmap's "Periodic Capsule work" row, which groups instance-refresh under Capsule cycles (the roadmap marks its registry values as a codebase-scan seed); the telemetry contract value `background.cycle.name=instance_refresh` is unchanged. Plan 015 corrects the grouping wording when the roadmap page is condensed at closure.
- Reviewer focus: no per-frame/per-byte spans anywhere; skip-vs-failure outcomes on dedup paths; conversation id distinct from operator session id.
