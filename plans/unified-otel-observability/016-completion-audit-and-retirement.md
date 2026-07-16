# Plan 016: Independent completion audit and retirement

## Status

- **Priority**: P1
- **Effort**: M
- **Risk**: HIGH (deletion follows proof, never precedes it)
- **Depends on**: plans 001–015
- **Planned at**: commit `0a11f587f1`, 2026-07-16

## Why this matters

The roadmap and original fifteen plans were marked complete while requirement-matched audit evidence still contradicted them. This plan prevents a second paper closure and retires planning artifacts only after authoritative evidence proves every row.

## Steps

1. Re-read the original roadmap with `git show fa8194882:docs/content/docs/roadmap/unified-otel-observability.mdx` and extract every promise, invariant, artifact, acceptance criterion and out-of-scope boundary.
2. Re-read plans 001–015 and [`worksheets/016-gap-audit.md`](worksheets/016-gap-audit.md). Record direct current code, test, CI, documentation or runtime evidence for every row; missing or indirect evidence remains open.
3. Run every plan criterion plus `cargo xtask ci --fast`, required telemetry conformance, full workspace nextest, benchmarks, docs build/link checks, expected-absence searches, and PR checks. Confirm each gate covers its claimed requirement.
4. Run expected-absence checks for every original out-of-scope boundary: no Collector, gateway, telemetry agent/sidecar, backend/dashboard/alert/saved-query configuration, backend-neutral telemetry viewer API, disk-backed telemetry queue/spool, or production profiling export was added.
5. Replace every repository link to `/roadmap/unified-otel-observability/` with a canonical shipped guide/reference page or accurate past-tense prose. Remove the page from sidebar metadata and roadmap overview.
6. Only when steps 1–5 prove every requirement: delete the roadmap file and entire `plans/unified-otel-observability/` directory, remove its active row from `plans/README.md`, and record the shipped retirement there.

## Done criteria

- [ ] Every original roadmap and plan requirement has direct, current evidence.
- [ ] No row in `worksheets/016-gap-audit.md` remains open.
- [ ] All implementation, conformance, performance, privacy, docs and CI gates pass.
- [ ] The full original out-of-scope expected-absence inventory passes and is recorded with current evidence.
- [ ] `rg -n 'unified-otel-observability|Unified OpenTelemetry observability' . --glob '!target/**'` returns no live route/plan references.
- [ ] The roadmap file and plan directory no longer exist.
- [ ] `plans/README.md` records retirement instead of active work.

## STOP conditions

- Any requirement lacks direct evidence.
- Any gate is skipped, flaky, narrower than its claimed requirement, or fails.
- Any remaining roadmap link would break after deletion.
- Any implementation change is uncommitted, unpushed, or absent from the open PR.
