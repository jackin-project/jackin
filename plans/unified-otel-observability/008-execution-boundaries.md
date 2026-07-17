# Plan 008: Shared boundary instrumentation — launch pipeline, subprocess, Docker, provider HTTP, usage DB, connections

> **Executor instructions**: Follow this plan step by step. Run every
> verification command and confirm the expected result before moving to the
> next step. If anything in the "STOP conditions" section occurs, stop and
> report — do not improvise. When done, update the status row for this plan
> in `plans/unified-otel-observability/README.md`.
>
> **Drift check (run first)**: `git diff --stat fa8194882..HEAD -- crates/jackin-runtime/src/runtime/launch crates/jackin-docker/src crates/jackin-process/src crates/jackin-usage/src crates/jackin-image/src crates/jackin-core/src/launch_progress.rs`
> If any in-scope file changed since this plan was written, compare the
> "Current state" excerpts against the live code before proceeding; on a
> mismatch, treat it as a STOP condition.

## Status

- **Priority**: P1
- **Effort**: L
- **Risk**: MED
- **Depends on**: plans/unified-otel-observability/004-telemetry-facade-api.md, 005-async-spawn-helpers.md, 007-identity-lifecycle-roots.md
- **Roadmap item**: [Unified OpenTelemetry observability](../../docs/content/docs/roadmap/unified-otel-observability.mdx) (`docs/content/docs/roadmap/unified-otel-observability.mdx`) — this plan implements the "Execution and integration boundaries" case table (launch pipeline, subprocess, `jackin-exec`, Docker Engine, provider usage/API, binary download, usage database), the connection/config/trust/cache/isolation rows of "Product state and health", and the `error.type` failure-classification row; the roadmap item is the binding contract and overrides this plan on any conflict.
- **Planned at**: commit `fa8194882`, 2026-07-15

## Why this matters

The roadmap requires shared boundary instrumentation to land BEFORE leaf call sites migrate ("Ad hoc subprocess, Docker, HTTP, database, launch, and async instrumentation → shared boundary instrumentation using the explicit case contracts above before leaf call sites migrate"). One instrumented choke point per boundary means the hundreds of leaf sites in plan 011 become deletions, not rewrites. This plan instruments: the launch pipeline (one bounded operation, 11 stage children), subprocess execution, Docker Engine HTTP, provider HTTP, binary downloads, the usage SQLite DB, connection attempts, and config/trust/cache/isolation decision events — all with allowlisted attributes only.

## Current state

(verified at planning commit)

