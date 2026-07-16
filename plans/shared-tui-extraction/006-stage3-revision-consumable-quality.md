# Plan 006: Revision-consumable quality — docs catalog, examples, gates (Stage 3)

> **Executor instructions**: Follow this plan step by step. Run every verification command and confirm the expected result before moving on. If anything in "STOP conditions" occurs, stop and report — do not improvise. When done, update this plan's row in `plans/shared-tui-extraction/README.md`.
>
> **Drift check (run first)**: confirm plan 005 is DONE; TermRock `main` is published and green; `evidence/stage2-checkpoints.md` records the published head. TermRock work continues in the extraction clone with forward-only pushes to `main` at green checkpoints.

## Status

- **Priority**: P1
- **Effort**: L
- **Risk**: MED
- **Depends on**: plans/shared-tui-extraction/005-stage2-adapters-and-first-publish.md
- **Category**: migration
- **Planned at**: commit `03928e9dd`, 2026-07-15

## Why this matters

**Stage 3** of the [Shared TUI Extraction roadmap item](../../docs/content/docs/roadmap/(operator-surface)/shared-tui-extraction.mdx) per [ch. 04, "Stage 3: Establish revision-consumable quality"](../../docs/content/docs/reference/research/shared-tui-extraction/04-extraction-migration-plan.mdx): make one immutable TermRock revision independently buildable, documented, reproducible, and consumable. The exit gate is concrete: an external minimal consumer can pin one full commit SHA, build and render without any `jackin❯` code, open the Fumadocs catalog, and reproduce the committed previews. Without this, plan 007's migration would pin an unconsumable revision.

Sequencing note (recorded deviation, not a silent one): ch. 04 lists "establish the … Fumadocs application" under Stage 2's repository-engineering bullet, while its catalog content is Stage 3 work. These plans scaffold the Fumadocs application at the start of this plan instead of inside plans 003–005 so the Stage 2 checkpoints stay Rust-focused. No Stage 2 exit-gate item references the docs application (its gate is neutrality + first buildable published head), and the docs-required aggregator carries a placeholder until here, so no gate weakens. If a reviewer prefers strict bullet placement, move Step 2.1 before plan 005's publish — everything else is order-independent.

## Current state

- TermRock `main` (published, plan 005): workspace with `termrock` + `termrock-lookbook`, neutral stories, committed previews under `docs/public/component-previews/`, CI fast gates + `rust-required` aggregator, `docs-required` placeholder.
- Missing for Stage 3 (from [ch. 03](../../docs/content/docs/reference/research/shared-tui-extraction/03-target-repository.mdx), [ch. 07](../../docs/content/docs/reference/research/shared-tui-extraction/07-repository-engineering.mdx), [ch. 09](../../docs/content/docs/reference/research/shared-tui-extraction/09-component-redesign-catalog.mdx)): the Fumadocs application, per-component catalog pages, the seven architecture examples, PTY restoration tests, macOS lane, API/package gates, catalog-coverage gate, the reviewed public API report, and the migration guide.
- Reference for the docs pipeline: `jackin❯`'s `.github/workflows/docs.yml` (source-path checks, frozen Bun installs, static link validation, Pages artifacts, deploy retries, post-deploy verification) — replace product-specific brand/roadmap/research checks with catalog completeness and Rust-path checks ([ch. 07, "Documentation gates"](../../docs/content/docs/reference/research/shared-tui-extraction/07-repository-engineering.mdx)).
- Reference for Fumadocs conventions: `jackin❯`'s `docs/` app (Fumadocs on TanStack Start + Vite, Bun, TypeScript strict, MDX in `content/docs/`, `meta.json` sidebars). TermRock's site is smaller but follows the same stack.
- The neutral interaction conventions (W3C-style tab pattern, focusability, hover-scroll, dialog/backdrop layering, hint discipline, navigation model) currently live in `jackin❯`'s TUI design pages under `docs/content/docs/reference/tui/` — the debranded generic versions become TermRock catalog pages; `jackin❯` keeps product policy and links out (executed `jackin❯`-side in plan 008).

## Commands you will need

In the extraction clone (Rust) and its `docs/` (Bun):

