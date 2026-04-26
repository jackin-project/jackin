# ITEM-015: Split `operator_env.rs` into `operator_env/` directory

**Phase:** 2  
**Risk:** medium  
**Effort:** medium (1 day)  
**Requires confirmation:** yes  
**Depends on:** none (independent)

## Summary

`src/operator_env.rs` is 2130L total, ~880L production (test section starts at line 881). Contains 4 distinct concerns. Only used by `console/` (verified: no other module imports from `operator_env`).

## Proposed output

```
src/operator_env/
  mod.rs      ← re-exports, OpRunner trait, OpStructRunner trait, dispatch (~100L)
  client.rs   ← OpCli struct + subprocess helpers (~280L, lines 105–364)
  layers.rs   ← EnvLayer, merge_layers, validate_reserved_names, resolve_operator_env* (~470L, lines 365–808)
  picker.rs   ← PR #171 1Password picker integration (~250L)
```

## Dependency graph — verified safe (iteration 15)

- `layers.rs` imports both `mod.rs` (OpRunner trait) AND `client.rs` (OpCli) — this creates a `layers → client` dep but NOT a `client → layers` dep. No circular risk.
- `picker.rs` only uses types from `mod.rs` (OpStructRunner, OpRunner)
- Zero external crate imports from `console/` into `operator_env` — the dependency is one-way

## Migration caveat

After split, all callers currently doing `use crate::operator_env::resolve_operator_env` continue to work because `mod.rs` re-exports everything. No call-site changes needed outside the `operator_env/` directory itself.

## What needs confirmation

- Whether `picker.rs` should be a separate file or merge with `mod.rs` (it's ~250L — borderline)
- Whether `print_launch_diagnostic` (the diagnostic output function) belongs in `client.rs` or `layers.rs`
