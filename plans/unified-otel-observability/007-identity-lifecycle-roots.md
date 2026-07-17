# Plan 007: Identity and lifetimes — `cli.invocation.id`, `session.id`, command roots, startup/shutdown roots

> **Executor instructions**: Follow this plan step by step. Run every
> verification command and confirm the expected result before moving to the
> next step. If anything in the "STOP conditions" section occurs, stop and
> report — do not improvise. When done, update the status row for this plan
> in `plans/unified-otel-observability/README.md`.
>
> **Drift check (run first)**: `git diff --stat fa8194882..HEAD -- crates/jackin/src/app.rs crates/jackin/src/cli.rs crates/jackin-diagnostics/src/run.rs crates/jackin-usage/src/telemetry.rs crates/jackin/src/app/load_cmd.rs crates/jackin/src/console/tui/run.rs`
> If any in-scope file changed since this plan was written, compare the
> "Current state" excerpts against the live code before proceeding; on a
> mismatch, treat it as a STOP condition.

## Status

- **Priority**: P1
- **Effort**: L
- **Risk**: MED
- **Depends on**: plans/unified-otel-observability/004-telemetry-facade-api.md, 006-cross-process-propagation.md
- **Roadmap item**: [Unified OpenTelemetry observability](../../docs/content/docs/roadmap/unified-otel-observability.mdx) (`docs/content/docs/roadmap/unified-otel-observability.mdx`) — this plan implements "Resource and correlation lifetimes", the "Identity and correlation" case table, and the CLI/startup/shutdown rows of "Bounded traces, logs, and events"; the roadmap item is the binding contract and overrides this plan on any conflict.
- **Planned at**: commit `fa8194882`, 2026-07-15

## Why this matters

The governing model: a long-running CLI invocation is a **correlation domain, not one enormous trace**. `cli.invocation.id` (opaque UUID per top-level launch) is the application correlation key; `session.id` covers TUI/attach ownership until detach/exit; no process-, invocation-, session-lifetime span may exist. Today correlation is `parallax.run.id` (6-hex, minted in `RunDiagnostics::start`, adoptable from `PARALLAX_RUN_ID`/`OTEL_RESOURCE_ATTRIBUTES` env) and the capsule session id (`jk-session-<hex>`); one-shot commands have no root span for the command itself, interactive startup/shutdown have no bounded roots, and `session.start`/`session.end` events do not exist. This plan introduces the new identity plumbing and lifecycle roots. Legacy keys keep flowing in parallel until plan 013 removes them.

## Current state

(verified at planning commit)

