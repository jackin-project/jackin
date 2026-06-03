---
description: Drive the codebase-readability program — pick the next checklist item and finish it end-to-end
argument-hint: "[optional: stage number, file path, or item keyword to focus on]"
---

# Goal: finish the Codebase Readability & Restructuring program

Your job this session is to advance — and ultimately complete — the codebase-readability program. The end state is a **tiered Cargo workspace whose inter-crate dependency graph is a DAG**: a dependency-light vocabulary crate (`jackin-core`) at the bottom, the heavy config/env/runtime machinery in the middle, a thin `jackin` binary on top. The readability work (deduplication, file splits, module docs) and the crate extraction are the same motion — you cannot extract a clean crate from a duplicated, cyclic monolith, so the cleanup is what makes each boundary reviewable.

## The two source-of-truth documents — read both first

- **Program / design:** `docs/content/docs/reference/roadmap/codebase-readability.mdx` (the why, the target crate tiers, the cycle analysis).
- **The checklist you are executing:** `docs/content/docs/reference/roadmap/codebase-readability-checklist.mdx`. This is your worklist. It is ordered by stage and dependency, every item is independently verifiable, and it already carries the measured ground-truth (real line counts, real file:line targets, the corrected cycle map, the target structure diagram, the per-crate ownership contract, and the Rust best-practices acceptance gates). **Treat its `[ ]` items as the tasks and its gates as the definition of done.**

`$ARGUMENTS` — if the operator named a stage, file, or keyword, scope this session to the matching checklist items. If empty, work the next actionable item in stage/dependency order.

## Before touching code — three gates

1. **Branch discipline (hard rule).** Run `git branch --show-current` and `gh pr list --head <branch>`. If an open PR is in scope, stay on that branch all session. If on `main`, stop and ask the operator to confirm a `refactor/...` branch name before any change. Never create a second branch for work that belongs on the active one.
2. **Prerequisite decisions.** The checklist's top "Prerequisite decisions (operator sign-off)" section lists three calls that gate Stage 1 / Stage 4 — core error-type policy (add `anyhow` to `jackin-core` vs typed `thiserror` errors), `JackinPaths` purity vs ergonomics, and the selector prefix source. If the item you are about to do is gated on an unresolved decision, **surface it to the operator and get a ruling first** — do not pick one silently. These ripple across ~50 files each.
3. **One schema-version bump per PR.** `AppConfig`, `WorkspaceConfig`, and `RoleManifest` are versioned schemas. A pure type *relocation* must keep the serde representation byte-identical → no version bump, no migration step, no fixture re-bake. If a move would change the serialized shape, that is a separate, deliberate schema PR with all five migration artifacts (see `AGENTS.md`). Confirm "serde shape unchanged" on every config/workspace/manifest move.

## The working loop

Repeat until the scoped items are done or you are blocked on an operator decision:

1. **Pick the next actionable `[ ]` item** respecting stage order (0 → 1 → 2 → 3 → 4) and the dependency notes in the item. Earlier stages are the safety net and the cycle-breaking that later stages depend on; do not start a crate extraction whose cycle is not yet cut.
2. **Re-verify the ground truth** for that item before editing — line numbers and counts in the checklist were measured on 2026-06-03 and the tree moves. Grep/Read the named symbols; if reality drifted, fix the checklist line in the same PR.
3. **Implement it**, holding to every applicable **Rust best-practices acceptance gate** in the checklist:
   - `pub(crate)` by default, `pub` only what the next crate imports; private fields unless the serde shape is the contract.
   - `jackin-core` stays types + traits only — no `tokio`, no subprocess, no filesystem; the I/O seam is the `DockerApi` / `CommandRunner` traits, concrete impls in `jackin-runtime`.
   - Typed errors (`enum` + `thiserror`) at crate boundaries; `anyhow` context only at the binary/CLI edge. Convert any production `unwrap` you touch to `expect("why")` or a typed error.
   - Newtypes/enums over `bool`/`String`/primitives; derive the standard traits on vocabulary types; borrowed params (`&str`, `&[T]`, `&Path`) unless ownership is required; `.clone()` only as a deliberate decision.
   - Reuse before writing (DRY) and prefer a maintained crate over a hand-rolled parser — both are hard repo rules in `AGENTS.md`.
   - Every newly split module gets a `//!` contract stating the non-obvious why. A new `#[allow(clippy::too_many_lines)]` is a request to extract, not annotate — use `#[expect(..., reason = "...")]` only when a split is genuinely deferred.
4. **Verify** before declaring the item done: `cargo fmt --check`, `cargo clippy --workspace --all-targets --all-features --locked -- -D warnings`, and the relevant tests (`cargo nextest run` / `cargo test`). On this dev box pass `-j 1` if a full `--all-targets` link OOMs. If you split or moved a file, build the docs too when docs changed (`cd docs && bun run build && bun run check:repo-links`).
5. **Update the planning docs in the same change** (`AGENTS.md` docs-as-source-of-truth + roadmap-freshness rules):
   - Tick the `[ ]` → `[x]` (or `[~]`) in the checklist, and correct any stale count you touched.
   - Advance the matching `**Status**` line and metrics in `codebase-readability.mdx`, and the roadmap overview/sidebar if an item's status changed (`cd docs && bun run check:roadmap-sidebar`).
   - If the change is operator-visible, update the matching `guides/` / `commands/` page; if it changes an internals detail, update the matching `reference/` page.
6. **Commit and push immediately** (hard rule — no local-only commits). Conventional Commits, DCO sign-off (`git commit -s`), and a `Co-authored-by: Claude <noreply@anthropic.com>` trailer. Stage only the files for this item — do not sweep unrelated in-flight work into the commit.

## Scope discipline

- Keep each PR to one coherent slice (one cycle cut, one file split, one crate extraction step). The checklist's Stage 4 already lays out the dependency-gated PR 0–7 sequence — follow it; do not stack two crate moves in one PR.
- Solo-maintainer project: there is no second human reviewer. Lean on the multi-agent review passes (`code-reviewer`, `silent-failure-hunter`, `type-design-analyzer`) before asking to merge, and for irreversible or high-blast-radius moves prefer one more operator confirmation over assuming green CI is enough.
- When an item is genuinely blocked (unresolved decision, missing snapshot coverage that must land first, a cycle not yet cut), say so plainly and move to the next unblocked item rather than forcing it.

Start by reading both documents and reporting which item you are taking and why, then execute the loop.
