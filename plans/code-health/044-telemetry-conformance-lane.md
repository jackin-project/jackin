# Plan 044: Telemetry conformance suite — the dossier's acceptance checks become a permanent gate

> **Executor instructions**: Follow this plan step by step. Run every
> verification command and confirm the expected result before moving to the
> next step. If anything in the "STOP conditions" section occurs, stop and
> report — do not improvise. When done, update the status row for this plan
> in `plans/code-health/README.md`.
>
> **Drift check (run first)**: `git diff --stat fabe88406..HEAD -- crates/jackin-diagnostics/src`
> Plans 018/041/042/043 landing IS expected drift — this plan asserts their
> outputs; read their diffs and continue. Any OTHER change to
> `observability.rs`/`run.rs`/`screen.rs` since the excerpts: STOP condition.

## Status

- **Priority**: P2
- **Effort**: M
- **Risk**: LOW (test-only lane + one span-link addition; no production telemetry behavior changes except the named link sites)
- **Depends on**: plans/code-health/018 (registry, real ids), 041 (facade + prefix-free contract), 042 (instruments). Soft: 017 (export-volume budgets migrate into `ratchet.toml` when the engine exists — this plan ships them as in-source consts either way), 043 (adds a per-sink scenario when landed).
- **Category**: tests
- **Planned at**: commit `fabe88406`, 2026-07-09

## Why this matters

Roadmap Phase 8 item 8: "a conformance lane runs a standard launch against an in-process OTLP capture exporter asserting the dossier's acceptance checks as tests. Export-volume budgets join the Phase 7 ratchet engine so firehose regressions cannot creep back." The dossier defines the checks (its "Acceptance Checks" section): no `[jackin debug` in exported bodies, no token-shaped values, forced failures produce one ERROR log + error span + stable `error.type`, waterfall rows distinct, logs correlate to traces, DEBUG volume bounded. Plans 018/041/042 each add one-off tests for their own slice; without a single conformance scenario those guarantees fragment and silently regress the moment a new emit site skips the facade. This plan also closes the two remaining Phase 8 item-4/-5 residuals that are concrete today: span links for the image-build subtrace, and the honest disposition of feature-decision events.

## Current state

All excerpts verified by direct read at `fabe88406`.

