# Plan 005: Runtime/OSC/Crossterm/lookbook adapters and first `main` publish (Stage 2, part 3 of 3)

> **Executor instructions**: Follow this plan step by step. Run every verification command and confirm the expected result before moving on. If anything in "STOP conditions" occurs, stop and report — do not improvise. When done, update this plan's row in `plans/shared-tui-extraction/README.md`.
>
> **Drift check (run first)**: confirm plan 004 is DONE; extraction-clone workspace fully green; `tailrocks/termrock` still empty. This plan ends with the **first public push** — every step before the final one stays local.

## Status

- **Priority**: P1
- **Effort**: L
- **Risk**: HIGH (first public, irreversible push; forward-only repairs afterwards)
- **Depends on**: plans/shared-tui-extraction/004-stage2-widget-neutralization.md
- **Category**: migration
- **Planned at**: commit `03928e9dd`, 2026-07-15

## Why this matters

Final third of **Stage 2** of the [Shared TUI Extraction roadmap item](../../docs/content/docs/roadmap/(operator-surface)/shared-tui-extraction.mdx) per [ch. 04, "Stage 2"](../../docs/content/docs/reference/research/shared-tui-extraction/04-extraction-migration-plan.mdx): Batch D from [ch. 08](../../docs/content/docs/reference/research/shared-tui-extraction/08-migration-evidence-and-gates.mdx) — the executor-neutral `runtime` module, the typed `osc` surface, the optional `crossterm` integration, and the neutral lookbook package — then the Stage 2 exit gate: pushing the first neutral, scanned, buildable TermRock `main` head. After this push, history is immutable: no force pushes, no rewrites, repairs land forward ([Decision 18](../../docs/content/docs/reference/research/shared-tui-extraction/05-decision-record.mdx)).

## Current state

- Extraction clone after plan 004: foundations + widgets neutral and green; `error_dialog`'s raw-overlay seam marked `TODO(osc)`; Crossterm conversion stubs behind the feature flag from plan 003.
- Donor `runtime.rs` (10.3K, verified at freeze) mixes executor-neutral contracts (`Dirty`, `UpdateResult`, `SubscriptionPoll`, `Subscription`, `Component`, `View`, `drive_frame`, `drive_render`) with Tokio channel/task helpers and a fallback runtime. Split per [Decision 3](../../docs/content/docs/reference/research/shared-tui-extraction/05-decision-record.mdx): contracts + both pure-Ratatui drivers extract into base `runtime` (optional by use, not a feature, referenced by no widget); Tokio receiver impls/spawn helpers/fallback runtime **stay donor-side** (re-homed to `jackin-console` in plan 007). Coherence corner: the shared module ships a subscription **closure adapter** (plus std-channel adapter) because consumers cannot implement the foreign `Subscription` trait for foreign Tokio receiver types.
- Donor OSC surface: `POINTER_DEFAULT`/`POINTER_HAND` + `encode_osc52_clipboard_write` re-exported from `jackin-core` (`ansi.rs:14`, `ansi.rs:193`), root `PointerShape`, and raw post-frame overlay byte vectors from error/container-info dialogs. Target ([ch. 06, "Typed terminal requests"](../../docs/content/docs/reference/research/shared-tui-extraction/06-public-api-and-refactoring.mdx)): `osc` module with typed hyperlink (stable ID, rect, URL), pointer-shape, and clipboard-write request values plus **pure encoders**; emission stays consumer-owned; no widget returns raw escape bytes.
- Donor `terminal_modes.rs` (870B — two mouse helpers) → reimplement as the partial-initialization-safe scoped session per [ch. 06, "Terminal ownership"](../../docs/content/docs/reference/research/shared-tui-extraction/06-public-api-and-refactoring.mdx): record each mode only after successful enable; on halfway failure restore already-enabled modes; explicit fallible `restore()`; `Drop` as idempotent best-effort fallback; reverse-order restoration; alternate-screen and inline options; no global panic hook/signal handler/logger.
- Donor lookbook (`crates/termrock-lookbook` after rename): `main.rs` (27.6K, interactive terminal browser + SVG rendering), `stories.rs` (26.7K, many product-flavored stories), `svg.rs` (8.8K, **duplicated hardcoded hex color table**), `interactors.rs`, `tests.rs`. Current CLI: `tui-lookbook --terminal | tui-lookbook [out-dir] | tui-lookbook --check <dir>`. Target ([ch. 03, "Lookbook model"](../../docs/content/docs/reference/research/shared-tui-extraction/03-target-repository.mdx), [Decision 2](../../docs/content/docs/reference/research/shared-tui-extraction/05-decision-record.mdx)): `termrock-lookbook` with subcommands `terminal`, `list`, `render --out <dir>`, `check --dir <dir>`; donor flags preserved only during extraction-parity validation, then removed.
- 29 committed SVGs (retained at `docs/public/tui-lookbook/` in the filtered history) are the parity seed; product-specific stories (agents, roles, workspaces, containers, OnePassword) are **replaced with neutral data** before becoming public stories; `jackin❯` keeps the product stories in-tree until Stage 4.

