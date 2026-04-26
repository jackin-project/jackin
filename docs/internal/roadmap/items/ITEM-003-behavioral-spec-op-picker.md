# ITEM-003: Author behavioral spec for `op_picker/mod.rs`

**Phase:** 1  
**Risk:** low  
**Effort:** small (half day)  
**Requires confirmation:** no  
**Depends on:** none (has //! doc already)

## Summary

`op_picker/mod.rs` is AI-generated (PR #171), ~775L production, and has a 7-line `//!` doc but no INV-format invariant contract. The spec captures the 4-level drill-down state machine and the critical `op://` reference format invariant. Goes to `docs/src/content/docs/internal/specs/op-picker.mdx`.

## Key invariants

| INV | Description | Verify by |
|---|---|---|
| INV-1 | `committed_reference` is 3-segment `op://Vault/Item/Field` — never 4-segment `op://Account/Vault/Item/Field` | grep `committed_reference` — must be `format!("op://{}/{}/{}"...)` |
| INV-2 | No secret values in picker path — `RawOpField` struct has no `value` field; serde drops it silently | grep `value` in op_picker/mod.rs for field access; exhaustive destructure test at `operator_env.rs:~2055` |
| INV-3 | Loading is async (background worker + channel); key handlers are synchronous — no blocking I/O in key dispatch | key handler fns must not call `std::thread::spawn` or block on channel recv |

## State machine

4 stages: `Accounts` → `Vaults` → `Items` → `Fields`. Each stage has a background loader and a key handler. Loaders post results via channel; `poll_*_load` drains them before each render.

## Steps

1. Create `docs/src/content/docs/internal/specs/op-picker.mdx`.
2. Use the template from §8.1 of the research doc (concrete example is already written there).
3. Add sidebar entry under Developer Reference → Specs.
4. Link from the `op_picker/mod.rs` `//!` doc to the spec URL.

## Key file

`src/console/widgets/op_picker/mod.rs` — 1712L total, ~775L production.
