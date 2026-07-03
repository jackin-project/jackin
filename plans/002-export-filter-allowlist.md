# Plan 002: Convert the OTLP export filter from denylist to jackin-target allowlist

> **Executor instructions**: Follow this plan step by step. Run every
> verification command and confirm the expected result before moving to the
> next step. If anything in the "STOP conditions" section occurs, stop and
> report — do not improvise. When done, update the status row for this plan
> in `plans/README.md` — unless a reviewer dispatched you and told you they
> maintain the index.
>
> **Drift check (run first)**: `git diff --stat 5d3661cff..HEAD -- crates/jackin-diagnostics/src/observability.rs crates/jackin-diagnostics/src/observability/otlp/tests.rs`
> If any in-scope file changed since this plan was written, compare the
> "Current state" excerpts against the live code before proceeding; on a
> mismatch, treat it as a STOP condition. (Plan 001 legitimately extracts the
> directive string into `export_filter_directive` — that exact change is
> expected, not drift.)

## Status

- **Priority**: P1
- **Effort**: S
- **Risk**: LOW
- **Depends on**: plans/001-otlp-export-test-seam.md
- **Category**: bug / perf
- **Planned at**: commit `5d3661cff`, 2026-07-03

## Why this matters

The OTLP export filter is a **default-allow** directive: a global level (`info` or `debug`) applies to *every* tracing target, with only a hand-enumerated denylist of transport crates silenced. Any dependency instrumented with `tracing` exports straight into the backend. This is live damage, not theory: the `turso` embedded DB (a direct dependency of `jackin` and `jackin-usage`) emits storage-engine spans, and a live Parallax store showed **109,579 `emit_insn` spans across 1,358 traces**, `usage:refresh_accounts` traces with 440–1005 spans, and a 1.65 GiB log spool — operator-meaningful jackin spans drown in SQLite-VM internals, and the SDK's default 2048-entry batch queue overflows and drops jackin's own spans alongside the noise. The denylist is unmaintainable by construction: every newly instrumented dependency re-opens the firehose. The same file already uses the correct allowlist form for another layer, so this converges on the pattern the codebase already knows.

## Current state

- `crates/jackin-diagnostics/src/observability.rs:797-800` (host `init`) and `:882-885` (capsule `init_capsule`) — after plan 001, both call one `export_filter_directive(level)` returning:

```rust
format!(
    "{level},hyper=off,h2=off,tower=off,tonic=off,reqwest=off,\
     opentelemetry=off,opentelemetry_sdk=off,opentelemetry_otlp=off"
)
```

- The leading `{level}` is an `EnvFilter` **global default directive** — it applies to all targets not otherwise listed. That is the enabling condition for the leak.
- The allowlist counter-example 15 lines below (`observability.rs:894-899`, capsule OTLP self-diagnostics layer): `EnvFilter::new("off,opentelemetry=warn,opentelemetry_sdk=warn,opentelemetry_otlp=warn")` — global `off`, explicit opt-ins.
- Tracer providers are built with no sampler (`observability.rs:765-768` host, `:867-870` capsule) and default batch config; do not change that here (plan 011 owns batch/flush tuning).
- Known jackin-owned tracing targets that MUST stay exported (verified in code):
  - `jackin_diagnostics::jsonl` — every RunDiagnostics log event (`observability.rs:15`).
  - `jackin_diagnostics::session` — capsule session-start marker (`observability.rs:947`).
  - `jackin_capsule` — capsule `bridge_log` lines (`crates/jackin-usage/src/telemetry.rs:65-67`) and `record_capsule_activity` (`screen.rs:252`).
  - Module-path targets from spans/events emitted without `target:` — these default to the emitting module path, so the crate-name prefixes must be allowed: `jackin_diagnostics` (spans `launch_stage` in `run.rs:304`, `screen` in `screen.rs:117`, `capsule.session`/`capsule.tab`), `jackin_usage` (`usage:refresh_accounts` span, `crates/jackin-usage/src/usage.rs:377-380`, and `usage/refresh.rs` probe spans), `jackin_runtime` (`tracing::warn!` sites in `git_pull.rs`, `image.rs`, `attach.rs`, `host_attach.rs`, `cleanup.rs`), plus every other workspace crate: `jackin`, `jackin_core`, `jackin_docker`, `jackin_image`, `jackin_instance`, `jackin_isolation`, `jackin_env`, `jackin_launch_tui`, `jackin_console`, `jackin_host`, `jackin_config`, `jackin_manifest`, `jackin_protocol`, `jackin_term`, `jackin_tui`.
