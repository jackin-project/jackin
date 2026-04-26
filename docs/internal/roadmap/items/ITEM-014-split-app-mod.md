# ITEM-014: Split `app/mod.rs` into `app/` directory

**Phase:** 2  
**Risk:** low-medium  
**Effort:** small-medium (1 day)  
**Requires confirmation:** yes  
**Depends on:** none (independent of other splits)

## Summary

`src/app/mod.rs` is 979L total, ~957L production (test section starts at line 957 — only 22L of tests). Nearly all production code is the `run()` dispatch function. Split into a module directory.

## Proposed output

```
src/app/
  mod.rs          ← re-exports, pub fn run() dispatcher (~80L)
  dispatch.rs     ← the giant match on cli::Command (~700L)
  workspace_cmd.rs ← workspace subcommand handlers (~150L)
  config_cmd.rs   ← config subcommand handlers (~100L)
```

Note: `src/app/context.rs` (784L) already implements the impl-extension pattern and stays as-is.

## Auditability gain

To audit "does `jackin workspace create` correctly validate the workdir?", a reviewer reads `workspace_cmd.rs` (~150L) instead of scanning 957L of mixed-command dispatch.

## Import notes

- `app/mod.rs` is referenced from `main.rs` as `mod app; app::run(...)` — this stays the same
- `dispatch.rs` is `mod dispatch` inside `app/`; `run()` calls `dispatch::dispatch_command(args)`
- `context.rs` already lives in `app/` — no change

## What needs confirmation

- Whether `run()` stays in `mod.rs` or also moves to `dispatch.rs`
- The exact boundary between `dispatch.rs` and `workspace_cmd.rs` (some workspace handling is interleaved in the dispatch match)
