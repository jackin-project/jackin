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

Status: OPEN; generation and execution gates are IN PROGRESS.

The [PR](https://github.com/tailrocks/velnor-new/pull/59) is `fix/required-actions-read-scope`. Its head is `81e65fee08edc3b73839d9ff810cb7da6a170a65`. The recorded base is `1856b5b9f47569515c8fa00657a2c8dde6aada9f`. A later direct ref check found main at `c4fc31efd2fbb39b7cfc2cce423d99b7c4733c3d`; checks at that newer main are not recorded here.

`architect_manifest_fix` is the sole heavy owner for generator build and self-CI regeneration. An immutable source, archive, and isolation packet is pending, followed by applicable review. No MBX or Cargo execution is recorded for this PR. Do not claim generated-source or test acceptance.

## Current Jackin consumer check

Consumer source review PASS at `07f5ce7efe38c6c608fb975013df43e770d92b2b`. It pins Architect PR head `7db69b62f598a0971809ee4a006ad3f5477d0996` and manifest SHA-256 `b38e506587c98137d0a1a88247fb68afc9f9f215c8104c838df251933a917ae5`. The source review covers the immutable manifest fixture and CI contract. Tests remain NOT RUN.

Task-order fix `41265a7550dda498c2e32126a9e68d4733215da5` sorts configured workflow tasks by ID. The release manifest still pins Velnor `0.1.0`. Its three configured tasks do not appear in generated `.github/workflows/ci.yml`.

Jackin run `37262862647` at `41265a7550dda498c2e32126a9e68d4733215da5` failed. Actionlint passed. Plan failed during generated-file checking because pinned Velnor rejected `.velnor/config.toml` field `tasks` as unknown. Required then failed because the Plan artifact was missing. Rust jobs were skipped. No test or build result is available from this run.

## Task invocation gate

At Jackin base `0aa821a088e1bacf3d4d85a4c9faaa67faa85132`, the `mise.toml` build, test, and lint root tasks call `cargo xtask` directly.

The Velnor `VerificationTask` uses `mise run` only for proven non-Rust tasks. A Rust-MBX variant and Jackin Mise-wrapper integration remain pending design and activation review. Do not claim the invocation bypass is fixed.

Resolve this integration before CI acceptance. Verify generated output at the final Velnor head.

## Owner

`velnor_recon` and `jackin_generator_config` own generator and CI coverage. Record their final job matrix and required-check result here.

See [branch findings](branches.md), [build results](build-results.md), and [reviews](reviews.md).
