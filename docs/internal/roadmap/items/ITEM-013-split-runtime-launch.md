# ITEM-013: Split `runtime/launch.rs` into 4 files

**Phase:** 2  
**Risk:** medium-high  
**Effort:** medium (1–2 days)  
**Requires confirmation:** yes  
**Depends on:** ITEM-002 (behavioral spec MUST exist first — spec is the verification oracle)

## Summary

`src/runtime/launch.rs` is 2368L total, ~1077L production (test section starts at line 1078). It contains 4 distinct concerns mixed together. Split into a flat module directory.

## Proposed output

```
src/runtime/
  launch.rs          ← thin public API: pub fn load_agent (line 533), LoadOptions struct (line 23)
  launch_pipeline.rs ← fn load_agent_with (553–894), LaunchContext, StepCounter, LoadCleanup
  terminfo.rs        ← resolve_terminal_setup (141), export_host_terminfo (167)
  trust.rs           ← confirm_agent_trust (216) — already isolated via FnOnce injection
```

## Why the split is safe — FnOnce injection pattern (verified iteration 15)

`confirm_agent_trust` is passed to `load_agent_with` as a `FnOnce` argument at line 553–560:
```rust
fn load_agent_with(..., confirm_trust: impl FnOnce(...) -> anyhow::Result<()>)
```
`launch_pipeline.rs` does NOT import `trust.rs` by name — it receives the function as a generic parameter. After split, `launch.rs` imports `confirm_agent_trust` from `trust.rs` and passes it in. Zero circular import risk.

## Import chain safety (verified)

- `terminfo.rs`: only imports from `std` — zero crate dependencies
- `trust.rs`: imports from `std`, `owo_colors`, `dialoguer`, `crate::config` — no runtime dependency
- `launch_pipeline.rs`: imports from `crate::config`, `crate::instance`, `crate::tui`, etc.
- `launch.rs` (thin API): imports from `launch_pipeline`, `trust`, `terminfo`

## Test suite impact

The test suite is 1282L at `#[cfg(test)]` starting line 1078. Tests use `FakeRunner` from `runtime/test_support.rs`. All tests stay in `launch_pipeline.rs` alongside the code they test.

## What needs confirmation

- The exact split of `LoadCleanup` (lines 1030–1085 in current file — is it pipeline concern or launch concern?)
- Whether `claim_container_name` (line 918–957) and `verify_token_env_present` (line 959–992) stay in `launch_pipeline.rs` or move to a `launch_helpers.rs`

## Ordering note

Do LAST among the Phase 2 splits. The test suite is the most complex and any compilation error here blocks all runtime changes.