- Minting today: `crates/jackin-diagnostics/src/run.rs:1119-1124` `mint_run_id()` → 6 lowercase hex; `:1126-1161` external adoption from `OTEL_RESOURCE_ATTRIBUTES` (`parallax.run.id=` pair) or `PARALLAX_RUN_ID` env, normalized (strip `run_`, ≤ 64 chars); `:1166-1170` `mint_session_id()` → `jk-session-<6hex>`.
- Host startup: `crates/jackin/src/app.rs:95-97` — `set_debug_mode(debug)`, then panic hook; `RunDiagnostics::start` at ~`:127` and `activate()` at `:128` (the `ActiveRunGuard`). CLI parse: `crates/jackin/src/cli.rs:77-99` (global `--debug`/`JACKIN_DEBUG`), `Command` enum `:114-170` with variants `Load, Hardline, Eject, Exile, Purge, Prewarm, Prune, Console, Role, Workspace, Config, Daemon(unix), Logs, Doctor, Diagnostics, Status, Usage, Help` (note: `Logs` exists today but is removed by plan 013 — its command-name registry entry is intentionally absent from the schema).
- Interactive session boundaries (host): console loop owns terminal in `run_console` (`crates/jackin/src/console/tui/run.rs:796`, terminal session `:825-998`); launch/attach path `TerminalSession::enter` at `crates/jackin/src/app/load_cmd.rs:178`; capsule attach/detach in `crates/jackin-runtime/src/runtime/host_attach.rs` (detach = `ServerFrame::Shutdown`/stdin-EOF breaks at `:289/:423`).
- Capsule session identity: minted at daemon start in `jackin-usage/src/telemetry.rs:45-67` (`init()` — mints, stores `SESSION_CONTEXT`, calls `init_capsule_tracing`); a **session-start span** is emitted at `observability.rs:957`/`emit_session_start` `:1135-1152` — a session-scoped span the contract forbids (replace with a `session.start` EVENT).
- Env injection to capsule: `launch_runtime.rs:1250-1256` sets `JACKIN_RUN_ID`; `:750-760` sets `TRACEPARENT` + endpoint. `JACKIN_SESSION_ID` env is separately injected per agent pane (`crates/jackin-capsule/src/client.rs:96` / `session.rs:1578-1592` `inject_status_env`) — that one is the **pane index**, unrelated; do not confuse.
- Roadmap identity contract:
  | Concept | Lifetime |
  |---|---|
  | `cli.invocation.id` | random UUID for the top-level process lifetime (even a week-long console); on relevant roots/logs, never a metric dimension, never Resource |
  | `session.id` | TUI/attach ownership → detach/exit; reattach mints new id + optional `session.previous_id`; one-shot commands omit it; `session.start`/`session.end` events |
  | `gen_ai.conversation.id` | one PTY-hosted agent lifetime (plan 010 wires it) |
  | CLI invocation case | `cli.command.name` from registry (nested = dotted path like `role.validate`); `process.exit.code`; `outcome`; `error.type` on failure |
  | Trace shapes | one-shot: one `cli.command` root; interactive: separate `app.startup` and `app.shutdown` roots; NO lifetime span for process/invocation/session |
- The legacy `parallax.run.id` is a deliberate ecosystem-compat key today, but this roadmap item explicitly ends it: "The legacy `parallax.run.id` and every `jackin.*` telemetry key are removed. `cli.invocation.id` is the application correlation key." Removal is plan 013's cutover; THIS plan adds the new key alongside.

## Commands you will need

| Purpose | Command | Expected on success |
|---|---|---|
| Workspace tests | `cargo nextest run --workspace --all-features --locked` | all pass |
| Diagnostics tests | `cargo nextest run -p jackin-diagnostics --all-features --locked` | all pass |
| Conformance | `cargo nextest run -p jackin-diagnostics -p jackin-capsule --all-features --locked -E 'test(/conformance/)'` | all pass |
| Manual smoke (one-shot) | `OTEL_EXPORTER_OTLP_ENDPOINT= cargo run --bin jackin -- status --debug` | command works, no endpoint needed |
| Lint | `cargo xtask ci --only lint` | exit 0 |

## Scope

**In scope:**
- `crates/jackin-telemetry/src/identity.rs` (new): `InvocationId` (UUID v4 newtype), `SessionId` (UUID-based newtype; the `jk-session-` prefix convention may be kept or dropped — pick UUID per the contract "Random `session.id`" and record it), mint helpers, thread-safe current-invocation storage set once at startup
- `crates/jackin/src/app.rs` + `src/cli.rs` + `src/cli/dispatch.rs` — invocation id minting; `cli.command.name` derivation from the parsed `Command` (dotted subcommand paths, e.g. `Workspace(Env(Set))` → `workspace.env.set`); one-shot `cli.command` root span wrapping dispatch with `process.exit.code` + `outcome` (+ `error.type` via the E001–E016 mapping — `JackinError` downcast already happens in `main.rs`/`app.rs`; stamp `error.type` from `ErrorCode` using the schema names); interactive path emits `app.startup` root (arg parse → console ready) and `app.shutdown` root (exit request → teardown) instead
- Host session: `crates/jackin/src/console/tui/run.rs` + `src/app/load_cmd.rs` — mint `session.id` when the interactive surface takes ownership (console entry at `run.rs:825` region; attach path at `load_cmd.rs:178` region); emit `session.start`/`session.end` events; reattach (`jackin hardline`) mints a new id and sets `session.previous_id` when the previous id is known in-process
- Capsule session: `crates/jackin-usage/src/telemetry.rs` + `jackin-diagnostics` — replace the session-start SPAN (`observability.rs:957`, `emit_session_start`) with `session.start`/`session.end` events; capsule session id becomes a UUID; the launch `TRACEPARENT` link moves onto the capsule's `app.startup`-equivalent bounded startup operation (capsule daemon startup is a bounded operation, not a lifetime)
- `crates/jackin-runtime/src/runtime/launch/launch_runtime.rs:1250-1256` — inject `JACKIN_INVOCATION_ID` alongside (not replacing) `JACKIN_RUN_ID`; capsule reads it (`telemetry.rs`) and stamps `cli.invocation.id` on its signals
- Plan 006 envelope senders — fill `invocation_id`/`session_id` fields from the new identity module
- `crates/jackin-diagnostics/src/run.rs` — `RunDiagnostics` gains the invocation id (so legacy + new correlate during the transition); external run-id adoption (`PARALLAX_RUN_ID`) is untouched here