## Commands you will need

Plan 003's table, plus:

| Purpose | Command | Expected on success |
|---|---|---|
| Feature powerset | `cargo hack check --workspace --feature-powerset --all-targets --locked` | exit 0 |
| Lookbook render | `cargo run -p termrock-lookbook -- render --out /tmp/tr-a` (twice, to `/tmp/tr-b`) | `diff -r /tmp/tr-a /tmp/tr-b` empty (byte-determinism) |
| Lookbook check | `cargo run -p termrock-lookbook -- check --dir docs/public/component-previews` | exit 0 |
| Secret scan (full history) | `gitleaks git --redact .` | zero findings |
| First publish | `git push termrock main` | only `main`, no tags/branches |

## Scope

**In scope** (extraction clone): `crates/termrock/src/{runtime,osc,crossterm}/`; `crates/termrock-lookbook/` (CLI restructure, neutral stories, theme-derived SVG colors); story/SVG relocation to `docs/public/component-previews/` + `fixtures/renders/`; the first push to `tailrocks/termrock` `main`; Stage-2 checkpoint evidence + pinned-revision record in `jackin❯`.

**Out of scope**: Fumadocs site content and full catalog coverage (plan 006); Tokio anything in the shared crate; product stories as public stories; `jackin❯` workspace changes beyond evidence; donor-flag removal from the lookbook CLI *before* extraction parity is validated; quality-backlog fixes.

## Git workflow

Extraction clone `main`: DCO-signed, buildable commits. The final push publishes `main` **only** — verify no other refs are pushed. After the push, TermRock `main` is immutable (no force pushes ever); later checkpoints (plans 006, 009) push forward-only after local gates pass.

## Steps

### Step 1: Base `runtime` module

Create `crates/termrock/src/runtime/{contract.rs,subscription.rs,frame.rs}`: move `Dirty`, `UpdateResult<E>`, `SubscriptionPoll`, `Subscription` (+ std-channel and closure adapters), `Component<Ev, Msg>`, `View<Model>`, and the persistent `drive_frame` + one-shot closure-based `drive_render` drivers from donor `runtime.rs`. Both drivers pure Ratatui, sharing one canonical draw path; `drive_render` adapts short-lived modal/prompt renderers onto `drive_frame`, not a second loop ([Decision 3](../../docs/content/docs/reference/research/shared-tui-extraction/05-decision-record.mdx)). Delete Tokio helpers from the TermRock tree (they remain donor-side in `jackin❯` for plan 007 re-homing). No widget may reference `runtime`.

**Verify**: `rg -ln 'runtime::' crates/termrock/src/widgets` → empty; `cargo tree -p termrock --no-default-features | rg -i tokio` → empty; moved runtime tests pass.

### Step 2: `osc` module

Create `crates/termrock/src/osc/{request.rs,encode.rs}`: typed `PointerShape` naming (donor root type + the `POINTER_DEFAULT`/`POINTER_HAND` semantics reimplemented from `jackin-core` with provenance lineage), hyperlink region values (stable ID, rect, URL) derived from widget layout functions, clipboard-write requests, and pure OSC 8/22/52 encoders (reimplementing `encode_osc52_clipboard_write` semantics; record lineage). Replace the `TODO(osc)` seams: `MessageDialog`/`DetailTable` return typed link regions/outcomes instead of raw overlay byte vectors. Encoders perform no I/O; emission policy is consumer-owned.

