# ITEM-002: Author behavioral spec for `runtime/launch.rs`

**Phase:** 1  
**Risk:** low  
**Effort:** small (half day)  
**Requires confirmation:** no  
**Depends on:** ITEM-001 (do //! doc first, then spec)

## Summary

`runtime/launch.rs` is the highest-priority behavioral spec target: no `//!` doc, ~1077L production, the critical path for all `jackin load` operations. Five invariants have been verified by reading the actual code (lines 553–892). The spec goes to `docs/src/content/docs/internal/specs/runtime-launch.mdx` and is browsable at `jackin.tailrocks.com/internal/specs/runtime-launch/`.

## Verified invariants (from iteration 35 — read lines 553–892)

| INV | Description | Verify by |
|---|---|---|
| INV-1 | Trust gate (`line 594`) precedes image build (`line 736`) — untrusted agent is cloned but NOT built until confirmed | trust call before `build_agent_image` in `load_agent_with` |
| INV-2 | Container name claimed (`line 754`) between image build and network creation (`line 827`) | `claim_container_name` between `build_agent_image` and `launch_agent_runtime` |
| INV-3 | Token verified (`line 763`) before network creation — fail fast if auth token missing | `verify_token_env_present` before `launch_agent_runtime` |
| INV-4 | `render_exit` called on ALL exit paths (`lines 886 and 890`) — both `Ok` and `Err` arms | both match arms call `render_exit` |
| INV-5 | Cleanup disarm semantics are state-dependent: `Running` → disarm (hardline can restart), clean exit → cleanup, crash → disarm | `match inspect_container_state(...)` arm mapping |

## Steps

1. Create `docs/src/content/docs/internal/specs/runtime-launch.mdx`.
2. Use the INV-N template from §8.1 of the research doc.
3. Frontmatter: `title: runtime/launch.rs — Behavioral Spec`, `spec_type: behavioral`, `subsystem: runtime`.
4. Sections: Purpose, 4-step pipeline overview (Steps 1–4), Behavioral invariants (INV-1 through INV-5), Testing notes (injection seams via `LoadOptions.op_runner` and `LoadOptions.host_env` at lines 657–693).
5. Add sidebar entry in `docs/astro.config.ts` under Developer Reference → Specs.

## Key file

`src/runtime/launch.rs` — specifically `fn load_agent_with` at line 553.

## Why this must come before ITEM-012 (structural split)

The spec is the contract against which the post-split code is verified. If launch.rs is split before the spec exists, there is no oracle to verify the split preserved all invariants.
