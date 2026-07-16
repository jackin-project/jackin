# Plan 015: Documentation cutover and roadmap closure

> **Executor instructions**: Follow this plan step by step. Run every
> verification command and confirm the expected result before moving to the
> next step. If anything in the "STOP conditions" section occurs, stop and
> report — do not improvise. When done, update the status row for this plan
> in `plans/unified-otel-observability/README.md`.
>
> **Drift check (run first)**: `git diff --stat fa8194882..HEAD -- docs/content/docs ENGINEERING.md TESTING.md DEPRECATED.md`
> If the docs tree changed since planning, re-run the greps in step 1 to
> rebuild the page inventory before editing; on structural mismatch with
> "Current state", treat it as a STOP condition.

## Status

- **Priority**: P1 (docs gates are pre-merge requirements on the single PR — this plan is the consolidated docs checklist and MUST be completed on the branch before the one PR for this roadmap item is marked ready for review)
- **Effort**: M
- **Risk**: LOW
- **Depends on**: plans/unified-otel-observability/012-diagnostics-validate-health.md, 013-artifact-removal-cutover.md, 014-verification-suite.md (final status flip)
- **Roadmap item**: [Unified OpenTelemetry observability](../../docs/content/docs/roadmap/unified-otel-observability.mdx) (`docs/content/docs/roadmap/unified-otel-observability.mdx`) — this plan implements the acceptance criterion "Operator and contributor documentation describes direct OTLP-to-Parallax behavior, backend-owned history, in-memory current-run UI state, and the absence of local telemetry files" and the roadmap-page closure itself ("The landing implementation replaces those pages with the direct OTLP-to-Parallax application contract above"); the roadmap item is the binding contract and overrides this plan on any conflict.
- **Planned at**: commit `fa8194882`, 2026-07-15

## Why this matters

The repo enforces docs-as-source-of-truth pre-merge gates (roadmap freshness + user/contributor docs in the same PR — `PULL_REQUESTS.md`). After the cutover, a dozen pages describe files, commands, and env vars that no longer exist. This plan is the authoritative page inventory, the target content per page, and the roadmap bookkeeping (status, overview, sidebar) that closes the item.

## Current state — page inventory

(paths verified at planning commit; all under `docs/content/docs/` unless noted)

**Rewrite (operator surface):**
- `(public)/guides/run-telemetry.mdx` (route `/guides/run-telemetry/`) — today: two mutually exclusive sinks (OTLP vs `~/.jackin/data/diagnostics/runs/<run-id>.jsonl`), `JACKIN_DIAGNOSTICS_FILE`, `parallax.run.id` trace model, screen-trace description. Target: direct OTLP/gRPC to Parallax only; enable via `OTEL_EXPORTER_OTLP_ENDPOINT`; `jackin diagnostics validate` as the delivery check; correlation via `cli.invocation.id` + `session.id`; backend-owned history; explicitly: no local telemetry file exists, `--debug` is operator output not telemetry delivery.
- `(public)/guides/environment-variables.mdx:100-116` — env table: drop `JACKIN_DIAGNOSTICS_FILE`, `JACKIN_RUN_ID`, `PARALLAX_RUN_ID`-adoption/`OTEL_RESOURCE_ATTRIBUTES` run-id row; keep `OTEL_EXPORTER_OTLP_*`, `JACKIN_DEBUG`, `JACKIN_TELEMETRY_LEVEL/CATEGORIES`; add `OTEL_SDK_DISABLED`. The launch-injected internal vars (`JACKIN_INVOCATION_ID`, `JACKIN_CAPSULE_OTLP_SAFE`) are contributor-facing only — document them in `reference/runtime/diagnostics.mdx`, not the operator env table.
- `(public)/commands/diagnostics.mdx` — becomes the `validate` page (synopsis, output, exit codes, failure modes). Remove summary/compare.
- `(public)/commands/logs.mdx` — DELETE (command removed). `(public)/commands/daemon.mdx` — drop the `logs` subcommand row (`:16`) and `log:` status line (`:15`); document telemetry-health line instead. `(public)/commands/meta.json:4` — drop `logs` entry.
- `(public)/commands/doctor.mdx:21`, `load.mdx:23`, `prewarm.mdx:12`, `status.mdx:94`, `usage.mdx:47`, `console.mdx:17` — sweep the "run diagnostics handle / diagnostic tracing / run id" phrasings to invocation-id + OTLP wording (one-line edits each).
- `(public)/guides/host-affordances.mdx:39` — "Copy diagnostics path" → copyable invocation id (matches plan 013 UI change).
- `(public)/guides/docker-profiles.mdx:116` — still true (allowlist includes OTLP host) but re-check wording against plan 010's `network.mode=none` gating; state that `network.mode=none` disables capsule telemetry egress.
- `(public)/guides/security-model.mdx:182` — extend: allowlist-first telemetry, no path/name/content attributes, redaction limits.

