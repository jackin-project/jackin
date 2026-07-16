# Plan 003: TermRock workspace, CI, and foundation modules (Stage 2, part 1 of 3)

> **Executor instructions**: Follow this plan step by step. Run every verification command and confirm the expected result before moving on. If anything in "STOP conditions" occurs, stop and report — do not improvise. When done, update this plan's row in `plans/shared-tui-extraction/README.md`.
>
> **Drift check (run first)**: confirm plans 001–002 are DONE; the extraction clone exists at the recorded path with the filtered history and drafted `provenance.toml`; `tailrocks/termrock` is still empty (`gh api repos/tailrocks/termrock --jq '.size'` → 0).

## Status

- **Priority**: P1
- **Effort**: L
- **Risk**: MED
- **Depends on**: plans/shared-tui-extraction/002-stage1-external-history.md
- **Category**: migration
- **Planned at**: commit `03928e9dd`, 2026-07-15

## Why this matters

First third of **Stage 2** of the [Shared TUI Extraction roadmap item](../../docs/content/docs/roadmap/(operator-surface)/shared-tui-extraction.mdx) per [ch. 04, "Stage 2: Break product coupling and publish"](../../docs/content/docs/reference/research/shared-tui-extraction/04-extraction-migration-plan.mdx). It turns the filtered donor history into a standalone Rust workspace with the repository engineering from [ch. 07](../../docs/content/docs/reference/research/shared-tui-extraction/07-repository-engineering.mdx), then lands **Batch A foundations** from [ch. 08, "Refactoring order"](../../docs/content/docs/reference/research/shared-tui-extraction/08-migration-evidence-and-gates.mdx): text, input, focus/hover, scroll, layout primitives, and the semantic theme — the packages with the smallest behavioral surface, which unlock removal of the five `jackin_core` reference lines. Foundations must exist before any widget (plan 004) can be neutralized, or high-level widgets become accidental owners of low-level primitives.

All work happens in the extraction clone as **new DCO-signed, buildable commits** after the provenance boundary. Nothing is pushed to the public repository until plan 005.

## Current state

- Extraction clone (plan 002): filtered history with donor layout `crates/jackin-tui/`, `crates/jackin-tui-lookbook/`, retained docs/assets, `LICENSE`, `NOTICE`, drafted `provenance.toml`. The tip does not build standalone (no workspace root, `jackin-core` dependency unresolved) — expected.
- The five `jackin_core` coupling lines (verified at freeze): `crates/jackin-tui/src/lib.rs:28` (`shorten_home` — **remove, stays in `jackin-core`** per [Decision 17](../../docs/content/docs/reference/research/shared-tui-extraction/05-decision-record.mdx)), `lib.rs:29-32` (`BOTTOM_CHROME_ROWS`, `BottomChromeAreas`, `DialogBodyScroll`, `StatusFooterHover`, `TailScroll`, `bottom_chrome_areas`, `is_scrollable`, `max_line_width`, `max_offset` — **reimplement in TermRock**), `scroll.rs:20` (same scroll family), `ansi.rs:14` (`POINTER_DEFAULT`, `POINTER_HAND` — becomes typed `osc` data, plan 005), `ansi.rs:193` (`encode_osc52_clipboard_write` — typed `osc` encoder, plan 005).
- Donor foundation modules and their decided targets ([ch. 09, "Foundation module decisions"](../../docs/content/docs/reference/research/shared-tui-extraction/09-component-redesign-catalog.mdx)): `geometry.rs` → split into `text`/`layout` (leave `agent_display_name` behind for `jackin❯`); `keymap.rs` → parameterize into `input` (hint text becomes caller data; Crossterm conversion feature-gated; Capsule raw-byte decoder separated); `scroll.rs` → consolidate into `scroll` (keep `tui-scrollbar` as the single thumb-metrics source; `usize` logical units, one saturating `u16` edge helper); `theme.rs` + root RGB constants → reimplement as `style` (semantic `Theme`, private storage, builder, `Theme::tailrocks_phosphor()` preset reproducing exact donor RGB values); `ownership.rs`, `host_colors.rs`, `url_text.rs`, `prune_output.rs`, `output.rs`, `animation.rs` → **remain in `jackin❯`** (deleted from TermRock tree in this plan; they stay in the donor crates in `jackin❯` until plan 008).
- Workspace policy to replicate ([ch. 07, "Workspace policy"](../../docs/content/docs/reference/research/shared-tui-extraction/07-repository-engineering.mdx)): resolver 3, `unsafe_code = "forbid"`, workspace-inherited metadata, edition 2024, `rust-version = "1.95"`, toolchain `1.97.0`, committed `Cargo.lock`, `mise.toml` with pinned tools, fresh minimal `deny.toml`, Conventional Commits + DCO.
- Feature/dependency cell is **fixed** ([Decision 7](../../docs/content/docs/reference/research/shared-tui-extraction/05-decision-record.mdx), [ch. 06](../../docs/content/docs/reference/research/shared-tui-extraction/06-public-api-and-refactoring.mdx)): base `termrock` on `ratatui-core` 0.1.2; `[features] default = []`, `crossterm = ["dep:crossterm", "dep:ratatui-crossterm"]` (crossterm 0.29.0, ratatui-crossterm 0.1.2); umbrella `ratatui` 0.30.2 only in lookbook/examples; **no Tokio anywhere in the shared crate**.

