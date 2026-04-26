# ITEM-008: Enable `unreachable_pub` lint + pub(crate) pass

**Phase:** 1  
**Risk:** low  
**Effort:** small (half day)  
**Requires confirmation:** no  
**Depends on:** none

## Summary

`jackin` is a binary crate with no external consumers. 257 bare `pub` items exist (verified iteration 40) but only 21 use `pub(crate)`. The `unreachable_pub` lint detects items that are `pub` but not reachable from the crate root — these should be `pub(crate)`. Enabling this lint and doing a cleanup pass improves encapsulation signalling and makes the visibility intent explicit.

## Verified numbers (iteration 40 grep)

- 257 bare `pub` items (functions, structs, enums, traits, types, consts)
- 21 `pub(crate)` items
- 61 `pub(super)` items
- 0 files use `unreachable_pub` — no enforcement anywhere

## Top violators

| File | Bare pub items | Notes |
|---|---|---|
| `src/operator_env.rs` | 17 | OpRunner, OpCli, OpStructRunner traits — should be `pub(crate)` |
| `src/tui/output.rs` | 13 | All operator-facing output fns — `pub(crate)` is correct |
| `src/workspace/planner.rs` | 8 | Plan structs used internally — `pub(crate)` |

## Steps

1. Add to `Cargo.toml` `[lints.rust]` section:
   ```toml
   [lints.rust]
   unreachable_pub = "warn"
   ```
2. Run `cargo check` — compiler lists all `pub` items unreachable from the crate root.
3. Convert each flagged item from `pub` to `pub(crate)`.
4. Verify `cargo nextest run` still passes.
5. Verify `cargo check` with no warnings.

Estimated conversions: ~150–200 items (excluding genuine entry points like `pub fn load_agent`, CLI structs, `bin/validate.rs` items).

## What stays `pub`

- `pub fn load_agent` / `pub fn run_console` — binary entry points called from `main.rs`
- All items in `src/bin/validate.rs` — the validate binary's public interface
- CLI structs derived from `clap::Parser` — need `pub` for clap
- Any item re-exported in `lib.rs` (there is none currently, but check)

## Caveats

- The lint runs with `cargo check`, not `cargo clippy`. Adding it to `[lints.rust]` means it runs in the existing `cargo clippy -- -D warnings` CI gate, but only after converting all current violations first.
- Do a single-commit conversion so CI doesn't have a partial state.