**Out of scope:**
- Deleting `parallax.run.id`, `mint_run_id`, run JSONL (plan 013).
- Screen visits / `ui.*` (plan 009), conversation ids / PTY (plan 010).
- Daemon autonomous-cycle identity (daemon has NO invocation/session; requests carry the caller's — enforced by plan 006's no-retention rule).

## Git workflow

- Branch: `feature/unified-otel-observability` — single branch, single PR for the whole roadmap item (plans 001–015 together); no per-plan branch or separate PR. Conventional Commits, e.g. `feat(telemetry): cli.invocation.id and bounded lifecycle roots`. Sign `-s`, push after every commit.

## Steps

### Step 1: Identity module

`identity.rs` in jackin-telemetry: `InvocationId::mint()` (uuid v4), `SessionId::mint()`, `set_current_invocation`/`current_invocation` (OnceLock), session registry able to hold previous-id for reattach. Validation helpers already exist in `propagation.rs` (plan 006) — share them.

**Verify**: `cargo nextest run -p jackin-telemetry --locked -E 'test(identity)'` → pass.

### Step 2: Command-name registry mapping

In `crates/jackin/src/cli/` add `pub fn command_name(cmd: &Command) -> CliCommandName` mapping every variant + nested subcommand to the schema enum's dotted paths. Cover: `Prune` (roles/cache/images/instances/system), `Role` (validate/migrate/create/construct_version/published_image/published_image_repository/publish_labels), `Workspace` (create/list/show/edit/prune/remove/env.set|unset|list/claude_token.setup|rotate|revoke|doctor), `Config` (mount.*/trust.*/auth.set|show/env.*/git.*), `Daemon` (serve/install/uninstall/start/stop/restart/status/logs†), `Diagnostics` (summary/compare†). † = commands slated for removal in plan 013 — map them to the nearest registry value for now and mark with `// TODO(otel-cutover)`; the schema registry (plan 001) contains only the surviving names, so these two map to `daemon`/`diagnostics` bare values, not new entries. Write an exhaustiveness test: the mapping `match` has no wildcard arm, so a new CLI command fails compilation until named.

**Verify**: `cargo nextest run -p jackin --locked -E 'test(command_name)'` → pass.

### Step 3: One-shot and interactive roots

In dispatch (`crates/jackin/src/app.rs` — the point after `RunDiagnostics::start`/activate at ~`:127-128`): mint + store `InvocationId`; branch:
- one-shot commands: wrap dispatch in a `cli.command` root (facade guard; attrs `cli.command.name`, `cli.invocation.id`; on completion `process.exit.code`, `outcome`, `error.type` on failure). The guard must END before process exit and before final flush.
- `Console` (and `Load`'s interactive path): emit an `app.startup` root spanning init → surface ready, then run WITHOUT any open span; on exit path emit an `app.shutdown` root spanning exit request → teardown-before-flush. Exit code still recorded — as an attribute on `app.shutdown` plus the final `cli.command`-style log record (the roadmap requires `process.exit.code` on the invocation's logs/roots; put it on `app.shutdown` and on a final INFO event).

**Verify**: in-memory export test (`jackin-diagnostics` or `jackin` test with test layers): running dispatch of a fake one-shot yields exactly one root span named `cli.command`, no span alive after it; console-simulating test yields `app.startup` + `app.shutdown` roots and zero open spans in between.

