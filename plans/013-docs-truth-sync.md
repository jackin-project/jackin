# Plan 013: Docs truth sync — fix the stale JSONL triage contract, run-id format, and missing telemetry env-var docs

> **Executor instructions**: Follow this plan step by step. Run every
> verification command and confirm the expected result before moving to the
> next step. If anything in the "STOP conditions" section occurs, stop and
> report — do not improvise. When done, update the status row for this plan
> in `plans/README.md` — unless a reviewer dispatched you and told you they
> maintain the index.
>
> **Drift check (run first)**: `git diff --stat 5d3661cff..HEAD -- TESTING.md ENGINEERING.md docs/content/docs`
> If plans 008/009/010/012 landed first, their doc edits overlap — reconcile,
> don't duplicate sections.

## Status

- **Priority**: P1 (cheap, actively-wrong docs mislead every contributor today)
- **Effort**: S
- **Risk**: LOW
- **Depends on**: none (docs-only; content adjusts if later plans landed)
- **Category**: docs
- **Planned at**: commit `5d3661cff`, 2026-07-03

## Why this matters

Two committed docs are actively wrong and one canonical page is silent:

1. **TESTING.md promises a file that OTLP mode doesn't write.** TESTING.md:67 says `--debug` captures everything into `~/.jackin/data/diagnostics/runs/<run-id>.jsonl` and the triage loop is "ask for the run id, read the JSONL". But the code gates the file on OTLP being INACTIVE (`crates/jackin-diagnostics/src/run.rs:151`: `persist = !otlp_active || diagnostics_file_forced()`), and `otlp` is a default cargo feature. A contributor whose shell exports `OTEL_EXPORTER_OTLP_ENDPOINT` (exactly what the operator guide `run-telemetry.mdx` tells them to do persistently) gets **no file**: `cargo xtask pty-fixture` fails file-not-found and the documented triage loop silently returns nothing. Two committed docs contradict each other in one shell.
2. **`commands/diagnostics.mdx` shows run ids that are never minted.** Its examples use `jk-run-046bca` etc.; the code mints bare six-hex ids (`run.rs:834-839`, comment: "A bare unique value — no prefix; six lowercase hex digits"), and the contributor reference + run-telemetry guide already show the real shape (`8b4766`).
3. **The env-vars reference omits telemetry entirely.** `guides/environment-variables.mdx` documents `JACKIN_*` runtime vars but none of: `OTEL_EXPORTER_OTLP_ENDPOINT` (+ per-signal + `_PROTOCOL`), `JACKIN_DIAGNOSTICS_FILE` (the ONLY way to get the JSONL back under OTLP — currently documented in a single sentence of one guide), `JACKIN_DEBUG`, `PARALLAX_RUN_ID`/`OTEL_RESOURCE_ATTRIBUTES` run-id adoption.
4. **ENGINEERING.md's telemetry hard-rule describes only the `clog!`/`cdebug!` tier**, never mentioning the structured `RunDiagnostics` stage/timing API or OTLP export — so new instrumentation lands as prose lines and misses the structured path.

## Current state

Verified claims and their code anchors:

- Persist gate: `run.rs:151` + module doc `run.rs:8-21` ("the file is the *fallback* sink, keyed on whether OTLP export is active — not on `--debug`").
- Run-id mint: `run.rs:834-839`.
- Correct existing docs to stay consistent with: `docs/content/docs/(public)/guides/run-telemetry.mdx` (accurate operator guide: endpoint vars, gRPC-only, `JACKIN_DIAGNOSTICS_FILE=1`, file-vs-backend model) and `docs/content/docs/reference/runtime/diagnostics.mdx` (accurate contributor reference, run-id example `8b4766`).
- Stale/gapped files: `TESTING.md:56-58,65-83`; `docs/content/docs/(public)/commands/diagnostics.mdx:13,25-26,46-50,72-73,90,93`; `docs/content/docs/reference/capsule/terminal-model.mdx` (also uses `jk-run`); `docs/content/docs/(public)/guides/environment-variables.mdx`; `ENGINEERING.md:64-79`; root `AGENTS.md` telemetry line.

Docs conventions (from docs/CLAUDE.md, binding):

- Never hard-wrap prose; one paragraph = one line.
- Operator pages must not leak internals; `JACKIN_DIAGNOSTICS_FILE` + on-disk paths are already on `run-telemetry.mdx` (operator guide) as a documented exception pattern — match its framing ("the file jackin❯ writes for triage") rather than introducing new internal detail; deep detail links to `/reference/runtime/diagnostics/`.
- Repo-file references in MDX use the repository-file component / site routes per docs rules; plain code spans for repo paths fail `cargo xtask docs repo-links`.

## Commands you will need

| Purpose | Command | Expected |
|---|---|---|
| Repo-link check | `cargo xtask docs repo-links` | exit 0 |
| Research sidebar check | `cargo xtask research check` | exit 0 (unchanged) |
| Docs build (optional but preferred) | `cd docs && bun install --frozen-lockfile && bun run build` | exit 0 |

## Scope

**In scope**: `TESTING.md`, `ENGINEERING.md`, root `AGENTS.md` (one line), `docs/content/docs/(public)/commands/diagnostics.mdx`, `docs/content/docs/(public)/guides/environment-variables.mdx`, `docs/content/docs/reference/capsule/terminal-model.mdx`.