**Rewrite (contributor surface):**
- `reference/runtime/diagnostics.mdx` (route `/reference/runtime/diagnostics/`) — full replacement: the application observability contract (identity model, bounded traces, event/metric families, correlation lifetimes, OTLP runtime contract, health, validate) — effectively the durable engineering-reference distillation of the roadmap design; JSONL schema tables, `multiplexer.log` rotation (`:62`), sidecar docs all go.
- `ENGINEERING.md:64-81` — replace the `clog!`/`cdebug!` two-tier section with the governed model: facade-only emission, event tier rules (INFO/WARN/ERROR/DEBUG/TRACE semantics from the contract), the ~10/min DEBUG rule carried over, spawn-ownership helpers, "schema registry first" rule, operator-output port. Line 20's `telemetry_store.rs` link → renamed file (plan 013 F).
- `TESTING.md` — `:88-102` fixture flow rewritten for the `JACKIN_PTY_FIXTURE_CAPTURE` gate; `:104-129` local-validation flow: `--debug` mandate stays, JSONL fallback wording replaced by OTLP/validate + testbed pointer; `:108` sink-fallback sentence deleted.
- `reference/crates/jackin-diagnostics.mdx`, `jackin-usage.mdx:7` ("owns the telemetry store" → usage snapshot store), `jackin-capsule.mdx:18`, `jackin-process.mdx:12` — align with final crate responsibilities; ADD `reference/crates/jackin-telemetry.mdx` (new crate page) and testbed crate page if the codebase-map convention requires one (`reference/getting-oriented/codebase-map.mdx:54` row additions for both new crates).
- `reference/tui/{chrome,dialogs,navigation}.mdx` — Debug-info dialog rows (diagnostics log path → invocation id); `navigation.mdx:231` `parallax.run.id` mention.
- `reference/errors/E016.mdx` — still valid (gRPC-only guard); verify wording against plan 002's config module.
- `reference/runtime/configuration.mdx:67,89` + `reference/runtime/schema-versions.mdx:41` — `[telemetry]` keys survive (level/categories); confirm no file-path keys documented.

**Roadmap bookkeeping:**
- `roadmap/unified-otel-observability.mdx` — status line 5 `Open - application design finalized and implementation-ready` → `Resolved`; per the roadmap-page discipline (docs/CLAUDE.md): shrink the page to status + canonical-doc links (the new `/reference/runtime/diagnostics/` contract page + `/guides/run-telemetry/`) + residual/deferred notes (e.g. profiles remain out-of-scope pending the OTel Rust profiling signal); the full design text moves/condenses into the contributor reference rather than living on the roadmap page.
- `roadmap/index.mdx:135` — move the bullet from **Planned** to **Completed** with a one-line outcome.
- Sidebar: `roadmap/(codebase-health)/meta.json:5` keeps the entry (resolved items stay listed); run the audits.
- Cross-referencing roadmap pages that point at this item (verify their claims still hold after landing; update status phrasing where they say "will remove"): `(isolation-security)/sensitive-boundary-code-health.mdx:36,43`; `(agent-orchestrator-research)/(phase-2-operator-surface)/launch-progress-tui.mdx:11` (says incoming `parallax.run.id` stays as external correlation — now false post-cutover; align it); `console-resource-panel.mdx:9`; `(containment-egress-recovery)/network-egress-policy.mdx:15`; `(operator-surface)/clipboard-image-bridge.mdx:31`; `(operator-surface)/jackin-join-and-debug-bundle.mdx:5`; `(reactive-daemon-program)/jackin-daemon.mdx:5`.
- Research pages (`reference/research/agent-telemetry/*`) are historical records — leave content, but fix any repo-file component links that break.

**Conventions that bind this work** (from `docs/CLAUDE.md`): no hard-wrapped prose; site-absolute doc links; repository-file component for repo files in MDX; three-audience separation (no internals on operator pages — the run-telemetry guide must NOT name Rust modules or on-disk internals); brand `jackin❯` in prose; never reference open PRs; roadmap sidebar/overview discipline (audits below).

## Commands you will need

| Purpose | Command | Expected on success |
|---|---|---|
| Docs deps | `cd docs && bun install --frozen-lockfile` | exit 0 |
| Build | `cd docs && bun run build` | exit 0 |
| Link check | `cd docs && bun run check:links:fresh` | exit 0 (needs `lychee`) |
| Repo-file links | `cargo xtask docs repo-links` | exit 0 |
| Roadmap audits | `cargo xtask roadmap audit && cargo xtask research check` | both exit 0 |
| Overview coverage | the `comm -23` snippet from `docs/CLAUDE.md` (roadmap files vs overview) | empty output |
| Spelling | codebook runs in editor/CI; add any new terms to `.codebook.toml` | no unknown-word failures |
| Docs lane | `cargo xtask ci --only docs` | exit 0 |

## Scope

**In scope:** every file in the inventory above; `.codebook.toml` (new terms like `otlp` variants if flagged); `docs/content/docs/reference/crates/` new pages; `PROJECT_STRUCTURE.md` if it names removed modules (check: `grep -n "diagnostics/runs\|multiplexer" PROJECT_STRUCTURE.md`).

