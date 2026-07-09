# Plan 029: Phase 5 — docs drift reconciliation: five measured doc/code disagreements

> **Executor instructions**: Follow this plan step by step. Run every
> verification command and confirm the expected result before moving to the
> next step. If anything in the "STOP conditions" section occurs, stop and
> report — do not improvise. When done, update the status row for this plan
> in `plans/code-health/README.md`.
>
> **Drift check (run first)**: `git diff --stat c856acc9d..HEAD -- README.md docs/content/docs/roadmap/index.mdx docs/content/docs/reference/getting-oriented/codebase-map.mdx crates/jackin-capsule/README.md crates/jackin-core/src/env_model.rs`
> On a mismatch with the "Current state" excerpts, treat it as a STOP condition.

## Status

- **Priority**: P3
- **Effort**: M (five independent S/M items)
- **Risk**: LOW (docs-only; code is never edited — code is the source of truth)
- **Depends on**: none
- **Category**: docs
- **Planned at**: commit `c856acc9d`, 2026-07-09

## Why this matters

Five first-wave documentation-drift findings, each a measured doc/code disagreement that misleads its reader today: the root README links three non-existent doc routes; the roadmap overview claims Apple-backend lifecycle dispatch is still open while the code has it wired; the operator env-vars guide under-documents the reserved-name rejection list (an operator setting a reserved name hits an undocumented hard rejection); the capsule crate README is 711 bytes of intro prose missing every mandatory section the per-crate convention requires; and the Codebase Map's tier DAG and extraction-ledger numbers are stale. Docs rules here are hard rules (docs/CLAUDE.md: "when they differ, code is source of truth"; crates/AGENTS.md README template is mandatory), so each fix direction is predetermined — this is reconciliation, not judgment.

## Current state

Verified at the planning commit (re-verify each before editing — docs move).