- **Launch stages**: `jackin_core::LaunchStage` (`crates/jackin-core/src/launch_progress.rs:16-39`) — exactly 11 variants `Identity, Role, Credentials, Construct, AgentBinaries, DerivedImage, Workspace, Network, Sidecar, Capsule, Hardline`; `ALL: [Self; 11]` at `:43`. Stage transitions are split across two mechanisms: `StepCounter` (`crates/jackin-runtime/src/runtime/launch/progress_helpers.rs:30`, text→stage map at `:144`) and direct `progress.stage_done/stage_skipped` calls in `launch_pipeline.rs` (`:74,433,954,1029,1057-1060,1201`), `orchestrate.rs` (`:946,950,1440,1479,1521,1789`), `launch_runtime.rs` (`:1086,1184`). Parallel legacy telemetry via `active_timing_started/done(DiagnosticStage…)`. The launch future is already wrapped by `launch_trace` (`crates/jackin/src/app/load_cmd.rs:127,406,456` → `jackin-diagnostics/src/screen.rs:255`) — a screen-trace mechanism plan 009 removes; the launch OPERATION span this plan adds replaces its causal role.
- **Subprocess**: pure transport `crates/jackin-process/src/lib.rs` (`ExecRequest` `:49-179`, `ExecResult { code, success, … }` `:183`, `exec_async` `:202`); its crate doc says "explicitly NOT telemetry (callers own it)". The instrumented caller today is `crates/jackin-docker/src/shell_runner.rs` (`enter_process_execute` at `:257` uses the OLD operation facade with `process.command` + redacted args — 4 call sites of `jackin-diagnostics` operation API). Known executables across callers: `git` ×22, `docker` ×22, `gh` ×4, `op` ×2, `mise` ×2, `ps` ×2, plus `codex`, Apple `container`, `sh`, `osascript` (`host_daemon.rs:655`, `jackin-host/src/host_clipboard.rs:321,360,387`), pagers, agent binaries.
- **Docker Engine**: bollard 0.21 client `crates/jackin-docker/src/docker_client.rs` — `BollardDockerClient` implementing `DockerApi` (`jackin-core/src/docker.rs:134`); ~16 methods (`ping :330`, `inspect_container_state :338`, `create_container :441`, `pull_image :603`, `exec_capture :620`, …). Contract: HTTP client conventions over Unix socket — `http.request.method`, bounded `url.template`, `outcome`, `error.type`, target `container.id` when applicable; `server.address` omitted (socket path is user-specific); container names/image refs/labels omitted.
- **Provider HTTP** (`crates/jackin-usage`): reqwest blocking; `fetch_claude_oauth_usage` (`usage/claude.rs:825`, api.anthropic.com), `fetch_codex_oauth_usage` (`usage/codex.rs:883`, chatgpt.com backend), `fetch_amp_api_usage` (`usage/amp.rs:214`), `fetch_grok_web_billing` (`usage/grok.rs:446`), plus zai/kimi/minimax/refresh modules. Contract: HTTP client conventions + registry `gen_ai.provider.name` ∈ {anthropic, openai, amp, xai, zai, minimax, kimi}; no account id, auth material, URL query, response body, or arbitrary model string.
- **Binary download / version check**: `crates/jackin-image/src/agent_binary.rs` (release metadata refresh `:180`, retry/backoff `:660,694`) and `capsule_binary.rs`. Contract: HTTP conventions, bounded route template, server authority, `cache.result`, outcome, error type; no full URL/local path/headers/content.
- **Usage DB**: turso SQLite via `crates/jackin-usage/src/store_backend.rs` (`connect_local :10`); ops in `telemetry_store.rs` — `BEGIN :283`, `INSERT … ON CONFLICT … DO UPDATE :289-319`, `COMMIT :380`, `ROLLBACK :374`, selects `:740,:819`. Contract: `db.system.name=sqlite`, bounded `db.operation.name` ∈ {begin, select, insert, upsert, update, delete} as actually called, `db.client.operation.duration` histogram; no SQL text, DB path, account id, provider response. (The `telemetry_store` RENAME is plan 013.)
- **Connections**: host→daemon (`host_daemon.rs:370`), host→capsule control/attach (`client.rs:62,527`, `host_attach.rs:115`, probes `attach.rs:107,199` with retry/backoff at `attach.rs:192`), capsule→host exec (`exec.rs:147,291`), Docker ping. Contract: one `connection.attempt` trace per attempt with `connection.peer.type` ∈ {host_daemon, capsule_control, capsule_attach, docker, provider, parallax}; retry scheduling is a WARN log; reconnects are new attempts.
- **Config/migration**: `crates/jackin-config` — `versions.rs:11-15` (`v1alpha9` global / `v1alpha8` workspace), migration chains `migrations.rs:43-136`. Contract: `config.scope`, `config.operation`, schema from/to + step count, outcome, `error.type`; no path/workspace name/key/value.
- **Trust / isolation / cache**: trust decisions in config/trust command paths (`crates/jackin/src/app/config_cmd.rs` + `jackin-manifest` trust checks — locate via `grep -rn "trusted" crates/jackin-config/src crates/jackin-manifest/src --include='*.rs' -l`); isolation/egress at launch-plan/application boundaries (`crates/jackin-isolation`, `orchestrate.rs` network stage `:1479`, fail-closed firewall apply `launch_runtime.rs:995-1030`); cache decisions currently ad-hoc (`jackin.cache.hits/misses` metric + `feature.decision` events). Contract rows: trust.decision/trust.source.type; workspace.isolation.mode/network.mode/dind.mode with one failure event on fail-closed firewall; cache.name ∈ {role_repository, agent_binary, capsule_binary, derived_image, usage_snapshot} × cache.result ∈ {hit, miss, stale, reuse, bypass}.
- **error.type registry**: `crates/jackin/src/error.rs:16-33` `ErrorCode` E001–E016; schema names from plan 001 (e.g. E001→`docker_daemon_unreachable`, E016→`unsupported_otlp_protocol`), plus `timeout`, `connection_refused`, `panic`.

