# crates/AGENTS.md

Rules specific to the `crates/` workspace crates. These apply to all crates under this directory in addition to the root `AGENTS.md`.

## Rust module layout (workspace crates — hard rule)

**Use Rust 2024 self-named module files. Do not create `mod.rs` files.**

For modules with child files, use a self-named module root:

```text
# correct
crates/jackin-foo/src/bar.rs        ← module root
crates/jackin-foo/src/bar/baz.rs    ← child module
crates/jackin-foo/src/bar/tests.rs  ← test module
```

Not:

```text
# wrong — legacy layout
crates/jackin-foo/src/bar/mod.rs    ← do not create
crates/jackin-foo/src/bar/baz.rs
```

`lib.rs` and `main.rs` are the allowed crate-root exceptions.

`clippy::mod_module_files = "deny"` is enabled in the workspace `[lints.clippy]` table and enforced by CI. Any PR that introduces a new `mod.rs` will fail.

### Rationale

Rust 2024 edition explicitly recommends the self-named layout: `mod foo;` can load either `foo.rs` or `foo/mod.rs`, and the Rust Reference encourages `foo.rs` because it avoids dozens of files all named `mod.rs`. The `jackin-tui`, `jackin-console`, and other workspace crates already follow this convention.

## Naming

Follow standard Rust naming:

- crates/modules/files: `snake_case`
- functions/methods/variables: `snake_case`
- types/traits/enums/structs: `UpperCamelCase`
- constants/statics: `SCREAMING_SNAKE_CASE`

Avoid clever abbreviations unless they are established domain terms (e.g. `tui`, `cli`, `dind`, `pty` are acceptable; `mgr`, `cfg_ed`, `ws` are not).

## Migration note (root `src/` crate)

The legacy root `src/` crate (`src/config/mod.rs`, `src/runtime/mod.rs`, etc.) uses the old `mod.rs` layout while it is still a monolith. When a module is extracted into a new workspace crate, the extracted crate **must** use the self-named layout from the start. Legacy `src/mod.rs` files are tolerated only until each module is extracted.

## Workspace lint baseline (hard rule)

All crates inherit the root workspace metadata and lints:

```toml
[lints]
workspace = true
```

Do not add per-crate copies of `edition`, `rust-version`, `license`, `repository`, or lint tables unless the crate has a documented reason to diverge.

The root `[workspace.lints.rust]` table is the source of truth. Dead-code and hygiene lints stay at deny: `unused`, `unused_imports`, `unused_variables`, `unused_must_use`, `dead_code`, and `unreachable_pub`. Rust 2024 unsafe-hygiene lints are enabled, and `unsafe_code = "forbid"` remains workspace-wide.

Clippy is enforced by CI with:

```text
cargo clippy --workspace --all-targets --all-features --locked -- -D warnings
```

The workspace enables `clippy::all` at deny and `pedantic`/`cargo` as the modern baseline, then carries explicit allow entries for noisy style-only lints that do not match jackin' current API shape. Do not enable `nursery` or `restriction` wholesale. Cherry-pick individual lints only.

Suppression discipline:

- Prefer fixing the finding.
- If code must intentionally stay unused, use `#[expect(dead_code, reason = "...")]`, never a blanket `#[allow(dead_code)]`.
- `unused_crate_dependencies` stays off; dependency scanners cover that class with fewer Cargo target false positives.
- `unwrap_used`, `expect_used`, `panic`, and print lints are documented policy lints, but the current workspace table leaves them allowed while the pre-release codebase is being reduced. Runtime input paths still must not use `unwrap()`/`expect()` as validation.

Dead-code scanner layers:

- `cargo shear` is PR-blocking in CI. It detects unused dependencies, misplaced dependencies, and unlinked Rust source files.
- `cargo udeps` and `cargo workspace-unused-pub` are pinned in `mise.toml` for scheduled/manual hygiene sweeps.
- Tools are installed through `mise`, not ad-hoc `cargo install` in workflows.
