# Roadmap

Actionable items derived from the readability and modernization research (40 iterations, 2026-04-26). Pick an item, work on it, mark it done.

## Items by phase

### Phase 1 — Documentation & Setup (low risk, no structural changes)

| Item | Title | Confirmation needed |
|---|---|---|
| [ITEM-001](items/ITEM-001-module-contracts.md) | Write `//!` module contracts for 10 priority files | No |
| [ITEM-002](items/ITEM-002-behavioral-spec-launch.md) | Author behavioral spec for `runtime/launch.rs` | No |
| [ITEM-003](items/ITEM-003-behavioral-spec-op-picker.md) | Author behavioral spec for `op_picker/mod.rs` | No |
| [ITEM-004](items/ITEM-004-per-directory-readme.md) | Add per-directory README.md to major directories | No |
| [ITEM-005](items/ITEM-005-starlight-developer-reference.md) | Set up Starlight "Developer Reference" section | **Yes** |
| [ITEM-006](items/ITEM-006-update-project-structure.md) | Update PROJECT_STRUCTURE.md with PR #171 additions | No |
| [ITEM-007](items/ITEM-007-ci-gate-project-structure.md) | Add CI gate for PROJECT_STRUCTURE.md freshness | No |
| [ITEM-008](items/ITEM-008-pub-crate-visibility.md) | Enable `unreachable_pub` lint + pub(crate) pass | No |
| [ITEM-009](items/ITEM-009-msrv-toolchain.md) | Add rust-toolchain.toml + MSRV CI check | No |
| [ITEM-010](items/ITEM-010-adrs-decisions.md) | Author first Architecture Decision Records (ADRs) | No |
| [ITEM-011](items/ITEM-011-snapshot-tests-tui.md) | Add snapshot tests for TUI render output | No |
| [ITEM-016](items/ITEM-016-agent-workflow-ccsdd.md) | Install cc-sdd + remove superpowers plugin | **Yes** |
| [ITEM-018](items/ITEM-018-move-contributing-testing.md) | Move CONTRIBUTING.md + TESTING.md to docs/internal/ | **Yes** |

### Phase 2 — Structural splits (moderate risk, confirmation required)

| Item | Title | Confirmation needed |
|---|---|---|
| [ITEM-012](items/ITEM-012-split-input-editor.md) | Split `input/editor.rs` (~1141L production) | **Yes** |
| [ITEM-013](items/ITEM-013-split-runtime-launch.md) | Split `runtime/launch.rs` into 4 files | **Yes** |
| [ITEM-014](items/ITEM-014-split-app-mod.md) | Split `app/mod.rs` into `app/` directory | **Yes** |
| [ITEM-015](items/ITEM-015-split-operator-env.md) | Split `operator_env.rs` into `operator_env/` directory | **Yes** |

### Phase 3 — Future (deferred)

| Item | Title | Confirmation needed |
|---|---|---|
| [ITEM-017](items/ITEM-017-rustdoc-json-starlight.md) | rustdoc JSON → Astro Starlight API pipeline | **Yes** |
| [ITEM-019](items/ITEM-019-greenfield-workspace.md) | Greenfield workspace split (when LOC > 150K) | **Yes** |

## Ordering notes

- Do **ITEM-002** before **ITEM-013** — behavioral spec is the verification oracle for the split
- Do **ITEM-005** before **ITEM-002**, **ITEM-003**, **ITEM-010** — specs and ADRs need the Starlight section to exist
- Do **ITEM-006** before **ITEM-007** — fix existing staleness before enabling the gate
- Do **ITEM-011** (snapshot tests) before Phase 2 splits — regression net
- Do **ITEM-001** before **ITEM-017** — pipeline value scales with //! coverage

## Research archive

Full research document: [`READABILITY_AND_MODERNIZATION.md`](READABILITY_AND_MODERNIZATION.md) (2343L — reference only, not the source of truth for execution).  
Iteration history: [`_iteration_log.md`](_iteration_log.md) (40 iterations of analysis).