- In-memory rig: `crates/jackin-diagnostics/src/observability.rs:911-952` — `TestExport { spans, logs, tracer_provider, logger_provider }` + `test_layers(debug, run_id)` building `InMemorySpanExporter`/`InMemoryLogExporter` with the real filter directives. `pub(super)` today; consumed by `observability/otlp/tests.rs`. (The README matrix's `911-943` citation is stale; the fn ends at :952.)
- Shipped link model to extend (the exemplar for any new link): `crates/jackin-diagnostics/src/screen.rs` — `enter_screen` builds a detached root span, links the predecessor (`span.add_link(ctx)` :137), stamps `jackin.screen.name` (:126) / `jackin.screen.from` (:138); `launch_trace()` (:210); `current_traceparent()` (:276) W3C-injects for the capsule; capsule side links `capsule.session` → launch at `observability.rs:959-975` (`emit_session_start`).
- Missing links (dossier "Traces And Linked Subtraces"; roadmap item 4 "extends to the remaining big subsystems"): the BuildKit/buildx image-build trace is a *peer*, not a linked child, of the launch trace — no `add_link` exists outside `screen.rs` and `emit_session_start` (grep `add_link` in `crates/` to confirm before editing).
- Feature-decision events (roadmap item 5's last dimension): zero OpenFeature-style code exists (grep `feature_flag|flag_evaluation` clean). Depends on facade maturity + a config-toggle census that does not exist yet — this plan records the disposition, does not build it.
- Volume context: plan 042 Step 4 already asserts zero `send:`/`render:` debug rows for 100 frames; the dossier's store-level measurements (968,593 DEBUG rows; 242,689 PTY/render; 21,611 mouse) are the regression classes the scenario budgets guard.
- Secret guard: `crates/jackin-diagnostics/src/secret_scrub.rs` (`scrub_secrets` re-export in lib.rs:47) + `redact.rs` exist — the conformance scenario asserts they hold on the export path.
- Test conventions: all tests for a module in a single sibling `tests.rs`; no inline `mod tests { … }` bodies in source files (crates/AGENTS.md hard rule).

## Commands you will need

| Purpose | Command | Expected on success |
|---|---|---|
| Diagnostics tests | `cargo nextest run -p jackin-diagnostics` | all pass |
| Conformance module only | `cargo nextest run -p jackin-diagnostics -E 'test(conformance)'` | all pass |
| Workspace clippy | `cargo clippy --workspace --all-targets --all-features --locked -- -D warnings` | exit 0 |
| Full local gate | `cargo xtask ci --fast` | `ci gate OK` |

## Scope

**In scope**:
- `crates/jackin-diagnostics/src/conformance.rs` (create: the scripted scenario driver, `#[cfg(test)]`-reachable helpers) + `crates/jackin-diagnostics/src/conformance/tests.rs` (create: the assertions)
- `crates/jackin-diagnostics/src/observability.rs` — widen `test_layers` visibility to `pub(crate)` if needed; nothing else
- The image-build link site: the host span that wraps derived-image build — locate via `grep -rn "launch.stage\|launch_stage\|derived image" crates/jackin-diagnostics/src/run.rs crates/jackin-runtime/src/runtime/image.rs` and add ONE `add_link`/traceparent-carry following the `screen.rs:137` pattern (exact file confirmed in Step 4; likely `run.rs`'s stage span or the image-build call site in `jackin-runtime`)
- `crates/jackin-diagnostics/README.md` structure row; roadmap Phase 8 items 4/5/8 status notes

**Out of scope** (do NOT touch):
- Building feature-decision events (item 5 residual — recorded as deferred with its census prerequisite, in the roadmap note and the README matrix row).
- The Docker-backed E2E — this lane is in-process only; chaos/E2E variants are plan 046's territory.
- `ratchet.toml` wiring when 017 has not landed (budgets stay in-source consts; a maintenance note hands them to 017).
- Any renaming of emitted wire names (never in any telemetry plan).

## Git workflow

- Branch off `main`: `test/telemetry-conformance-lane`.
- Conventional Commits (`test(diagnostics): …`), `-s`, push per commit. PR to `main`; do not merge. Capsule closure touched (jackin-diagnostics) → capsule smoke block verbatim.

## Steps

### Step 1: The scripted standard-launch scenario

Create `conformance.rs`: a `fn drive_standard_scenario()` (test-support, `pub(crate)`, cfg-gated as needed) that — against an installed `test_layers` subscriber — replays a representative run using ONLY public diagnostics APIs: `enter_screen` (list → launch), `launch_trace`, `RunDiagnostics` stage start/done (2-3 stages incl. one named `derived image`-equivalent), one `operation_span`/`operation_log` per 041's facade, one `error_typed` failure, one clean detach outcome (018 S4's expected-shutdown), N=100 hot-path emissions through 042's converted `record_frame` path, and one secret-shaped string routed through a log body (e.g. `token=abc123FAKE` — synthetic, clearly fake). The scenario is the fixture; keep it deterministic (no clocks beyond what the APIs stamp, no randomness).

**Verify**: `cargo check -p jackin-diagnostics` → exit 0.

### Step 2: The acceptance assertions

In `conformance/tests.rs`, one test per dossier check (each small, each named after the check):

1. `no_bracket_prefix_in_exported_bodies` — no captured log body contains `[jackin debug` or `[jackin-capsule`.
2. `no_token_shaped_values_exported` — the synthetic secret appears in NO exported body or attribute (scrub/redact held).
3. `forced_failure_groups` — exactly one ERROR-severity log for the failure; it carries `error.type`; the active span has error status; the expected-shutdown detach is NOT failure-shaped (018 S4 outcome).
4. `waterfall_rows_distinct` — captured spans' display names (otel.name) are not all identical (the `launch_stage`-times-12 symptom): assert ≥3 distinct span names among stage/screen/operation spans.
5. `logs_correlate_to_traces` — every log record captured while a span was active carries that span's trace id (the rig's records expose span context; model on `otlp/tests.rs`' existing id assertions from 018 S3).
6. `export_volume_budget` — captured record counts within in-source budget consts: `const MAX_DEBUG_LOGS: usize`/`MAX_SPANS: usize` seeded from the scenario's measured actuals + slack ~20% (measure first, then set; record the measured numbers in the PR body). A regression that re-inflates the firehose fails here.
7. `screen_dimension_stamped` — records emitted inside `enter_screen` carry `jackin.screen.name`.

**Verify**: `cargo nextest run -p jackin-diagnostics -E 'test(conformance)'` → all 7 pass.

### Step 3: Run it in the local gate

Nothing extra: the tests are part of `-p jackin-diagnostics`, which PR CI's nextest shards and `cargo xtask ci --fast` already run. Confirm no `#[ignore]`/feature-gate accidentally excludes them.

**Verify**: `cargo xtask ci --fast` → `ci gate OK`; `cargo nextest run -p jackin-diagnostics --no-capture -E 'test(conformance)' | grep -c PASS` → 7 (or nextest's equivalent pass summary).

### Step 4: The image-build span link

Locate the derived-image build's host-side span/stage site (grep per Scope). Following `screen.rs:137`/`emit_session_start` (`observability.rs:959-975`): make the image-build stage span link to (or carry the traceparent of) the launch trace so the BuildKit subtrace stops being an unparented peer. This is ONE link at one site. Extend the Step 2 scenario: the build-stage span carries a link (assert `span.links` non-empty for it in the rig — `InMemorySpanExporter` exposes links on `SpanData`).

**Verify**: `cargo nextest run -p jackin-diagnostics` (+ `-p jackin-runtime` if the site lives there) → pass, including the new link assertion.

### Step 5: Dispositions + docs

Roadmap Phase 8: item 8 → conformance lane shipped, volume budget in-source (017 migration noted); item 4 → build-link shipped, remaining subsystem links ride 041's adoption waves; item 5 → per-crate/step/screen shipped (cite `EXPORT_TARGETS`, taxonomy, `screen.rs`), per-feature events deferred pending facade maturity + toggle census. `crates/jackin-diagnostics/README.md` structure row for `conformance.rs`.

**Verify**: `cargo xtask roadmap audit && cargo xtask docs repo-links` → pass.

## Test plan

- The 7 conformance tests ARE the deliverable; plus the link assertion (Step 4).
- Pattern to model: `observability/otlp/tests.rs` (rig install, capture, assert attrs/status/ids).
- Regression: full `-p jackin-diagnostics` suite; clippy workspace.

## Done criteria

- [ ] `conformance.rs` + `conformance/tests.rs` exist; 7 named acceptance tests green
- [ ] Volume budgets are consts with measured seeds recorded in the PR body
- [ ] Image-build span links to the launch trace; link asserted in the rig
- [ ] Tests run inside the normal crate suite (no ignore/feature gate); `ci --fast` green
- [ ] Roadmap items 4/5/8 dispositions updated; README structure row added
- [ ] `plans/code-health/README.md` row updated

## STOP conditions

Stop and report back if:

- 041 or 042 have not landed (the scenario cannot drive the facade or the converted hot path — the lane would assert nothing real). 018-only is NOT enough.
- `InMemorySpanExporter`'s `SpanData` in the pinned opentelemetry_sdk does not expose links (Step 4's assertion has no seam) — report the version's API.
- The image-build site turns out to emit from a crate that cannot see the diagnostics link helpers without a new dependency edge — report the edge instead of adding it (the tier gate owns that decision).
- Any budget const would need >2× slack to pass (signal the scenario is nondeterministic — find the nondeterminism, do not widen).

## Maintenance notes

- When plan 017's `ratchet.toml` engine lands, migrate `MAX_DEBUG_LOGS`/`MAX_SPANS` into an `export-volume` ratchet family (shrink-only); the conformance test then reads the budget from the engine.
- When plan 043 lands, add scenario #8: per-sink divergence (console-quiet, backend-rich under `--debug`).
- Each 041 adoption wave (HTTP, Docker lifecycle, launch timing, capsule attach) should extend the scenario + budgets in the same PR — the lane is the ratchet that keeps waves honest.
- Reviewer scrutiny: budget seeds must come from measured runs (PR body), not guesses; the synthetic secret must be obviously fake.
