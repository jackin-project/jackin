# Plan 014: Verification — OTLP wire receiver, conformance matrix, soak, and the 5% performance gate

> **Executor instructions**: Follow this plan step by step. Run every
> verification command and confirm the expected result before moving to the
> next step. If anything in the "STOP conditions" section occurs, stop and
> report — do not improvise. When done, update the status row for this plan
> in `plans/unified-otel-observability/README.md`.
>
> **Drift check (run first)**: `git diff --stat fa8194882..HEAD -- crates/jackin-test-support .config/nextest.toml ratchet.toml .github/workflows/ci.yml crates/jackin-telemetry`
> If any in-scope file changed since this plan was written, compare the
> "Current state" excerpts against the live code before proceeding; on a
> mismatch, treat it as a STOP condition.

## Status

- **Priority**: P1
- **Effort**: L
- **Risk**: MED (new test infrastructure; flakiness risk in network-ish tests must be engineered out)
- **Depends on**: plans/unified-otel-observability/002-otlp-composition-root.md through 013 (final acceptance runs against the post-cutover tree; the receiver harness itself can start after 002)
- **Roadmap item**: [Unified OpenTelemetry observability](../../docs/content/docs/roadmap/unified-otel-observability.mdx) (`docs/content/docs/roadmap/unified-otel-observability.mdx`) — this plan implements the "conformance, privacy, propagation, cardinality, volume, lifecycle, soak, and performance verification" scope bullet and the "Acceptance criteria" section end-to-end; the roadmap item is the binding contract and overrides this plan on any conflict.
- **Planned at**: commit `fa8194882`, 2026-07-15

## Why this matters

The acceptance criteria demand proof, not intent: a test OTLP receiver representing Parallax receiving all three signals over gRPC from host, daemon, and a Capsule-safe configuration; no lifetime traces under a week-long-console simulation; W3C propagation matrices; privacy negatives; cardinality/volume bounds; graceful-shutdown and failure-mode behavior (endpoint loss, partial success, saturation, slow export) keeping product work bounded; and a checked-in benchmark proving the disabled fast path allocates nothing and active launch/render/PTY telemetry stays within 5% of the reviewed baseline. Today the only OTLP test surface is the SDK's in-memory exporters — no wire-level receiver exists (no tonic/prost dependency anywhere).

## Current state

(verified at planning commit)

- **No wire receiver**: no `tonic`, `prost`, or `opentelemetry-proto` direct dependency in any workspace manifest; tonic 0.14.6 exists only transitively via `opentelemetry-otlp`'s `grpc-tonic`. In-memory test surface: `TestExport` (`crates/jackin-diagnostics/src/observability.rs:1032-1090`), `observability_test_support.rs`, `InMemoryMetricExporter` usage (`metrics.rs:210-240` region), feature `test-support = ["opentelemetry_sdk/testing"]` (post-plan-002 shape).
- **Test infra**: nextest profiles (`.config/nextest.toml`: `default` excludes `binary(dind_e2e)`; `docker-e2e` profile; `ci` profile with 2 retries + flaky detection via `flaky-tests.toml`); `jackin-test-support` (T3) holds fakes (`FakeRunner`, `FakeDockerClient`) with deps only `jackin-core`, `jackin-manifest`, `anyhow`; CI job `telemetry-conformance` runs `cargo nextest run -p jackin-diagnostics -p jackin-capsule --all-features --locked -E 'test(/conformance/)'` (`.github/workflows/ci.yml:939-954`).
- **Ratchets**: `export-volume` family (`ratchet.toml:643-662`) reads `target/telemetry-volume.json` produced by `conformance_export_volume`; regen flow documented at `:650-652`. `suite-time` ratchet bounds total test wall-time — big new suites must stay cheap or run in scheduled lanes (`hygiene.yml`).
- **Benches**: criterion pinned `=0.8.2`; existing targets incl. `jackin-runtime` `launch_attach`/`launch_pipeline`, `jackin` `console_frame`, `jackin-capsule` `pane_body`/`scrollback_snapshot`, plan 004's `disabled_fast_path`. Scheduled `bench-run` in `hygiene.yml:164`; DHAT budgets in `perf_budgets.rs` + `perf` ratchet family.
- **Propagation/lifecycle tests** from plans 006/007/009/010 already cover unit-level shapes; this plan composes them into the wire-level and long-duration matrix.
- Env knobs that make failure-mode tests deterministic: endpoints are injectable (`resolve_otlp_config`), batch delays are 1 s, export timeout 5 s, metric interval 30 s (plan 002) — tests must override intervals via provider-construction test hooks, not sleeps (add narrow `#[cfg(any(test, feature = "test-support"))]` constructors where needed).

