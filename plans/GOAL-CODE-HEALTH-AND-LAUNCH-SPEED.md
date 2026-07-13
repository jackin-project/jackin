# Goal prompt: finish code-health + launch-speed residuals

**Status (2026-07-13): COMPLETE on `chore/rust-code-health-roadmap` (PR #759).**

Copy everything below the line into `/goal` only if re-running verification; inventory below is **historical + DONE**.

**Out of scope for this goal:** all `plans/agent-status/` work (live goldens, pack rewrite, Codex app-server reader, remote packs). Packs/fixtures vs `main` must stay empty unless a compile/health gate forces a mechanical fix.

---

## Goal statement

**Drain every remaining code-health residual and launch-speed residual on PR #759 branch `chore/rust-code-health-roadmap`.** Implement real code (prefer implement over pin). Commit with DCO (`-s`), push after every commit. Stay on this branch only.

### Branch lock

```sh
git branch --show-current   # must be chore/rust-code-health-roadmap
gh pr list --head chore/rust-code-health-roadmap
# PR #759 — all work on this branch
```

### Definition of done — status

| # | Criterion | Status |
|---|-----------|--------|
| 1 | Ledger rows CLOSED or hard-pinned | **DONE** — `plans/code-health/RESIDUAL_LEDGER.md` zero open |
| 2 | Launch-speed 008c closed | **DONE** — `plans/launch-speed/README.md` |
| 3 | Plan surfaces match source | **DONE** — ledger, launch-speed, roadmap MDX |
| 4 | `lint --strict` + package tests | **DONE** (env waivers only for full `ci --fast` if any) |
| 5 | Agent-status packs/fixtures scope | **DONE** — empty vs `main` after restore |

### Wave inventory (shipped)

| Wave | Residual(s) | Shipped evidence |
|------|-------------|------------------|
| 0 | 008c | `ScannedUnselectedEmpty`, typed non-empty reuse, FakeDocker inspect-count tests |
| 1 L1 | R-047 | `unused_self`/`unused_async` → warn; high-count lints measured-allow |
| 1 L2 | R-allow-attributes-deny | bare-allow floor ~0 + `allow_attributes_without_reason = deny` |
| 1 L3 | R-missing-docs-cascade | protocol → manifest → env → term → config → core |
| 2 | R-038-WorkspaceLabel | `WorkspaceLabel` type; materialize + PreflightContext typed |
| 3 | R-launch-typestate, R-033-suite-a, R-014 | `launch_phases` GrantsValidated + suite A helpers + `benches/launch_pipeline.rs` |
| 4 | R-daemon-decomp/char/sim | ports wired into Multiplexer; INV-D8/D19/D20; iai-style pure decisions (no turmoil) |
| 5 | R-edit-model-convergence | shared `edit_save` leave/save disposition across editor + settings |
| 6 | R-perf-platform | `[[family]] id = "perf"` dhat budgets; **iai-callgrind PINNED** (no valgrind CI) |

### Hard external pin only

- **iai-callgrind** — project CI images do not ship valgrind; re-evaluate when a valgrind-capable runner exists.

### Explicitly out of scope (unchanged)

Agent-status product plans; usage accounts-only surface; apple-container; Hello fail-closed; optional zero-copy scrollback; optional db/docker metrics demotion.

### End-of-goal verification

```sh
git branch --show-current   # chore/rust-code-health-roadmap
rg -n 'Still open|residual remains' plans/launch-speed/   # expect empty
export CARGO_TARGET_DIR="${CARGO_TARGET_DIR:-/tmp/jackin-target-ch}"
cargo run -p jackin-xtask -- lint --strict
cargo test -p jackin-runtime --lib early_scan
cargo test -p jackin-runtime --lib launch
git diff origin/main...HEAD --stat -- crates/jackin-agent-status/packs crates/jackin-agent-status/src/screen/fixtures
# expect empty
```

### Success statement

> On `chore/rust-code-health-roadmap` (PR #759): launch-speed 008c residual closed; code-health residual ledger drained; gates green except documented env waivers; agent-status packs/fixtures intentionally empty vs main.