**Out of scope**: any code; `run-telemetry.mdx`/`diagnostics.mdx` (already accurate — plans 008/009/012 own their future edits); the research doc under `reference/research/agent-telemetry/` (findings record — historical, don't edit); roadmap pages.

## Git workflow

- Propose branch `docs/telemetry-truth-sync`; operator confirm; `git commit -s -m "docs: sync telemetry docs with OTLP file-gating reality"`; push.

## Steps

### Step 1: TESTING.md

In the "Walking the operator through local validation" section (`TESTING.md:65-83`) and the pty-fixture section (`:50-58`), add the gating truth right where the JSONL path is promised:

> `--debug` writes `~/.jackin/data/diagnostics/runs/<run-id>.jsonl` **only when OTLP export is inactive**. If `OTEL_EXPORTER_OTLP_ENDPOINT` is set in your shell, the backend is the sink and no file is written — either unset it for JSONL-based flows (pty-fixture extraction, "agent reads the JSONL" triage) or set `JACKIN_DIAGNOSTICS_FILE=1` to write both.

Keep phrasing consistent with `run.rs:8-21`'s model. Also note the alternative triage pointer in OTLP mode: the run id + backend query.

**Verify**: `rg -n "JACKIN_DIAGNOSTICS_FILE" TESTING.md` → ≥1 hit.

### Step 2: Run-id format fixes

`commands/diagnostics.mdx`: replace every `jk-run-*` example id with bare six-hex ids (`046bca`, `8b4766`, `cold01`→ use realistic hex like `c01d0a`/`3a9f42` — hex chars only). Also fix its line 13 claim "reads run JSONL artifacts that jackin❯ already wrote" with the same OTLP caveat sentence (one line, linking to `/reference/runtime/diagnostics/`). `terminal-model.mdx`: same id-shape replacement where `jk-run` appears (content context stays).

**Verify**: `rg -rn "jk-run" docs/content/docs` → no matches.

### Step 3: Env-vars page telemetry subsection

Add a "Telemetry" subsection to `guides/environment-variables.mdx` listing (name → one-line effect → link):

- `OTEL_EXPORTER_OTLP_ENDPOINT` (+ `OTEL_EXPORTER_OTLP_{TRACES,LOGS,METRICS}_ENDPOINT`, `OTEL_EXPORTER_OTLP_*_PROTOCOL` gRPC-only note → link E016 page)
- `JACKIN_DEBUG` (backs `--debug`)
- `JACKIN_DIAGNOSTICS_FILE` (write the JSONL even when OTLP is active)
- `PARALLAX_RUN_ID` / `OTEL_RESOURCE_ATTRIBUTES` `parallax.run.id=` (adopt an external run id)
- If plan 008 landed: `JACKIN_TELEMETRY_LEVEL` / `JACKIN_TELEMETRY_CATEGORIES`; if plan 012 landed: `OTEL_METRIC_EXPORT_INTERVAL`. (Check `rg -n "JACKIN_TELEMETRY_LEVEL" crates/` — include only vars that exist in code at your HEAD.)

Each row links to `/guides/run-telemetry/` for the how-to; keep operator framing (no internals beyond the documented file-path exception).

**Verify**: `cargo xtask docs repo-links` → exit 0.

### Step 4: ENGINEERING.md + AGENTS.md pointer

At the end of the ENGINEERING.md telemetry section (`:64-79`): one short paragraph — beside `clog!`/`cdebug!`, structured run telemetry goes through the `RunDiagnostics` stage/timing/error APIs and exports over OTLP; new instrumentation for operations (durations, outcomes, failures) uses that path, not bare prose lines; pointer to `docs/content/docs/reference/runtime/diagnostics.mdx`. Root `AGENTS.md` two-tier bullet: append "; structured run/OTLP tier → ENGINEERING.md" (keep the slim-index style).

**Verify**: `rg -n "RunDiagnostics" ENGINEERING.md` → ≥1 hit.

### Step 5: Gate

**Verify**: `cargo xtask docs repo-links` exit 0; optional `bun run build` exit 0; `git diff --stat` touches only the six in-scope files.

## Test plan

Docs-only: the verification commands above are the tests (repo-links + optional build + greps). No Rust tests.

## Done criteria

- [ ] TESTING.md states the OTLP file gate + `JACKIN_DIAGNOSTICS_FILE` recovery
- [ ] `rg -rn "jk-run" docs/content/docs TESTING.md` → no matches
- [ ] environment-variables.mdx has the Telemetry subsection (only vars that exist at HEAD)
- [ ] ENGINEERING.md names the structured tier; AGENTS.md pointer updated
- [ ] `cargo xtask docs repo-links` exit 0
- [ ] `plans/README.md` updated

## STOP conditions

- A later plan (008/010/012) is mid-flight touching the same doc sections on another branch — coordinate via the operator rather than racing.
- `cargo xtask docs repo-links` demands the repository-file component for a reference you added and the component usage is unclear — copy an existing usage from `run-telemetry.mdx` rather than inventing syntax; if none matches, STOP.

## Maintenance notes

- The stale-docs class here came from code changing under docs (persist gate landed; TESTING.md didn't move). The per-PR docs gate (PULL_REQUESTS.md) covers code PRs; this plan is the back-fill. Reviewer: check no NEW promise about file paths is unconditional.
- The research doc `parallax-observability-findings.mdx` stays as-is by design (dated findings record); when the program completes, a follow-up may add a status line at its top pointing at what shipped.
