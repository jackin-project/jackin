# Planning tools

These tools assist evidence capture and bundle consistency. They do not run Jackin tests, perform semantic review, fetch remote data, merge branches or publish anything. Requirements: Python 3.10+ and Git for the collector/tests. Standard library only; no dependency installation is needed.

## Read-only Git collector

Inspect the script before running it in a real checkout. Preserve existing user state. Its output must be a **new external directory**, outside the worktree, private Git directory and common Git directory; an existing output is refused.

```sh
python3 goal-references/tools/collect_git_evidence.py \
  --repo . \
  --target 1c3dd1be4a3b1ed518133ec3899c084c792370ff \
  --output ../jackin-evidence-20261013 \
  --workers 4 --pairwise --patch-ids
```

Use a new output name for every attempt. Revalidate M and all source refs first; do not silently substitute a moving branch name. The script does not fetch: capture current refs before any fetch, inspect configured remotes/refspecs, and fetch approved read-only evidence without pruning or force-updating user refs. Use fresh audit tracking refs when remote histories diverge.

The collector records refs/upstreams/symbolic refs, local/tracking branch tips, reachable commit parents/trees/subjects, all-parent tip differences, direct target-to-tip changed paths/renames, tip tree manifests, graph relationships and optional all-pair relationships. Multi-parent tip trees are retained without assuming every multi-parent commit is a stash. Internal multi-parent commits remain a manual review obligation in `commit_metadata`.

`--patch-ids` pipes all reachable non-merge patches to `git patch-id --stable`; it retains hashes and commit IDs, not full source patch bytes. Equal patch IDs do not prove semantic equivalence, squash preservation, correct merge resolution or test coverage. `--pairwise` computes graph relationships only, not pairwise function-level review. The 1–10 workers are bounded local Git read threads, not AI subagents.

Output includes `inventory.json` and one `tips/<full-sha>.json` per distinct tip. Snapshot instability and shallow history produce errors and exit code 2. Git/object access failures also return 2; a partial output may remain and must not be reported complete or overwritten. Exit 0 means evidence collection completed under these checks, **not** that every branch is reviewed or safe to merge. No project source/build script is executed. Missing objects must be resolved through an approved read workflow before retrying.

The collector disables optional Git locks, hooks, filesystem-monitor execution, replacement objects and lazy fetch; avoids external diff/textconv; and strips HTTP credentials/query strings from recorded remote URLs. It still records repository paths, filenames, branch names and commit subjects, which can be sensitive. Review/redact reports before sharing them. A real working tree can change while another process runs; no collector can promise an atomic snapshot of concurrent external edits, so stable postchecks and a frozen execution workspace remain required.

## Synthetic tests

```sh
python3 goal-references/tools/test_collector.py
```

Creates and removes disposable temporary Git fixtures only. It makes fixture commits/refs and one linked worktree to exercise the collector; it does **not** consolidate Jackin. Actual planning run: eight tests passed. Full output is [collector-test-results.txt](../evidence/collector-test-results.txt).

## Artifact validator

```sh
python3 goal-references/tools/validate_bundle.py --root .
```

Checks required documents, relative links/anchors, unique exact-ref/dossier mappings, valid source identifiers, decision/task references, dependency ordering and required task fields. It prints JSON to standard output and exits nonzero on errors. It does not write to the source repository or independently validate GitHub evidence. To retain a report, direct output into an explicitly owned external file; the delivered [artifact-validation.json](../evidence/artifact-validation.json) records the planning run.

The inventory may grow during execution; the validator checks its consistency rather than imposing 79 as a permanent cap. Update normalized JSON and Markdown together. The validator is not a source-code, test-quality or authorization checker. Passing it does not close OPEN semantic remainders or independent review requirements.
