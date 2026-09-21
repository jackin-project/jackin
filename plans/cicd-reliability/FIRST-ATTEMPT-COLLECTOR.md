# First-attempt evidence collector

`cargo xtask ci-evidence collect` records post-merge `CI/Main` and `Desktop`
runs for an explicit first-parent main-commit window. It fetches every run page,
then fetches every run attempt and every jobs page. A record is keyed by
`(run_id, attempt)`; repeated scheduled deliveries update the same key instead
of duplicating it, while reruns remain separate records.

The collector joins runs to expected obligations before the rollup is built:
each expected main commit has one CI/Main and one Desktop obligation. A missing
workflow is therefore an explicit `missing` result. The expected set is derived
from first-parent history after a shallow-since fetch, or supplied through a
fixture for replay and audit. Base SHA, tree SHA, commit time, workflow path and
ID, runtime revision, contract digest, event, run/attempt, timestamps, jobs,
and run/job evidence URLs are retained.

`cargo xtask ci-evidence rollup` emits JSON and Markdown with separate cohort
counts plus an end-to-end status per commit. Reruns never replace a first
attempt. Cancellation, infrastructure conclusions, product failures, missing
obligations, inapplicability, and data-quality conflicts remain visible. The
rollup always sets `six_nines_claimed = false`; observed sample size is evidence,
not a statistical claim.

The generated `ci-evidence.yml` scheduled check runs this task and uploads the
three files under `target/ci-evidence/` as one artifact. The artifact is the
immutable scheduled snapshot; rerunning the task against an existing output
file appends new `(run_id, attempt)` records without rewriting terminal
verdicts. A subsequent repository-side archival sink can consume these JSON
snapshots without changing the collector's denominator or semantics.

Local replay:

```sh
cargo test -p jackin-xtask ci_evidence
cargo xtask ci-evidence rollup \
  --input fixture/attempts.json \
  --json target/ci-evidence/rollup.json \
  --markdown target/ci-evidence/rollup.md
```

The test fixture surface covers paginated pages, reruns, duplicate deliveries,
workflow-path renames, missing jobs, cancellations, missing obligations, and
aggregation. No fixture is treated as representative production reliability
evidence.
