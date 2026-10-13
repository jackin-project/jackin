# First-attempt evidence collector

`cargo xtask ci-evidence collect` records post-merge `CI/Main` and `Desktop`
runs for an explicit main-branch commit window. The authoritative denominator
is the checked-out remote branch's bounded `git log --first-parent` result after
`git fetch --shallow-since`; the artifact stores the branch, window, fetch
result, commit/tree/parent identities, and commit count as proof. The capped
GitHub `/events` feed is not used as a denominator. Workflow runs are resolved
by stable Actions workflow ID only; missing/retired IDs remain unclassified.
A record is keyed by
`(run_id, attempt)`; repeated scheduled deliveries update the same key instead
of duplicating it, while reruns remain separate records.

The collector joins runs to expected obligations before the rollup is built:
each expected main commit has one CI/Main and one Desktop obligation. A missing
workflow is therefore an explicit `missing` result. A workflow run whose head
is outside the bounded history is retained as an explicitly unclassified
`outside_denominator` record; it never expands the denominator. Base SHA, tree
SHA, commit time, workflow path and ID, runtime revision, contract digest,
event, run/attempt, timestamps, jobs, provenance, and run/job evidence URLs are
retained.

`cargo xtask ci-evidence rollup` emits JSON and Markdown with separate cohort
counts, timing counts, unclassified workflow runs, plus an end-to-end status per
expected main commit. Reruns never replace a first attempt. Cancellation, infrastructure
conclusions, product failures, missing obligations, skipped required work,
partial jobs, unknown workflow identities, outside-denominator runs, and
data-quality conflicts remain visible.
The rollup recomputes classifications from raw status/conclusion/job evidence
and requires structured raw observations for terminal conflicts. It exits
nonzero after writing JSON/Markdown whenever the denominator proof is not
first-parent history, the fetch proof is incomplete, any required obligation is
non-green, or any run is unclassified. It always sets `six_nines_claimed =
false`; observed sample size is evidence, not a statistical claim.

The generated `ci-evidence.yml` scheduled check runs this task with an explicit
`GH_TOKEN`, `actions: read`, and `contents: read`, then uploads the three files
under `target/ci-evidence/` as one required artifact. On GitHub-hosted runs the
Mise task downloads the latest completed `ci-evidence` artifact before
collection when no local ledger exists, so scheduled rolling append is
operational. Collection merges `(run_id, attempt)` records without rewriting
terminal verdicts and prunes attempts outside the new history window. Missing
artifacts and API/rollup failures are red. Six-nines remains unclaimed until an
independent archival sink and sufficient sample are available.

Schema 4 is a hard migration boundary. A restored artifact with any other schema
(including schema 3) is discarded before deserialization; collection starts a
fresh schema 4 ledger and derives the current denominator from first-parent
history. Old rows are never merged into the hardened ledger and cannot poison
the new rollup.

Local replay:

```sh
cargo test -p jackin-xtask ci_evidence
cargo xtask ci-evidence rollup \
  --input target/ci-evidence/attempts.json \
  --json target/ci-evidence/rollup.json \
  --markdown target/ci-evidence/rollup.md
```

The test fixture surface covers pagination, zero attempt numbers, stable-ID
classification, reruns, duplicate deliveries, missing jobs, cancellations,
skipped required work, missing obligations, first-parent derivation, tampered
provenance/classifications, terminal-conflict evidence, and aggregation. No
fixture is treated as representative production reliability evidence.