- EnvFilter target matching is **prefix-based**: `jackin=info` also matches `jackin_capsule` etc.? NO — EnvFilter prefix semantics match on module-path separators (`jackin` matches `jackin::foo` but NOT `jackin_capsule`). Each crate-name target must be listed individually.

## Commands you will need

| Purpose | Command | Expected on success |
|---|---|---|
| Format | `cargo fmt --check` | exit 0 |
| Lint | `cargo clippy --workspace --all-targets --all-features --locked -- -D warnings` | exit 0 |
| Tests (crate) | `cargo nextest run -p jackin-diagnostics --all-features` | all pass |
| Tests (workspace) | `cargo nextest run --all-features` | all pass |
| Target inventory | `rg -n 'target:\s*"' crates/ --type rust -g '!*/tests.rs' \| rg -v '^\s*//'` | list of explicit targets to cross-check the allowlist |

## Scope

**In scope**:

- `crates/jackin-diagnostics/src/observability.rs` — the `export_filter_directive` function only (and its doc comment).
- `crates/jackin-diagnostics/src/observability/otlp/tests.rs` — update/extend tests.
- `docs/content/docs/reference/runtime/diagnostics.mdx` — one short paragraph documenting the allowlist + the new escape hatch (docs-same-PR gate).

**Out of scope**:

- Sampler / batch-queue configuration (plan 011).
- Severity of individual events (plan 003) and which events exist (plans 004/006/007).
- The capsule self-diagnostics stderr layer at `observability.rs:894-899` — already correct.
- Silencing turso at its call sites or reusing DB connections (plan 012).

## Git workflow

- Propose branch `fix/otlp-export-allowlist` to the operator; wait for confirmation (never commit `main`).
- `git commit -s -m "fix(diagnostics): allowlist jackin targets in OTLP export filter"` then `git push`.

## Steps

### Step 1: Rewrite `export_filter_directive` as an allowlist

Replace the directive construction with: global `off`, one `=<level>` directive per workspace crate target (the full list in "Current state"), plus the two explicit dotted targets `jackin_diagnostics::jsonl` and `jackin_diagnostics::session` (redundant with the `jackin_diagnostics` crate directive but harmless and self-documenting — include them). Keep a `const EXPORT_TARGETS: &[&str]` slice so the list is data, not a format string, and build the directive by joining:

```rust
/// Tracing targets exported over OTLP. Global default is `off`: a dependency
/// that starts emitting `tracing` data (as turso's storage engine does) must
/// be added here deliberately instead of leaking into the backend.
const EXPORT_TARGETS: &[&str] = &[
    "jackin", "jackin_core", "jackin_diagnostics", "jackin_usage",
    "jackin_capsule", "jackin_runtime", "jackin_docker", "jackin_image",
    "jackin_instance", "jackin_isolation", "jackin_env", "jackin_launch_tui",
    "jackin_console", "jackin_host", "jackin_config", "jackin_manifest",
    "jackin_protocol", "jackin_term", "jackin_tui",
];

fn export_filter_directive(level: &str) -> String {
    let mut directive = String::from("off");
    for target in EXPORT_TARGETS {
        directive.push_str(&format!(",{target}={level}"));
    }
    directive
}
```

Note the old transport denials (`hyper=off` etc.) become redundant under global `off` — remove them. The comment at `observability.rs:794-796` ("Scope the export to jackin❯'s own telemetry…") finally becomes true; keep/adjust it.

**Verify**: `cargo nextest run -p jackin-diagnostics --all-features` → plan 001's test `dependency_targets_pass_the_filter_today` now FAILS (expected — this is the fix landing).

### Step 2: Add the dependency-internals escape hatch

Honor `JACKIN_OTEL_INTERNAL` (truthy per the existing vocabulary — reuse `crate::run::flag_is_truthy`, `run.rs:819-824`; it is `pub(crate)`): when set, append `,{level}` — wait, a trailing bare level re-enables the global default; that is exactly the intended escape hatch. Implementation:

