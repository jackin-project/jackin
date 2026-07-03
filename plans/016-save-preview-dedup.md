# Plan 016: One diff-preview pipeline for workspace and settings save confirmations

> **Executor instructions**: Follow this plan step by step. Run every
> verification command and confirm the expected result before moving to the
> next step. If anything in the "STOP conditions" section occurs, stop and
> report — do not improvise. When done, update the status row for this plan
> in `plans/README.md` — unless a reviewer dispatched you and told you they
> maintain the index.
>
> **Drift check (run first)**: `git diff --stat a2ec1b237..HEAD -- crates/jackin-console/src/tui/components/save_preview.rs`
> On mismatch with "Current state": STOP.

## Status

- **Priority**: P2
- **Effort**: L
- **Risk**: MED (preview text is snapshot-tested; output must be byte-identical)
- **Depends on**: none (independent; complements plan 015's parity theme)
- **Category**: tech-debt
- **Planned at**: commit `a2ec1b237`, 2026-07-03
- **Execution status**: DONE — added byte-level characterization tests, recorded the `diff_view` verdict, split the save-preview module, and shared the semantic diff-count path.

## Why this matters

`save_preview.rs` is a 1467-line module holding two near-parallel "confirm changes" pipelines — one for the workspace editor, one for settings — plus triplicate-ish count helpers. The two previews are exactly the parity surface `dialogs.mdx` §"Settings ↔ Workspace Editor Parity" protects: a diff-format or count fix in one body silently skips the other, and every save-preview vocabulary change wades through one giant file. This plan extracts scope-neutral diff-row/count primitives and collapses the two `*_save_lines` bodies onto them, byte-identical output.

## Current state

`crates/jackin-console/src/tui/components/save_preview.rs`, 1467 lines (verified inventory):

- Workspace pipeline: `workspace_save_preview` (`:68`), `build_workspace_save_lines` (`:142`), `workspace_mount_diffs_preview` (`:171`), `workspace_auth_change(s)` (`:261,:344`), `workspace_env_preview` (`:303`), `workspace_save_lines` (`:737` — ~235 lines), `append_workspace_auth_lines` (`:972`).
- Settings pipeline: `settings_save_preview` (`:552`), `build_settings_save_lines` (`:636`), `settings_env_preview` (`:705`), `settings_save_lines` (`:1008` — ~150 lines), `settings_mount_diff_lines` (`:1298`), and per-tab stats `settings_{general,mount,env,auth,trust}_stats` (`:1162-:1227`).
- Shared-ish helpers already: `credential_presence/label` (`:275,:289`), `source_folder_text` (`:294`), `collapse_section_lines/removal_lines` (`:493,:507`), `summarize_diff_counts` (`:1190`).
- Duplicated count helpers: `mount_diff_counts` (`:1242`), `env_config_diff_counts` (`:1263`), `env_map_diff_counts` (`:1279`) — plus the workspace pipeline computing equivalent counts inline.
- Consumed by `ConfirmSaveState` previews (`input/save.rs`, `input/global_mounts.rs` — the two confirm flows).

Related investigation (LOW confidence, resolve in Step 0): the shared `crates/jackin-tui/src/components/diff_view.rs` renders text diffs; `save_preview` emits *semantic config-change rows* (mount/env/auth add/remove/modify). These are probably legitimately distinct abstractions — confirm and record, so future audits stop flagging it.

## Commands you will need

| Purpose | Command | Expected |
|---|---|---|
| fmt / clippy | `cargo fmt --check` / `cargo clippy --all-targets --all-features -- -D warnings` | exit 0 |
| Tests | `cargo nextest run -p jackin-console` then full | pass |

## Scope

**In scope**:
- `save_preview.rs` — restructure into focused siblings per repo convention (coordinator file + `save_preview/` submodules; the repo enforces "zero `mod.rs`", so follow the existing pattern: `save_preview.rs` as coordinator + `save_preview/<part>.rs` files — mirror how other split modules in this crate do it, e.g. look at `screens/editor/view/` layout)
- Extraction of a scope-neutral core: `DiffPreview` builder (added/removed/modified row sets → `Vec<Line>`), one count-summary path (`summarize_diff_counts` already exists — feed everything through it), one mount-row / env-row / auth-row line vocabulary
- `docs/content/docs/reference/getting-oriented/codebase-map.mdx` — module split entry (repo rule)

**Out of scope**:
- `ConfirmSaveState` (plan 014) and its call sites — the preview payload types (`WorkspaceSavePreview`, `SettingsSavePreview`) keep their public shape.
- Changing any preview wording/format — byte-identical output is the acceptance bar.
- `diff_view.rs`.

## Git workflow

Branch (operator confirm): `refactor/save-preview-dedup`. `git commit -s` + push; commit per extraction.

## Steps

### Step 0: Resolve the diff_view question (15 minutes, then record)

Read `diff_view.rs`'s row model and `workspace_save_lines`' output shape. Expected conclusion: distinct abstractions (text-diff renderer vs semantic change summary). Record the verdict as a sentence in the module doc of the new coordinator file ("not built on `diff_view` because …"). If, unexpectedly, `diff_view` CAN express these rows cleanly — STOP and report; that changes the plan.

### Step 1: Characterize

The two `*_save_lines` outputs must not change. Confirm existing snapshot coverage: `rg -n 'save_lines|save_preview' crates/jackin-console/src/tui --glob '*test*'`. If either pipeline lacks a test that pins full output for a representative change-set (mounts added+removed, env modified, auth changed, collapse present), ADD those characterization tests FIRST, against the current code.

**Verify**: new tests pass against unmodified code; commit them separately.

### Step 2: Extract the scope-neutral core

Create `save_preview/diff.rs` (or similarly named): the `DiffPreview` builder — takes typed added/removed/modified sets per domain (mounts/env/auth) and emits the line vocabulary both pipelines share (the `+`/`-`/`~` rows, count summaries via `summarize_diff_counts`, collapse lines). Fold `mount_diff_counts`/`env_config_diff_counts`/`env_map_diff_counts` and the workspace pipeline's inline equivalents into it — one counting implementation.

**Verify**: `cargo nextest run -p jackin-console` — characterization tests green.

### Step 3: Collapse the two pipelines

Rewrite `workspace_save_lines` (`:737`) and `settings_save_lines` (`:1008`) as thin drivers: build domain diff-sets from their respective preview structs, feed the shared builder, append scope-specific sections (workspace name/roles summary; settings general/trust stats) via small local fns. Delete the now-dead parallel helpers.

**Verify**: characterization tests still byte-green; `cargo nextest run -p jackin-console` full pass.

### Step 4: File split + map

Arrange as coordinator + submodules (`diff.rs`, `workspace.rs`, `settings.rs`, shared `rows.rs` — adjust to what fell out naturally); update `codebase-map.mdx`.

**Verify**: fmt/clippy/full nextest exit 0; `wc -l` of the largest resulting file < ~500 (record numbers in PR body).

## Test plan

- Step 1 characterization tests are the core: full-output pins for both pipelines across a representative change-set each, plus empty-diff ("no changes") and single-domain cases.
- Keep all existing preview tests; zero expectation changes anywhere.

## Done criteria

- [x] fmt / clippy / `cargo nextest run` exit 0 with ZERO preview-output expectation changes
- [x] One counting implementation: `rg -n 'fn .*diff_counts' crates/jackin-console/src` → 1 (or 0 if folded into the builder)
- [x] `workspace_save_lines` and `settings_save_lines` each < ~60 lines (drivers, not builders)
- [x] Codebase-map updated for the split
- [x] Step 0 verdict recorded in module docs
- [x] `plans/README.md` updated

## STOP conditions

- Any characterization test diff at any step — the refactor changed output; find the divergence, do not update the pin.
- The two pipelines turn out to differ *semantically* for the same domain (e.g. settings mount diff includes scope rows workspace doesn't) in a way the shared builder can't express as data — report the specific divergence; it may be a real parity bug to surface rather than paper over.

## Maintenance notes

- Future preview vocabulary changes land in the shared builder once.
- Reviewer: the two driver fns should read as "assemble sections"; any residual span construction in them is a miss.
- Deferred: rendering previews through a richer widget (scrollable body is already handled by `ConfirmSaveState`).
