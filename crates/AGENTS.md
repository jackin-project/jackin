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

The root `[workspace.lints.rust]` table is the source of truth. Dead-code and hygiene lints stay at deny: `unused`, `unused_imports`, `unused_variables`, `unused_must_use`, `dead_code`, and `unreachable_pub`. Rust 2024 unsafe-hygiene lints are enabled, and `unsafe_code = "forbid"` remains workspace-wide. The table also denies the current strict allowed-by-default set (`unused_qualifications`, `unused_lifetimes`, `redundant_lifetimes`, `trivial_casts`, `trivial_numeric_casts`, `unnameable_types`, `unit_bindings`, `macro_use_extern_crate`, `meta_variable_misuse`, `single_use_lifetimes`, `let_underscore_drop`) and keeps `missing_debug_implementations` at `warn`, which CI promotes during the clippy gate.

Clippy is enforced by CI with:

```text
cargo clippy --workspace --all-targets --all-features --locked -- -D warnings
```

The workspace enables `clippy::all` at deny and `pedantic`/`cargo` as the modern baseline, then carries explicit allow entries for noisy style-only lints that do not match jackin' current API shape. `warnings = "deny"` is deliberately not baked into the manifest; the `-D warnings` gate stays in CI so a future compiler warning does not brick local builds while jackin' rides each new stable toolchain. Do not enable `nursery` or `restriction` wholesale. Cherry-pick individual lints only.

Suppression discipline:

- Prefer fixing the finding.
- If code must intentionally stay unused, use `#[expect(dead_code, reason = "...")]`, never a blanket `#[allow(dead_code)]`.
- `clippy::disallowed_methods` is enabled for blocking `Command::output`,
  `std::thread::sleep`, `std::fs::File::open`, and
  `std::fs::OpenOptions::open`. New render/runtime-thread call sites must move
  behind async helpers or `spawn_blocking`. Non-render exceptions need a local
  `#[expect(clippy::disallowed_methods, reason = "...")]` naming the boundary
  (startup, build helper, test harness, owned OS thread, etc.).
- `unused_crate_dependencies` stays off; dependency scanners cover that class with fewer Cargo target false positives.
- `print_stdout`, `print_stderr`, `unwrap_used`, `expect_used`, and `panic` are documented policy lints, but the current workspace table leaves them allowed while the pre-release codebase is being reduced. Runtime input paths still must not use `unwrap()`/`expect()` as validation. `exit` stays at `warn`.
- The allowed-by-default maintainability lints are split deliberately. `manual_assert` is `deny`; `manual_let_else`, `match_bool`, `trivially_copy_pass_by_ref`, `large_enum_variant`, `result_large_err`, `rc_buffer`, `str_to_string`, `clone_on_ref_ptr`, and `return_self_not_must_use` are `warn` and therefore gate under CI `-D warnings`. `needless_pass_by_value` and `large_futures` stay `allow` for now because the first currently fires on many intentional by-value state/view handoffs and the second on capsule async protocol readers where boxing every call site would add indirection without a measured win. `string_to_string` is not in the table because Clippy 1.96 removed it in favor of `implicit_clone`. `large_futures`, `needless_pass_by_value`, and `implicit_clone` need a fresh cleanup pass before being raised.

Dead-code scanner layers:

- `cargo shear` is PR-blocking in CI. It detects unused dependencies, misplaced dependencies, and unlinked Rust source files.
- `cargo udeps` and `cargo workspace-unused-pub` are pinned in `mise.toml` for scheduled/manual hygiene sweeps. The unused-`pub` scanner is intentionally niche: it covers the workspace-wide public-API dead-code class rustc cannot see. This is valid while jackin' is pre-release and no crate is published; if any crate becomes a public downstream API, unused public items stop being automatically dead and this layer needs a fresh policy decision.
- `.github/scripts/check-workspace-unused-pub.sh` is the CI wrapper for `cargo workspace-unused-pub`. The raw 0.1.0 tool currently reports test functions and required trait-impl methods as unused functions, so the wrapper allowlists only those documented false positives plus deliberate roadmap exceptions. Any new finding outside that list is a failure and must be deleted or documented before the allowlist grows.
- Tools are installed through `mise`, not ad-hoc `cargo install` in workflows.

## Supply-chain and feature-matrix hygiene

`cargo-deny` is the single supply-chain gate. PR CI runs:

```text
cargo deny check licenses bans sources
```

The scheduled hygiene workflow runs:

```text
cargo deny check advisories
cargo workspace-unused-pub (via .github/scripts/check-workspace-unused-pub.sh)
cargo hack check --workspace --feature-powerset --all-targets --locked
```

`deny.toml` is strict by default: crates.io is the only allowed registry, the
`donbeave/vt100-rust` oracle is the only allowed Git source, wildcard
dependencies are denied, yanked crates are denied, and the license allowlist is
Apache-2.0 plus MIT. Any non-Apache/MIT license must be a version-pinned
exception with a short comment explaining that it awaits an operator ruling.

Current transitive duplicate-version debt is recorded as version-pinned
`bans.skip` entries. Keep `multiple-versions = "warn"` so a new duplicate
version still trips the gate; do not add broad duplicate allows.

`cargo-audit` is not a separate gate because `cargo-deny` already runs RustSec
advisories. `cargo-vet` and `cargo-crev` are deliberately not adopted while
jackin' is a solo-maintainer project: they are shared-audit systems, and there
is no audit-sharing organization here. Revisit them only if jackin' gains a
real multi-person review/audit group or production deployment requirements that
need provenance attestations beyond RustSec advisory checks.
