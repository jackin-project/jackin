# Implementation progress

## Current result

**Product status:** NOT IMPLEMENTED BY THIS DOCUMENT UPDATE.  
**Live-agent checks:** NOT RUN.  
**Pkl fixture evaluation:** NOT RUN.  
**Sandbox and recovery checks:** NOT RUN.  
**MVP release decision:** NOT READY.

The coordinator updates this file during implementation.
Do not change a result to PASS without a command result and evidence reference.

## Selected environment

| Field | Value |
|---|---|
| Implementation commit | `30e804aa` (Step 0 base; branch `chore/workflows-mvp`). |
| Host OS and architecture | macOS 27.0.1, arm64. |
| Local Docker version and profile | Docker 29.8.2. Profile `workflow-local` to be created in Step 2. |
| Claude binary, model, account ID, and auth mode | `claude` present. Account `default-claude` (profile dir, subscription login presumed; proven in Step 1). No secrets belong here. |
| Codex binary, model, account ID, and auth mode | `codex` present. Account `default-codex` (profile dir, subscription login presumed; proven in Step 1). No secrets belong here. |
| Role and image digests | Role `agent-smith` (mise provides python 3.14.7). Image digest recorded in Step 2. |
| Fixture repository and base commit | Fixture to be created in Step 2. Workspace `workflows-mvp` to be created (none configured yet). |
| Backup mode and authorized test remote | Mode `off` for runs; exact-ref backup tested against a temporary bare remote in Step 2. |
| Current test and CI commands | `mise exec -- mbx +1.99.0 nextest run -p <crate>`; Rust 1.99.0; turso `=0.8.3`; docs gate `cargo xtask docs mvp-links`. |

## Step record

| Step | Check | State | Commit | Command and evidence |
|---|---|---|---|---|
| 0 | K0 | PASS | Step 0 commit | Gate `cargo run -p jackin-xtask -- docs mvp-links` prints `mvp links OK`. New tests `docs::mvp_links::tests` 4/4 pass. Existing baseline `docs::site_links::tests` 1/1 passes. `cargo fmt --check -p jackin-xtask` clean. `cargo clippy -p jackin-xtask --all-targets` clean. Env: macOS 27.0.1 arm64, Docker 29.8.2, Rust 1.99.0, turso `=0.8.3`. No active instruction names deleted `TESTING.md` (only superseded `plans/workflows/implementation/01-build-sequence.md:97`; README now redirects MVP work to `mvp/`). Independent verifier re-ran all four K0 checks at exact commit `894c9af2` in a separate read-only worktree with its own target dir: PASS, no files changed, worktree removed. |
| 1 | K1 | PARTIAL (Claude PASS, Codex retry) | `48d9acb9` | Claude: two prompts (`K1TURN1-OK`, `K1TURN2-OK`) answered in one live session (container `jk-9vb1d12g`, session 1, account `default-claude`, Claude Code v2.1.299, Sonnet 4.6, Claude Max subscription). Observer: `k1-observer.ndjson` (20 events, subscribe+activity+idle on session 1, no input). Ownership: `daemon::tests::case_26` 4/4, `session_control::tests::case_02` 3/3 pass. Refresh: codex provider `case_02` 2/2 pass (fake-expiry). Evidence: `/tmp/workflows-mvp-evidence/`, `fixture-revisions.txt` (`5c9f200` base, `74c969f` review rev). Network: OUTPUT DROP default with `jackin-allowed` ipset (5 entries) in `probe.log`. Codex review BLOCKED (not credential failure; credentials never exercised). Attempt 1 under `the-architect`: container preflight fails (`codex caveman skill missing`, exit 1, no session; see `load-codex9.typescript` tail). Attempt 2 under `agent-smith`: rejected at identity resolution, manifest declares `agents = ["claude"]` (`jackin.role.toml:7`, verified). Missing prerequisite: a codex-capable role with a working preflight (install the `caveman` skill into the `the-architect` role image via its own repo, or select another codex role). No role rebuild or skipped preflight inside MVP scope. Safe work continues: Step 2 needs only the proven Claude profile, which cannot change with the Codex role choice. |
| 2 | K2 | NOT RUN | None | None |
| 3 | K3 | NOT RUN | None | None |
| 4 | K4 | NOT RUN | None | None |
| 5 | K5 | NOT RUN | None | None |
| 6 | K6 | NOT RUN | None | None |
| 7 | K7 | NOT RUN | None | None |

## Blocker record

Record a blocker only after inspection or an attempted operation establishes it.
Use this short form:

```text
Step:
Observed failure:
Command or source:
Evidence:
Missing prerequisite:
Safe work that can continue:
Exact next action:
```

## Final release record

Record the accepted demonstration checkpoint and original physical session key.
Record the independent review session and trusted test result.
Record the host restart result and operator-control result.
Record backup transport separately from local preservation.
Name unverified platforms, authentication modes, and capabilities.
Use `MVP PASS` only when every required check has evidence.
