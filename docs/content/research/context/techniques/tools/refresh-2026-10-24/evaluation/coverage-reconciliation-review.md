# Coverage reconciliation review and remediation

Reviewer: `/root/rtk_leanctx`. This began as an independent read-only review of the coordinator's reconciliation script and generated inventory. The coordinator then assigned this reviewer the corrective implementation. The final checks below are verification of that authored correction, **not an independent review of the reviewer's own fix**; the coordinator separately read and accepted the strict implementation.

## Original findings

Two defects could make the inventory look complete without preserving the intended section dispositions:

1. The original recursive walk selected every matching record in files named `*disposition*.json`, including material and numerical arrays. Later matches silently replaced canonical heading records. The observed frontier inventory contained only 25 rows attributed to its canonical workstream although its heading ledger contained 66; numerical-line classifications had replaced several authored section dispositions.
2. `choose_heading` accepted substring and approximate matches, then selected `Summary` or the first heading when no match existed. Its final check verified that its own chosen heading existed, which could conceal the absence of an explicit semantic destination.

A direct canonical-array audit found **833 unique authored heading records** at that stage. **170** lacked an exact declared destination: 147 provider entries used descriptive section names, and 23 Headroom/LemonCrow entries omitted a target. These gaps were returned to the authors. They supplied exact reviewed targets, including multiple target headings where a topic split. The Caveman/Ponytail owner also added section-specific reasons to all 26 product-heading records. The final market/literature owner supplied another 53 exact heading records.

## Remediation

The [strict reconciler](../integration/reconcile_coverage.py) reads only **12 explicit ledger/array selectors**. It never traverses material or numerical arrays to claim heading coverage. Baseline ID, document, line, heading, commit and blob hash are checked whenever supplied. Duplicate records are errors even if identical; conflicting records cannot silently win by file order.

Each destination must be an exact current heading. A supplied current anchor must resolve to that declared destination and is preserved when valid. Split destinations remain in `current_headings` and `current_anchors` arrays; the singular columns identify the primary destination for compatibility. No fuzzy, substring, old-heading or Summary fallback remains.

The separate [artifact ledger](../integration/artifact-dispositions.json) explicitly maps the 19 navigation, instrument and historical entries. The dated October23 artifacts were checked byte-for-byte against their baseline hashes; their old execution is not counted as a fresh run. This replaces broad file-pattern coverage rules with reviewable records.

## Executed verification

The [focused verifier](verify_coverage_reconciliation.py) exercised 15 positive/negative guard conditions. Misspellings, descriptive substrings, absent destinations, conflicting IDs/anchors, missing rationale, identical/conflicting duplicates and numerical-only pseudo-coverage all fail as intended. Valid explicit routes and split destinations survive unchanged.

The verifier also parsed the actual Markdown/MDX with the site's installed parser and used its installed `github-slugger`. All **663 heading occurrences across 57 mapped Markdown/MDX documents** matched the reconciler's anchors. The generated inventory contains **905 unique census entries from 12 canonical ledgers**, with zero missing entries, invalid destinations or duplicate IDs. The census spans 71 files, including 56 MDX files.

Two initial verifier attempts failed in the comparison harness: first because standalone Markdown parsing treated YAML frontmatter as a setext heading, then because Python interpreted escapes inside the embedded JavaScript expression. Both failures are retained in their operation/log records. Correcting the harness yielded exit 0. Neither was a failed product test or a missing semantic destination.

Records: [final operation](coverage-integrity-operation.json), [results and hashes](coverage-integrity-results.json), [stdout](logs/coverage-integrity.stdout), [stderr](logs/coverage-integrity.stderr), [first harness attempt](coverage-integrity-operation-initial.json), [second harness attempt](coverage-integrity-operation-second.json).

## What the count means

The result verifies provenance, identity and exact destination integrity for all recovered census entries. It does not mechanically prove that every sentence inside a rewritten section is correct or that every numerical claim has been independently replicated. Detailed material/numerical ledgers and the source, execution and independent-review records provide that separate evidence. No paid calls, network requests or heavy builds ran for these mapping checks.
