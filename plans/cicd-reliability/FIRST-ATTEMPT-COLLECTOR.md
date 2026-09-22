# First-attempt evidence collector

`cargo xtask ci-evidence collect` records post-merge `CI/Main` and `Desktop`
runs for an explicit main-push window. It derives obligations from GitHub
`PushEvent` heads (not every commit in a multi-commit push), resolves stable
workflow IDs before collecting, then fetches every run page, every run attempt,
and every jobs page. A record is keyed by
`(run_id, attempt)`; repeated scheduled deliveries update the same key instead
of duplicating it, while reruns remain separate records.

The collector joins runs to expected obligations before the rollup is built:
each expected main head has one CI/Main and one Desktop obligation. A missing
workflow is therefore an explicit `missing` result. The denominator starts with
GitHub `PushEvent` heads, then adds every observed main-branch cohort-run head
that the event feed omitted. Such fallback heads carry
`observed_run_fallback` provenance and an explicit event-source gap listing the
run IDs and cohorts that reconstructed both obligations; they remain usable for
missing/failure accounting but block a qualified green claim. Base SHA, tree
SHA, commit time, workflow path and ID, runtime revision, contract digest,
event, run/attempt, timestamps, jobs, provenance, and run/job evidence URLs are
retained.

`cargo xtask ci-evidence rollup` emits JSON and Markdown with separate cohort
counts, timing counts, unclassified workflow runs, plus an end-to-end status per
push head. Reruns never replace a first attempt. Cancellation, infrastructure
conclusions, product failures, missing obligations, inapplicability, partial
jobs, unknown workflow identities, and data-quality conflicts remain visible.
The rollup recomputes classifications from raw status/conclusion/job evidence
and always sets `six_nines_claimed = false`; observed sample size is evidence,
not a statistical claim.

The generated `ci-evidence.yml` scheduled check runs this task with an explicit
`GH_TOKEN`, `actions: read`, and `contents: read`, then uploads the three files
under `target/ci-evidence/` as one required artifact. The collector recomputes
the full configured window on each run; rerunning against an existing output
file appends new `(run_id, attempt)` records without rewriting terminal
verdicts and prunes attempts outside the new expected window. Missing artifacts and API/rollup failures are red. Artifact retention
is the current durable snapshot boundary; six-nines remains unclaimed until an
independent archival sink and sufficient sample are available.

Local replay:

```sh
cargo test -p jackin-xtask ci_evidence
cargo xtask ci-evidence rollup \
  --input target/ci-evidence/attempts.json \
  --json target/ci-evidence/rollup.json \
  --markdown target/ci-evidence/rollup.md
```

The test fixture surface covers paginated pages, reruns, duplicate deliveries,
workflow-path renames, missing jobs, cancellations, missing obligations, and
aggregation. No fixture is treated as representative production reliability
evidence.
