# Final local verification record

Source cutoff: **2026-10-24T01:15:00Z**. Commands ran on October 24 UTC. This record preserves actual command outcomes, failures, input identities and coverage limits. It does not certify a future publication commit or remote CI.

## Publication serialization exception

Four JSON evidence files received a **whitespace-only publication representation** after independently verified secret-scan false positives on document digests. Their decoded keys and values, review dates, execution timestamps and source-file hashes are unchanged. The [formatting record](publication-json-formatting.json) identifies all four original and published identities; the [read-only verifier](verify_publication_serialization.py) reconstructs the exact original bytes. This is a publication-format change, not a new execution or review.

Within the 51-entry archive described here, **50 records remain byte-identical to their original files**. The sole exception is [logs/render-input-manifest.json](logs/render-input-manifest.json): its original execution representation was 7,954 bytes with SHA256 `bf1fb18b6aeaff69adc7c7c73ebe1779307713d9d8cf87e074d661d8ec33a2f0`; its published representation is 8,534 bytes with SHA256 `9be3dacd76bad6bea648b3c6cd4e6c282f3ba3f54d74422c36d17f47207cf290`. Standard two-space JSON serialization with `ensure_ascii=False` and a final newline exactly reconstructs the original bytes. All command logs remain byte-identical.

The original assembly check at **08:53:31 UTC**, earlier input-manifest hashes and historical review/integrity observations retain their original time and scope. The machine-readable record distinguishes the original source digest from the current published-file digest and records the later read-only correspondence check. No scanner rule, baseline, ignore setting, evidence value or source document was changed by this formatting operation.

## Latest completed documentation gate

After integrating main `e737f4b5eb7c1a0d15ba6595fc260407ad178452`, the latest seven-command gate ran **08:46:41–08:47:57 UTC** from the repository's `docs` directory. Every command exited 0:

| Command | Result |
|---|---|
| `bun install --frozen-lockfile` | Passed; 491 installs across 609 packages checked with no changes |
| `bun run build` | Passed; 1,659 static routes generated |
| `mbx +1.99.1 xtask docs repo-links` | Passed |
| `mbx +1.99.1 xtask roadmap audit` | Passed; 18 sidebar metadata files resolve |
| `mbx +1.99.1 xtask research check` | Passed; 63 sidebar metadata files resolve |
| `bunx tsc --noEmit` | Passed |
| `bun test` | 19 passed, 0 failed; 32 expectations across 2 files |

The exact argv, cwd, timestamps, durations and exits are preserved in [the original final result record](logs/docs-final-main-results.json). [verification.json](verification.json) maps each command to its byte-identical raw log and SHA256.

The [frozen input manifest](logs/render-input-manifest.json) identifies **56 context MDX pages plus source.config.ts and tsconfig.json**. All 58 hashes still matched after the late main integration. It is a targeted build/render input record, not a complete environment or dependency manifest. Earlier runs without contemporaneous input hashes are labeled accordingly.

## Failures and repairs remain visible

| Stage | Actual outcome |
|---|---|
| Initial documentation gate, 08:15 UTC | 4 failures: build, repository links, research routes and TypeScript; 3 commands passed |
| Intermediate build/type/test rerun, 08:22–08:23 UTC | 3 commands passed; this was not the full gate |
| First complete final gate, 08:30–08:31 UTC | 6 commands passed; repository links still failed on 3 references |
| Corrected gate for the first integration, 08:33–08:34 UTC | All 7 commands passed |
| Gate after later main integration, 08:46–08:47 UTC | All 7 commands passed |

The initial build treated research JSON records as sidebar metadata. Metadata discovery now accepts `**/meta.json`. The TypeScript check initially combined independent before/after fixture programs and reported duplicate variables; the site compiler now excludes the immutable fixture directories. The fixture bytes themselves were not changed. Six stale routes, missing in-progress artifact references and three filename-link references were corrected. Original failed logs are retained alongside later successes.

