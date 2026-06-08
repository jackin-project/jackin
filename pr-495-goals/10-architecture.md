# Goal — Phase 1: Architecture cleanup

Parent index: [`../PR-495-FIXES.md`](../PR-495-FIXES.md). HEAD baseline: `f920b29a`.

The big architecture items from the audit (orphan deletion, diagnostics→tui dependency) are already landed — see `ARCH-0` in `00-preflight.md`. What remains is lint-policy adoption and one documentation reconciliation.

## Tasks

| ID | Status | Files / evidence | Helper | Verify | Acceptance |
|---|---|---|---|---|---|
| `ARCH-1` | pending | Root `Cargo.toml:60+` already has `[workspace.lints]` + `[workspace.lints.clippy]`; **17** crates carry private `[lints]` tables; **0** use `lints.workspace = true` | `[workspace.lints]` | `cargo clippy --workspace --all-targets --all-features --locked -- -D warnings` | Every workspace crate opts in with `lints.workspace = true`; private per-crate lint tables deleted unless a documented exception remains; `crates/AGENTS.md` describes the single-source policy. |
| `ARCH-2` | pending | FIXES claims "~60 dispatch arms vs documented ~17"; the "~17" text was **not located** in `architecture.mdx` at HEAD | — | docs build | Either the doc's stated count is corrected to match the real arm count, or — if the doc makes no such claim — this task is dropped with a one-line note. Do not "reconcile" a number the doc never states. |

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

### `ARCH-2` — reconcile or drop the enum-count claim
Search `docs/content/docs/reference/tui/architecture.mdx` for any stated dispatch-arm count. If found, count the real arms in the referenced message enum(s) (`crates/jackin-launch/src/tui/message.rs`, `crates/jackin-console/src/tui/message.rs`) and correct the doc. If no count is stated, the FIXES claim was imprecise — record that and close the task. Low confidence; do not invent a discrepancy.

## Done definition
- `rg "lints.workspace = true" crates/*/Cargo.toml` returns 17; clippy is green; `crates/AGENTS.md` updated.
- `ARCH-2` either corrects a real doc number or is closed with evidence the doc makes no count claim.