### Step 4: Host session lifecycle

Mint `session.id` at interactive-ownership start (console: `run_console` after terminal claim ~`run.rs:825`; launch-attach: around `TerminalSession::enter`, `load_cmd.rs:178`). Emit `session.start` (fields: `session.id`, optional `session.previous_id`) and `session.end` on detach/exit (including the drop paths at `load_cmd.rs:241-242`, `run.rs:998`). Stamp `session.id` on signals emitted while a session is active (facade: session context is part of the ambient attrs the facade reads — add an ambient session slot in `identity.rs`). One-shot commands never mint one.

**Verify**: test asserting `session.start`/`session.end` pairing and `session.id` presence on an in-session event but absence on one-shot events.

### Step 5: Capsule identity

`jackin-usage/src/telemetry.rs::init()`: session id → UUID mint; read `JACKIN_INVOCATION_ID` (new) + keep `JACKIN_RUN_ID` transition-side-by-side; replace `emit_session_start` span with `session.start` event carrying launch-trace LINK unnecessary (events don't link — instead the capsule's bounded startup operation span, if the daemon startup work is instrumented as `app.startup`, carries the `TRACEPARENT` remote link the old session span carried at `observability.rs:1139`). `FlushGuard::drop`/shutdown path emits `session.end` before flush. Host injection: `launch_runtime.rs` adds `-e JACKIN_INVOCATION_ID={id}`.

**Verify**: `cargo nextest run -p jackin-usage -p jackin-capsule -p jackin-diagnostics --all-features --locked` → pass; updated capsule tests assert NO span named `capsule.session` is exported and `session.start` event is.

### Step 6: No-lifetime-span conformance

Add `conformance_no_lifetime_spans`: simulate (test-level) a console session with startup, several operations, shutdown; assert every exported span's duration is bounded by its operation (no span covering the whole simulation) and that `cli.invocation.id` appears on roots/logs but NOT on Resource and NOT as a metric attribute (hook plan 004's cardinality guard: assert the facade rejects `cli.invocation.id` as a metric dimension — add it to the metric-dimension prohibition list in the facade if plan 004 didn't already).

**Verify**: `cargo nextest run -p jackin-diagnostics -p jackin-capsule --all-features --locked -E 'test(/conformance/)'` → pass.

## Reopened audit additions (2026-07-16)

- Start the product-binary lifecycle before parsing/classification so help/version/parse errors and pre-dispatch configuration failures have an explicit, tested policy. Route `jackin-role` and the host `role` command through the same harness; make developer-binary exclusions explicit.
- Generate an exhaustive typed mapper for every live nested command variant. No wildcard or silent parent collapse is allowed unless the roadmap contract explicitly lists that exception.
- Centralize result classification so process exit, root outcome, `process.exit.code`, and the stable E001–E016/common `error.type` agree, including operator cancellation.
- Every bounded CLI root emits invocation count, duration, and failure metrics with only `cli.command.name`, `outcome`, and stable `error.type` dimensions.
- Startup spans readiness work and shutdown spans exit request through teardown; neither is a zero-work marker. Service identity/app mode is typed independently of command name.
- Begin console/attach sessions only after ownership succeeds. Reattach mints a new ID with the last-ended ID as `session.previous_id`; concurrent ownership cannot overwrite one global ambient session.
- Governed events and operations merge ambient invocation/session IDs with deterministic duplicate rules, while metrics explicitly reject invocation/session/job/visit/conversation identities.
- Capsule identity exists before fallible startup, a bounded startup root covers listener readiness and links the launch context, and inactive/failed shutdown always clears paired ambient session state.
- Capsule daemon configuration and startup failures pass through one top-level `ResultTelemetryExt` boundary while the provider/session guard is alive; handled daemon failures use the typed recovered-degradation warning so raw error values and duplicate terminal ERROR events are never exported.
- Host and `jackin-role` parse/help/version exits now pass through one shared pre-dispatch boundary. It initializes configured export, emits and completes one bounded `cli.command` root under the governed `help` command name, records success or stable `config_error` plus exit code and CLI metrics, and drops the active diagnostics guard so all signals flush before clap terminates. The developer-only capsule builder remains explicitly excluded. Exporter-backed coverage proves the failure shape without exporting clap text.
- Session ownership is an exclusive scoped guard used after console terminal acquisition and at the runtime attachment boundary. A console may transfer its existing ownership into attachment without a duplicate lifecycle pair; competing console/attachment/Capsule owners are rejected, a non-owning attachment cannot clear the owner, and the next independently claimed attachment mints a new UUID with the last-ended UUID as `session.previous_id`. Threaded ownership tests and production call-site inspection close the concurrency/reattach requirement.
- Capsule PID-1 initialization claims session identity before exporter setup, starts the bounded startup root while the provider is live, links valid launch context, and completes startup exactly once only after socket-listener readiness. Pre-readiness failure completes the root with `launch_failed`, clears ambient session ownership, emits the paired session end when started, and shuts the exporter down from the guard. The PID-1 entrypoint applies a single typed terminal boundary to each mutually exclusive fallible stage and keeps the guard alive through daemon execution; handled rule-pack/config degradations use the governed recovered-warning path without raw error export.

