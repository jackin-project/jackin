# 10 — Artifact integrity and planning verification

## Delivered structure

The bundle contains **95 Markdown documents**: `GOAL.md`, `GOAL_PROMPT.md`, a bundle README, the nine required numbered references 00–08, two additional evidence/integrity references 09–10, the complete index, a tools guide and **79 exact-ref branch dossiers**. Normalized evidence contains **79 server refs / 50 distinct tips**, **50 tip observations**, **33 proposed seed decisions** and **20 executable task specifications**.

This is a planning overlay, not a clone or an integration branch. No Jackin source was merged, pushed or remotely edited. Full source-history coverage, local inventory, Jackin baselines/platform gates and independent review remain incomplete. The first execution gates explicitly block source implementation until those gaps are closed.

## Actual structural checks

The [artifact validator report](evidence/artifact-validation.json) records a **PASSED** structural check with no errors. It verifies required files; valid/unique branch IDs, exact refs and full tip formats; one matching dossier per ref; observation coverage; decision/source/task consistency; required task fields; dependency order and acyclicity; all nine GOAL sections; all 20 full task cards in both authoritative task views; `/goal` prompt prefix; and internal Markdown links and anchors.

The final report gives the exact link count. The initial complete pass checked 1,874 internal links; later integrity-report links are included in the final rerun. External links were not re-fetched by the validator, and commit-ID format checks do not independently confirm remote object contents.

Three controlled negative tests also passed: a missing relative file link, a task dependency cycle and a mismatched decision source SHA were each rejected. See [negative-test results](evidence/validator-negative-tests.json). These are synthetic copies, not modifications to the delivered source records.

All three bundled Python scripts parsed successfully with Python's syntax parser. The [collector fixture run](evidence/collector-test-results.txt) executed **eight tests with eight passes**. It covers aliases/patch-equivalent commits and preservation of untracked work, refusal to overwrite output, repository/common-Git path safety, renames, multi-parent trees, changed-ref detection and shallow-history rejection. These tests exercise the supplied tooling only—not Jackin's implementation, performance, security or release gates.

## Manual coordinator self-check

The coordinator sampled the main/usage/recovery dossiers and cross-checked the task matrix, path corrections, command families, authorization boundaries and full prompt. Known corrections and remaining review gates are in [08-review-and-decision-log.md](08-review-and-decision-log.md). No runnable scanner-bypass command or live credential/cutover instruction was included. There are no empty placeholder tasks. Unknown destination commits and unexamined history are explicitly marked rather than invented.

This self-check is **not an independent plan review**. No subagent was available, no independent reviewer approved this bundle, and none of the 20 execution tasks is claimed complete. The structural validator cannot establish that every meaningful source change has been found; all 79 remainder rows remain open until actual full-history review.

## Delivery integrity and use

The package includes a SHA-256 manifest covering every delivered file except the manifest itself. Verify it from the extracted bundle root with `sha256sum -c SHA256SUMS.txt` where that utility is available. Checksums detect byte changes; they are not a signature or source-provenance guarantee.

Preserve directory layout and follow [README.md](../README.md) before placing the overlay in an actual checkout. Reconcile existing files rather than overwriting them. Copy the full contents of [GOAL_PROMPT.md](../GOAL_PROMPT.md) into the execution workflow. The initial investigation gates, independent reviews, exact-head tests and separately authorized target integration remain mandatory.