```rust
fn export_filter_directive(level: &str) -> String {
    // ...allowlist as Step 1...
    if internal_export_enabled() {
        // Operator explicitly asked for dependency internals: restore the
        // global default level so instrumented deps (DB engine, etc.) export.
        directive.push_str(&format!(
            ",{level},hyper=off,h2=off,tower=off,tonic=off,reqwest=off,\
             opentelemetry=off,opentelemetry_sdk=off,opentelemetry_otlp=off"
        ));
    }
    directive
}
```

The transport/OTel-SDK denials return ONLY inside this branch (they matter again once the global default is non-off; without them the exporter re-exports its own request logs — a feedback loop, see the original comment at `observability.rs:794-800`).

**Verify**: `cargo nextest run -p jackin-diagnostics --all-features` → tests from Step 3 below cover both branches (write them now if working test-first).

### Step 3: Update tests

In `observability/otlp/tests.rs`:

- Flip plan 001's `dependency_targets_pass_the_filter_today` into `dependency_targets_are_filtered_out`: `tracing::info!(target: "turso_core", ...)` under `test_layers(false)` → **zero** exported logs; same for a fabricated target `some_random_crate`.
- `jackin_targets_still_export`: events on `jackin_diagnostics::jsonl` (via `emit_jsonl_event`) and `target: "jackin_capsule"` → exported.
- `spans_from_workspace_crates_still_export`: `launch_stage` span (module-path target `jackin_diagnostics::run`) → present in `spans.get_finished_spans()`.
- Pure-function tests on `export_filter_directive`: starts with `"off"`, contains `jackin_capsule=info`, does NOT contain a bare `info` global directive; with the internal flag forced (make `export_filter_directive` take `internal: bool` as a parameter and have the env read live in a tiny wrapper — pure core, testable without env mutation, matching the repo's pattern of pure helpers like `resolve_endpoints`).

**Verify**: `cargo nextest run -p jackin-diagnostics --all-features` → all pass.

### Step 4: Docs + workspace green

Add to `docs/content/docs/reference/runtime/diagnostics.mdx` (contributor reference), near its existing filter/export description: one paragraph stating export is allowlisted to jackin❯ targets, and `JACKIN_OTEL_INTERNAL=1` re-enables dependency-internal spans/logs at the active level. Do not hard-wrap prose (docs rule: one paragraph = one line).

**Verify**: `cargo fmt --check` && `cargo clippy --workspace --all-targets --all-features --locked -- -D warnings` && `cargo nextest run --all-features` → all exit 0.

## Test plan

Step 3's tests: filter-out (turso_core, unknown crate), filter-in (jsonl target, capsule target, workspace-crate span), directive pure-function shape, internal-flag branch. Pattern: existing pure tests in `observability/otlp/tests.rs`.

## Done criteria

- [ ] `rg -n '"\{level\},hyper=off' crates/jackin-diagnostics/src` → no matches outside the `JACKIN_OTEL_INTERNAL` branch
- [ ] `export_filter_directive("info")` output starts with `off,` (unit-tested)
- [ ] `cargo nextest run --all-features` exits 0; `dependency_targets_are_filtered_out` and `jackin_targets_still_export` pass
- [ ] `cargo clippy --workspace --all-targets --all-features --locked -- -D warnings` exits 0
- [ ] `docs/content/docs/reference/runtime/diagnostics.mdx` documents allowlist + `JACKIN_OTEL_INTERNAL`
- [ ] `plans/README.md` status row updated

## STOP conditions

- Plan 001 is not landed (no `export_filter_directive` / no `test_layers` seam) — this plan's verification depends on it.
- A jackin span/log that step 3's filter-in tests cover stops exporting and the cause is not a missing target in `EXPORT_TARGETS` (would indicate EnvFilter semantics differ from the plan's understanding — report, don't guess).
- You find call sites emitting on explicit non-`jackin*` targets that must keep exporting (run the target-inventory command); add them to `EXPORT_TARGETS` only if they are jackin-owned code, otherwise STOP and report.

## Maintenance notes

- New workspace crates that emit telemetry must be added to `EXPORT_TARGETS`; the doc comment says so. A cheap guard: a unit test asserting every `crates/jackin*` directory name (underscored) appears in the list would rot-proof it — nice-to-have, not required.
- Plan 008 (telemetry level/categories) builds category filtering on top of this directive builder — keep `export_filter_directive` the single construction point.
- After this lands, re-run a `--debug` session against Parallax: `emit_insn`/`normal_step`/`begin_read_tx` spans must disappear from new traces (manual acceptance from the research doc).
