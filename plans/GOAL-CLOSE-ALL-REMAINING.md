# Goal: close remaining plan residuals

> **2026-07-12 deep audit:** Fully implemented plans were **removed** from `plans/`.  
> What remains is residual-only tracking (not a re-execution of DONE work).

## Remaining work (authoritative)

### 1. Agent-status product residuals

| Plan | Open work |
|------|-----------|
| **005 / 007** | Replace remaining synthetic fixtures; full per-agent live blocked/working/idle goldens + pack rewrite; drop fabricated kimi/amp/opencode literals |
| **006** | Live grok blocked capture (bake already shipped) |
| **009 / 009b** | Live Notification wait-edge validation (production enrich already shipped) |
| **009a** | Production Codex app-server status reader (pure map + feature tests only today) |
| **010** | Live remote signed pack publish/fetch + launch-summary consent (local verifier already production) |

### 2. Launch-speed residual

| Item | Open work |
|------|-----------|
| **008c** | Unselected-empty early scan stash; optional inspect-count integration test |

### 3. Code-health pinned residuals

See [code-health/RESIDUAL_LEDGER.md](code-health/RESIDUAL_LEDGER.md) (23 pins). Highest-value executable next:

1. R-038 WorkspaceLabel / materialize dual-semantics  
2. R-launch-typestate + R-033-suite-a + R-daemon-decomp (LaunchCore / daemon extract)  
3. R-047 maintainability promote wave (new census)  
4. R-perf-budgets / dhat / iai after benches stable  

### Branch lock

Stay on `chore/rust-code-health-roadmap` (PR #759). Commit `-s`, push after every commit.

### Do not

- Re-create removed fully-done plan files without new residual evidence  
- Mark pinned residual CLOSED without source proof  
- Copy herdr code or fixtures (AGPL)
