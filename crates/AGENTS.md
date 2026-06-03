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
