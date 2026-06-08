# Goal — Phase 6: Verify & closeout

Parent index: [`../PR-495-FIXES.md`](../PR-495-FIXES.md). HEAD baseline: `f920b29a`.

The cargo gates are already green at HEAD; the **live merge blockers are in docs CI**. This phase resolves those and produces the completion report.

## Live PR check state at HEAD `f920b29a`

From `gh pr checks 495`: green include `cargo fmt`, `cargo clippy`, `cargo nextest`, `DCO`, `amd64`, `arm64`, `cargo audit`, `repo-link-check`, `docs-link-check`. **Failing:** `spell-check-docs`, `docs-required`.

## Tasks

| ID | Status | Files / evidence | Verify | Acceptance |
|---|---|---|---|---|
| `CI-1` | pending | `spell-check-docs` fails; job scans `.github/**/*.md`, `docs/**/*.md(x)` (`.github/workflows/docs.yml:189-192`) — the root `PR-495-*.md` files are **not** scanned | `gh pr checks 495` | Offending words in the `docs/` diff corrected, or legitimate terms added to the spell dictionary. `spell-check-docs` green. |
| `CI-2` | pending | `docs-required` fails (aggregator; gates on the docs jobs incl. `CI-1`) | `gh pr checks 495` | `docs-required` green once its dependencies are green. No stale PR-state claims in published docs. |
| `CI-3` | pending | docs build/type/test gates | `cd docs && bun run build && bun run check:repo-links && bunx tsc --noEmit && bun test` | All four pass locally before relying on CI. |
| `CI-4` | done (keep green) | Cargo gates green at HEAD | `cargo fmt --check`; `cargo clippy --workspace --all-targets --all-features --locked -- -D warnings`; `cargo nextest run --workspace --all-features` | Stay green after every phase. Run after each structural change. |

## Detail

### `CI-1` — fix the spell-check
Find what the job flags. Inspect the failing run:
```sh
gh run view --log-failed --job <spell-check-docs job id>
```
Resolve each hit by correcting a real typo in `docs/` or adding a legitimate technical term (brand words, crate names, agent names like `the-architect`) to the configured dictionary. Re-run. Note: brand prose must use `jackin'` (apostrophe); literal identifiers use `jackin`.

### `CI-2` — clear the aggregator
`docs-required` is a path-aware roll-up. It goes green when its underlying docs jobs (spell-check, build, link, type) pass. Confirm no other docs job is red and that any doc touched by phases 0–5 (`dialogs.mdx`, `chrome.mdx`, `navigation.mdx`, `crates/AGENTS.md`, lookbook stories) builds and links.

### `CI-3` — run docs gates locally first
Do not push to discover failures. Run the four `bun` commands from `docs/` and fix locally.

### `CI-4` — keep cargo green
After each phase's edits, re-run fmt/clippy/nextest. The lint-adoption task (`ARCH-1`) is the most likely to surface new clippy findings — budget for that.

## Closeout (the completion report)

Produce, per the index spec:
- Each task's final status (`done`/`pending`/`deferred`) across all phase files.
- Remaining `pending`/`deferred` with exact reason and remaining files/behaviors.
- Shared components/helpers changed or added (expected: `container_info` hint placement, `ErrorDialog` sizing, `action_row_style` reach, a possible `jackin-tui` scrollable-panel shell, `[workspace.lints]` adoption).
- Docs + lookbook artifacts updated.
- Verification commands run and results.
- Residual risk — especially `CAP-2` (capsule vertical scrollback) and `DBG-3` if it stayed a smoke-only confirmation.
- Roadmap freshness: update any item under `docs/content/docs/reference/roadmap/` this PR ships/advances/defers, and the roadmap index, before requesting merge.

## Done definition
- `spell-check-docs` and `docs-required` green; all four local docs gates pass.
- Cargo gates green.
- Completion report written; `gh pr checks 495` all green or each red explained.