**Out of scope:** code (none — this plan is docs-only; if a doc claim can't be written truthfully, that's a STOP, not a code fix); marketing/landing pages; `DEPRECATED.md` (stays "None" — pre-release removals aren't deprecations).

## Git workflow

- Branch: `feature/unified-otel-observability` — single branch, single PR for the whole roadmap item (plans 001–015 together); no per-plan branch or separate PR. The repo's same-PR docs gates are satisfied because this plan lands on the branch BEFORE the single PR is opened (or leaves draft): code and docs merge together. Conventional Commits `docs(...)`. Sign `-s`, push after every commit.

## Steps

### Step 1: Re-inventory

Regenerate the affected-page list against the landed code:
```
grep -rln "multiplexer.log\|diagnostics/runs\|JACKIN_DIAGNOSTICS_FILE\|parallax.run.id\|jackin logs\|daemon logs\|diagnostics summary\|diagnostics compare\|RunDiagnostics" docs/content/docs/ ENGINEERING.md TESTING.md PROJECT_STRUCTURE.md
```
Diff against the inventory; add stragglers.

**Verify**: list captured in the PR description (or worksheet).

### Step 2: Operator surface

Rewrite/edit the operator pages per the inventory targets. Audience check on every page: "could a reader follow this using only the CLI/TUI?" — no crate names, no internal paths.

**Verify**: `cd docs && bun run build` → exit 0.

### Step 3: Contributor surface

Rewrite `reference/runtime/diagnostics.mdx` (the contract page), `ENGINEERING.md`, `TESTING.md`, crate pages (+ new ones), TUI reference pages.

**Verify**: `cargo xtask docs repo-links` → exit 0 (renamed/deleted repo files repointed).

### Step 4: Roadmap closure

Status flip, page condensation, overview move, cross-reference sweep. While condensing, correct the one code-verified divergence recorded in plan 010: `instance_refresh` is a host-console cycle, not Capsule work (the "Periodic Capsule work" grouping in the original design text was a codebase-scan seed; the registry value is unchanged).

**Verify**: `cargo xtask roadmap audit` → exit 0; overview `comm` snippet → empty; `grep -rn "parallax.run.id" docs/content/docs/ | grep -v "research/"` → only historical-record mentions remain (research pages), roadmap/reference pages clean.

### Step 5: Full docs gates

**Verify**: `bun run check:links:fresh` → exit 0; `cargo xtask ci --only docs` → exit 0; `bun test` (docs) → pass.

## Reopened audit additions (2026-07-16)

- Keep the roadmap Open until Plans 012–016 have direct passing evidence; do not describe the telemetry-store rename, delivery proof, or full closure before they land.
- Remove all JSONL/run-ID/reveal/open/sidecar/local-path guidance from codebase map, TUI chrome, host affordances and related roadmap pages; describe `cli.invocation.id`, bounded in-memory current-run state, and backend-owned history.
- Align validate and health prose with implemented evidence only: per-signal delivery proof, actual wire fields, concurrent force-flush followed by ordered provider shutdown, and no invented drop/last-state counters.
- Document every supported/validated standard OTLP endpoint, protocol, disabled, service/resource, compression, timeout, header, CA/client TLS and fixed-sampler variable, including sanitization and Capsule-safe auth restrictions.
- Sweep stale source comments, fixture instructions, usage README terminology and prewarm operator prose after the owning code plans land. Update every contradictory roadmap cross-reference and run docs build/link/RepoFile gates.

## Test plan

The docs gates ARE the tests (build, lychee, repo-links, roadmap audit, research check, codebook). Additionally: a manual read-through of `/guides/run-telemetry/` and `/reference/runtime/diagnostics/` against the roadmap acceptance criterion sentence — the four claims (direct OTLP, backend-owned history, in-memory current-run UI state, no local telemetry files) must each appear explicitly.

## Done criteria

- [ ] All commands in the table exit 0
- [ ] Roadmap item page shows `**Status**: Resolved`; overview lists it under Completed
- [ ] Step 1 grep returns only intentional matches (research history + this plans/ directory)
- [ ] `docs/content/docs/(public)/commands/logs.mdx` deleted; `meta.json` consistent
- [ ] Four acceptance-claims present in the two core pages
- [ ] `plans/unified-otel-observability/README.md` status row updated (and the whole plan set marked DONE if this is the last)

## STOP conditions

Stop and report back (do not improvise) if:
- A page edit would document behavior that hasn't landed on the branch (a prior plan is incomplete or was skipped) — docs describe steady-state reality; finish the owning plan first, on this same branch.
- The roadmap-page condensation would delete design rationale not yet captured in `reference/runtime/diagnostics.mdx` — move it, never drop it.
- Link/audit gates fail on files outside this plan's scope (unrelated docs drift) — report, don't fix drive-by.

## Maintenance notes

- The contract page (`reference/runtime/diagnostics.mdx`) becomes the living instrumentation reference — future schema-registry additions must update it (note this in the page's own intro).
- Update the operator-facing Parallax docs pointers if Parallax's own docs URL scheme changes (out of jackin❯'s control; the guide links generically).
- After this lands, update project memory: the roadmap item is Resolved; the `parallax-run-id-contract` memory is obsolete (plan 013 note).