**Verify**: `rg -n 'Vec<u8>' crates/termrock/src/widgets` → no raw-escape widget outcomes; encoder unit tests assert exact escape bytes for known inputs; `rg -n 'TODO\(osc\)' crates/termrock/src` → empty.

### Step 3: Optional `crossterm` integration

Create `crates/termrock/src/crossterm/{event.rs,backend.rs,session.rs}` behind the `crossterm` feature: pure event→`KeyChord`/pointer conversion (key, mouse, resize, focus, paste where supported), `CrosstermBackend` construction/re-export via `ratatui-crossterm`, and the scoped partial-initialization-safe session/builder implementing all eight rules from [ch. 06, "Terminal ownership"](../../docs/content/docs/reference/research/shared-tui-extraction/06-public-api-and-refactoring.mdx) ("Current state" summarizes them). Layering rules from ch. 06 are hard: the feature adds adapters/session only — no widget/theme/layout/input type behind the flag, no panic hook, no changed base behavior.

**Verify**: `cargo check -p termrock --no-default-features` and `--features crossterm` both green; `cargo hack check --workspace --feature-powerset --all-targets --locked` → exit 0; `rg -ln 'cfg\(feature = "crossterm"\)' crates/termrock/src/{widgets,style,layout,input,interaction,scroll,text}` → empty.

### Step 4: Neutral lookbook package and CLI

1. Restructure `crates/termrock-lookbook` into `src/lib.rs` (typed story registry, harness, SVG/golden renderer, fixture runner) + `src/bin/termrock-lookbook.rs` with subcommands `terminal`, `list [--format json]`, `render --out <dir>`, `check --dir <dir>`; keep the donor `--terminal`/`[out-dir]`/`--check <dir>` flags as temporary aliases for extraction-parity validation (removed in plan 009 — pre-release, no indefinite shims, [Decision 2](../../docs/content/docs/reference/research/shared-tui-extraction/05-decision-record.mdx)).
2. **Derive the SVG color table from `Theme` values**, deleting `svg.rs`'s hardcoded hex table, and add the required test asserting the committed donor SVGs are **byte-identical** under the consolidation ([ch. 06, "Theme API"](../../docs/content/docs/reference/research/shared-tui-extraction/06-public-api-and-refactoring.mdx)).
3. Debrand stories: for every story in `stories.rs` flagged `product_terms = true` in plan 001's `story-manifest.json`, create the neutral replacement (generic labels/data, same component states); drop product-only stories from the public registry. The lookbook consumes **public** `termrock` API only — no crate-private access ([ch. 08, Batch D](../../docs/content/docs/reference/research/shared-tui-extraction/08-migration-evidence-and-gates.mdx)).
4. Relocate assets: neutral generated SVGs → `docs/public/component-previews/`; buffer-fixture goldens → `fixtures/renders/`; `render` emits a machine-readable manifest alongside SVGs.
5. Regenerate: `render --out docs/public/component-previews`, commit SVGs + manifest; run the double-render byte-determinism check.

**Verify**: `termrock-lookbook list --format json` → valid JSON, unique story IDs; `check --dir docs/public/component-previews` → exit 0; double-render diff empty; `rg -in 'agent|workspace|container|role|onepassword' crates/termrock-lookbook/src docs/public/component-previews` → no public story hits.

### Step 5: Pre-publish audit (the Stage 2 exit gate, locally)

1. Full gate sweep: fmt, clippy `-D warnings`, nextest all-features, doctest, feature-powerset, MSRV check (`cargo +1.95 check` via toolchain override if available, else document), rustdoc `-D warnings`, `cargo deny`, `reuse lint`, `cargo shear --deny-warnings`.
2. Dependency audit: `cargo tree -p termrock --no-default-features` and `--all-features` — no `jackin*`, no Tokio anywhere, Crossterm only under the feature; save both trees to `dependency-tree.txt` in the TermRock repo.
3. History scan (full history including inherited commits): gitleaks; oversized-object scan; `rg -il 'tablepro|tableplus|zedis'` → empty.
4. DCO/provenance: every commit after the boundary signed + was buildable; `provenance.toml` complete (filter command, boundary, all `[[reimplemented]]` lineage rows).
5. Confirm quality backlog untouched: char-count sites and color-only focus still present, byte-compatible.