1. **Root README dead links** — `README.md:77`: "Behind jackin❯ (Internals) — [Architecture](https://jackin.tailrocks.com/reference/architecture/), [Codebase Map](https://jackin.tailrocks.com/reference/codebase-map/), [Roadmap](https://jackin.tailrocks.com/reference/roadmap/)". Real routes: `/reference/getting-oriented/architecture/`, `/reference/getting-oriented/codebase-map/`, `/roadmap/` (confirm each by finding the MDX file under `docs/content/docs/` and deriving the route per file-based routing; parenthesized dirs are absent from URLs).
2. **Roadmap overview vs Apple-backend code** — `docs/content/docs/roadmap/index.mdx:93` says "roadmap tracks command-path lifecycle dispatch (reconnect/eject/purge) and Phase 0 hardware validation". The audit found lifecycle dispatch already wired (`crates/jackin-runtime/src/runtime/cleanup.rs` and `attach.rs` dispatch through `backend_for_state`). Verify: `rg -n 'backend_for_state' crates/jackin-runtime/src/runtime/{cleanup,attach}.rs` — if present in both, the overview line and the item page (`docs/content/docs/roadmap/**/apple-container-backend.mdx` — locate it) must stop listing lifecycle dispatch as open (Phase 0 hardware validation genuinely remains).
3. **Reserved env names under-documented** — code truth, `crates/jackin-core/src/env_model.rs:65-85` `RESERVED_RUNTIME_ENV_VARS`: 20 entries — 10+ named `JACKIN_*` constants (env/dind-hostname/container-name/instance-id/agent/agent-codename/role/workdir/git-coauthor-trailer/git-dco/network-mode/allowed-hosts/firewall-installed/network-enforcement/sudo — resolve each constant's actual string by reading the const definitions above line 65) plus `DOCKER_HOST`, `DOCKER_TLS_VERIFY`, `DOCKER_CERT_PATH`, and the testcontainers host-override name. The operator guide documenting env vars was cited as `guides/environment-variables.mdx` but was NOT found at that path at planning time — Step 3 locates it first (`rg -ln 'environment variable' docs/content/docs -g '*.mdx'` filtered to the operator-guide group). Audience rule (docs/CLAUDE.md): the operator page lists the NAMES an operator cannot set and the observed behavior (rejection), never the Rust const identifiers or `env_model.rs` internals.
4. **Capsule README missing mandatory sections** — `crates/jackin-capsule/README.md` is 711 bytes (verified), intro paragraphs only. The `crates/AGENTS.md` template requires: What this crate owns / Architecture tier and allowed dependencies / Structure (clickable table with Tests column) / Public API / How to verify — for a 31-file crate. Exemplar to match: `crates/jackin-core/README.md` (largest conforming README). Tier/dependency facts come from `crates/jackin-capsule/Cargo.toml` + its lib.rs header (T4/binary tier; deps: agent-status, core, diagnostics, protocol, term, tui, usage).
5. **Codebase Map staleness** — `docs/content/docs/reference/getting-oriented/codebase-map.mdx`: audit found the tier DAG omits 4 crates (`jackin-build-meta`, `jackin-dev`, `jackin-pr-trailers`, `jackin-tui-lookbook`), extraction-ledger counts claim "94 files/21,382 LOC" vs actual 77/24,417, and it lists a root `src/`+`tests/` that no longer exist. Re-verify each claim against the live file before editing; fix = correct the DAG (add the 4 crates at their plan-012 tiers if that landed, else at graph-derived positions), delete or refresh stale counts (prefer DELETING hand-maintained counts the roadmap already wants generated — Phase 5 item 5), remove the phantom root dirs. Do NOT restructure the page into the index form (that is the separate README→Fumadocs program); minimal truth-restoring edits only.

Docs conventions binding every edit: no hard-wrapped prose; site-absolute routes for doc links; repo files linked via the RepoFile component in MDX; `cargo xtask roadmap audit` + `docs repo-links` must stay green; roadmap-item status changes must keep `roadmap/index.mdx` and the item page in agreement.

## Commands you will need

| Purpose | Command | Expected on success |
|---|---|---|
| Docs gates | `cargo xtask roadmap audit && cargo xtask docs repo-links && cargo xtask research check` | all pass |
| Agents/README gates | `cargo run -p jackin-xtask -- lint agents` (+ `docs brand`/`docs specs` if plan 015 landed) | OK |
| Link check (if bun env available) | `cd docs && bun run check:links:fresh` | no dead links (else note skipped — CI covers it) |
| Full local gate | `cargo xtask ci --fast` | `ci gate OK` |

## Scope

**In scope**: `README.md` (the three links only), `docs/content/docs/roadmap/index.mdx` + the apple-container-backend item page (status lines only), the located operator env-vars page (reserved-names section), `crates/jackin-capsule/README.md` (full template conformance), `codebase-map.mdx` (truth-restoring edits), `plans/code-health/README.md`.

**Out of scope**: ANY Rust code; the README→Fumadocs pipeline and map-becomes-index restructure; other roadmap items' statuses; DOCS findings not listed here.

## Git workflow

- Branch off `main`: `docs/drift-reconciliation`.
- One `docs(...)` commit per numbered item, `-s`, push each. PR to `main`; do not merge.

## Steps

### Step 1: README links
Fix the three URLs to the real routes (derive each from the MDX file location). **Verify**: `rg -n 'reference/architecture/|reference/codebase-map/|reference/roadmap/' README.md` → 0 matches; the three replacement paths each have a matching MDX file.

### Step 2: Apple-backend status
Run the `backend_for_state` check. If wired in both files: update `roadmap/index.mdx:93` and the item page so lifecycle dispatch reads as shipped and only Phase 0 hardware validation remains open; keep overview and item page in agreement (docs rule). If NOT wired in both: STOP — the audit claim reversed. **Verify**: `cargo xtask roadmap audit` → pass; `rg -n 'lifecycle dispatch' docs/content/docs/roadmap/index.mdx` shows the corrected phrasing.

### Step 3: Reserved env names
Locate the operator env-vars page. Cross-check its reserved/blocked list against the 20 resolved names from `env_model.rs` (resolve each `*_ENV_NAME` const to its string). Add the missing names in the page's existing list format, describing operator-visible behavior ("these names are reserved by the runtime and rejected if set") — no Rust identifiers, no file paths. If no operator page documents env vars at all, STOP and report (adding a new page is an information-architecture call). **Verify**: every resolved name appears on the page: for each of the 20 strings, `rg -c '<NAME>' <page>` ≥ 1.

### Step 4: Capsule README
Rewrite `crates/jackin-capsule/README.md` to the crates/AGENTS.md template, modeled on `crates/jackin-core/README.md`: owns-list from the crate's actual modules (read `src/` listing + lib.rs/main.rs headers), tier + allowed deps from Cargo.toml, Structure table with every top-level module linked and Tests column per the `<mod>/tests.rs` rule, Public API from lib.rs exports, verify commands (`cargo nextest run -p jackin-capsule`, the e2e note). Right-size: this is a big crate — table rows for top-level modules only, one line each; link the capsule internals reference page for design rationale instead of inlining. **Verify**: `cargo run -p jackin-xtask -- lint agents` OK; every Structure-table link resolves (`cargo xtask docs repo-links` covers repo links in MDX, but this is a crate README — click-check via `ls` per linked path: `for p in $(grep -o 'src/[a-z_/.]*' crates/jackin-capsule/README.md); do test -e crates/jackin-capsule/$p || echo MISSING $p; done` → no output).

### Step 5: Codebase Map
Re-verify each staleness claim against the live MDX; apply the minimal corrections (add 4 crates to the DAG at correct tiers, delete stale counts, remove phantom root dirs). **Verify**: `rg -n 'jackin-build-meta|jackin-pr-trailers|jackin-dev|jackin-tui-lookbook' docs/content/docs/reference/getting-oriented/codebase-map.mdx` → all four present; `rg -n '21,382|94 files' …codebase-map.mdx` → 0; `cargo xtask docs repo-links` → pass.

### Step 6: Index + gates
Strike the five DOCS-* deferred entries in `plans/code-health/README.md` (→ this plan). Full docs gate run.
**Verify**: `cargo xtask roadmap audit && cargo xtask docs repo-links && cargo xtask research check` → pass; `cargo xtask ci --fast` → `ci gate OK`.

## Test plan

Docs-only: the gates above are the tests. If plan 023 (docs-command drift harness) landed, run `cargo nextest run -p jackin -E 'binary(docs_commands)'` too — Step 3's page edits may touch fenced commands.

## Done criteria

- [ ] All five items corrected with their per-step verifications green
- [ ] Docs gate trio + agents gate pass; `cargo xtask ci --fast` → `ci gate OK`
- [ ] Five DOCS-* ledger entries struck; index row updated

## STOP conditions

- Step 2's code check fails (claim reversed) — report, don't guess.
- Step 3 finds no operator env-vars page (IA decision needed).
- Any codebase-map claim turns out already fixed (skip that sub-item, note it) or the page has been restructured into the index form (the minimal-edit instructions no longer apply — report).
- A capsule README owns-list item requires understanding you don't have from module headers (mark that row's Owns cell from the module's own `//!` line verbatim; if a module has none, write the row with "(no module header — see plan 016)" rather than inventing).

## Maintenance notes

- Plan 015's README-presence gate and the deferred freshness-vs-diff check keep item 4 from rotting again; the deferred README→Fumadocs pipeline (P56-02) supersedes item 5's hand-maintenance entirely — this plan just stops the bleeding.
- Reviewer should scrutinize: Step 3's operator-audience phrasing (no internals leakage — docs/CLAUDE.md hard rule) and Step 4's Structure-table link correctness.
