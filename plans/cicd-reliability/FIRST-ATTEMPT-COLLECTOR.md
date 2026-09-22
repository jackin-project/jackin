# First-attempt evidence collector

`cargo xtask ci-evidence collect` records post-merge `CI/Main` and `Desktop`
runs for an explicit main-branch commit window. The authoritative denominator
is the durable `ci-push-head-ledger.yml` artifact: one successful ledger run
per `push` to `main`. The collector fetches complete `main` history, binds
each artifact to its Actions run, verifies the raw event payload, before/after
chain, first-parent range, tree identity, commit time, repository, branch,
event, workflow path, and raw-payload digest, then derives one CI/Main and one
Desktop obligation per recorded push head. The capped GitHub `/events` feed and
observed CI/Desktop run heads are never denominator sources. Workflow runs are
resolved by stable Actions workflow ID only; missing/retired IDs remain
unclassified. A record is keyed by `(run_id, attempt)`; repeated scheduled
deliveries update the same key instead of duplicating it, while reruns remain
separate records.

The collector joins runs to expected obligations before the rollup is built:
each expected push head has one CI/Main and one Desktop obligation. A missing
workflow is therefore an explicit `missing` result; it cannot disappear by
being absent from observed runs. If one push contains multiple commits, the
ledger retains every raw `commits[].id` and verifies the complete first-parent
range, but the workflow contract is still one obligation per push head because
GitHub schedules CI/Main and Desktop once per push. Intermediate commit SHAs
are evidence inside that push record, not extra workflow obligations. A
workflow run whose head is outside the durable ledger is retained as an
explicitly unclassified `outside_denominator` record; it never expands the
denominator. Base SHA, tree SHA, commit time, workflow path and ID, runtime
revision, contract digest, event, run/attempt, timestamps, jobs, provenance,
and run/job evidence URLs are retained.

`cargo xtask ci-evidence rollup` emits JSON and Markdown with separate cohort
counts, timing counts, unclassified workflow runs, plus an end-to-end status per
expected push head. Reruns never replace a first attempt. Cancellation,
infrastructure conclusions, product failures, missing obligations, skipped
required work, partial jobs, unknown workflow identities,
outside-denominator runs, contaminated event/branch provenance, and data-quality
conflicts remain visible. The rollup recomputes classifications from raw
status/conclusion/job evidence and requires sticky, structured raw observations
for terminal conflicts. It exits nonzero after writing JSON/Markdown whenever
the denominator is not a proven push-head ledger, ledger coverage/artifact
provenance is incomplete, the fetch proof is incomplete, any required
obligation is non-green, or any run is unclassified. Missing ledger workflow
discovery, missing ledger runs, expired/missing artifacts, failed ledger runs,
chain gaps, feature/manual contamination, and malformed raw payloads are hard
failures; the collector never falls back to first-parent history or observed
run heads. It always sets `six_nines_claimed = false`; observed sample size is
evidence, not a statistical claim.

The generated `ci-push-head-ledger.yml` runs on every `push` to `main` with
`cancel-in-progress: false` and uploads exactly `push-head.json` plus the raw
`event.json` under `target/ci-push-head-ledger/`. The Velnor
`scheduled-checks` primitive always adds `workflow_dispatch`; the task rejects
manual dispatches because they are not push evidence, and no manual artifact
is accepted by the collector. This intentional push-only behavior is
documented rather than treated as a denominator event. The generated
`ci-evidence.yml` scheduled check runs the collector with an explicit
`GH_TOKEN`, then selects only a completed scheduled `main` run of the exact
`ci-evidence.yml` workflow and validates its schema, repository, branch,
window, runtime identity, denominator source, and collection provenance before
restoring its artifact. Feature, manual, wrong-workflow, stale-runtime, and
malformed artifacts are rejected. Collection merges `(run_id, attempt)` records
without rewriting terminal verdicts, preserves every raw observation, and
prunes attempts outside the new denominator window. Missing artifacts and
API/rollup failures are red. Six-nines remains unclaimed until an independent
archival sink and sufficient sample are available.

Schema 4 is a hard migration boundary. A restored artifact with any other
schema (including schema 3) is discarded before deserialization; a schema-4
artifact missing hardened provenance, tree, raw-observation, or push-head
fields fails closed during deserialization/validation. Collection starts a
fresh schema 4 ledger only for pre-schema-4 input; it never silently downgrades
the durable denominator. Old rows are never merged into the hardened ledger
and cannot poison the new rollup.

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
skipped required work, missing obligations, push-head denominator cardinality
and chain gaps, missing tree/runtime/remote provenance, scheduled/manual
provenance, tampered classifications, sticky terminal-conflict evidence with
raw snapshots, and aggregation. No fixture is treated as representative
production reliability evidence.
