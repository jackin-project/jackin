# Goal — Phase 1: Architecture cleanup

Parent index: [`../PR-495-FIXES.md`](../PR-495-FIXES.md). HEAD baseline: `f920b29a`.

The big architecture items from the audit (orphan deletion, diagnostics→tui dependency) are already landed — see `ARCH-0` in `00-preflight.md`. What remains is lint-policy adoption and one documentation reconciliation.

## Tasks

| ID | Status | Files / evidence | Helper | Verify | Acceptance |
|---|---|---|---|---|---|
| `ARCH-1` | pending | Root `Cargo.toml:60+` already has `[workspace.lints]` + `[workspace.lints.clippy]`; **17** crates carry private `[lints]` tables; **0** use `lints.workspace = true` | `[workspace.lints]` | `cargo clippy --workspace --all-targets --all-features --locked -- -D warnings` | Every workspace crate opts in with `lints.workspace = true`; private per-crate lint tables deleted unless a documented exception remains; `crates/AGENTS.md` describes the single-source policy. |
| `ARCH-2` | done | The "~17" claim lived in the roadmap checklist (not `architecture.mdx`) and is **already corrected**: `post-restructure-fixes-checklist.mdx` and `agent-runtime-trait.mdx` now state "58 production arms across 6 files." | — | docs build | Count reconciled. Optional residual — collapse or `#[expect]`-justify the `agent_binary.rs` + `multiplexer_utils.rs` exception arms — tracked as `RMP-6` in `60-roadmap-reconcile.md`. |
| `ARCH-3` | pending | `crates/jackin-launch` + `crates/jackin-console` exist and `runtime/progress.rs` facade is gone, but root `crates/jackin/src/console/` still holds the manager loop (`manager.rs`, `domain/`, `services/`, `tui/`, `effects.rs`, 472-line `console.rs`). Was TODO.md `jackin-console-jackin-launch-extraction` (last verified 2026-06-01); this PR is "finish TUI architecture epic", so it lands here. | `jackin-console`, `jackin-launch` | `cargo build -p jackin-console -p jackin-launch` | Root `src/console/` is a thin integration facade (CLI/runtime routing) or removed; the manager event loop, screen state, and render/input modules live in `jackin-console`; roadmap `tui-architecture.mdx` Phase 10 marked complete. |

## Detail

### `ARCH-1` — adopt `[workspace.lints]`, delete the duplicates
The policy table already lives at the root (`mod_module_files = "deny"`, `unwrap_used = "deny"`, `expect_used = "deny"`, `print_stdout/stderr = "deny"`, the clippy `all`/`pedantic`/`cargo` groups, etc.). The drift the audit warned about is real but the fix is **adoption, not hoisting**:

1. For each of the 17 crates under `crates/*/Cargo.toml`, add:
   ```toml
   [lints]
   workspace = true
   ```
2. Delete that crate's private `[lints]` / `[lints.clippy]` table.
3. If a crate genuinely needs a local exception (e.g. a generated module), keep only that one line with a comment naming why — not the whole table.
4. Re-run clippy across the workspace. Newly surfaced lints are the point; fix them or justify a scoped `#[allow]` with a comment (no broad crate-level `allow`).
5. Update `crates/AGENTS.md` so the documented guarantee ("`clippy::mod_module_files = "deny"` is workspace-enforced") matches the now-real mechanism.

Verify adoption count:
```sh
rg -l "lints.workspace = true" crates/*/Cargo.toml | wc -l   # expect 17
rg -l "\[lints\]" crates/*/Cargo.toml                          # only crates with documented exceptions
```

### `ARCH-2` — enum-count claim (already reconciled)
The "~17 arms" undercount was in the roadmap checklist, not `architecture.mdx`. It is already fixed: `post-restructure-fixes-checklist.mdx` and `agent-runtime-trait.mdx` now state the verified **58 production `Agent::Variant =>` / `Provider::Variant =>` arms across 6 files**, with the per-file breakdown (deliberate named-field accessors vs the `agent_binary.rs` / `multiplexer_utils.rs` exceptions). Nothing to reconcile here. The only optional residual — collapse or `#[expect]`-justify the two real exception arms — is tracked as `RMP-6`.

### `ARCH-3` — finish the console/launch crate extraction
The launch half is effectively done: `crates/jackin-launch` owns the launch model/view and the `runtime/progress.rs` cockpit facade is gone. The console half is not: root `crates/jackin/src/console/` still carries the manager event loop, root-specific screen state, and the large render/input modules. Finish it:

1. Move the remaining manager loop, screen state, and render/input modules from `crates/jackin/src/console/` into `crates/jackin-console`, with per-screen state/update/tui modules.
2. Leave root `jackin` only the thin integration that routes CLI/runtime into the crate (or remove `src/console/` entirely).
3. Keep each surface's Elm boundary intact; do not let the crate depend back on unrelated root CLI/runtime modules.
4. Update roadmap `docs/content/docs/reference/roadmap/tui-architecture.mdx` (Phase 10) status and the roadmap index/sidebar in the same change, per the roadmap-freshness rule.

This was tracked in `TODO.md` as `jackin-console-jackin-launch-extraction`; that entry has been removed in favor of this task. Scope to what reasonably lands in PR #495 — if the console extraction cannot fully complete here, mark `ARCH-3` `deferred` with the exact remaining modules and keep the roadmap item in **Partially implemented**, not done.

## Done definition
- `rg "lints.workspace = true" crates/*/Cargo.toml` returns 17; clippy is green; `crates/AGENTS.md` updated.
- `ARCH-2` done: roadmap count already corrected to 58; optional arm-collapse tracked as `RMP-6`.
- `ARCH-3`: `cargo build -p jackin-console -p jackin-launch` builds the real implementations; root `src/console/` is a thin facade or gone; roadmap Phase 10 status reconciled (`done` or `deferred` with named remaining modules).
