# ITEM-017: rustdoc JSON → Astro Starlight API pipeline

**Phase:** 3  
**Risk:** medium  
**Effort:** large (3–5 days)  
**Requires confirmation:** yes  
**Depends on:** ITEM-001 (//! coverage sprint — pipeline value ∝ //! coverage), ITEM-005 (Starlight Developer Reference section exists)

## Summary

Replace `cargo doc` HTML output (never published, lives in `target/doc/`) with a bun TypeScript build script that consumes `rustdoc --output-format json` and generates Starlight MDX pages at `docs/src/content/docs/internal/api/`. API docs become browsable, searchable, and cross-linked to behavioral specs — on the same site as user guides.

## Pipeline

```
cargo +nightly rustdoc --output-format json -p jackin --document-private-items
→ target/doc/jackin.json
→ docs/scripts/gen-rust-api.ts (new bun TypeScript script)
→ docs/src/content/docs/internal/api/<module>/<Item>.mdx  (gitignored)
```

## Key design decisions (to confirm)

1. **What to include:** All `pub` items + private items with `//!` or `///` docs.
2. **URL structure:** `jackin.tailrocks.com/internal/api/runtime/LoadOptions/`
3. **Cross-links:** Hardcoded map in the script from type name → spec URL (e.g. `OpPickerState` → `/internal/specs/op-picker/`)
4. **Nightly requirement:** CI adds a separate step with `dtolnay/rust-toolchain@nightly` — isolated from the stable `check` job.
5. **Generated files:** `.gitignore`d — regenerated on each `bun run build`.

## Connection to §11 (future project)

The `gen-rust-api.ts` script IS the prototype for the generalized modern docs.rs alternative described in §11. Once it works for jackin, the processing layer can be extracted and applied to any crate's rustdoc JSON.

## What needs confirmation

- Confirm nightly toolchain in CI is acceptable (does not affect the stable Rust check job).
- Confirm generated MDX files are `.gitignore`d rather than committed.
- Confirm the `/internal/api/` URL prefix.
- Confirm this is Phase 3 (after Phase 1 //! sprint) — do NOT do before ITEM-001.