| Purpose | Command | Expected on success |
|---|---|---|
| Docs install | `bun install --frozen-lockfile` | exit 0 |
| Docs typecheck | `bunx tsc --noEmit` | exit 0 |
| Docs build | `bun run build` | exit 0, static site emitted |
| Docs links | `lychee` against built site (mirror jackin❯ `check:links`) | exit 0 |
| Catalog coverage | `cargo run -p termrock-lookbook -- list --format json` + coverage script | every public component covered |
| Examples (no features) | `cargo check --workspace --examples` | exit 0 |
| Examples (crossterm) | `cargo check --workspace --examples --features crossterm` | exit 0 |
| Package gate | `cargo package -p termrock --locked --list` | contains LICENSE/NOTICE/readme/src; no target output |
| API report | `cargo public-api -p termrock` (pin the tool version in `mise.toml`) | report generated |
| External-consumer proof | scratch crate build pinning `rev` | `cargo build` exit 0 |

## Scope

**In scope** (TermRock repo): `examples/` (7 files), `docs/` Fumadocs app + component pages + application-patterns section, `catalog/catalog.toml` or generated manifest wiring, PTY tests, macOS CI lane, API/package/coverage CI gates, reviewed + committed API report, migration guide (`MIGRATING.md` or docs page), README supported-terminal statement. `jackin❯`-side: evidence updates only.

**Out of scope**: quality-backlog fixes (plan 009); first tag (plan 009); crates.io publication (separate operator decision); any `jackin❯` code migration (plan 007); Windows.

## Git workflow

Extraction clone `main`, DCO-signed buildable commits, forward-only pushes at green checkpoints (source/story/SVG/docs changes for a component stay atomic in one commit — [ch. 03, "Repository shape"](../../docs/content/docs/reference/research/shared-tui-extraction/03-target-repository.mdx)). `jackin❯` side: signed evidence commits on `feature/shared-tui-extraction`, pushed immediately.

## Steps

### Step 1: Seven architecture examples

Create `examples/{direct,tea,component,flux,buffer_only,crossterm_manual,crossterm_managed}.rs` per [ch. 09, "Required architecture examples"](../../docs/content/docs/reference/research/shared-tui-extraction/09-component-redesign-catalog.mdx): all present the same small neutral screen using the same public tabs/list/input/action/outcome/theme types; only state ownership and routing differ (the table in ch. 09 defines what each must and must not imply). The two Crossterm examples declare `required-features = ["crossterm"]` in the manifest. Examples use public API only — no `#[cfg(test)]` internals access.

**Verify**: `cargo check --workspace --examples` (skips crossterm pair) and `--features crossterm` both exit 0; `rg -n 'pub(crate)|#\[cfg\(test\)\]' examples/` → empty.

### Step 2: Fumadocs catalog application

1. Scaffold `docs/` (Fumadocs + TanStack Start + Vite + Bun, TypeScript strict, committed `bun.lock`) mirroring the `jackin❯` docs conventions ("Current state").
2. Per-component MDX pages covering the [ch. 03 required content](../../docs/content/docs/reference/research/shared-tui-extraction/03-target-repository.mdx): purpose/appropriate use; Rust API + focused examples; keyboard/focus/mouse/non-color/Unicode/narrow-terminal behavior; theme tokens + consumer inputs; canonical SVG states (default/focused/disabled/empty/loading/error/edge) served from `docs/public/component-previews/`; the lookbook story ID + CLI commands for preview/regeneration.
3. "Application patterns" section explaining the seven examples ([ch. 07, "Documentation gates"](../../docs/content/docs/reference/research/shared-tui-extraction/07-repository-engineering.mdx)).
4. Debranded interaction-convention pages (tab pattern, focusability/hover-scroll, dialog/backdrop layering **without** a shared modal stack, hint discipline, navigation model) — sourced from `jackin❯`'s `docs/content/docs/reference/tui/` pages, product policy stripped ([ch. 03, "Fumadocs component catalog"](../../docs/content/docs/reference/research/shared-tui-extraction/03-target-repository.mdx)).
5. README: supported-terminal statement (Ghostty-class truecolor, OSC 8/22/52; no degradation paths — [Decision 12](../../docs/content/docs/reference/research/shared-tui-extraction/05-decision-record.mdx)), MSRV/platform statement, Git-`rev` consumption snippet from [ch. 03, "Git-first consumption"](../../docs/content/docs/reference/research/shared-tui-extraction/03-target-repository.mdx).

**Verify**: `bun run build` exit 0; lychee over the built site exit 0.

