# ITEM-001: Write `//!` module contracts for priority files

**Phase:** 1  
**Risk:** low  
**Effort:** small (1–2 days)  
**Requires confirmation:** no  
**Depends on:** none

## Summary

41% of source files have `//!` orientation docs. The 10 files below have the highest impact: they are large, AI-generated, or have no documentation at all. Each `//!` doc should state: (1) what the module does, (2) what invariants it maintains, (3) what it is NOT responsible for.

## Priority files (verified by grep — zero `//!` docs)

| File | Total L | Production L | Why urgent |
|---|---|---|---|
| `src/runtime/launch.rs` | 2368 | ~1077 | Largest production file, critical path, no //! at all |
| `src/app/mod.rs` | 979 | ~957 | Dispatch entry point, 0 docs |
| `src/operator_env.rs` | 2130 | ~880 | 1Password + env layers, complex |
| `src/instance/mod.rs` | — | — | AgentState, auth provisioning |
| `src/runtime/cleanup.rs` | 587 | ~220 | gc_orphaned_resources, lifecycle |
| `src/runtime/image.rs` | — | — | Docker image build + naming |
| `src/docker.rs` | — | — | CommandRunner trait + ShellRunner |
| `src/derived_image.rs` | — | — | Agent image derivation |
| `src/tui/animation.rs` | 582 | — | 21 eprintln! calls, no docs |
| `src/app/context.rs` | 784 | — | app helpers |

## Steps

1. For each file, read the first 50 lines and the public function/struct list.
2. Write a `//!` block at the top covering: purpose, scope claim, key invariants, what callers can assume.
3. Follow the `env_model.rs` exemplar pattern (3-element: purpose + scope + history).
4. Do not add `///` item-level docs in this pass — only `//!` module-level.

## Research backing

Iterations 1–20 (//! coverage measurement). The `agent_allow.rs` (55L, //! doc present) is the exemplar. `env_model.rs` is the 3-element exemplar. Current coverage: 37 of 94 files (39%).

## Caveats

None — purely additive, zero behavior change, zero CI risk.
