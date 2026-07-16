# crates/AGENTS.md

Rules for `crates/` workspace crates. Apply all crates under this dir, plus root `AGENTS.md`.

## Rust module layout (workspace crates — hard rule)

**Use Rust 2024 self-named module files. No `mod.rs` files.**

Modules with child files use self-named module root:

```text
# correct
crates/jackin-foo/src/bar.rs        ← module root
crates/jackin-foo/src/bar/baz.rs    ← child module
crates/jackin-foo/src/bar/tests.rs  ← test module (all tests inline, no child modules)
```

Not:

```text
# wrong — legacy layout
crates/jackin-foo/src/bar/mod.rs    ← do not create
crates/jackin-foo/src/bar/baz.rs
```

`lib.rs` and `main.rs` = allowed crate-root exceptions.

`clippy::mod_module_files = "deny"` enabled in workspace `[lints.clippy]`, CI-enforced. Any PR with new `mod.rs` fails.

### Test file rule (hard rule)

**All tests for a module live in a single `tests.rs` file. `tests.rs` must never declare child modules.**

Correct — every test function is inline in `bar/tests.rs`:

```text
crates/jackin-foo/src/bar.rs          ← implementation
crates/jackin-foo/src/bar/tests.rs    ← ALL tests here, nothing else
```

Wrong — `tests.rs` is a thin shell that splits tests across sub-files:

```text
crates/jackin-foo/src/bar/tests.rs            ← declares mod a; mod b;  (do not do this)
crates/jackin-foo/src/bar/tests/a.rs          ← test split-out (do not create)
crates/jackin-foo/src/bar/tests/b.rs          ← test split-out (do not create)
```

Splitting tests into sub-modules adds navigation friction and breaks the "one file = one test surface" contract. If a `tests.rs` is getting large, that is a signal the module under test is doing too many things — not a signal to split the test file. Temporary exceptions must be recorded in root [`test-layout-allowlist.toml`](../test-layout-allowlist.toml); the preferred fix is to remove exceptions rather than add new splits.

### Rationale

Rust 2024 recommends self-named layout: `mod foo;` loads `foo.rs` or `foo/mod.rs`; Reference encourages `foo.rs` to dodge dozens of `mod.rs` files. Workspace crates already follow this layout.

## Naming

Standard Rust naming:

- crates/modules/files: `snake_case`
- functions/methods/variables: `snake_case`
- types/traits/enums/structs: `UpperCamelCase`
- constants/statics: `SCREAMING_SNAKE_CASE`

Avoid clever abbreviations unless established domain terms (`tui`, `cli`, `dind`, `pty` OK; `mgr`, `cfg_ed`, `ws` not).

## Tests in own file (hard rule)

No inline `#[cfg(test)] mod tests { … }` in source. Logic + tests split, always.

`foo.rs` declares exactly `#[cfg(test)] mod tests;` — no `#[path]`, alias, visibility, or intervening attribute. Tests live in `foo/tests.rs` (self-named, no `mod.rs`). Rust resolves that sibling path by default.

The only non-suite exception is an external module named `test_support`. It may contain fixtures but no `#[test]` functions, and exists only when two or more sibling suites or feature-enabled downstream tests consume the same fixture. Keep it in `test_support.rs`; never use an inline `mod test_support { … }` body as a second test suite. Every approved parent is explicit in the test-layout gate's fixture registry; adding one requires documenting its consumers in review.