## Deliverables

### D1 — `jackin-otlp-testbed` (new dev-only crate or module in `jackin-test-support`)

A minimal OTLP/gRPC receiver: implements the three OTLP collector services (`TraceService`, `LogsService`, `MetricsService`) over tonic on a random localhost port, recording decoded requests into shared state with typed accessors (`spans()`, `logs()`, `metrics()`, `find_event(name)`, …) and scriptable behaviors: respond OK; respond with `partial_success`; respond `UNAVAILABLE`/`RESOURCE_EXHAUSTED` (retryable) vs `UNAUTHENTICATED` (non-retryable); delay responses (slow export); drop connection (endpoint loss). Dependencies: `tonic` (workspace-pin the version matching the locked transitive 0.14.6), `opentelemetry-proto` (0.32 family, `gen-tonic-messages` + service features) — dev-/test-support-scoped so product binaries gain nothing. Placement decision: a new crate `crates/jackin-otlp-testbed` (tier: add to `TIERS` beside `jackin-test-support` at T3) keeps `jackin-test-support` dependency-light; choose that unless the arch gate argues otherwise.

### D2 — Wire conformance suite (`conformance_wire_*`)

Against a live testbed instance (host-process side runs real providers pointed at `http://127.0.0.1:<port>`):
- all three signals arrive over gRPC from a host-shaped init, a daemon-shaped init, and a capsule-shaped init (Resource `service.name` per binary; capsule config marked Capsule-safe);
- Resource assertions (stable per process, cloned across providers, no forbidden keys);
- every registered event arrives once with native EventName/severity/TraceId/SpanId (no duplicate span events);
- namespace scan over EVERY received attribute/metric/span name: nothing matches `^jackin\.` or `^parallax\.` (final acceptance form of the plan 001 ban);
- `jackin diagnostics validate` exits 0 against the testbed and non-zero when the testbed is stopped (plan 012's end-to-end).

### D3 — Lifecycle & failure-mode suite

- **Week-long console simulation** (accelerated): drive a synthetic invocation through N=10k action/cycle/connection operations across simulated hours (no real sleeps — inject clocks where plan code took `Instant::now()`; where injection is impractical, assert structurally instead: after each batch, zero open spans and bounded queue depth), assert: no span duration exceeds the longest single operation; memory bounded (queue caps respected); `cli.invocation.id` constant; session ids rotate on reattach.
- **Failure modes** (each: product work continues, bounded memory, health counters move): no endpoint (no-op fast path — zero telemetry work); `OTEL_SDK_DISABLED`; endpoint loss mid-run (testbed dies; saturation drops, no blocking); partial success (not retried); retryable vs non-retryable status codes (retry at most 3, only retryable); slow export (5 s cap honored); saturation (flood > queue caps; product thread never blocks — assert via time budget on the emitting loop).
- **Graceful shutdown**: full order per plan 002; final events (`session.end`) present in the testbed after shutdown; double-shutdown safe.
- **Propagation matrix (wire level)**: CLI→daemon and host→capsule-control through real serialization (plan 006 unit matrix re-run over the testbed), disabled-child `or_current()`, links (prewarm PRODUCER/CONSUMER share `job.id`, link resolves), malformed context → local root, concurrent sessions (two simulated sessions interleaved — ids never cross-contaminate).
- **Privacy negatives (wire level)**: drive representative flows (launch w/ fake runner, subprocess, provider request against a stub, config migrate, PTY session fixture, exec command) and scan every received signal for prohibited material: absolute paths, URLs with queries, `authorization`/tokens (reuse `secret_scrub` patterns as the scanner), workspace/role/container names from the fixtures, tab labels, raw argv, PTY bytes, mouse coordinates.
- **Cardinality/volume**: per-stream attribute-set count ≤ 256 enforced; default-mode volume within the export-volume ratchet; DEBUG/TRACE absent in default mode.

### D4 — Performance gates

- Disabled fast path: promote plan 004's bench to an asserting harness — an allocation-counting test (custom `GlobalAlloc` counter in a `#[cfg(test)]` binary or `dhat` testing mode, mirroring `alloc_telemetry.rs`'s approach) proving zero heap allocation and zero formatting on the disabled path for: event emit, guard create/drop, counter add, spawn helper passthrough.
- Active-path ±5%: checked-in baseline file `crates/jackin-telemetry/benches/baseline.json` recording reviewed medians for `launch_pipeline`, `console_frame`, `pane_body` (and the PTY byte-pump micro-bench — add one if none exists in `jackin-capsule` benches) captured on the reference machine; a scheduled (hygiene-lane) comparator script/xtask lane flags >5% regressions vs baseline. PR lanes get the cheap version: bench targets still compile (`bench-build` job exists) + the allocation gate. Document the baseline-refresh procedure in the bench module docs (re-record on reviewed perf-affecting PRs).