## Commands you will need

| Purpose | Command | Expected on success |
|---|---|---|
| Workspace | `cargo nextest run --workspace --all-features --locked` | all pass |
| Launch benches still build | `cargo check -p jackin-runtime --benches --locked` | exit 0 |
| Conformance | `cargo nextest run -p jackin-diagnostics -p jackin-capsule --all-features --locked -E 'test(/conformance/)'` | all pass |
| Lint + spawn/telemetry gates | `cargo xtask ci --only lint` | exit 0 |
| Manual smoke | `cargo run --bin jackin -- doctor --debug` | passes; no behavior change |

## Scope

**In scope:**
- `crates/jackin-runtime/src/runtime/launch/` — launch operation + stage children (`launch_pipeline.rs`, `orchestrate.rs`, `launch_runtime.rs`, `progress_helpers.rs`)
- `crates/jackin-docker/src/shell_runner.rs` (subprocess CLIENT span via new facade) and `docker_client.rs` (Docker HTTP spans — wrap the `DockerApi` impl methods; a decorator struct implementing `DockerApi` around `BollardDockerClient` is the cleanest single choke point)
- `crates/jackin-process/src/lib.rs` — **no telemetry added here** (T0 transport stays clean); instead, audit non-ShellRunner callers and route their spans at the call-site boundary helpers in the owning crates (`jackin-host` caffeinate/clipboard, `jackin-image` binaries, `jackin-env` op_cli, capsule `util.rs`/`pr_context.rs`)
- `crates/jackin-usage/src/usage/*.rs` — one shared instrumented HTTP helper (provider requests) + DB op wrapper in `store_backend.rs`
- `crates/jackin-image/src/agent_binary.rs`, `capsule_binary.rs` — download/version-check spans + cache events
- Connection attempt spans at the client connect sites listed above
- Config/trust/isolation/cache events in `jackin-config`, `jackin-isolation`, launch path
- `crates/jackin/src/error.rs` — `impl ErrorCode { pub fn telemetry_error_type(self) -> &'static str }` mapping to schema constants (compile-time exhaustive)
- Schema registry additions (plan 001's YAML + regen): span defs `launch`, `launch.stage.*` naming, `process.command`, `connection.attempt`, HTTP/DB instrument defs, decision event defs

**Out of scope:**
- TUI/action wiring (plan 009), capsule cycles/PTY/agent status (plan 010), prewarm PRODUCER/CONSUMER (plan 010).
- Deleting `active_timing_*`/`run.stage` legacy calls — they coexist until plan 013 (removal) / plan 011 (call-site migration) drain them.
- `LaunchProgress` UI behavior (in-memory progress state is product UI, stays).

## Git workflow

- Branch: `feature/unified-otel-observability` — single branch, single PR for the whole roadmap item (plans 001–015 together); no per-plan branch or separate PR. Conventional Commits per boundary commit (e.g. `feat(runtime): bounded launch operation with stage child spans`). Sign `-s`, push after every commit.

## Steps

### Step 1: Launch operation + stage children

Create the bounded launch operation at the single entry (`launch_role_runtime` callers' top — practical anchor: where `launch_trace` wraps today, `load_cmd.rs:127/406/456`; replace the wrapper's telemetry role with a facade operation span `launch` carrying `launch.target.kind` (∈ workspace|directory — derive from the selector kind already known at that site), `outcome`, `error.type` on terminal failure via `ErrorCode::telemetry_error_type`). Give `LaunchProgress`'s stage transitions a telemetry twin: in `progress_helpers.rs`/the direct-call sites, open a child span per stage (`launch.stage.name=<stage>`) at `stage_started` and close at `stage_done/skipped/failed` (skipped ⇒ `outcome=skip`; failed ⇒ Error + `error.type`). Since stage calls are scattered, add one helper owned by the launch module (`fn stage_span(stage: LaunchStage) -> OperationGuard2`) and call it from the same places `progress.stage_*` fire — a table-driven audit: every `LaunchStage::ALL` member must appear; add the cross-crate equality test LaunchStage↔schema enum here (deferred from plan 001 step 4).

**Verify**: `cargo nextest run -p jackin-runtime --locked` → pass; new in-memory-export test asserts a launch produces one `launch` span + N stage children all parented to it, none open at the end.

### Step 2: Subprocess boundary

In `shell_runner.rs`, swap the old operation facade calls (4 sites, `enter_process_execute :257`, exit-code recording `:98`) to the new facade: `process.command` CLIENT span; attributes `process.executable.name` (basename only), `process.exit.code`, `outcome`, `error.type`; **omit argv, command line, cwd, executable path** (the old `process.args_redacted` attr is NOT carried over — prohibited by the contract). `jackin-exec` flows (capsule `input_dispatch.rs` exec path) additionally set executable classification `configured_command` and must never attach operator command/args/stdout/stderr. For non-ShellRunner subprocess callers, add a tiny `instrumented_exec` wrapper in each owning crate that wraps `jackin_process::exec_async` with the same span shape, and migrate those callers (locate: `grep -rn "jackin_process::\|exec_async\|exec_sync" crates/ --include='*.rs' | grep -v "jackin-process/src\|tests.rs"`).

**Verify**: workspace tests pass; new test asserts span for a fake runner call carries executable name but no argv attr.

### Step 3: Docker Engine boundary

Decorator `InstrumentedDockerApi<C: DockerApi>` in `jackin-docker`: per method, CLIENT span with `http.request.method` + bounded `url.template` (hand-written per method: e.g. `/containers/{id}/json`, `/images/create`), Unix transport attrs, `container.id` where the method targets one, `outcome`, `error.type` (map bollard errors: connect-refused → `connection_refused`, timeouts → `timeout`, daemon-down → `docker_daemon_unreachable`). Install the decorator at client construction (`BollardDockerClient::connect` callers). Never attach container name, image reference, or labels.

**Verify**: `cargo nextest run -p jackin-docker --locked` → pass (FakeDockerClient in jackin-test-support keeps working — decorator is transparent); template test: each method's `url.template` is a static, brace-parameterized string.

### Step 4: Provider HTTP + downloads + DB

- One `provider_request` helper in `jackin-usage` (wraps the blocking reqwest calls): HTTP CLIENT span, `gen_ai.provider.name` from the registry, `http.request.method`, bounded `url.template` (per provider endpoint, no query), `outcome`, `error.type`. Migrate the fetch functions listed in Current state to route through it. Assert no header/body/account-id attributes.
- `jackin-image` downloads: same HTTP shape + `cache.name`/`cache.result` events (`agent_binary`/`capsule_binary` caches) at the existing hit/miss decision points.
- DB: wrap execute/query in `store_backend.rs` with `db.client.operation.duration` histogram + span per op, `db.system.name=sqlite`, `db.operation.name` derived from the statement kind at the call site (pass an enum, don't parse SQL).

**Verify**: `cargo nextest run -p jackin-usage -p jackin-image --locked` → pass; privacy negative test: a provider span exported from a test request contains no `authorization`, no URL query, no account id.

### Step 5: Connection attempts + decision events

- `connection.attempt` root/child spans (per the trace-shape table: a connection attempt is its own bounded trace when detached from an operation, or a child when synchronous inside one — the probes with retry loops at `runtime/attach.rs:192` produce one attempt-trace per iteration and the fixed bodyless `retry.scheduled` WARN between) with `connection.peer.type`; wire at: daemon client connect, capsule control/attach connect, exec host.sock connect, Docker ping. The registered retry event now owns name-slot lock contention followed by another bounded attempt; connection and download retry loops still require migration and exact count/privacy proof.
- Config events at load/validate/migrate/save choke points in `jackin-config` (the migration chain runner in `migrations.rs` knows from/to/steps). Trust decision events where trust is granted/revoked/rejected. Isolation/egress decision events at the launch-plan boundary (`workspace.isolation.mode`, `network.mode`, `dind.mode` are all known in the launch plan structs) + the single fail-closed firewall failure event at the `docker exec` firewall-apply failure path (`launch_runtime.rs:995-1030`).

**Verify**: targeted tests per event (fields exactly per contract rows; no mount path/host/workspace/role/container name fields — assert absence).

### Step 6: Conformance sweep

Extend the conformance group: every span/event added in this plan appears in the schema registry; `error.type` values on exported signals ⊆ schema enum; a full fake launch produces the documented shape (launch + 11 stages + subprocess + docker children in one trace).

**Verify**: `cargo nextest run -p jackin-diagnostics -p jackin-capsule --all-features --locked -E 'test(/conformance/)'` → pass; regenerate export-volume ratchet (counts change): `cargo nextest run -p jackin-diagnostics --all-features -E 'test(conformance_export_volume)'` + `cargo xtask lint ratchet --print export-volume` → update `ratchet.toml`.

## Reopened audit additions (2026-07-16)

- Inventory long-lived streams and watchers. Trace bounded open, handshake, control, and close operations with truthful outcomes, while proving no stream/watcher lifetime span exists.

Capsule Unix-listener coverage is complete: separate bounded autonomous open and close operations own setup and terminal shutdown, every setup/accept failure emits privacy-safe `io_error`, every continued accept attempt emits `retry.scheduled`, normal receiver shutdown closes successfully, and the accept-loop task retains no listener-lifetime span. The Git-context inotify watcher uses the same facade phases around registration and terminal shutdown; filesystem, thread, watch, and cache recovery omits paths, Git content, errno, and raw errors. Other inventoried streams and watchers remain open until their rows have equivalent proof.

- Emit launch-stage/cache and connection count, active, and duration metrics at the shared boundary choke points with only their bounded dimensions.
- Capsule runtime setup routes captured, null-output, and optional synchronous commands through one `process.command` owner. The shared Capsule owner covers all eight audited direct callers: arbitrary `jackin-exec` commands as `configured_command`, dirty-exit Git run/capture, firewall prerequisite and `ipset` operations, Git/GitHub availability probes, bounded command output, and PR-context GitHub lookup. Child ownership closes pipe, write, read, wait, accepted/nonzero exit, timeout, externally reaped, and abandoned instrumentation outcomes; typed lookup failures do not retain command, args, cwd, stderr, JSON, URL, or exit detail. Raw Capsule process calls are confined to this owner and the already-owned runtime-setup helper. Isolation Git inspection uses the same shape. Image ownership is complete; host-CLI probes/viewers and bounded daemon readiness, host desktop/clipboard children, instance credential lookups, Docker context inspection, and both Claude CLI paths use privacy-tested owners with fixed identity and stable errors. The interactive Claude PTY traces only bounded spawn/setup, immediate read failure, and post-EOF reap phases, so no span covers operator idle time; its custom binary path and PTY content are absent from export. Runtime child ownership fails closed on every abandoned early-return path, and Docker shell execution distinguishes spawn from post-spawn I/O failures. The 1Password state machines share a fixed-identity RAII owner for spawn retry, success, nonzero exit, timeout, wait/write I/O failure, and abandoned instrumentation, with an exported privacy/outcome matrix. Create/edit now route secret stdin through the finite shared `jackin-process` deadline while retaining one operation across transient spawn retries; focused tests prove timeout and stdin privacy. The shared usage CLI runner owns Claude/Amp spawn, pipe, read, wait, external-reap, timeout, success, and nonzero outcomes; it bounds and drains output and exports no path, argument, stdout, stderr, or raw error. Remaining subprocess work is exhaustive 1Password pipe/reap fault injection plus the managed Codex/Grok children after their schema prerequisite. Launch root and eleven stages, Docker/provider/download HTTP, SQLite operations, isolation and cache decisions, and launch/cache/connection metrics already exist. The binding contract and generated registry include `db.operation.name=connect`; the shared Turso chokepoint owns build/connect success and failure with a path-negative exporter test. Host cache schema update, metadata upsert, account upsert, table lookup, row iteration, and count queries now route through the same typed DB owner; its exporter test excludes paths, SQL, table names, window labels, and account identity. Config load, global/workspace validation, bootstrap/editor save, and migrate operations now emit the registered event. A transparent typed ownership marker makes nested config boundaries suppress duplicate failures; production load and synthetic nested-owner exporter tests prove exactly-once `config_error` without paths, values, or raw errors. Launch-time grants now emit success only after persistence, while prompt failures and operator rejection emit one typed failure; exporter tests exclude role identity, repository location, and raw prompt errors. Remaining work is exact integrated proof plus missing connections, bounded stream/watcher operations, and complete boundary privacy matrices.

## Test plan

- Launch shape test (step 1), subprocess privacy test (step 2), Docker template/decorator tests (step 3), provider/DB privacy + shape tests (step 4), decision-event field tests (step 5), conformance sweep (step 6).
- Perf guard: `cargo bench -p jackin-runtime --bench launch_pipeline -- --quick` completes; plan 014 enforces the ±5% budget — here just confirm no obvious regression (compare quick numbers to a pre-change run if the harness makes it easy; otherwise rely on plan 014).
- Model integration tests on existing launch tests in `crates/jackin-runtime/src/runtime/launch/tests.rs` and FakeRunner/FakeDockerClient from `jackin-test-support`.

## Done criteria

- [ ] `cargo nextest run --workspace --all-features --locked` exits 0
- [ ] Launch trace shape test passes (1 launch span, 11 possible stage children, correct parentage)
- [ ] `grep -rn "process.args_redacted" crates/ --include='*.rs' | grep -v "jackin-diagnostics\|tests.rs"` → no new-facade usage (legacy sites remain until plans 011/013)
- [ ] Privacy negatives pass (no argv/URL-query/account-id/SQL-text/container-name attrs)
- [ ] `cargo xtask lint --strict` exits 0 (ratchet regenerated)
- [ ] `plans/unified-otel-observability/README.md` status row updated

## STOP conditions

Stop and report back (do not improvise) if:
- Launch stage start/done call sites cannot be paired reliably for some stage (the split StepCounter/direct-call mechanisms drifted) — report the unpairable stage with file:line instead of guessing.
- The `DockerApi` trait has `&self` methods that make a decorator impossible without arch-tier violations.
- Provider request functions turn out to stream bodies through the helper in a way that would buffer sensitive payloads to build attributes — attributes must be derivable without touching the body; report if not.
- Any contract row requires an attribute the schema (plan 001) lacks — schema first, never an inline literal.

## Maintenance notes

- Plan 011 deletes the parallel `active_timing_*`/`run.stage` calls at these same sites; the twin-emission overlap is temporary and intentional.
- New Docker/provider endpoints must add a `url.template` — reviewer should reject any `format!`-built template.
- The `ErrorCode::telemetry_error_type` mapping is the single E-code→`error.type` authority; `main.rs` error rendering and plan 012's validate command both read it.
