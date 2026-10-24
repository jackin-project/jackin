# Independent review: RTK, LeanCTX and usage accounting

Reviewer: `/root/caveman_ponytail`. Author reviewed: `/root/rtk_leanctx`. Date: 2026-10-24. The reviewer did not author the product dossiers, accounting implementation or their original probes.

## Scope

Read the complete current RTK and LeanCTX dossiers, artifact manifests, relevant raw probe results, `provider_usage.py`, `session_cost.py` and the four explicit accounting fixtures. Challenged the principal boundary claims against pinned source and current official usage contracts. Four bounded RTK subprocess fixtures and the four explicit accounting calculations were executed independently. No builds, model calls, subscription access or native agent sessions ran.

## Identity verification

The RTK OSS release archive SHA-256 is `a44e8ffc43981100b4ddf1710399a1649a6c874591aa6d0c0727fe6cbd983c41`. The extracted binary SHA-256 is `38b915f2a52ce4b7ef7d6dc3966aaeccb9f754731b2933571bf85026bcba8677` and matches the archive member; it reports `rtk 1.0.1`. The archive and executable hashes identify different byte objects. The product record's table heading should name the archive explicitly.

The reviewed RTK `pipe_cmd.rs` and engine license, and LeanCTX `engine.rs` and `conversation.rs`, match the recorded pinned-source hashes. See `identity-crosscheck.json` for exact paths, commits and values. This verifies the inspected bytes against the curated pinned register; it is not a complete independent source-to-binary reproducibility proof.

## Findings and dispositions

### RTK boundary claims corroborated

Independent fixture input `operation failed: no permission\r\nlast` produces:

| Surface | Result |
| --- | --- |
| `rtk diff -` | Input plus a terminating newline, exit 0 |
| `rtk pipe --filter git-diff` | Empty stdout, exit 0 |
| Generic `pipe --passthrough` with a separate CRLF/NUL/invalid-UTF-8 fixture | Exact byte preservation, exit 0 |

These support the dossier's distinction between structural diff fallback and the Git compacting filter. An exact diff/review consumer still needs the captured original. The filtered command's status is not the producer's status.

### Additional mixed-grep omission

The independent fixture contains 60 ordinary `file:line:match` rows followed by `SCAN INCOMPLETE: protected directory unreadable`. The pinned OSS grep filter returns 311 bytes from 3,630 input bytes, exits 0, and omits that unstructured diagnostic. The source's grouping loop ignores nonmatching lines when at least one valid match is found; the bounded output also selects a subset of matches.

This is a synthetic mixed-format fixture, not evidence that every real grep invocation emits this exact text or that a host merges streams. It strengthens the existing requirement to retain original bytes, keep producer stdout/stderr/status distinct, validate the admitted format and qualify each filter family. It does not overturn the proposed capture-first boundary. The author was notified and asked to add a narrowly scoped statement.

### RTK licensing split corroborated

The pinned engine license's Additional Use Grant permits specified production use but excludes offering the work/derivatives as paid or hosted third-party products/services under that grant. The product correctly separates that engine/default binary from the OSS command-filter artifact. No broader legal permission or source-to-binary license audit is inferred from the root package metadata.

### LeanCTX source contracts corroborated

The pinned embedding facade calls a static `Once` initializer that assigns all four process-wide LeanCTX paths, with `temp_dir()/lean-ctx-embed` as its default. It sets environment variables before starting its own runtime. This does not prove an embedding application's other threads are absent; the dossier already retains that prerequisite and does not claim a reproduced race.

The conversation gate explicitly withholds stubs for unprovable/process-derived identity, allows legacy warm reuse for an unknown caller, and requires a matching known identity for cold persisted reuse. This supports the dossier's central distinction between cached bytes and a recipient's current context. This review did not execute two simultaneous embedded engines or a live host reset.

### Usage accounting corroborated

Checked the implementation against the official contracts:

- [OpenAI cache accounting](https://developers.openai.com/api/docs/guides/prompt-caching): billed read/write counts are disjoint; lifetime parts sum to total writes; ordinary input subtracts both classes.
- [Claude threshold compaction](https://platform.claude.com/docs/en/build-with-claude/compaction-threshold) and [on-demand compaction](https://platform.claude.com/docs/en/build-with-claude/compaction-on-demand): sum final `usage.iterations` once; top-level input/output omit compaction.
- [Claude thinking](https://platform.claude.com/docs/en/build-with-claude/adaptive-thinking): the thinking breakdown is a subset of billed output and only appears at the final streaming delta.
- [Gemini thinking](https://ai.google.dev/gemini-api/docs/thinking): output and thought tokens are disjoint generation classes; the documented 62 + 171 + 297 example totals 530.
- [Gemini caching](https://ai.google.dev/gemini-api/docs/caching): cached input is reported separately within input accounting.

Independent Decimal arithmetic agrees with all four fixture partitions and prices: OpenAI paid-write example **$0.0379**, Claude compaction **$0.0009**, Gemini disjoint generation **$0.004804**, and **unknown** for Claude writes with unspecified lifetime. The code preserves unknown reasoning splits and refuses incomplete/contradictory counters rather than silently pricing them as zero.

No implementation blocker was found within these inspected paths and fixtures. Exact rate selection still depends on caller-supplied model, date, tier, route and prompt threshold. These results are not invoices, quota conversions or a whole-session completeness proof.

## Artifacts and execution

`rtk-independent-results.json` records argv, cwd, selected environment overrides, exit status, input/output hashes and excerpts. `independent_rtk_review.py` generates every input exactly. `accounting-review-results.json` records implementation/fixture hashes and each independent calculation. `review_accounting.py` is the executed review calculation. `execution.json` records the two review commands and scope.

Paths in the review scripts reflect the exact research workspace and can be adjusted for a new scratch checkout. Product binaries and dependencies are deliberately omitted from this bundle. Timing fields are bookkeeping, not randomized performance measurements. No broad test suite rerun was necessary.
