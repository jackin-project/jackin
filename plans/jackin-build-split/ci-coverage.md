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

Status: IN PROGRESS.

The [PR](https://github.com/tailrocks/velnor-new/pull/55) targets `7fb8367d7daa67f13ccaa7c76caae47d55d6262b`. The current reported head is `2cb1b4ea5ffcb6c6fd54b25e78197f392bbfdae3`. Its earlier head was `68f969bb4b9c71be8ff2126f04f6a2bcebe38a78`.

At check run `37254450133`, Actionlint passed. The orchestrator job `111589804766`, CLI job `111589804784`, and workflow-renderer job `111589804869` failed. The remaining matrix is in progress. Logs are unavailable until the run completes. Do not infer a cause or claim a passing run.

An earlier Plan failure at head `260f17c` reported helper compile errors `E0432` and `E0425`. Commit `68f969b` fixes all eleven old-name references. Do not claim that upstream merged the PR or that local tests passed.

### Focused renderer run

At run `37256105661` and head `2cb1b4ea5ffcb6c6fd54b25e78197f392bbfdae3`, renderer job `111593988338` passed 339 unit and integration tests with zero skipped. Format, Clippy, executable tests, doctests, and documentation checks passed under verified Mise and MBX.

Named unit regressions cover case-insensitive extensions, the inclusive 500000-byte boundary, and UTF-8 marked-byte accounting. Integration cases cover the base boundary and `+1`, direct CI/release/schema-2/freshness renderers, and an extra release workflow. Typed-task tests `emitted_verification_job_scrubs_credentials_without_disabling_mise_config` and `task_job_is_unconditional_cache_off_and_credential_scrubbed` passed.

The orchestrator failure-preservation job `111593988311` is still running. The PR run remains incomplete. Do not report an overall pass or merge readiness.

## Coverage questions

The current workflow inventory has no separate release, macOS Swift, Docker, Bun, or scheduled workflow files.

The generator coverage review is still active. Recheck after upstream fixes merge. Confirm which jobs belong in the required CI surface before changing generated files.

Do not claim generated workflow parity. Compare generator source, configuration, output, and current checks after refs are fetched.

## Task invocation gate

At Jackin base `0aa821a088e1bacf3d4d85a4c9faaa67faa85132`, the `mise.toml` build, test, and lint root tasks call `cargo xtask` directly.

The Velnor `VerificationTask` uses `mise run` only for proven non-Rust tasks. A Rust-MBX variant and Jackin Mise-wrapper integration remain pending design and activation review. Do not claim the invocation bypass is fixed.

Resolve this integration before CI acceptance. Verify generated output at the final Velnor head.

## Owner

`velnor_recon` and `jackin_generator_config` own generator and CI coverage. Record their final job matrix and required-check result here.

See [branch findings](branches.md), [build results](build-results.md), and [reviews](reviews.md).
