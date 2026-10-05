# CI Coverage

- Status: IN PROGRESS
- Evidence: Static workflow and generator configuration inspection.

## Current workflow

- The repository contains one workflow file: `.github/workflows/ci.yml`.
- The workflow defines 31 jobs. The three other top-level keys counted by a text search belong to event triggers.
- The [Velnor configuration](../../.velnor/config.toml) sets four compiler processes and four test processes.
- The workflow pins MBX `1.21.0` and the `velnor-actions` release `0.1.0`.
- The [release manifest](../../.velnor/release-manifest.json) pins generator source commit `c57c700459bbe1549fe7eedcb7d8689585c38986` from `tailrocks/velnor-new`.
- The reported upstream generator main SHA is `0c40d077fcad5450521351f497ce003c69915eff`.
- A later direct ref check reported `ad73ae9f0500ddd02d64aad142bbecb2122c0617` at `2026-10-05T00:39:47Z`.
- A later fetch recorded `origin/main` at `d9f3f3be03d67021748fd6adb4a18684d046e5e7` at `2026-10-05T01:09:24Z`. The generator work branch was rebased onto that head before edits.

## Velnor PR #55

Status: MERGED; merge-commit CI PASS; current-main CI NOT RUN.

The [PR](https://github.com/tailrocks/velnor-new/pull/55) targeted `7fb8367d7daa67f13ccaa7c76caae47d55d6262b`. Earlier checks below are historical snapshots.

At check run `37254450133`, Actionlint passed. The orchestrator job `111589804766`, CLI job `111589804784`, and workflow-renderer job `111589804869` failed. The remaining matrix is in progress. Logs are unavailable until the run completes. Do not infer a cause or claim a passing run.

An earlier Plan failure at head `260f17c` reported helper compile errors `E0432` and `E0425`. Commit `68f969b` fixes all eleven old-name references.

### Focused renderer run

At run `37256105661` and head `2cb1b4ea5ffcb6c6fd54b25e78197f392bbfdae3`, renderer job `111593988338` passed 339 unit and integration tests with zero skipped. Format, Clippy, executable tests, doctests, and documentation checks passed under verified Mise and MBX.

Named unit regressions cover case-insensitive extensions, the inclusive 500000-byte boundary, and UTF-8 marked-byte accounting. Integration cases cover the base boundary and `+1`, direct CI/release/schema-2/freshness renderers, and an extra release workflow. Typed-task tests `emitted_verification_job_scrubs_credentials_without_disabling_mise_config` and `task_job_is_unconditional_cache_off_and_credential_scrubbed` passed.

Orchestrator job `111593988311` later reported the scale checks. One crate generated successfully in `317 ms`; ten crates generated successfully in `701 ms`. The 100-crate case failed with `workflow_too_large:.github/workflows/ci.yml:812452:500000`. This was one run, not a repeated performance comparison.

### Current P13 snapshot

At `2026-10-05 03:16 UTC`, PR #55 head `d10472a32b227e98ac09180feba0ca6f8899ccf8` had parent `2cb1b4ea5ffcb6c6fd54b25e78197f392bbfdae3`. Plan, Alint, Zizmor, Cargo Deny, and orchestrator job `111599377218` setup passed. Unit and integration tests were running. Doctests and documentation checks were pending.

At `2026-10-05 03:19:19Z`, job `111599377218` failed. Unit and integration tests ran from `03:14:24Z` to `03:18:05Z` and then failed. Doctests and documentation checks were skipped. Report upload passed. Initial CLI output did not expose the failure logs. The owner later reported the exact test failure below. Do not claim merge acceptance.

The owner reports `impl_perf_p13::generate_scales_with_crate_count` failed because the test expected the ten-crate workflow to exceed the cap. Earlier run `37256105661` showed ten crates succeed. Current test progress reported `778/1060`; totals were 780 passed, 1 failed, and 279 not run. The first rejection assertion failed before the 100-crate rejection log. A one-crate success reported prepare `47 ms`, generate `343 ms`, and four files. This single timing is not a performance comparison. No correction or retry is verified.

The corrected source expects one, ten, and forty crates to succeed and 100 crates to reject above the byte cap. At the d104 snapshot, these corrected cases had not run. The correction is in `a81566e17d055716780dcf3e5abf02ea7665c7ce`. Remote PR head `6baa3a1f729d45a764fd4250d1300cf17fa196e6` is a sync merge with parents `a81566e17d055716780dcf3e5abf02ea7665c7ce` and `d10472a32b227e98ac09180feba0ca6f8899ccf8`. The owner reports that the merge commit tree matches the corrected parent tree. Its delta from d104 is 57 paths, `+1,386/-615`; the 2cb-to-6baa common-base delta is 57 paths, `+1,393/-592`.

The owner also reports an unpushed local sync merge with the same tree. Its full SHA was not supplied. At `2026-10-05 03:32:27Z`, run `37259299811` had 17 checks passed, 2 in progress, and 0 failed. The orchestrator remained pending at that observation. The run had no P13 result then.

The GitHub API reported PR #55 closed and merged at head `f17ebbc992da8549197f63d2aaaf1c317ed57426`, using merge commit `3ec6f32b5bafa5fa34ce9aa22afd7cfe2e797132`. Run `37260546503` completed with conclusion `success`; its `Required` job also succeeded. This supersedes the earlier running snapshot. Post-merge workflow run `37261457091` completed successfully at merge commit `3ec6f32b5bafa5fa34ce9aa22afd7cfe2e797132`. Its job query returned 20 jobs, all successful. A later `git ls-remote` check found Velnor main at `1856b5b9f47569515c8fa00657a2c8dde6aada9f`; CI at that newer head is NOT RUN.

Sol's Velnor security source review passed at PR head `6baa3a1f729d45a764fd4250d1300cf17fa196e6` against main `6180ccebc7eff8b8f40f988eea2cf948bb235c9d`. The review records fixes for typed-job Mise environment true/unset behavior, a workflow-wide byte guard, UTF-8 accounting, and no-partial/in-place tests. It rejects the untrusted-cache-writer allegation because cache saves remain push-only. The later merge and successful PR run are recorded above. The source review remains bounded to `6baa3a1`. The successful run above verifies the merge commit. A later exact-main workflow run is recorded below; it is a separate result and does not extend the PR #55 source-review scope.

### Separate optional Rust setup-factoring proposal

This proposal is not part of PR #55. Do not attribute its findings to the current PR head.

The proposal extracts the byte-identical eight-step Rust setup prefix into `.github/actions/velnor-rust-setup/action.yml`. Linux Rust jobs repeat 127,467 raw bytes in that prefix. Exact review used Velnor main `6180ccebc7eff8b8f40f988eea2cf948bb235c9d` and PR head `d10472a32b227e98ac09180feba0ca6f8899ccf8`.

Disposition: FAIL pending redesign. `mbx_bundle.rs` exports and saves `steps.mbx.outputs` and `steps.mbx-bundle.outputs`. A composite action has no `id` or outputs. Extraction would hide required producers. `document.rs:219-260` derives credential scrubbing, `RUSTUP_TOOLCHAIN`, and acquisition provenance from the remaining steps. Compute those values from original steps before extraction.

The design review has not approved implementation. The 375–380 KB output size is an estimate. Generated output, determinism, boundary behavior, and tests remain NOT RUN. Preserve job IDs, Required dependencies, metadata, and step order in a revised proposal.

## Coverage questions

The current workflow inventory has no separate release, macOS Swift, Docker, Bun, or scheduled workflow files.

The generator coverage review is still active. Recheck after upstream fixes merge. Confirm which jobs belong in the required CI surface before changing generated files.

Do not claim generated workflow parity. Compare generator source, configuration, output, and current checks after refs are fetched.

## Velnor PR #59

Status: MERGED on a later main snapshot; permission source review PASS; Jackin generation remains separately gated.

The [PR](https://github.com/tailrocks/velnor-new/pull/59) is `fix/required-actions-read-scope`. Earlier snapshots at head `81e65fee08edc3b73839d9ff810cb7da6a170a65` and base `1856b5b9f47569515c8fa00657a2c8dde6aada9f` are superseded. A later snapshot reported PR head `dccf0fbe04a43cd0b04791663a6c9fe619687a62`, base `c8a891bfb9e9a692e328732dc950e918c182fc78`, and Velnor main `7ccc7617253343d6e59feb1727b068a0cc0e937e`. The PR was merged by that main snapshot. GitHub reported 19 completed checks successful.

Exact Sol source review at PR head `dccf0fbe` and tree `d073b2c4008202705935fb062ff615e8d00f67f3` passed against base `c8a891b`. The review covered only the permission-scope change: workflow defaults do not grant Actions access, while Plan and Required receive the reviewed read scope. It did not re-review unrelated imported runner changes. The 19 successful checks are the reported Velnor snapshot; they do not establish current Jackin generation, role validation, or runtime acceptance.

## Velnor PR #65

Status: SOURCE PASS; generated-workflow CI and execution remain NOT PASS.

The publisher source at `ff41745394712604bddf94b6a4e9f1069100789e` received exact Sol source review PASS. CI run `37275543333` built the helper through Mise and MBX and uploaded its preseed artifact, but failed the generated-workflow comparison. Required then failed, and Rust jobs were skipped. No successful generator or Rust-test result is recorded. The artifact remains unexecuted: the security review found the isolated workspace and cleared environment do not constrain filesystem, socket, or network access. PR #65 remains draft and unmerged; no release or publication occurred.

These Velnor snapshots do not validate the Jackin fixture rebake or source tests. The current Jackin migration fixture archive and pending execution gates are recorded in [reviews](reviews.md#current-migration-source-and-fixture-checkpoint).

### Velnor PR #65 latest generated-workflow result

PR [#65](https://github.com/tailrocks/velnor-new/pull/65) remains open and draft at merge commit `bc7f784a51233bd29676353392c9fc07c0ad01a7`. Run `37289064072` completed on that exact SHA. Zizmor, Actionlint, Cargo Deny, Cargo Machete, Alint, and DCO passed. Plan failed at generated-file comparison because `.github/workflows/generator-release.yml` differed from the rendered preview; Required failed because no plan artifact was produced. All 12 Rust crate jobs and Publish baseline were skipped.

The Plan log confirms the helper-only build path completed `Build helper`, `Verify MBX compile`, `Write helper manifest`, `Upload helper`, and `Stage helper` successfully before the generated-file comparison. This is helper build/upload evidence only, not a passing generated workflow or PR gate. The earlier source review at `ff41745394712604bddf94b6a4e9f1069100789e` predates the 81-path change in the merged `bc7f784` tree; it does not re-review that exact later tree. No final renderer/generator review or Rust test acceptance is claimed.

The base-asset materializer source gate passed at artifact SHA `6cea80e6c5cb1303f1e2180cfc73d3f18a9b0b18f32af6fdda8572dbedbe6506`; the reviewer verified four packet hashes and source/dependency inventories but ran no materializer or fixture. Stage attempts 1 and 2 stopped safely: attempt 1 used a wrong key; attempt 2 rejected an unsafe relative path after partial staging. Attempt 3 then completed the bounded base-stage command at 2026-10-05 09:43:48–09:44:08 UTC with exit 0. Its staged tree SHA-256 is `1cef88b359a08c32cccb6078a55309d4db67b883dc260ccc3bbeb80ddce55763`; manifest SHA-256 is `1a69b71279d9c5a7e7c1c5937d9c55898f1e5b0f1fc7405010d6ffb273f47acf`; the private attempt record SHA-256 is `89b9feb8b946c8d435e3978395db69b632f23769e18da41fcfc5e57b9afb37e`. The stage inventory reported 36 directories, 37 host files, 5 links, 1 `/dev/null`, and 218 Rust files. No tool, helper, Cargo, network, account, or generation action ran. This is a base-input staging PASS only, not a generator or image execution result.

Later materializer evidence is separate from that successful attempt. A limited local fixture passed with materializer SHA-256 `27d65c5831d1e9409cb196868624f89aef06b66c6f34b793a49c1f356c876c97`, fixture script SHA-256 `00d28c2116d65217fcc8a61386eab5a0fcd0c17fa8b706436dabef784e24c61b`, and result-record SHA-256 `2aa7e71ad9c3a2e7cb909bd31c78f35fd0aa35ae3af248e37725c8b8bc947027`; it exercised only synthetic Git stdin/no-input/argv paths. Sol later passed the current materializer source at SHA-256 `b28542a70e9eae4a932c87ef5d1a259678f6d14320953dc0cf4bd566d3994019`. The first source-materialization attempt stopped before tracked files because `.git/info` was absent (private log SHA-256 `eb561005240890e8abcde32f2193dddbdea01e59465dc3c098b04cbc91c4bb6a`). A full-boundary fixture review rejected a literal `\\0` separator; the corrected fixture v6 SHA-256 `10037434712df96c71a33858018e7ca4ae2f200235f13a74bd83a64218c6f329` then passed its bounded execution at 10:57:43–10:57:47Z (record SHA-256 `592141adbca7eed8ecb9dc06bab0297a3642a52075bdad98e748f1499eebf8a0`). This fixture PASS does not establish source materialization or helper execution. A separate Cargo acquisition wrapper attempt (outer artifact SHA-256 `942bedafa295f8a4ae29a6f13687f32d6e75c4134a039c62bb8dfa6cf2112d96`, namespace SHA-256 `d66c3a5c826a4a3916281324a52e83d41a4ea463db7200da720f6d122bfefb25`, private preflight log SHA-256 `376fd4ce727faaa962c90208f671a28528ab5f7a0aed564ed0beb44cedd25582`) passed selectors but stopped on missing input before Cargo or network. Separately, source-materialization attempt 2 wrote a private raw tracked tree and then failed worktree verification with a bytearray/string `TypeError`; its log SHA-256 is `166dbdcc9638a47652ce3e30518e0099df50ab6125f02348af95cbc202dbd0e6`. No manifest, helper, or generated workflow was produced. The owner is fixing the preview-buffer prefix mutation and adding a regression; exact native command, UTC interval, and attempt record are pending. No current-source materialization or helper run is accepted yet.

## Velnor main exact helper and CI run

At the 2026-10-05T10:42Z metadata refresh, Velnor `main` was `ac3ab6a3d5ba3701c1300bbdd8114390093c5c29`, tree `b4cc6aa0df400e111cbee27a2cd5edc9f0144bac`; the commit message is `feat(cache): make the native MBX action the sole cache owner`. Workflow run `37294389760` was triggered by a push at 10:06:14Z on that exact SHA and completed successfully at 10:20:34Z. Its job API returned 20 jobs, all successful, including Plan, Required, Publish baseline, and all 12 Rust crate jobs. Plan job `111712067190` passed `Build helper (pre-seed trust-on-review)`, `Verify MBX compile`, manifest creation, helper upload, and helper staging. It recorded helper binary SHA-256 `55d98571518c93a90f28b6b298281eae76275d637560c959178fb8e8ad77ea84` and manifest SHA-256 `c36bfa0b15c61ffce422a279b102ad5600715b64aea9d27e1858cc42c0520473`. The exact commit's `mise.toml` pins Rust `1.98.1` and MBX `1.22.0`; the Plan job passed its pinned-tool and native-MBX version checks. The run reported zero cache hits and three compiler bypasses.

This validates the workflow and source build on that exact main snapshot. The route was a pre-seed helper path and reported zero cache hits plus three compiler bypasses; it does not establish cache reuse or cache effectiveness. It is not a controlled Jackin performance sample and does not pass the separate PR #65 at head `bc7f784a51233bd29676353392c9fc07c0ad01a7`, which remains open and draft with its generated-file comparison failing. Keep the main-run result separate from PR #65's exact-head result and from the private base-stage PASS above.

## Current Jackin consumer check

Consumer source review PASS at `07f5ce7efe38c6c608fb975013df43e770d92b2b`. It pins Architect PR head `7db69b62f598a0971809ee4a006ad3f5477d0996` and manifest SHA-256 `b38e506587c98137d0a1a88247fb68afc9f9f215c8104c838df251933a917ae5`. The source review covers the immutable manifest fixture and CI contract. Tests remain NOT RUN.

Task-order fix `41265a7550dda498c2e32126a9e68d4733215da5` sorts configured workflow tasks by ID. The release manifest still pins Velnor `0.1.0`. Its three configured tasks do not appear in generated `.github/workflows/ci.yml`.

Jackin run `37262862647` at `41265a7550dda498c2e32126a9e68d4733215da5` failed. Actionlint passed. Plan failed during generated-file checking because pinned Velnor rejected `.velnor/config.toml` field `tasks` as unknown. Required then failed because the Plan artifact was missing. Rust jobs were skipped. No test or build result is available from this run.

### PR #1112 exact-head gate snapshot

Read-only refresh at `2026-10-05 08:48:25Z` found PR #1112 open, draft, and blocked. Base is `0aa821a088e1bacf3d4d85a4c9faaa67faa85132`; head is `f4902db386e029a4a481b13767f7a29b5351af49`. The source under review is unchanged at `17b2b1be6a58a0e34af6d8308df915d110f4a785`; the later task-branch commits only update plan records. Run `37285807560` completed on the exact PR head.

| Check | Result | Evidence |
|---|---|---|
| Actionlint | PASS | Run `37285807560`. |
| Plan | FAIL | Velnor `0.1.0` rejects `.velnor/config.toml` field `tasks` as `unknown_config_field`; the generated-file step cannot run. |
| Required | FAIL | The Plan artifact is absent after Plan failed. |
| Rust crate jobs | NOT RUN | Every `Rust / *` job was skipped. No source test result exists for this run. |
| Publish baseline | SKIPPED | Not run after the failed required path. |
| DCO | ACTION REQUIRED | The current check rollup does not satisfy DCO. |

The exact source has not passed Cargo tests, Clippy, generated migration-golden checks, or a complete schema check. Those are NOT RUN, not test failures. The source-review passes recorded in the branch matrix do not replace these gates.

PR #1112 can proceed as soon as its own source, generated workflow, feedback, and required checks pass. It need not wait for separate live Architect-role testing, host account discovery, or the post-integration performance baseline. Record those follow-ups after the merge without treating them as PR-specific check results.

### DCO-signed replacement PR #1113

At `2026-10-05 09:27 UTC`, PR #1113 is open and draft at head `7b6ea934490b6ef0d867a43def527e29caa3b63f`, based on main `0aa821a088e1bacf3d4d85a4c9faaa67faa85132`. It replaces the DCO-invalid history of #1112 without changing the final tree. The independent replay review verified all 60 old/new commit pairs and the shared final tree `9eebc07e80d91f2d29cc9d975b78a53ca4198b3b`; 39 Codex-authored commits have matching sign-offs, and 21 existing valid sign-offs remain. See [DCO replay review](reviews.md#dco-signed-history-replacement).

The old #1112 head `73ea2117b8e584c64dec92242271a86149a53125` remains open and frozen as the source destination. Its latest refreshed run `37288558605` had Actionlint PASS, Plan and Required FAIL, 25 Rust jobs SKIPPED, Publish baseline SKIPPED, and DCO ACTION REQUIRED. Paginated REST and GraphQL refreshes found zero inline comments, reviews, issue comments, or review threads.

Replacement run `37289881948` completed on `7b6ea934490b6ef0d867a43def527e29caa3b63f`. DCO-2 and Actionlint passed. Plan failed because acquired Velnor `0.1.0` rejects `.velnor/config.toml` field `tasks` as `unknown_config_field`; no plan artifact was produced. Required failed while merging reports, all 25 Rust jobs were skipped, and Publish baseline was skipped. The current Velnor task schema is not yet available through the pinned release. The replacement PR has no reviews, inline comments, issue comments, or review threads at this refresh. These results are not Rust test or compile results. Any later docs commit changes the head and requires a new exact-head check refresh.

After docs-only evidence commit `edde91c92d64d9c04211f20012d271d8b1b28cf3`, run `37290996910` completed at that exact head with the same result: DCO-2 and Actionlint PASS; Plan FAIL on `tasks: unknown_config_field`; Required FAIL because Plan produced no artifact; all 25 Rust jobs and Publish baseline SKIPPED. The Plan log confirms the same Velnor `0.1.0` invocation. REST and GraphQL feedback refreshes at `edde91c` found zero inline comments, reviews, issue comments, or review threads. The docs-only commit did not change executable source; tests remain unrun on this task branch.

At the current source head `e17a5dd5c0ef8a34ed20b1837534b851d38e78d1` (tree `5e07bdacd3cdbf4dc24071f9e5ef88c2be53f595`), run `37300944505` completed with DCO and Actionlint PASS, Plan and Required FAIL, all 27 Rust jobs SKIPPED, and Publish baseline SKIPPED. Plan still fails during generated-workflow comparison because Velnor `0.1.0` rejects `.velnor/config.toml`'s `workflow.tasks`; this is a generator/configuration failure, not a Rust or native test result.

The maintained workflow inventory has six active workflows. Only `.github/workflows/ci.yml` is the project CI workflow, and all of its jobs use `ubuntu-26.04`; it has no Swift/native job and no `workflow_dispatch` trigger. The current `.velnor/config.toml` declares macOS format and SwiftLint verification tasks, but the failed Plan prevents their creation or execution. It declares no SwiftPM test task. Jackin's native project requires macOS 26/Xcode 26.6 and `native/Package.swift` targets macOS 26. The generator's current verification-task contract is standalone and compile-free, so the existing task mechanism cannot run the Rust-backed XCFramework producer and Swift tests together. A supported native CI path remains required; no manual workflow run or local Swift test was available on this Linux host.

## Task invocation gate

At Jackin base `0aa821a088e1bacf3d4d85a4c9faaa67faa85132`, the `mise.toml` build, test, and lint root tasks call `cargo xtask` directly.

Commit `091bbae649ba663ce8a77239938ac31493eb7eda` adds the supported Mise `[wrappers.cargo]` route to MBX and enables `MBX_CARGO_SHIM_MODE=1`; the existing Rust version remains owned by `rust-toolchain.toml`, with MBX pinned separately in the task configuration. Mise 2026.10.1 parsed the wrapper configuration, and exact pinned Mise/MBX source review passed.

Runtime verification of wrapper dispatch is pending the reviewed fake-command harness and source-bound MBX execution. The explicit invocation `mise exec -- mbx test ...` is a direct MBX CLI command; it does not itself prove transparent wrapper behavior for nested `cargo xtask` or Boltffi commands. The Velnor `VerificationTask` continues to use `mise run` only for proven non-Rust tasks. Resolve the supported generated-CI Rust test path and verify generated output at the final Velnor head before CI acceptance.

## Owner

`velnor_recon` and `jackin_generator_config` own generator and CI coverage. Record their final job matrix and required-check result here.

## Latest task PR refresh

Read-only GitHub refresh at 2026-10-05 15:39 UTC:

| PR | Exact head | Current result | Remaining gate |
|---|---|---|---|
| Jackin #1111 | `e45405bd4a799ea7b0b30dbc80f45e2e5103690d` | OPEN, draft, mergeable. Run `37334128618` had Plan, Actionlint, and DCO PASS. `Rust / jackin-instance` failed; several Rust jobs remained active. | Resolve two OMP snapshot findings from exact 8ca review, review the changed head, finish all CI, and run the promised live/docs gates. |
| Jackin #1113 | `25701ee67199857e3b33f49de9e9e2acdcd4b7a7` | OPEN, draft. The last recorded run `37329637685` failed Plan and Required because pinned Velnor `0.1.0` rejects `tasks`; Rust jobs were skipped. | Velnor typed build-task contract and current-source generation; re-fetch checks and feedback at the new head. |
| Velnor #65 | `c6e18543b33d295d613c874aa41273ec608c6653` | OPEN, draft, mergeable. Run `37334164232`: DCO, Actionlint, Cargo Deny, Cargo Machete, Zizmor, and Alint PASS. Plan and Required failed; Rust jobs were skipped and Publish baseline was skipped. | Fix generated workflow ShellCheck errors at `generator-release.yml:69,93`; rerun checks and review current source. |

For Jackin #1111, Sol's exact 8ca source review used head `8ca6152972bf157084e06d606a089980837d07c9`, tree `db5a842dfb9030bcef3ad825f31ad701ce9e0f1b`. Follow-up `e45405bd` changes `auth.rs` and has not received source re-review. The failed `jackin-instance` job's complete log was not available while run `37334128618` remained active. Do not attribute its cause without the job log.

Velnor #65's fetched Plan log identifies generated ShellCheck parse errors, including `SC1050` and `SC1072`, at the two listed workflow lines. This is a generator Plan failure, not a Rust test result.

See [branch findings](branches.md), [build results](build-results.md), and [reviews](reviews.md).


## PR #1113 exact head at `3986dfdd`

Direct `git ls-remote` and the local task ref matched `3986dfdd07dcfeb739f09f837d3a3ebdd944cbb9`, tree `50f5bbfc7e89fc66b71356ad12629028abfccea4`. PR #1113 remains open, draft, and mergeable against `main`. Run `37312549475` completed on that exact head. Actionlint and DCO passed; Plan and Required failed; every `Rust / *` job was skipped; Publish baseline was skipped.

The Plan log shows locked Cargo source fetching passed before generation. The failure is `velnor-actions-0.1.0`: `.velnor/config.toml: tasks: unknown_config_field`. Required then failed while merging the missing plan result. The Cargo.lock update removed the earlier locked-fetch failure; it did not resolve the pinned-generator schema mismatch. PR feedback queries found no reviews, comments, or review threads at this head. These checks do not establish Rust compilation or migration tests.

Since the previous recorded head `aa0316901eaf3be24a2dc746c40b0a2c1b2168ff`, three commits updated two paths: `8e356e2858fbfd390afd847604789b83bc0f451c` adds the MBX wrapper harness; `0cf350be6a66fd1316239452a718e890c051ad23` adds the two `toml_edit` serde lock edges; `3986dfdd07dcfeb739f09f837d3a3ebdd944cbb9` corrects the pinned Mise version parser. The parser source received exact Sol review PASS, but its fixtures and full harness have NOT RUN. The exact archive and packet hashes are recorded in [the migration source review](reviews.md#current-migration-source-and-mbx-wrapper-review).

## External Velnor freshness update

Velnor `main` currently resolves to `4fffbc22ce159305c62ae039668da2a14e2e3366`, tree `bc3fba64412ac80fd34b3d2367ecaf03415a2a0d`, parent `d435ac5b7e686ad9c9c594dde4b024b702435e47`. Alexey Zhokhov authored this upstream commit. It updates only `.velnor/freshness-inventory.json` and `docs/implemented/freshness-evidence-2026-10-05.md`. Run `37307721482` passed all 20 Velnor jobs, including Plan, Required, 12 Rust jobs, and Publish baseline. This is external Velnor evidence; it does not update Jackin's pinned generator or validate Jackin's generated workflow.

## Native build-task capability

The conditional Sol design review accepts a same-job macOS build-and-test task as the smallest supported capability; no artifact handoff is required. Existing `VerificationTask` jobs remain compile-free. Velnor Git owner reserved an isolated worktree at base `4fffbc22ce159305c62ae039668da2a14e2e3366` for a separate typed build-task contract and renderer. The implementation owner is `velnor_freshness_research`; reserved source and test files are listed in [the implementation review record](reviews.md#native-build-task-capability). Generated `.github/**` files remain owned by Velnor's generator. No source commit, generation, macOS job, or Apple-toolchain test has passed yet.

## PR #1111 exact head and live gates

Read-only refresh on 2026-10-05 found PR #1111 open, draft, and mergeable. Its head is `3a28c199f17da335ecd9abd8dd67ebf1aecc0421`, tree `ad74cbc0b3a69283bce851568222be8f688e3641`; base is `main`. Exact Sol source review calls it a merge candidate. The source archive and packet hashes are recorded in [the review record](reviews.md#pr-1111-exact-source-and-gate-status).

Run `37176424544` completed on that exact head. Actionlint, Plan, all 27 Rust jobs, Required, and DCO succeeded. Publish baseline was skipped by its main-only condition. GitHub returned no review submissions and no review threads. One issue comment corrected the PR's model-default statement and records that live Usage and docs-spec gates remain pending. The PR body now describes the broader implementation scope.

The current workflow invokes MBX for Rust compile, Clippy, Nextest, doctest, and documentation tasks. Dependency metadata and fetch steps use direct Cargo through Mise. The workflow does not run `docs specs`. A candidate invocation is `mise exec --locked --deny-net -- mbx run --locked --offline -p jackin-xtask --bin jackin-xtask -- docs specs`; it has not run. The source gate validates spec rows and cited test names. In CI mode it may spawn direct `cargo nextest list`; nested MBX routing is unverified.

The bounded `jackin console --debug`, `u`, `r` smoke also has not run. With a private empty home, isolated Jackin paths, sanitized environment, disabled network, and no discovered providers, it would cover TUI routing, background discovery, forced refresh, and empty broker handling. It would not prove authenticated provider requests. Stop if discovery finds a provider. PR #1111 remains blocked on both gates; do not infer completion from green workflow checks.

## PR #1113 current replacement head

Read-only refresh at 2026-10-05 14:46 UTC found PR #1113 open, draft, and mergeable at docs-only head `0ee479a93ebd0d4aaefa69809d0bfb4619761d5c`, tree `a1bfe1340c9b888a5e8c8eaa257e055fb71032fb`, against `main`. Its source snapshot is code commit `0cd9890607d65b6d4e2264acac9303a8b67b561a`, tree `229e4b2d45e0805ee28bb5ff4e06bbefa3a188e1`. Run `37327117521` completed on the exact PR head. Actionlint and DCO succeeded. Plan failed because pinned Velnor `0.1.0` rejects `.velnor/config.toml` field `tasks`. Required failed, all 27 Rust jobs were skipped, and Publish baseline was skipped. This is a generator-schema failure, not a Rust test result.

The refreshed PR API reports no review submissions or comments for #1113. The current check run has no generated Plan artifact. Source-only migration and diagnostics reviews do not satisfy compilation, fixture rebake, or test gates. PR #1112 remains open and frozen until #1113 reaches a verified destination. Re-fetch feedback and checks before any close or merge action.

## Current external Velnor main tip

A direct read-only `git ls-remote` at `2026-10-05 14:41 UTC` returned Velnor main `b640665abc7238050e856257c9fb3a150580bc8d`, tree `97655eb96bcd6a5da170ed15c4cd32126cb0666e`, parent `ccb337ecc66e694bf8cb292af1ff43332389fb97`. Its push CI run `37325218926` was queued as of 14:46 UTC. The earlier owner-reported snapshot `098be4ad61e614fa4ddfb30f7e7148e470974597` had run `37319085496` and Plan artifact `11349640822`, but the coordinator did not independently retrieve those records. No helper artifact or completed CI result is recorded for `b640665`. Rebind source and checks to a fresh immutable snapshot before claiming current generator compatibility.

## Exact PR check refresh at 2026-10-05 16:11 UTC

The following states were read from the paginated PR/check APIs and exact run records. These updates supersede older head snapshots in this file; earlier rows remain historical.

| PR | Head | Run | Passed | Failed or skipped | Result |
|---|---|---|---|---|---|
| Jackin #1111 | `a64af27dbefdf4d9239ad9e94209cb2416a4dbc4` | [`37336793714`](https://github.com/jackin-project/jackin/actions/runs/37336793714) | Plan, Actionlint, Required, DCO, all 27 Rust package jobs | Publish baseline skipped | Workflow run succeeded. OMP review, live Usage UI/broker smoke, and `docs specs` remain separate open gates. |
| Jackin #1113 | `010444548ed976f415289c53c22da8ab801d9e53` | [`37336019365`](https://github.com/jackin-project/jackin/actions/runs/37336019365) | Actionlint | DCO ACTION REQUIRED; Plan and Required failed; 27 Rust jobs and Publish baseline skipped | Original PR is blocked. Plan output: `velnor-actions: .velnor/config.toml: tasks: unknown_config_field`. |
| Jackin #1114 | `5914945f4d5613ee45837e0d61680c3e6be21258` | [`37338479371`](https://github.com/jackin-project/jackin/actions/runs/37338479371) | DCO, Actionlint | Plan and Required failed; 27 Rust jobs and Publish baseline skipped | Signed replacement is blocked by the same Velnor `0.1.0` task-schema rejection. |
| Architect #480 | `818ea17a727ec1ace911be44d78b13995e3f6571` | [`37322851929`](https://github.com/jackin-project/jackin-the-architect/actions/runs/37322851929) | Required, Actionlint, Plan, DCO, Sonar | Publish baseline skipped | PR checks pass, but the R10 runtime probe failed after its MBX compile invocation when execution from `/tmp` returned `Permission denied`; no MBX runtime pass or ARM64 result. |
| Velnor #65 | `a54e34b01b8bd5b327dcdac0de4831370303b86e` | [`37337142461`](https://github.com/tailrocks/velnor-new/actions/runs/37337142461) | DCO, Actionlint, Alint, Cargo Deny, Cargo Machete, Zizmor | Plan and Required failed; 14 Rust jobs and Publish baseline skipped | Plan log reports ShellCheck parse failures in generated `generator-release.yml` at lines 57, 69, and 93. PR state is DIRTY against main. |

For #1111, the API refresh shows two issue comments, no submitted reviews, and no review decision. The latest comment records the two outstanding OMP findings: omitted rollback-journal recovery/capture, and acceptance of a frame with simultaneous salt/checksum corruption. Both findings are based on exact source review of the OMP snapshot and remain open at a64 because its follow-up only changes a Clippy test expression. The docs-spec and credential-free UI/broker smoke are NOT RUN.

The DCO replacement is a normal new signed commit, not a rewrite of public history. Original #1113 remains open at `0104445` with DCO ACTION REQUIRED. PR #1114 at `5914945` contains a `-x` reference and sign-off, has the exact same source tree and parent as `0104445`, and its DCO check passed. Keep #1113 open until #1114 passes the generator and required checks and is otherwise verified as the destination.

PR #480 has no submitted review or review threads and five issue comments at the current refresh. Its checks pass, but the image-runtime test remains failed; draft status is appropriate. PR #65 likewise remains draft and blocked by the generated workflow parse errors. Neither green static checks nor DCO pass substitutes for the failed Plan/Required gates.
