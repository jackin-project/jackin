# Independent review of Headroom and LemonCrow runtime claims

Reviewer: `/root/rtk_leanctx`, independently of the product-page author. Frozen source cutoff: **2026-10-24T01:15:00Z**. This review and its local probes ran later on October 24 against the frozen product commits. The companion [review record](runtime-independent-review.json) pins the exact product-page bytes reviewed; subsequent edits are outside that byte-level record.

## Verdict and scope

The reviewed runtime findings support the current restricted recommendations: qualify selected Headroom transforms behind a caller-owned contract, and evaluate LemonCrow Runtime Lite as an isolated retrieval sidecar. They do not establish a current complete-task winner, justify full tool replacement, or demonstrate model understanding of compacted observations.

The review read both complete dossiers, independently inspected the high-risk implementation paths, and verified **14 source files against immutable Git objects**. It then executed small, free component probes and independently compared all **87 packaged LemonCrow client files** with immutable source bytes. It did not rerun the other author's Rust build, selected upstream test suites, native-host sessions or task benchmarks. Those results remain attributed to their original execution records.

## Headroom 0.41.0

Source: `76133cff59b71284399bf760c5578dc373c97231`. The installed Python distribution reports 0.41.0, and its executed `compress.py` bytes match that commit.

| Claim reviewed | Independent finding | Consequence |
|---|---|---|
| Python and Rust Kompress have different line handling | Source inspection confirms Python's line-aware reconstruction and Rust's `split_whitespace()` / `join(" ")` path. No learned inference was executed. | The Python issue closure cannot qualify Rust reconstruction. The no-ML build does not exercise this optional path. |
| JSON “lossless” verification has a narrower meaning than exact evidence preservation | Direct calls to the installed Python verifier accept duplicate-key collapse to the last value. A float-to-integer change inside a JSON string is rejected, providing a positive control for its strict JSON comparison. Rust's parsed-object implementation has the same duplicate-key domain concern by source inspection. | Require a unique-key typed JSON contract or a byte-preserving alternative; retain originals. |
| Non-string metadata uses a weaker equality check | A new direct verifier case accepts `False` changed to `0` in a scalar metadata field because Python equality treats them as equal. **No actual compression pipeline changing that metadata was demonstrated.** | Optional strict-typing hardening of the verifier. This is not a reproduced deployed-path mutation or an accepted-task failure. The product author accepted this bounded note. |
| Rust CCR cannot acknowledge durable writes through its current `put` signature | The trait returns unit and the SQLite implementation logs write failure. The other author's missing-table probe is consistent with this source contract. | A consumer needs acknowledged raw storage before omission. This review did not reproduce disk-full behavior or rerun the Rust fault probe. |
| No-ML and package defaults are different boundaries | The manifest retains native/storage/tokenizer dependencies, and Python beacon code has its own default and switch. | No-ML is not a universal offline or zero-side-effect promise. The 185-node dependency count and completed build remain the original author's results. |