```text
crates/jackin-console/src/workspace.rs        ← logic + `#[cfg(test)] mod tests;`
crates/jackin-console/src/workspace/tests.rs  ← tests
```

PR touching code: misplaced tests → relocate same PR. No exception.

## Per-crate README + AGENTS.md (hard rule)

**Every crate directory under `crates/` carries three files:** `README.md`, `AGENTS.md`, and `CLAUDE.md` (a symlink to `AGENTS.md`, matching the repo convention: every dir with `AGENTS.md` has `CLAUDE.md` beside it). Enforced by the `cargo xtask lint agents` gate, which scans every `crates/*/` member.

### No cross-links between AGENTS.md files (hard rule)

An `AGENTS.md` is per-folder and **self-contained** — the [agents.md](https://agents.md/) nearest-file-wins rule means an agent editing a file reads the closest `AGENTS.md`, so that file must stand alone. Therefore:

- A `README.md` never links to any `AGENTS.md`.
- An `AGENTS.md` never links to another `AGENTS.md` (no "see `../AGENTS.md` for the real rules").

Either file may still link to any other markdown or source file (a design doc, a spec) as a reference. Enforced by `cargo xtask lint agent-links`, which fails on any markdown link — inline `[t](path)` or reference `[id]: path` — whose target is an `AGENTS.md` (code-fence template examples are skipped).

### `README.md` — the always-current map of this crate

A cold reader (human or agent) who opens a crate must finish its `README.md` understanding: **what this crate is for, why it exists, what it owns, its architecture tier and allowed dependencies, its `src/` structure, the public API to copy, and how to verify it.** The root `PROJECT_STRUCTURE.md` and the docs Codebase Map are indexes that point *into* these READMEs; they are not a substitute for them. A stale crate README is a bug.

**Update the README in the same PR** whenever you change any of:

- what the crate is responsible for (add/remove a responsibility),
- the public API surface (new/removed/renamed public items, entry points, re-exports),
- the `src/` module layout (add/rename/split/remove a module or subdirectory),
- the architecture tier or allowed workspace dependencies,
- how the crate is verified.

Line-count churn inside an existing module does not require a README edit. When unsure whether a change is "structural," update — a too-fresh README costs nothing; a stale one misleads every later agent.

**Structure is a clickable table, not a bare list.** Every module path is a link so a reader can click straight to the file or folder, with a Tests column pointing at the sibling `tests.rs`. A top-level module `<mod>.rs` has tests at `<mod>/tests.rs` exactly when a `<mod>/` directory exists; modules without a subdir have no Tests link.

#### `README.md` template

```markdown
# <crate-name>

<One sentence: what this crate is for and why it exists. Answer "why is this a separate crate?">

## What this crate owns
- <responsibility>
- <responsibility>

## Architecture tier and allowed dependencies
<tier: T<n> — must match the TIERS table in crates/jackin-xtask/src/arch.rs (checked by cargo xtask lint headers)>.
Allowed workspace dependencies: <list>. Must NOT depend on: <list, if any>.

## Structure

| Module | Owns | Tests |
|---|---|---|
| [`lib.rs`](src/lib.rs) | crate root, re-exports | — |
| [`<mod>.rs`](src/<mod>.rs) · [`<mod>/`](src/<mod>) | <one-line> | [`tests.rs`](src/<mod>/tests.rs) |
| [`<mod>.rs`](src/<mod>.rs) | <one-line; no subdir = no sibling tests> | — |

## Public API
<The entry points an agent should copy: root re-exports, key types/traits/fns.
If the crate is internal-only, say so and point at the crate that re-exports it.>

## How to verify
`cargo nextest run -p <crate>` (plus any specific gate: snapshots, fuzz, doc tests).
```

Right-size the README to the crate. A leaf crate (e.g. `jackin-protocol`) needs only the short form above. A complex infrastructure crate (e.g. `jackin-term`) keeps its design rationale in the internal docs page it links, not inline. Never pad a small crate to match a big one.

### `AGENTS.md` — only the rules an agent cannot derive from the code

The per-crate `AGENTS.md` is the **smallest** file in the crate — a few rules, no title, no boilerplate. It holds only non-derivable, actionable rules: the conventions, invariants, traps, and boundary decisions that are not already encoded in the code, the `README.md`, the workspace lint table, or the `cargo xtask lint arch` dependency gate. Agents read `AGENTS.md` *and* `README.md` *and* the source; duplicating any of them wastes context tokens for no gain. Research on the [AGENTS.md](https://agents.md/) convention is unambiguous: files that duplicate README/code-derivable content measurably *reduce* agent task success (Atlan/MorphLLM surveys).

**Never put in a per-crate `AGENTS.md`:**

- **A title or the crate/folder name.** The file's location is its scope (nearest-file-wins); an `# AGENTS.md — <crate>` heading is pure overhead.
- **A one-line purpose.** It duplicates the crate's `//!` header in `src/lib.rs`.
- **Tier or allowed-dependency lists.** They are in the `//!` Architecture Invariant header, in `Cargo.toml`, and enforced by `cargo xtask lint arch`.
- **`src/` module structure or public-API summaries.** That is the `README.md`'s job (and rustdoc's).
- **Build/test/verify commands.** Standard (`cargo nextest run -p <crate>`, clippy) or in `TESTING.md`.
- **The "keep README current" rule.** Stated once, here; do not repeat per crate.
- **A footer linking to another `AGENTS.md`.** Forbidden (no-cross-links rule above).
- **Prose architecture overviews with no actionable instruction.**

**Do put in a per-crate `AGENTS.md`:** non-derivable rules only — conventions, invariants, and traps the compiler, lints, and arch gate do not enforce (e.g. "damage is recorded at mutation, never recomputed by re-read", "reach runtime through effects-as-data, not direct calls", "use TermRock for product-neutral TUI components"). Add a `## Boundaries` section only for a non-obvious ownership split that is a decision, not a derivable dependency.

If a file grows past ~30 lines, most of it is probably derivable and belongs elsewhere.

#### `AGENTS.md` template

```markdown
- <non-derivable rule — convention / invariant / trap the compiler, lints, and arch gate do not enforce>
- <non-derivable rule>

## Boundaries
- <non-obvious ownership split, only if it is a decision and not a derivable dependency>
```

## Workspace lint baseline (hard rule)

All crates inherit root workspace metadata + lints:

```toml
[lints]
workspace = true
```

No per-crate copies of `edition`, `rust-version`, `license`, `repository`, or lint tables unless crate has documented reason to diverge.

Root `[workspace.lints.rust]` = source of truth. Dead-code + hygiene lints stay deny: `unused`, `unused_imports`, `unused_variables`, `unused_must_use`, `dead_code`, `unreachable_pub`. Rust 2024 unsafe-hygiene lints enabled; `unsafe_code = "forbid"` workspace-wide. Table also denies current strict allowed-by-default set (`unused_qualifications`, `unused_lifetimes`, `redundant_lifetimes`, `trivial_casts`, `trivial_numeric_casts`, `unnameable_types`, `unit_bindings`, `macro_use_extern_crate`, `meta_variable_misuse`, `single_use_lifetimes`, `let_underscore_drop`) and keeps `missing_debug_implementations` at `warn`, which CI promotes during clippy gate.

Clippy CI-enforced with:

```text
cargo clippy --workspace --all-targets --all-features --locked -- -D warnings
```

Workspace enables `clippy::all` deny plus `pedantic`/`cargo` as modern baseline, then carries explicit allow entries for noisy style-only lints not matching jackin❯ current API shape. `warnings = "deny"` deliberately not baked into manifest; `-D warnings` gate stays in CI so future compiler warning don't brick local builds while jackin❯ rides each new stable toolchain. No `nursery` or `restriction` wholesale. Cherry-pick individual lints only.

Suppression discipline:

- Prefer fixing finding.
- Code intentionally unused: `#[expect(dead_code, reason = "...")]`, never blanket `#[allow(dead_code)]`.
- `clippy::disallowed_methods` blocks `Command::output`,
  `std::thread::sleep`, `std::fs::File::open`, and
  `std::fs::OpenOptions::open`. New render/runtime-thread call sites move
  behind async helpers or `spawn_blocking`. Non-render exceptions need local
  `#[expect(clippy::disallowed_methods, reason = "...")]` naming boundary
  (startup, build helper, test harness, owned OS thread, etc.).
- `unused_crate_dependencies` stays off; dependency scanners cover that class with fewer Cargo target false positives.
- `print_stdout`, `print_stderr`, `unwrap_used`, `expect_used`, `panic` enforced from workspace lint table. Route command output through explicit writer helpers; replace runtime-data panics with `Result`/`Option` control flow. Any survivor: narrow `#[expect(..., reason = "...")]` at smallest practical scope. Runtime input paths must not use `unwrap()`/`expect()` as validation. `exit` stays `warn`.
- Allowed-by-default maintainability lints split deliberately. `manual_assert` = `deny`; `manual_let_else`, `match_bool`, `trivially_copy_pass_by_ref`, `large_enum_variant`, `result_large_err`, `rc_buffer`, `str_to_string`, `clone_on_ref_ptr`, `return_self_not_must_use` = `warn`, gate under CI `-D warnings`. `needless_pass_by_value` and `large_futures` stay `allow` for now: first fires on many intentional by-value state/view handoffs, second on capsule async protocol readers where boxing every call site adds indirection without measured win. `string_to_string` not in table because Clippy 1.96 removed it for `implicit_clone`. `large_futures`, `needless_pass_by_value`, `implicit_clone` need fresh cleanup pass before raise.

Dead-code scanner layers:

- `cargo shear` PR-blocking in CI. Detects unused deps, misplaced deps, unlinked Rust source files.
- Tools installed via `mise`, not ad-hoc `cargo install` in workflows.

## Supply-chain and feature-matrix hygiene

PR CI runs standalone RustSec gate + deterministic policy checks:

```text
cargo audit
cargo deny check licenses bans sources
```

Scheduled hygiene workflow runs:

```text
cargo deny check advisories
cargo hack check --workspace --feature-powerset --all-targets --locked
```

`deny.toml` strict by default: crates.io = only allowed registry; Git sources denied; wildcard deps denied; yanked crates denied; license allowlist = Apache-2.0 plus MIT. Any non-Apache/MIT license must be version-pinned exception with short comment noting it awaits operator ruling.

`.cargo/audit.toml` mirrors any RustSec advisory ignores from `deny.toml` so standalone `cargo audit` PR gate and scheduled `cargo-deny` advisory gate agree on accepted risk. Keep rationale comments in both files in sync.

Current transitive duplicate-version debt recorded as version-pinned `bans.skip` entries. Keep `multiple-versions = "warn"` so new duplicate version still trips gate; no broad duplicate allows.

`cargo-audit` = deliberately duplicated RustSec PR gate so PR changing `Cargo.lock` can't introduce known-vulnerable dependency while waiting for scheduled hygiene workflow. `cargo-vet` and `cargo-crev` deliberately not adopted while jackin❯ solo-maintainer: they're shared-audit systems, no audit-sharing org here. Revisit only if jackin❯ gains real multi-person review/audit group or production deployment needs provenance attestations beyond RustSec advisory checks.