### Step 3: Catalog coverage gate

The typed story registry emits the machine-readable catalog (`termrock-lookbook list --format json`: component id, public Rust path, docs slug, story ids, canonical size set, interaction capabilities, theme roles — [ch. 07, "Render and catalog gates"](../../docs/content/docs/reference/research/shared-tui-extraction/07-repository-engineering.mdx)). Add a docs-side check (TypeScript, run in CI) that fails when any public catalog component lacks a story, docs page, keyboard contract, or canonical preview — and when a docs page references a story ID the registry doesn't have. No hand-maintained TypeScript component list.

**Verify**: coverage check passes; deleting one docs page locally makes it fail (prove the gate bites, then restore).

### Step 4: Remaining CI gates

1. **Docs lane** filling the `docs-required` aggregator: frozen Bun install, `tsc --noEmit`, docs unit tests, static build + prerender, internal-link check, lychee, spell-check with repo dictionary, coverage gate from Step 3; Pages deploy from `main` only after aggregators pass.
2. **Render determinism** (if not fully wired in plan 005): double `render` byte-identity; `check --dir docs/public/component-previews`; Linux is the only canonical render authority ([Decision 9](../../docs/content/docs/reference/research/shared-tui-extraction/05-decision-record.mdx)).
3. **macOS lane**: Crossterm compile/behavior surface on macOS + Linux (`cargo check -p termrock --features crossterm --all-targets` + relevant tests).
4. **API/package gates**: `cargo package -p termrock --locked` + archive-content inspection (license/notice/readme/source present, no target/docs caches); `termrock-lookbook` stays `publish = false`; API-diff artifact upload on public API change; committed reviewed API report (Step 5). `cargo-semver-checks` is installed/wired but only activates after the first tag ([Decision 10](../../docs/content/docs/reference/research/shared-tui-extraction/05-decision-record.mdx)).
5. **PTY restoration tests**: partial-init failure restore, explicit `restore()`, drop-fallback, alternate-screen + inline modes, on Linux + macOS lanes.
6. **Cache design** per [ch. 07, "Cache design"](../../docs/content/docs/reference/research/shared-tui-extraction/07-repository-engineering.mdx): warmed registry + `cargo fetch --locked` + `CARGO_NET_OFFLINE=true` compile jobs, purpose-scoped target caches, Bun/lychee caches, PR-cache cleanup; no `sccache` yet (Decision 11 — measure ≥20 runs first); never cache SVGs/API reports/packages/docs output.
7. **Scheduled hygiene** workflow: advisories refresh, deployed-link check, cache report, Renovate lockfile maintenance ([ch. 07, "Scheduled hygiene"](../../docs/content/docs/reference/research/shared-tui-extraction/07-repository-engineering.mdx)); benchmarks/fuzzing only if trivially portable now — otherwise record as post-roadmap items in TermRock's TODO doc.
8. **Release workflow, designed now, exercised in plan 009** ([ch. 07, "Release model"](../../docs/content/docs/reference/research/shared-tui-extraction/07-repository-engineering.mdx): "a release workflow is still designed early so packaging rules do not emerge accidentally"): serialized releases with concurrency cancellation disabled; requires a clean `main` revision + all required aggregators; asserts workspace version == tag; runs semver (post-first-tag), package, license, docs, standalone-example, and `jackin❯`-compatibility gates; creates annotated `v0.y.z` tag + GitHub release with migration notes; crates.io publish step gated on explicit operator decision; no cross-platform binary archive matrix.

**Verify**: push checkpoint; both aggregators green on TermRock `main`; Pages deploy succeeds and the deployed catalog renders the committed SVGs.

### Step 5: Bounded public API review (Decision 17)

Generate the public API report (`cargo public-api -p termrock`, tool pinned). Review every item against the [ch. 06 module-ownership table](../../docs/content/docs/reference/research/shared-tui-extraction/06-public-api-and-refactoring.mdx) and the [ch. 09 rejection checks](../../docs/content/docs/reference/research/shared-tui-extraction/09-component-redesign-catalog.mdx): every public item documented (rustdoc `-D warnings` already enforces); no product nouns, no glob exports, no mutable globals, no hidden I/O, no executor types, no Crossterm outside the feature module, no widget escape bytes, no public index identity. This review **may** improve illustrative names/signatures; it **may not** add product models, a required framework, another default feature, hidden I/O, or a second rendering path. Commit the reviewed report (e.g. `docs/api/public-api.txt`) — it becomes the first-tag semver baseline in plan 009.