## Commands you will need

All run inside the extraction clone:

| Purpose | Command | Expected on success |
|---|---|---|
| Build (default features) | `cargo check --workspace --all-targets --locked` | exit 0 |
| Base-crate isolation | `cargo check -p termrock --no-default-features` | exit 0 |
| No forbidden deps | `cargo tree -p termrock --no-default-features -e normal \| rg -i 'tokio\|crossterm\|jackin'` | no matches |
| Tests | `cargo nextest run --workspace --all-features --locked` (or `cargo test`) | all pass |
| Lints | `cargo clippy --workspace --all-targets --all-features --locked -- -D warnings` | exit 0 |
| Format | `cargo fmt --all -- --check` | exit 0 |
| License/dep policy | `cargo deny check licenses bans sources` | exit 0 |
| REUSE | `reuse lint` | exit 0 |
| DCO check | `git log --format='%h %(trailers:key=Signed-off-by,only)' <boundary>..HEAD` | every new commit has a trailer |

## Scope

**In scope** (extraction clone only):

- Repository skeleton per [ch. 07's tree](../../docs/content/docs/reference/research/shared-tui-extraction/07-repository-engineering.mdx): root `Cargo.toml`/`Cargo.lock`, `rust-toolchain.toml`, `mise.toml`, `deny.toml`, `nextest.toml`, `LICENSES/Apache-2.0.txt`, `NOTICE`, `REUSE.toml`, `provenance.toml`, `AGENTS.md` + `CLAUDE.md` symlink, `CONTRIBUTING.md`, `ENGINEERING.md`, `PULL_REQUESTS.md`, `SECURITY.md`, `TESTING.md`, `renovate.json`, `.github/workflows/`.
- Crate rename/moves: `crates/jackin-tui` → `crates/termrock`, `crates/jackin-tui-lookbook` → `crates/termrock-lookbook` (ordinary signed `git mv` commits preserving blame).
- Foundation modules: `crates/termrock/src/{text,input,interaction,layout,scroll,style}/`.
- Reimplemented `jackin-core` scroll/chrome helpers with `provenance.toml` lineage.

**Out of scope**:

- Widgets (plan 004), `runtime`/`osc`/`crossterm` modules and the lookbook CLI (plan 005).
- Any push to `tailrocks/termrock` (plan 005, final step).
- Any change in the active `jackin❯` workspace (its donor crates stay untouched until Stage 4).
- Fixing quality-backlog defects — carry the character-count width math and color-only focus **unchanged** into the new modules ([Decision 13](../../docs/content/docs/reference/research/shared-tui-extraction/05-decision-record.mdx)).
- Windows CI, capability degradation, new components.

## Git workflow

- Extraction clone, on `main` (local). Every commit after the provenance boundary: DCO-signed (`git commit -s`), Conventional Commits, **buildable** (`cargo check --workspace` green before committing; use small commits — workspace bootstrap may temporarily be one large commit to reach first buildability).
- No pushes. No rewriting of inherited history.

## Steps

### Step 1: Bootstrap the standalone workspace (first signed commit)

1. `git mv crates/jackin-tui crates/termrock && git mv crates/jackin-tui-lookbook crates/termrock-lookbook`.
2. Create root `Cargo.toml`: workspace members `crates/termrock`, `crates/termrock-lookbook`; resolver 3; `[workspace.package]` edition 2024, `rust-version = "1.95"`, license Apache-2.0; `[workspace.lints]` mirroring the `jackin❯` baseline (`unsafe_code = "forbid"`, rust idiom + clippy correctness/suspicious/performance + selected pedantic); `debug = 1` dev profile, thin LTO release.
3. Rewrite `crates/termrock/Cargo.toml`: name `termrock`, lib name `termrock`, description "Ratatui components, lookbook, and documentation for Tailrocks applications" family; dependencies `ratatui-core = "0.1.2"`, `unicode-width`, `unicode-segmentation`, `tui-scrollbar`, `anstyle-parse`, `similar`, `base64`; optional `crossterm = "0.29.0"`, `ratatui-crossterm = "0.1.2"`; `[features] default = []`, `crossterm = ["dep:crossterm", "dep:ratatui-crossterm"]`. **Remove `jackin-core`, `tokio`, `owo-colors`.** (The code still referencing them will not compile yet — acceptable inside this step; the commit lands only when Step 3 restores buildability. If you need interim buildable commits, comment-gate modules with `// TODO(extraction)` markers and keep each commit green.)
4. `rust-toolchain.toml` (channel 1.97.0, clippy+rustfmt), `mise.toml` (pin rust, actionlint, reuse, cargo-deny, cargo-nextest, cargo-hack, gitleaks), `nextest.toml`, fresh minimal `deny.toml` (Apache-2.0 + MIT allowed, deny yanked/unknown-registry/wildcards/duplicate versions — duplicates especially for `ratatui-core`), `renovate.json`.
5. `LICENSES/Apache-2.0.txt` (moved license text), root `NOTICE` retaining the donor notice + TermRock addition, new TermRock-specific `REUSE.toml`, finalized `provenance.toml` from plan 002's draft.
6. Governance docs: `AGENTS.md` (+ `CLAUDE.md` symlink), `CONTRIBUTING.md` (DCO, Conventional Commits, the bootstrap direct-`main` exception with its end date at first tag), `SECURITY.md`, `TESTING.md`, `ENGINEERING.md`, `PULL_REQUESTS.md`, `.github/pull_request_template.md` (in the [ch. 07 repository tree](../../docs/content/docs/reference/research/shared-tui-extraction/07-repository-engineering.mdx)). Include the [Decision 15](../../docs/content/docs/reference/research/shared-tui-extraction/05-decision-record.mdx) issue-routing policy in `CONTRIBUTING.md` or `AGENTS.md`: defects reproducible with a neutral story/fixture are TermRock issues; anything requiring product state/wording/effects is a consumer issue; cross-repository reports link both sides; solo-maintainer model inherited.

**Verify** (after Step 3 restores buildability): `cargo metadata --format-version 1 | python3 -c "import json,sys; d=json.load(sys.stdin); print(sorted(p['name'] for p in d['packages'] if p['source'] is None))"` → `['termrock', 'termrock-lookbook']`.

### Step 2: Delete remain-local modules from the TermRock tree

Delete `crates/termrock/src/{ownership.rs,host_colors.rs,url_text.rs,prune_output.rs,output.rs,animation.rs}` and the product-specific components/stories flagged `remain` in the plan-001 extraction ledger (`brand_header.rs` composition stays out per [ch. 09](../../docs/content/docs/reference/research/shared-tui-extraction/09-component-redesign-catalog.mdx) — its neutral panel/line mechanics, if any, fold into widgets in plan 004). These live on in `jackin❯`'s in-tree donor until Stage 4; TermRock never owns them. Update `lib.rs` module declarations accordingly.

**Verify**: `rg -l 'agent_display_name|digital.?rain|shorten_home' crates/termrock/src` → only `geometry.rs` (handled next step) or empty.

### Step 3: Land Batch A foundations (one signed commit per module family)

Apply [ch. 09, "Foundation module decisions"](../../docs/content/docs/reference/research/shared-tui-extraction/09-component-redesign-catalog.mdx) — target layout from its "Target module tree":

1. **`text/`** (`display_width.rs`, `sanitize.rs`, `window.rs`): move from `geometry.rs` the display-column measurement (`display_cols`, `display_cols_slice`, `take_display_cols`, `leading_space_cols`, `padded_line_display_cols`, `hint_row_cols`), fixed-prefix windows (`FixedPrefixSegment`, `fixed_prefix_scroll_segments`), terminal-control sanitization (`is_terminal_control_char`, `sanitize_terminal_title`); move pure ANSI-to-span parsing from `ansi_text.rs`; from `ansi.rs` keep only proven generic encoding/sanitization helpers. `agent_display_name` and brand banners/version/help output do NOT move — delete from the TermRock tree. `text` never reads `$HOME` or process environment ([Decision 17](../../docs/content/docs/reference/research/shared-tui-extraction/05-decision-record.mdx)). Carry the known char-count width defects **bug-compatible**.
2. **`input/`** (`action.rs`, `binding.rs`, `chord.rs`, `pointer.rs`): from `keymap.rs` keep logical chords, bindings, action dispatch; hint labels become caller data (no hardcoded `select`/`save`/`cancel`); park Crossterm event conversion behind `#[cfg(feature = "crossterm")]` stubs (fleshed out in plan 005); the Capsule raw-byte decoder does NOT move (stays donor-side, `jackin❯`).
3. **`interaction/`** (`focus.rs`, `hover.rs`, `outcome.rs`): generic `FocusState<Id>` replacing `ButtonFocus`/owner enums (`focus_owner.rs` split per ch. 09); `HoverState<Id>` over borrowed `HitRegion<Id>` from `hover_tracker.rs`; `ModalOutcome`-style outcome types from the donor facade.
4. **`scroll/`** (`state.rs`, `geometry.rs`, `viewport.rs`): consolidate donor `scroll.rs` **plus reimplementations of the `jackin-core` helpers** (`TailScroll`, `is_scrollable`, `max_line_width`, `max_offset`, `DialogBodyScroll`, `StatusFooterHover` hover-state mechanics): one canonical offset/axis/thumb/viewport implementation, `tui-scrollbar` retained as the single proportional thumb-metrics source, `usize` logical offsets/lengths/indices, clipping in logical space, exactly one centralized saturating `u16` conversion helper at the Ratatui edge ([Decision 19](../../docs/content/docs/reference/research/shared-tui-extraction/05-decision-record.mdx)). Record each reimplemented helper in `provenance.toml` `[[reimplemented]]` (source file, donor revision, meaningful source commit).
5. **`layout/`** (`dialog.rs`, `hit_region.rs`, `slots.rs`): generic responsive `DialogSpec` (min/preferred/max, placement, margins, overflow) replacing `modal_rects.rs` product variants (role/auth/mount/source/scope enums do not move); `HitRegion<Id>` primitives; `Slots` generalizing `bottom_chrome` + the reimplemented `BOTTOM_CHROME_ROWS`/`bottom_chrome_areas` geometry with caller-supplied slot heights, saturating geometry, zero-height behavior.
6. **`style/`** (`role.rs`, `theme.rs`, `tailrocks_phosphor.rs`): semantic `Theme` with private storage, `ThemeBuilder`, `Theme::style(Role)`, role set from [ch. 06, "Theme API"](../../docs/content/docs/reference/research/shared-tui-extraction/06-public-api-and-refactoring.mdx) (canvas/surface/elevated/backdrop, text/text_muted/text_disabled, border/border_focused/selection/focus, accent/success/warning/danger/info, link/link_hover, input/input_invalid, scroll_track/scroll_thumb); `Theme::tailrocks_phosphor()` reproducing the donor's **exact RGB values** and the `faded` alpha helper's exact scaling math; `Theme::default()` = phosphor ([Decision 8](../../docs/content/docs/reference/research/shared-tui-extraction/05-decision-record.mdx)). Product tokens (`CAPSULE_MENU_*`, `BRAND_BLOCK`, `DEBUG_AMBER`, `TAB_BG_*`, rain colors) do NOT enter `Theme` — delete from the TermRock tree; `jackin❯` keeps them locally (Stage 4).
7. Move each module's existing unit tests along with it; adapt imports only. Add tests for the reimplemented scroll/chrome helpers mirroring their `jackin-core` originals' behavior.

**Verify after each commit**: `cargo check --workspace --all-targets` → exit 0; `cargo nextest run --workspace` → all pass. After the last: `rg -n 'jackin_core|jackin-core' crates/ --glob '*.rs' --glob '*.toml'` → no matches.

### Step 4: Minimal CI workflows (fast source gates)

Create `.github/workflows/` with the [ch. 07 "Fast source gates"](../../docs/content/docs/reference/research/shared-tui-extraction/07-repository-engineering.mdx): `actionlint`, `fmt`, `check-default`, `clippy-all`, `doctest`, `nextest`, `feature-powerset` (`cargo hack check --workspace --feature-powerset --all-targets --locked`), `msrv` (1.95), `rustdoc` (`-D warnings`), plus `dependency-policy` (`cargo deny check licenses bans sources`, `cargo shear --deny-warnings`), `advisories`, `reuse`, `dco` (new commits after the `provenance.toml` boundary), `history-provenance`. Two aggregators: `rust-required`, `docs-required` (docs jobs arrive in plan 006 — give `docs-required` a placeholder success job until then). Least-privilege permissions, timeouts, actions pinned by full commit SHA. Add the base-crate isolation assertion as a CI step: `cargo check -p termrock --no-default-features` + `cargo tree` grep from the commands table.

**Verify**: `actionlint` → exit 0 locally. (Workflows first *run* after plan 005's push — local actionlint + the commands themselves passing locally is the Stage-2 evidence.)

### Step 5: Full local gate sweep

Run every command in "Commands you will need" in order.

**Verify**: all exit 0; DCO check shows a `Signed-off-by` trailer on every commit after the boundary; `git log --oneline <boundary>..HEAD | wc -l` matches your commit count.

### Step 6: Record checkpoint evidence in `jackin❯`

Append to `plans/shared-tui-extraction/evidence/stage2-checkpoints.md` (create): clone-local `main` tip SHA, gate results, date, and which Batch A items landed. Commit + push on `feature/shared-tui-extraction` (`chore(tui): record termrock stage 2 foundations checkpoint`).

**Verify**: committed, `git status` clean.

## Test plan

- Moved modules carry their donor tests; the moved-test pass count must not shrink relative to plan 001's recorded baseline for those modules.
- New tests: reimplemented `TailScroll`/`is_scrollable`/`max_line_width`/`max_offset`/`bottom_chrome_areas` (behavior mirrored from `jackin-core` originals — read them in the main workspace under `crates/jackin-core/` for the expected semantics, e.g. via `rg -n 'pub fn max_offset|pub struct TailScroll' crates/jackin-core/src`); `Theme::tailrocks_phosphor()` asserting the exact donor RGB constants; the saturating `u16` edge helper (0, small, `u16::MAX`, `usize::MAX` inputs); `usize` logical-space clipping.
- Model test structure on the donor's existing `#[test]` modules (e.g. the tests in `crates/termrock/src/scroll.rs` after the move).
- Verification: `cargo nextest run --workspace --all-features --locked` → all pass.

## Done criteria

ALL must hold (in the extraction clone unless noted):

- [ ] `cargo check -p termrock --no-default-features` exits 0 and its `cargo tree` contains no `tokio`, `crossterm`, `ratatui` umbrella, or `jackin*` package
- [ ] `rg -n 'jackin' crates/termrock/src --glob '*.rs'` → no code references (docs/NOTICE/provenance mentions of the donor are fine)
- [ ] Modules `text`, `input`, `interaction`, `layout`, `scroll`, `style` exist with tests passing; `theme` phosphor preset byte-matches donor RGB values
- [ ] `cargo fmt`/`clippy -D warnings`/`nextest`/`cargo deny`/`reuse lint` all green
- [ ] Every commit after the provenance boundary is DCO-signed and was buildable when committed
- [ ] `provenance.toml` lists every reimplemented `jackin-core` helper with lineage
- [ ] Nothing pushed to `tailrocks/termrock` (`gh api repos/tailrocks/termrock --jq '.size'` → 0)
- [ ] `stage2-checkpoints.md` evidence committed in `jackin❯`; index row → DONE

## STOP conditions

- A foundation move forces a rendering-behavior change (beyond mechanical import/rename): that breaks bug-compatibility — record and stop.
- You cannot reproduce a donor test's expectation after a move (semantics drifted — do not "fix the test").
- A dependency needed by a foundation module is neither in the fixed cell nor obviously license-compatible (new dependency = reviewed decision).
- You find yourself editing the active `jackin❯` workspace's donor crates or pushing to the public repo.
- The reimplemented scroll helpers' semantics diverge from `jackin-core`'s (consumer parity in plan 007 would silently break).

## Maintenance notes

- The exact public names finalize in plan 006's API review ([Decision 17](../../docs/content/docs/reference/research/shared-tui-extraction/05-decision-record.mdx) allows bounded renames) — keep foundation APIs `pub(crate)`-lean now to reduce later churn.
- Reviewers: scrutinize the phosphor preset's RGB values (byte-parity anchor for all of Stage 4) and the single `u16` edge helper (no second conversion path may appear later).
- Deferred: `osc` module (plan 005) still leaves `ansi.rs`'s pointer/OSC52 re-export lines dangling — acceptable interim state if the affected module is comment-gated or the re-export deleted with the helper reimplemented in plan 005; keep each commit green either way.
