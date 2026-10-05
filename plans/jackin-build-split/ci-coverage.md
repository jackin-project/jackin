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

Sol's Velnor security source review passed at PR head `6baa3a1f729d45a764fd4250d1300cf17fa196e6` against main `6180ccebc7eff8b8f40f988eea2cf948bb235c9d`. The review records fixes for typed-job Mise environment true/unset behavior, a workflow-wide byte guard, UTF-8 accounting, and no-partial/in-place tests. It rejects the untrusted-cache-writer allegation because cache saves remain push-only. The later merge and successful PR run are recorded above. The source review remains bounded to `6baa3a1`. The successful run verifies the merge commit only; checks at the later main SHA remain NOT RUN.

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

The base-asset materializer source gate passed independently at artifact SHA `6cea80e6c5cb1303f1e2180cfc73d3f18a9b0b18f32af6fdda8572dbedbe6506`; the reviewer verified four packet hashes and the source/dependency inventories, but ran no materializer or fixture. Two subsequent stage attempts are owner-reported as safely stopped: attempt 1 used a wrong key, and attempt 2 rejected an unsafe relative path after partial staging. The private owner records remain the evidence source; no staging PASS exists. Keep the static source PASS separate from actual staging and execution.

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

## Task invocation gate

At Jackin base `0aa821a088e1bacf3d4d85a4c9faaa67faa85132`, the `mise.toml` build, test, and lint root tasks call `cargo xtask` directly.

The Velnor `VerificationTask` uses `mise run` only for proven non-Rust tasks. A Rust-MBX variant and Jackin Mise-wrapper integration remain pending design and activation review. Do not claim the invocation bypass is fixed.

Resolve this integration before CI acceptance. Verify generated output at the final Velnor head.

## Owner

`velnor_recon` and `jackin_generator_config` own generator and CI coverage. Record their final job matrix and required-check result here.

See [branch findings](branches.md), [build results](build-results.md), and [reviews](reviews.md).
