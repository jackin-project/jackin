# Codebase health — execution playbook  (SUPERSEDED)

> **This file is archived.** It was the executor-grade, copy-pasteable log for slices
> **A1 → G3** of [Codebase health: structure & reviewability](/roadmap/codebase-health-enforcement/).
> All of those slices have **shipped** (verified against the tree). Its former
> "Blockers & open decisions" section is resolved history.
>
> **The live worklist is now [`plans/codebase-health-remaining.md`](./codebase-health-remaining.md)** —
> it holds only what is *not yet done* to fully close the roadmap item, verified against
> the code rather than the roadmap checkboxes.
>
> The full original 6870-line playbook (all mechanical per-slice steps for A1–G3)
> remains in this file's git history if a shipped slice ever needs to be re-traced:
> `git log --follow -p plans/codebase-health-playbook.md`.

## What shipped under this playbook (one line each)

- **A1–A5** — boundary fixes (build_log/progress ports, presentation helpers out of
  `jackin-core`, terminal state out of `jackin-diagnostics`), `cargo-deny` workspace-dep
  hygiene, 19/19 Architecture-Invariant headers, `FORBIDDEN_EDGES` 3 → 1.
- **B1** — `jackin-launch` → `jackin-launch-tui`. **B2** — 19 binary shims deleted.
- **C1/C2** — `jackin-host`, `jackin-usage` carved.
- **D1–D4** — image / env / op_cache / naming dedups.
- **E0** — `lto = "thin"` + launch/attach baseline. **E1 (partial)** — 4 of 6 isolation
  modules carved. **E2** — `jackin-instance` carved.
- **F1/F2** — `app_config/` coordinator + TEA stem normalization.
- **G0–G3** — shared `jackin-tui` Elm runtime + all four stacks migrated.
- **W5** — `usage.rs` + `tui/model.rs` decomposed.

## What is still open

See **[`plans/codebase-health-remaining.md`](./codebase-health-remaining.md)**. In short:
R1 break `jackin-runtime → jackin-tui` · R2 flip arch gate `--strict` · R3 finish E1
(`finalize.rs`/`git_inspect.rs` → `jackin-isolation`) · R4 W5 file decompositions ·
R5 editor/settings collapse (blocked on unify-settings) · R6 clippy grandfather
burn-down · R7 threshold tightening · R8–R11 hygiene/decision/docs cleanup.

## Executor contract (still in force for the remaining slices)

1. **One slice = one PR.** Do exactly the slice; don't bundle.
2. **Structure only — never behavior.** Move/relocate/rename/config only. If a step
   seems to require editing a test to pass, the move changed behavior — **stop**, don't
   edit the test, report it.
3. **Respect preconditions** (e.g. R5 waits on unify-settings; R3 needs the E0 baseline).
4. **Run Verify in order; stop on first failure.** Never force past a red gate.
5. **Do not improvise.** Code may have moved since a step was written; when in doubt, stop.
6. **Conventions:** no `mod.rs`; tests in a sibling `tests.rs`; every crate
   `[lints] workspace = true`; no wildcard imports.
7. **Commit:** Conventional Commit, sign off (`-s`), push immediately.

### Standard verify

`cargo fmt --check` · `cargo clippy --workspace --all-targets --all-features --locked -- -D warnings`
· `cargo nextest run --all-features` · `cargo run -p jackin-xtask --locked -- lint`
· behavioral specs `runtime-launch` + `op-picker` pass **unmodified**.

### Crate-carve recipe (referenced by R3)

1. Create/target `crates/<new>/` with `[lints] workspace = true` + `//!` Architecture-Invariant header.
2. `git mv` modules verbatim (byte-identical bodies).
3. Visibility-only: public surface `pub`, rest `pub(crate)`. No signature/logic edits.
4. Repoint importers to the new crate root (Parallel Change).
5. Add to root `Cargo.toml` `members`; old crate deps on new only if it still calls inward.
6. Add `cargo-deny` / arch-gate entries for the new allowed edges.
7. Update `PROJECT_STRUCTURE.md` + Codebase Map.
8. Run Verify; hot-path carve → E0 benchmark vs baseline, attach numbers.
