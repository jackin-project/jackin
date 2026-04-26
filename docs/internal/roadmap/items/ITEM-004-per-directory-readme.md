# ITEM-004: Add per-directory README.md to major src/ directories

**Phase:** 1  
**Risk:** low  
**Effort:** small (half day)  
**Requires confirmation:** no  
**Depends on:** none

## Summary

AI coding agents (Claude Code, Copilot, Cursor) load `README.md` automatically when they enter a directory context — this is the primary AI-native orientation mechanism. Adding README.md to 7 major directories gives AI agents immediate context without needing to open source files first.

## Target directories and content

| Directory | README.md should say |
|---|---|
| `src/` | Top-level module map + entry points (load_agent, run_console, validate). Link to PROJECT_STRUCTURE.md for full map. |
| `src/runtime/` | Container bootstrap pipeline. Entry: `load_agent()` in `launch.rs`. 4-step sequence. Behavioral spec at `/internal/specs/runtime-launch/`. |
| `src/console/` | Operator console TUI. Entry: `run_console()` in `mod.rs`. Two subsystems: `manager/` and `widgets/`. No import from `runtime/` — independently compilable. |
| `src/console/manager/` | Workspace manager TUI. 3-layer architecture: `state.rs` (data), `input/` (dispatch), `render/` (drawing). |
| `src/console/widgets/` | Reusable TUI widget library. Each widget is a self-contained state machine. `op_picker/` has behavioral spec. `file_browser/` is the exemplar of target module shape. |
| `docs/` | Astro Starlight documentation site. Public: `src/content/docs/`. Internal/browsable: `src/content/docs/internal/`. |
| `docs/internal/` | Developer Reference index. Architecture, specs, ADRs, roadmap items. Browsable at `jackin.tailrocks.com/internal/`. |

## Steps

1. Create `src/README.md` through `docs/internal/README.md` with the content above.
2. Keep each file under ~20 lines — orientation only, not comprehensive documentation.
3. Link to the relevant Starlight pages where they exist (specs, ADRs).

## Caveats

These files are NOT CI-checked for staleness yet (that's ITEM-007). They will drift if modules change significantly.
