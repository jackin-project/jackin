# ITEM-009: Add rust-toolchain.toml + MSRV CI check

**Phase:** 1  
**Risk:** low  
**Effort:** small (< half day)  
**Requires confirmation:** no  
**Depends on:** none

## Summary

Three separate files each assert a Rust version: `mise.toml` (1.95.0), `Cargo.toml` rust-version (1.94), CI SHA (1.95.0). No `rust-toolchain.toml` exists. No MSRV check job in CI — the declared MSRV of 1.94 is never verified.

## Steps

1. Create `rust-toolchain.toml` at repo root:
   ```toml
   [toolchain]
   channel = "1.95.0"
   ```
   `rust-analyzer` reads this automatically for IDE toolchain selection.

2. Update `mise.toml` to add a comment cross-referencing `rust-toolchain.toml` (so the two stay in sync).

3. Add MSRV check step to `.github/workflows/ci.yml`:
   ```yaml
   - name: Check MSRV
     run: cargo +1.94.0 check
     env:
       RUSTUP_TOOLCHAIN: "1.94.0"
   ```
   Note: `dtolnay/rust-toolchain` does NOT read `rust-toolchain.toml` — its version is encoded in the action SHA. The SHA in `ci.yml` and `release.yml` must be manually kept in sync with `rust-toolchain.toml`.

## Caveats

- If `cargo +1.94.0 check` fails, it means code uses features stabilized after 1.94. In that case, update `rust-version` in `Cargo.toml` to the correct floor.
- `u64::is_multiple_of` (used in `tui/animation.rs` lines 70, 264, 432, 437) was stabilized in 1.86 — safely within the 1.94 MSRV.
- `edition = "2024"` requires ≥ 1.85, which is below 1.94. No issue.