Pinned sources: [Python verification](https://github.com/headroomlabs-ai/headroom/blob/76133cff59b71284399bf760c5578dc373c97231/headroom/compress.py), [Python Kompress](https://github.com/headroomlabs-ai/headroom/blob/76133cff59b71284399bf760c5578dc373c97231/headroom/transforms/kompress_compressor.py), [Rust Kompress](https://github.com/headroomlabs-ai/headroom/blob/76133cff59b71284399bf760c5578dc373c97231/crates/headroom-core/src/transforms/kompress.rs), [JSON minifier](https://github.com/headroomlabs-ai/headroom/blob/76133cff59b71284399bf760c5578dc373c97231/crates/headroom-core/src/transforms/pipeline/reformats/json_minifier.rs), [CCR trait](https://github.com/headroomlabs-ai/headroom/blob/76133cff59b71284399bf760c5578dc373c97231/crates/headroom-core/src/ccr/mod.rs), [SQLite backend](https://github.com/headroomlabs-ai/headroom/blob/76133cff59b71284399bf760c5578dc373c97231/crates/headroom-core/src/ccr/backends/sqlite.rs).

## LemonCrow Runtime Lite 0.8.0

Source: `d96bad0e59dbc799d54ac5ca1a6a4fe9b8a8ab8d`. The release archive SHA-256 matches the frozen product record. Its embedded wheel is `lemoncrow_runtime_client-0.8.0-py3-none-any.whl`; all **87** `lemoncrow_client` files match immutable Git objects. Wheel metadata declares `lemoncrow-runtime-client`, version 0.8.0, Python `>=3.12,<3.14` and no dependency declarations. This comparison read archive members in memory; it did not install the wheel or claim reproducible construction of the archive.

| Claim reviewed | Independent finding | Consequence |
|---|---|---|
| Current Lite differs from the historical fuller engine | Public-boundary and Lite dispatcher/index source confirm distinct treatments. | Old fuller-engine/persona task results cannot qualify current Lite. |
| Lite worktree selection is explicit per call | The inspected call passes `active=None`; routing and per-checkout stores differ from historical sticky cwd. | Explicit roots remain necessary. This source review does not add a second concurrency or host test. |
| Shell history does not encode model context epochs | `RunHistory` remembers observations per session/process, with no current model-context epoch in that contract. | A process remembering a result does not prove the model retains it after compaction or resume. |
| Failed spill storage can coexist with an `unchanged` marker | An independent direct-call probe used a regular file where a spill directory was required **from the first call**. No original spill was created. The second identical synthetic result still became an `unchanged` marker. | This strengthens the failure example without relying on an earlier successfully stored original. Output replacement needs storage acknowledgement and a recovery contract. |
| Narrow controls avoid that marker | A fresh `RunHistory` and `LEMONCROW_BASH_UNCHANGED_DELTA=0` each avoided the marker. | Useful controls; they do not prove every other compactor preserves complete output. |

The failed-path probe used a 1,728-character synthetic string. The first presentation compressed repeated lines to 86 characters; the second became the recorded marker. It directly called presentation/history code: **no shell producer, native host, model, permission flow or disk-full fault was exercised**. The finding concerns the local output contract and does not block a separately enforced retrieval-only arm.

Pinned sources: [Lite dispatcher](https://github.com/lemoncrow-lab/lemoncrow/blob/d96bad0e59dbc799d54ac5ca1a6a4fe9b8a8ab8d/client/src/lemoncrow_client/lite/mcp.py), [worktrees](https://github.com/lemoncrow-lab/lemoncrow/blob/d96bad0e59dbc799d54ac5ca1a6a4fe9b8a8ab8d/client/src/lemoncrow_client/worktrees.py), [shell presentation and spills](https://github.com/lemoncrow-lab/lemoncrow/blob/d96bad0e59dbc799d54ac5ca1a6a4fe9b8a8ab8d/client/src/lemoncrow_client/localtools/shell.py), [history](https://github.com/lemoncrow-lab/lemoncrow/blob/d96bad0e59dbc799d54ac5ca1a6a4fe9b8a8ab8d/client/src/lemoncrow_client/kit/output_delta.py), [public source boundary](https://github.com/lemoncrow-lab/lemoncrow/blob/d96bad0e59dbc799d54ac5ca1a6a4fe9b8a8ab8d/docs/reference/open-source-boundary.md).

## Reproduction records

- [Runtime probe](review_runtime_boundaries.py), [exact operation](runtime-independent-review-operation.json), [results and source hashes](runtime-independent-review-results.json), [stdout](logs/runtime-independent-review.stdout), [stderr](logs/runtime-independent-review.stderr).
- [Package comparison](review_lemon_package.py), [exact operation](lemon-package-independent-operation.json), [results and all file hashes](lemon-package-independent-results.json), [stdout](logs/lemon-package-independent.stdout), [stderr](logs/lemon-package-independent.stderr).
- The original product execution record is in [the sibling evidence pack](../headroom-lemoncrow/README.md). Historical benchmark arithmetic and its limitations remain in this pack's [README](README.md).

Both recorded operations exited 0. No paid calls, model inference, Rust builds, global installation or product activation were performed in this review. No new blocker was found for the dossiers' already-restricted proposals; the scalar-type guard is a bounded additional hardening item.
