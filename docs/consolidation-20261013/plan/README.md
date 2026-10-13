# Jackin branch consolidation — planning bundle

Snapshot: **13 October 2026, Asia/Singapore**. Repository: `https://github.com/jackin-project/jackin`. Target: `main` at `1c3dd1be4a3b1ed518133ec3899c084c792370ff`.

> **Planning status: PARTIAL EVIDENCE / IMPLEMENTATION BLOCKED.** All 79 discovered server branches have dossiers and all 50 distinct tips received some inspection. Complete unique-history/semantic closure, the operator's local inventory, Jackin test execution and independent review are not complete. These are hard execution gates, not silently waived work.

No Jackin source was merged, checked out successfully, pushed or remotely edited during this planning session. This directory is a **planning overlay**, not a clone or a validated integration branch. The local clone failed with DNS resolution; authenticated GitHub read APIs supplied repository evidence.

## Start here

[GOAL.md](GOAL.md) is the execution plan and progress ledger. [GOAL_PROMPT.md](GOAL_PROMPT.md) is the standalone `/goal` prompt. [The document index](goal-references/INDEX.md) links every required reference and all 79 branch dossiers. [Evidence and limitations](goal-references/09-evidence-and-limitations.md) distinguishes observed results from source claims.

The first execution phase closes missing Git evidence and measures baselines. Implementation is not approved until those gates and independent architecture review pass. Do not interpret this bundle as approval to merge the 366-commit policy branch.

## Install and run safely

Place the bundle in a real checkout or a separately accessible planning directory for the coding agent. Inspect existing `GOAL.md`, `GOAL_PROMPT.md` and `goal-references/` before copying: reconcile existing files instead of overwriting uncommitted or unrelated work. Preserve the relative reference layout. Do not initialize this overlay as if it were the upstream Git repository.

Copy the full contents of `GOAL_PROMPT.md` into a compatible `/goal` coding workflow with access to the checkout. The prompt defaults to **no remote writes** and **no live credential/provider/state operations**. It stops at a reviewable integration branch if authorization or required gates are unavailable.

The read-only collector can run before any code edit; see [tools guide](goal-references/tools/README.md). It does not fetch, change branches or run project code. Its synthetic fixture tests are not Jackin tests.

## Delivery contents

Two primary Markdown files, nine required numbered reference files, additional evidence/limitations and index documents, one dossier per exact branch ref, normalized JSON observations/decisions/tasks, a read-only Git collector, fixture tests and an artifact validator. No original repository source code or credentials are bundled.