Optional mise tool installation attempts also failed, including existing-install collisions and Zig archive extraction. The first checkpoint hook reported staged-source changes requiring review. These are retained as failed attempts, not converted into passes by a later retry.

## Offline research reruns

The [offline command record](logs/offline-final-results.json) records:

- **17 accounting regression tests passed.**
- **11 curated numeric aggregation sections matched** their recorded expected arithmetic.
- **14 representation checks passed.** The regenerated `results.json` and all **35 fixture files** are byte-identical to the checked-in versions.

These commands made no provider requests and did not rerun paid historical agent experiments. Representation counts use Python 3.12.14, tiktoken 0.12.0 and identified proxy encodings with socket connections disabled. The correspondence check was a later read-only byte/hash comparison of already-generated outputs. Current code/data hashes are separately labeled assembly-time captures rather than invented historical input manifests.

## Hooks: process success and scan coverage differ

The local main-merge commit command ran **08:21:33–08:28:52 UTC**, invoked required hooks over 84 staged files and exited 0. The published merge's pre-push hook ran **08:30:28–08:30:34 UTC**, compared 84 files and exited 0. Exact records are [main-merge-commit.json](logs/main-merge-commit.json) and [merge-pre-push.json](logs/merge-pre-push.json).

**The earlier Kingfisher stages were not completed content scans.** Both the checkpoint and merge pre-commit logs show `commit-tree` exit 128, followed by **0 blobs/0 bytes scanned**, despite a green wrapper label. The successful wrapper/commit exits remain recorded, but do not prove secret-scan coverage. Gitleaks and Betterleaks logged separate nonzero scans; they do not erase Kingfisher's missing coverage.

The [isolated identity probe](logs/kingfisher-identity-probe.json) reproduced the cause. The temporary snapshot disables system/global Git configuration and lacks local author identity. The same tree/parent `commit-tree` fails 128 without identity and succeeds 0 with the verified session identity supplied through `GIT_AUTHOR_NAME`, `GIT_AUTHOR_EMAIL`, `GIT_COMMITTER_NAME` and `GIT_COMMITTER_EMAIL`. No hook source, repository configuration or global Git configuration was changed. **This probe is not itself a content scan.** Final publication verification must record a meaningful nonzero scan and its exact staged input.

`MISE_EXEC_AUTO_INSTALL=false` suppresses unrelated optional-tool auto-provisioning. It does not remove required hook commands or turn failed scans into passes. Exact outer argv/start/end were not retained for some earlier checkpoint logs; those fields remain null. Printed log-local timestamps and file mtimes are not substituted for missing execution timestamps.

Build CSS/chunk warnings, Git's exhaustive-rename warning, and error-level diagnostics from upstream malformed Cargo fixtures remain in the raw logs. They are distinguished from actual command exits and from substantive scan-coverage failure.

## Merge identity and source boundary

Locally created merge `50146c4cd66e96cd66f18277a1a212642898e01f` and API-published merge `9971e26249c2a601912419c84aad99980f88d28b` have the same tree:

`d4c970e041674a204a3e9f287763c2e6151225eb`

Their ordered parents are also identical: research checkpoint `e0fd8035bf344b51dcdf0700606846770fe399ee`, then the first integrated main revision `c98f5f5fdd831e6075e38fde93b9713575406140`. The local validation branch is preserved. These are equivalent trees with distinct commit identities; no force-push or claim of identical commit hashes is made. The original product/Jackin source audit and later main-overlay review remain separately pinned.

## Independent PR and requirements review

A different author fully read the final-draft PR description, all 255 instruction entries and the historical/publication metadata. All local evidence targets existed; the baseline census contained 905 rows. The reviewed draft explicitly retained a verification placeholder. No additional scope/evidence conflict was found; the subsequent log audit found the Kingfisher coverage issue above.

Reviewed hashes:

- PR draft: `07724ea858b0ccd20038f4afd00e9f6413e82763828d51841d3a20440f8db405`.
- Requirements reconciliation: `805f989cfc242d5f70259373477427b376058c9845be72ff29623d0608724576`.