## Test plan

- Unit: identity minting/uniqueness; command-name mapping exhaustiveness; session ambient stamping.
- Integration (in-memory export): one-shot root shape; startup/shutdown roots; session event pairing; capsule session events; invocation id flows through plan 006 envelope (daemon RPC carries it; server stamps it on the SERVER span).
- Conformance: `conformance_no_lifetime_spans` (step 6).
- Export-volume ratchet: counts WILL change (new roots/events; removed capsule session span) — regenerate: `cargo nextest run -p jackin-diagnostics --all-features -E 'test(conformance_export_volume)'` then `cargo xtask lint ratchet --print export-volume`, update `ratchet.toml`.

## Done criteria

- [ ] `cargo nextest run --workspace --all-features --locked` exits 0
- [ ] `grep -rn "cli.invocation.id" crates/ --include='*.rs' | grep -v tests | head` shows facade/schema usage (no ad-hoc string duplicates outside jackin-telemetry)
- [x] `grep -n "emit_session_start" crates/jackin-diagnostics/src/observability.rs` returns no span-emitting variant (event only; only the test helper remains)
- [x] `conformance_no_lifetime_spans` passes and proves bounded startup/command/shutdown operations do not cover the idle session interval, invocation identity is present on roots/logs but absent from Resource, and metric dimensions reject it
- [ ] One-shot smoke: `cargo run --bin jackin -- status` works with no endpoint set (no-op path intact)
- [ ] `cargo xtask lint --strict` exits 0 (export-volume regenerated)
- [ ] `plans/unified-otel-observability/README.md` status row updated

## STOP conditions

Stop and report back (do not improvise) if:
- Anything in the codebase semantically depends on the 6-hex run-id FORMAT (e.g. `normalize_external_run_id` consumers, Parallax-side display in `app.rs:247` banner) breaking under a UUID — the transition keeps `parallax.run.id` flowing, so nothing should; if something does, report it.
- The console's screen-guard machinery (`sync_active_screen`, `run.rs:853`) fights the "no open span between roots" invariant — plan 009 removes it; if it makes this plan's conformance test impossible to write cleanly, coordinate: implement step 6's assertion scoped to non-screen spans and leave a `TODO(plan-009)`.
- `session.previous_id` would require persistence across processes (contract only requires it when known; do not build storage for it).

## Maintenance notes

- Plan 013 deletes `parallax.run.id`/`mint_run_id`/`PARALLAX_RUN_ID` adoption; the side-by-side period is intentional — `launch-progress-tui` roadmap item notes incoming `parallax.run.id` stays only as EXTERNAL correlation until then.
- Reviewer focus: guard lifetimes — every root created here must provably end before flush; a `cli.command` guard that lives past shutdown ordering is the exact bug class the roadmap calls "guard abandonment… an instrumentation fault".
- The command-name mapping is the compile-time tripwire for new CLI commands — keep the no-wildcard match.
