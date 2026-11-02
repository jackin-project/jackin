# Jackin OpenShell rewrite program

This directory holds the program that rebuilds jackin around NVIDIA OpenShell. It replaces the earlier `docs/openshell-v2/` documents.

## Read these files in this order

| File | Mode | Content |
|---|---|---|
| [DESIGN.md](DESIGN.md) | Reference | The product statement, decisions D01 to D28, the CLI surface, and the launch sequence. |
| [PLAN.md](PLAN.md) | How-to | The autonomy rules, the orchestrator protocol, the gates, the tasks, and the sentinel journey. |
| [GOAL.md](GOAL.md) | How-to | The `/goal` command that starts the program. |
| [decisions.tsv](decisions.tsv) | Record | One row for each decision and each finished task. |
| [research/](research/) | Evidence | The analysis reports from 2026-11-01. If a report disagrees with DESIGN.md, DESIGN.md is correct. |
| [evidence/](evidence/) | Evidence | The raw output of live tests. |
| [blockers/](blockers/) | Record | One report for each missing OpenShell capability. |
| `API.md` | Reference | The crate APIs from task P2.1. |
| `drafts/` | Input | The role Dockerfile, the Docker lane drafts, the gateway how-to, and the Codex `AgentSpec` values from P0 and P7. The construct and derived Dockerfiles, `jackin-start`, and the Codex provider profile live in `crates/jackin-image/images/` and `crates/jackin-core/src/assets/`. |
| `probe-schema.json` | Reference | The output contract of `jackin-sentinel-probe` from task P7.2. |

## Start the program

Follow [GOAL.md](GOAL.md).

## Rules for this directory

- Only the orchestrator edits DESIGN.md, PLAN.md, and decisions.tsv.
- Subagents write their evidence and blocker files under the names that PLAN.md gives.
- Never put a secret in this directory.