Later PR edits are not silently covered by those hashes. The [central/frontier independent review](../market-literature/independent-review.json) records ten other reviewed files, primary-source checks and resolved recommendations.

## Later main advancement

Main subsequently advanced to `e737f4b5eb7c1a0d15ba6595fc260407ad178452`. The coordinator integrated that revision normally into the worktree; the latest documentation gate above records all seven actual passing commands. The [independent late-main overlay review](late-main-overlay.md) and [exact comparison record](late-main-overlay.json) establish that **25 of 26 original audited files remain byte-identical**. The exception is `Cargo.toml`, whose sole change updates the Velnor contract dependency from 0.1.48 to 0.1.52 and its source revision. All 13 files in the first main-overlay audit remain unchanged.

The 26-file main update changes no Rust source file and no context-research path. Its complete lock-package comparison changes only Velnor; the SQLite and Tokio package records remain unchanged. The CI graph still has 40 jobs, 32 Rust crate jobs and 37 dependencies of the required verdict. This is a source/configuration comparison, not a CI result or qualification of the changed release toolchain. That graph has no documentation-site build/typecheck/test lane, so the local documentation gate remains separate evidence.

The earlier `c98` integration, commit identities and gate logs remain historical snapshots. At final rendering, `HEAD` was `9971e26249c2a601912419c84aad99980f88d28b` and `MERGE_HEAD` was `e737f4b5eb7c1a0d15ba6595fc260407ad178452`: the clean main merge was present in the tested worktree but was not yet committed. The final publication commit, its hooks, research-only PR diff and remote CI are separate later checks.

## Rendered pages and visual review

The [final browser/visual review](../jackin-architecture/rendered-qa-review.md) passed on the rebuilt assembled worktree from **08:48:53 to 08:49:24 UTC**, with command exit 0. The [result JSON](../jackin-architecture/rendered-pages-results.json) and [exact argv/runtime record](../jackin-architecture/rendered-qa-command.json) establish:

- **112 of 112 page checks passed:** 56 routes at desktop 1440×1000 and mobile 390×844.
- **718 of 718 internal URL checks passed**, including **657 fragments checked against rendered DOM**.
- **Two actual scoped sidebar navigations passed**, one at each viewport; per-page navigation visibility was also checked.
- No captured console/page error, local request failure or page-wide horizontal overflow occurred. All **58 targeted source/config hashes** matched before and after.
- **22 representative images have visual coverage:** eleven desktop images reviewed by the browser workstream and eleven mobile images independently reviewed by another agent. Nineteen final images are byte-identical to the preceding reviewed run; the changed desktop table and two changed mobile tables were directly reopened and inspected.

The [build comparison](../jackin-architecture/rendered-qa-main-build-comparison.json) shows that all 56 HTML hashes changed after the late-main rebuild, despite unchanged targeted source hashes. The earlier sweep was preserved and the same parameterized script reran against the new output. [Run history](../jackin-architecture/rendered-qa-run-history.json) retains all four runs, including the initial harness failure and subsequent corrections. No MDX or application source changed during these sweeps.

The [screenshot inventory](../jackin-architecture/rendered-qa-screenshots.json) and [independent mobile review](../jackin-architecture/rendered-qa-mobile-independent-review.json) preserve exact image hashes, reviewer ownership and the corrected initial icon-reading observation. Representative static images establish readability and bounded table rendering; they do not prove touch/horizontal-scroll interaction, all table columns or every pixel on every page. One Linux Chromium build and two viewport sizes are not a complete accessibility or device/browser audit. External-link availability was not tested. The loopback-only server was stopped and its port confirmed closed after verification.

## Final publication boundary

Final publication hooks, the meaningful staged content scan and exact-head remote CI belong in the final PR metadata after these verification artifacts are staged. This tree does not fabricate a self-referential final commit hash or a future CI pass. The PR remains unmerged without explicit authorization.

No paid/subscription task benchmark, live native-host result-replacement/read-before-edit trial, or actual quota-depletion experiment was executed. Successful local checks do not establish current six-product accepted-task superiority.