## Commands you will need

| Purpose | Command | Expected on success |
|---|---|---|
| Testbed crate | `cargo nextest run -p jackin-otlp-testbed --locked` | all pass |
| Wire conformance | `cargo nextest run -p jackin-diagnostics -p jackin-capsule -p jackin-otlp-testbed --all-features --locked -E 'test(/conformance/)'` | all pass |
| Full suite | `cargo nextest run --workspace --all-features --locked` | all pass |
| Volume ratchet | `cargo xtask lint ratchet --print export-volume` | rows match `ratchet.toml` |
| Alloc gate | `cargo nextest run -p jackin-telemetry --locked -E 'test(disabled_alloc)'` | pass |
| Bench compile | `cargo check --workspace --benches --locked` | exit 0 |
| Suite-time check | `cargo xtask lint ratchet --print suite-time` (after a junit-producing run) | within bound |

## Scope

**In scope:** `crates/jackin-otlp-testbed/**` (new; + `TIERS` row, ratchet public-surface row, README/AGENTS/CLAUDE per crate rules); conformance suites in `jackin-diagnostics`/`jackin-capsule` (so the existing CI filter picks them up) + testbed-hosted integration tests; `crates/jackin-telemetry/benches/` + baseline + alloc-gate test; `crates/jackin-xtask` bench-compare lane; `.github/workflows/ci.yml` `telemetry-conformance` job package list (add `-p jackin-otlp-testbed`); `hygiene.yml` bench-compare wiring; `ratchet.toml`/`flaky-tests.toml` as needed; root `Cargo.toml` workspace pins for `tonic`/`opentelemetry-proto` (dev-scoped).

