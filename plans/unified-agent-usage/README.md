# Unified agent usage implementation package

Roadmap: [unified-agent-usage](../../roadmap/unified-agent-usage/README.md)  
Branch: `chore/roadmap-unified-agent-usage`  
PR: #898  
Planned at: `92d21efb`

All rows execute sequentially in this branch and this PR. Do not create another
branch or PR. Re-read current state before every row because earlier rows modify the
same integration seams.

| Plan | Outcome | Depends on | Status |
|---|---|---|---|
| [001](001-freeze-contract-and-baseline.md) | Executable contract/baseline gates | — | DONE |
| [002](002-build-canonical-projection.md) | Canonical identity and V1 projection | 001 | DONE |
| [003](003-build-durable-broker.md) | Process-independent broker authority | 002 | DONE |
| [004](004-complete-provider-adapters.md) | Eight-provider quota parity | 002, 003 | DONE |
| [005](005-ship-cli-and-console.md) | Simple CLI and native Console Usage | 003, 004 | DONE |
| [006](006-ship-capsule-usage.md) | Resolved-agent Capsule Usage | 003, 004 | DONE |
| [007](007-ship-desktop-usage.md) | FFI, popover, and native Usage window | 003, 004 | DONE |
| [008](008-prove-parity-and-release.md) | Cross-surface proof and signed distribution | 005, 006, 007 | REJECTED (external release authorization required) |

Frozen package fingerprint: `820bb764b86d288b2cee69d50a2f08014c509866`.

## Execution rules

- One row at a time; commit with DCO and required co-author trailer; push immediately.
- Keep PR #898 updated. Never create a branch or PR from a plan.
- A zero-test Cargo filter is failure. Fix the named test target before advancing.
- Preserve unrelated work. Stop on source drift that invalidates cited seams.
- Update roadmap/docs in the same PR as behavior.