**Verify**: report committed; a re-run of `cargo public-api` matches it byte-for-byte; review notes recorded in the commit message or `docs/api/REVIEW.md`.

### Step 6: Migration guide + immutable revision

1. Write the migration guide (donor path → TermRock path for every extracted item, from the extraction ledger; the `rev`-pin consumption recipe; the subscription closure-adapter pattern for Tokio consumers; what stayed in `jackin❯` and why).
2. Push the final Stage-3 checkpoint; record the green TermRock `main` head SHA as **the immutable revision** for plan 007.
3. External-consumer proof: in a scratch directory outside both repos, `cargo new termrock-smoke`; depend on `termrock = { git = "https://github.com/tailrocks/termrock.git", rev = "<full-sha>" }`; render one widget into a Ratatui `TestBackend` buffer; `cargo build && cargo run` → exit 0. Also verify `--no-default-features` builds. Record the transcript.
4. Advisory performance re-measure ([ch. 08, "Performance gates"](../../docs/content/docs/reference/research/shared-tui-extraction/08-migration-evidence-and-gates.mdx): advisory during neutralization, budgeted after parity): repeat the plan-001 `performance-baseline.md` measurements against TermRock (compile times, first lookbook frame, list/tabs/Unicode/diff render timings, SVG generation time/size) with the same recorded method; append results + deltas to that file. Regressions are advisory here — record them, do not block, but flag anything egregious to the operator.
5. `jackin❯`-side: append the Stage-3 revision + smoke transcript to `evidence/stage2-checkpoints.md` (or a new `stage3-revision.md`); tick roadmap Stage 3 checkbox; signed commit + push.

**Verify**: smoke crate builds from the pinned SHA with no `jackin❯` code; deployed catalog reachable; perf deltas recorded; evidence committed.

## Test plan

- PTY tests (Step 4.5) — the only genuinely new Rust test family; model on the donor's terminal-mode tests, extend with partial-init matrices.
- Docs-side unit tests for the coverage checker (component present/missing, story-ID mismatch).
- Everything else is CI-gate wiring verified by the gates themselves going green, plus the external smoke crate as the end-to-end test.

## Done criteria

Stage 3 exit gate ([ch. 04](../../docs/content/docs/reference/research/shared-tui-extraction/04-extraction-migration-plan.mdx)) plus the "External repository ready" checklist rows now checkable from [ch. 08, "Completion gates"](../../docs/content/docs/reference/research/shared-tui-extraction/08-migration-evidence-and-gates.mdx):

- [ ] External smoke crate builds + renders from one pinned full SHA, no `jackin❯` code
- [ ] All seven examples compile in their declared feature lanes
- [ ] Every public component has docs page + neutral story + committed SVG + keyboard contract; coverage gate enforces it
- [ ] Deployed Fumadocs catalog renders the committed SVG states
- [ ] Default/no-default/all-feature/powerset/MSRV/macOS/Linux/rustdoc/package gates green; PTY tests pass
- [ ] Reviewed API report committed; migration guide published
- [ ] Renders byte-deterministic (double-render diff empty in CI)
- [ ] Roadmap Stage 3 checkbox ticked; evidence committed in `jackin❯`; index row → DONE

## STOP conditions

- The API review finds a violation that requires a public-surface redesign beyond bounded renames (reopens ch. 06/09 decisions — operator call).
- The external smoke crate cannot build without something `jackin❯`-shaped (hidden coupling escaped Stage 2).
- Docs coverage cannot be satisfied for a component because it has no neutral story — that component's Stage-2 disposition was wrong; report, don't invent a story that hides product data.
- A gate can only pass by fixing a quality-backlog defect — sequencing violation ([Decision 13](../../docs/content/docs/reference/research/shared-tui-extraction/05-decision-record.mdx)).

## Maintenance notes

- The committed API report is the semver baseline at first tag — any public change between now and plan 009 must update it deliberately.
- Reviewers: check the coverage gate is registry-driven (no hand list) and that Pages deploys only from green `main`.
- Deferred to post-roadmap: benchmarks/mutation/fuzz targets (record in TermRock TODO), `termrock-testing` crate (Decision 6), Windows.
