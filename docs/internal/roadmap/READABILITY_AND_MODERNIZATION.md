# Readability & Modernization — Research Archive

> **This file is a research archive, not the execution guide.**  
> For actionable roadmap items, see [`README.md`](./README.md).

This document captures 40 iterations of analytical research into the `jackin` codebase structure, readability, and modernization opportunities (2026-04-26). It is the primary source that informed the roadmap items in `items/`.

## Key findings summary

**Codebase:** 94 `.rs` files, ~43,587L total. Single Rust crate, two binaries (`jackin`, `jackin-validate`). TypeScript Astro Starlight docs site.

**Primary goal:** Make AI-generated code verifiable — 59% of files lack `//!` module contracts, no behavioral specs exist, no systematic way to verify "did the AI implement this correctly?"

**Production LOC hot-spots (verified by `#[cfg(test)]` line positions):**

| File | Total L | Production L |
|---|---|---|
| `src/console/manager/input/editor.rs` | 2349 | ~1141 |
| `src/runtime/launch.rs` | 2368 | ~1077 |
| `src/app/mod.rs` | 979 | ~957 |
| `src/operator_env.rs` | 2130 | ~880 |
| `src/console/widgets/op_picker/mod.rs` | 1712 | ~775 |
| `src/console/manager/render/editor.rs` | 1666 | ~736 |
| `src/console/manager/render/list.rs` | 1989 | ~668 |
| `src/console/manager/input/save.rs` | 1472 | ~661 |
| `src/console/manager/state.rs` | 992 | ~628 |

**Phase 2 threshold:** Only files with >800L production code are split (reduces scope from 14+ to 4 files).

**Visibility:** 257 bare `pub` items, 21 `pub(crate)`, 61 `pub(super)`. Zero `unreachable_pub` enforcement.

**Dependency tiers (verified iteration 37):**
- Tier 0 (no cross-module deps): `workspace/`, `manifest/`, `docker.rs`, `paths.rs`, `selector.rs`  
- Tier 1: `config/`, `tui/`, `instance/`, `env_resolver/`  
- Tier 2: `operator_env/`, `runtime/`, `repo/`  
- Tier 3: `console/` — **no import from `runtime/`** (pre-existing clean boundary)

**Documentation model (operator-directed):**
- `README.md` at every major directory (AI-native orientation)
- `AGENTS.md` = agent rules; `CLAUDE.md` = single-line `@AGENTS.md` pointer only
- All internal docs browsable at `jackin.tailrocks.com/internal/`
- `//!` = file-level contracts

**Workspace decision:** Stay single-crate (43,587L, no external consumers). starship and fd-find stay single-crate at similar scale. Trigger: LOC > 150K or external consumer need.

**Future project (§11):** The rustdoc JSON → Starlight pipeline (ITEM-017) is the prototype for a modern docs.rs alternative with MCP server for AI agent queries (Context7-for-Rust alternative).

## Sections in original research

- §0: Meta + executive summary
- §1: Full project inventory (module map, hot-spots, //! coverage)
- §2: 25 concept-to-location mappings with discoverability ratings
- §3: Documentation hierarchy diagnosis and proposed structure
- §4: Source-code structural diagnosis, module rules, greenfield architecture
- §5: Naming pass candidates (16 entries)
- §6: .github/ workflows, Justfile, build.rs, docker-bake.hcl analysis
- §7: 15 modernization candidates (error handling through rustdoc JSON pipeline)
- §8: AI-agent development workflow (spec-driven development, cc-sdd, ADRs)
- §9: Risks, open questions, deferred scope
- §10: Execution sequencing with Track A (tooling) and Track B (behavioral specs)
- §11: Future project — modern Rust documentation platform

The full research text was 2343L as of the final iteration. It has been archived in git history (commit `b7e9fc2` on `analysis/code-readability`).