**Verify**: every item green and recorded in a `PUBLISH-AUDIT.md` (or equivalent) committed in the TermRock repo.

### Step 6: Publish `main` and pin the checkpoint in `jackin❯`

1. `git push termrock main` — first and only ref. Confirm: `gh api repos/tailrocks/termrock/branches --jq '.[].name'` → `["main"]`; `gh api repos/tailrocks/termrock/tags --jq 'length'` → 0.
2. Wait for the plan-003 CI workflows to run on the pushed head; both aggregators (`rust-required`, `docs-required` placeholder) must pass. Fix forward with new signed commits if not.
3. In `jackin❯` on `feature/shared-tui-extraction`: append to `evidence/stage2-checkpoints.md` the published TermRock `main` head SHA (full 40-char — this is the first pinnable revision), CI results, and audit summary; tick the roadmap Stage 2 checkbox in `docs/content/docs/roadmap/(operator-surface)/shared-tui-extraction.mdx`; commit + push; note the checkpoint in PR #794 (comment or body update).

**Verify**: TermRock `main` green; `jackin❯` evidence committed; PR #794 records the immutable commit.

## Test plan

- `runtime`: driver tests moved from donor `runtime.rs` (pure-Ratatui parts); std-channel + closure adapter tests; a compile test proving a consumer type can satisfy `Subscription` via the closure adapter.
- `osc`: exact-byte encoder tests (OSC 8 open/close, OSC 22 shapes, OSC 52 base64 payload vs. the `jackin-core` original's output for identical inputs); link-region derivation test (layout fn output == painted region).
- `crossterm` session: unit tests for mode bookkeeping (enable-order recording, reverse restore, partial-failure restore); PTY tests are plan 006 scope — mark with `#[ignore]` if scaffolded early.
- lookbook: SVG byte-identity test (theme-derived colors vs. committed donor SVGs); double-render determinism; `list`/`check` behavior tests (donor lookbook `tests.rs` is the pattern).
- Verification: `cargo nextest run --workspace --all-features --locked` → all pass.

## Done criteria

Stage 2 exit gate ([ch. 04](../../docs/content/docs/reference/research/shared-tui-extraction/04-extraction-migration-plan.mdx)):

- [ ] `cargo tree` + source audit show no Tailrocks product dependency; no Tokio; Crossterm feature-gated only
- [ ] Neutral tests/render fixtures pass; committed SVGs byte-identical under theme-derived colors
- [ ] First published TermRock `main` head is neutral, standalone, buildable, CI-green
- [ ] Only `main` exists publicly — no donor branch, no tag, no raw filtered tip advertised
- [ ] Widget escape-byte outcomes eliminated; typed `osc` surface is the only escape producer
- [ ] Quality backlog carried through untouched
- [ ] Pinned-revision record + roadmap Stage 2 checkbox committed in `jackin❯`; index row → DONE

## STOP conditions

- Any pre-publish audit item fails (secret finding, license issue, reference-project source, unbuildable head) — do not push.
- The SVG byte-identity test fails under theme-derived colors (means phosphor preset values drifted — fix the preset, never regenerate the fixtures).
- You are about to push a tag, a second branch, or force-push — prohibited.
- CI on the pushed head fails in a way requiring history rewrite — repairs are forward-only; if that seems impossible, stop and report.
- A widget still needs a raw-byte outcome the `osc` types cannot express — API gap; reopen with the operator, don't improvise a `Vec<u8>` escape hatch.

## Maintenance notes

- The published head SHA in `stage2-checkpoints.md` is the candidate first pin for plan 007 (plan 006 usually advances it — always pin the latest green checkpoint).
- Reviewers: scrutinize the OSC encoder byte tests against the `jackin-core` originals and the session partial-init restore paths — these are the two easiest places to silently diverge from donor behavior.
- Donor CLI flag aliases in `termrock-lookbook` are scheduled for deletion in plan 009; do not document them publicly in plan 006.