**Out of scope:** product code changes (if a test exposes a defect, STOP-report it against the owning plan rather than patching drive-by); Parallax itself; a Collector (explicitly excluded — the testbed is a test double for Parallax's OTLP endpoint, not a shippable component; keep it `publish = false`, dev-only).

## Git workflow

- Branch: `feature/unified-otel-observability` — single branch, single PR for the whole roadmap item (plans 001–015 together); no per-plan branch or separate PR. Conventional Commits, e.g. `test(telemetry): otlp wire receiver and conformance matrix`. Sign `-s`, push after every commit.

## Steps

1. **Testbed crate** (D1) + tier/ratchet/README plumbing. **Verify**: its own loopback test (SDK exporter → testbed → decoded span visible) passes; `cargo xtask lint --strict` → exit 0.
2. **Wire conformance** (D2). **Verify**: suite green locally ×3 consecutive runs (`--` repeat manually) — no flakes; namespace scan test fails when fed a synthetic `jackin.x` attr (self-test).
3. **Lifecycle & failure modes** (D3). Engineering rule: no `std::thread::sleep`/wall-clock waits in tests — use testbed response scripting + provider test hooks + bounded `tokio::time::pause`-style control where the dedicated runtime allows; anything needing >5 s real time goes behind the `docker-e2e`-style opt-in or a new `soak` nextest profile wired into the scheduled hygiene workflow, NOT the PR lane. **Verify**: PR-lane subset < 60 s added wall time (suite-time ratchet); soak subset runs green via `cargo nextest run --profile soak …` (add profile).
4. **Performance gates** (D4). **Verify**: alloc gate passes; baseline file recorded; comparator lane exits 0 on unchanged code and non-zero when fed a doctored baseline (self-test); revert doctoring.
5. **CI wiring + flake hardening**: update workflow package lists; run the conformance suite 10× locally (`for i in $(seq 10); do cargo nextest run … -E 'test(/conformance/)' || break; done`) — zero failures required before merge.

## Reopened audit additions (2026-07-16)

- Add named matrices for outcome/error ownership, all required metric families, second-line metric rejection, and bounded stream/watcher close operations.
- Make privacy coverage case-row-complete, including arbitrary model names, agent codenames, config keys/values, role source URL/id, cache keys, allowlisted hosts, image references/labels, and credential/client-key paths.
- Add an authenticated Capsule-safe three-signal receiver case; endpoint-only classification is insufficient when Parallax authentication is required.
- Replace identity-only marker helpers with representative host CLI, daemon RPC, Capsule launch/control, subprocess, provider, config, PTY and `jackin-exec` flows over the real receiver. Add live serialized propagation, disabled-child fallback, malformed context, and genuinely concurrent/interleaved session cases.
- Extend receiver privacy/namespace scanning to metric names/datapoints/exemplars, span-link attributes, and instrumentation-scope metadata, with injected canaries and detector self-tests rather than absence-only fixtures.
- Build a non-vacuous accelerated soak covering bounded span duration/open-span count, connection/cycle/job work, queue/memory caps, invocation stability and reattach linkage. Every failure-mode case records health deltas, product latency, bounds and all three signals.
- Prove cross-provider Resource equality and correlation-ID exclusion, registry-wide exactly-once event shape, default DEBUG/TRACE absence, ordered/bounded shutdown, all-stream cardinality, and representative wire volume.
- Make the 5% performance gate a controlled/calibrated comparison and prove disabled paths perform no formatting as well as no allocation. Run testbed detector/self-tests explicitly in CI.
- Replace every generic/nonexistent acceptance-map claim with exact executable test names/commands and durable evidence; specifically remove `container_info_state_has_no_local_telemetry_affordance` unless that test exists.

CI evidence: the `telemetry-conformance` job runs `cargo nextest run -p jackin-otlp-testbed --locked` without a name filter, so the three-signal receiver loopback and both injected-canary detector self-tests are required on every Rust CI run. The exporter-backed `/conformance/` matrix also includes the testbed package explicitly.

## Test plan

This plan IS the test plan; its own meta-verification: each suite contains at least one self-test proving the detector detects (namespace scan, privacy scanner, bench comparator, alloc counter), so green means verified, not vacuous.

## Done criteria

- [ ] All acceptance-criteria bullets of the roadmap item have a named test (commit a mapping table `plans/unified-otel-observability/worksheets/014-acceptance-map.md`: criterion → test name(s)) — no criterion row left "uncovered"
- [ ] `cargo nextest run --workspace --all-features --locked` exits 0; conformance suite 10×-stable
- [ ] Testbed receives 3 signals from host/daemon/capsule-shaped inits over real gRPC
- [ ] Alloc gate + baseline comparator in place and self-tested
- [ ] `cargo xtask ci --fast` exits 0; suite-time ratchet within bound
- [ ] `plans/unified-otel-observability/README.md` status row updated

## STOP conditions

Stop and report back (do not improvise) if:
- `opentelemetry-proto`/`tonic` version pinning conflicts with the locked exporter stack (the testbed must decode what 0.32 sends — version-match, don't wrangle).
- Any D2/D3 test exposes a product defect (wrong parentage, leaked attribute, blocking saturation) — file it against the owning plan (worksheet note + report), do not patch product code here.
- Deterministic failure-mode control turns out to need product test hooks that don't exist and would be invasive — list the exact hooks needed instead of building sleep-based tests.
- PR-lane wall-time budget cannot hold — propose the soak-profile split explicitly before merging slow tests into the default lane.

## Maintenance notes

- The acceptance map worksheet is the durable artifact reviewers audit for the roadmap item's closure — keep it current as tests are renamed.
- Baseline refresh procedure must be operator-reviewed (a silent baseline bump defeats the 5% gate).
- The testbed doubles as the harness for future Parallax-facing contract tests — keep its API small and typed.
