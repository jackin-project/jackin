# Security and Codex Review

Status: IN PROGRESS

## Coordinator prerequisite

The coordinator model prerequisite is FAIL. Task section 2.1 explicitly supersedes older instructions and requires Luna/max. The active root process uses Sol/medium.

Work agents are tool-assigned Luna/max. Runtime confirmation remains pending. Delegation does not clear the coordinator failure.

## Codex schema and catalog

- The official [Codex configuration reference](https://developers.openai.com/codex/config-reference) defines `model_reasoning_effort` as a string.
- Supported reasoning levels depend on the selected model.
- The filtered local catalog output is recorded in [checklist](checklist.md#codex-catalog-command).
- Luna supports `low`, `medium`, `high`, `xhigh`, and `max`.
- Sol supports those levels and `ultra`.
- Current root settings are Sol/medium; task section 2.1 requires Luna/max.
- Runtime confirmation of agent settings remains pending.

## Preliminary security requirements

The preliminary review sets these gates:

- Review root and unprivileged execution boundaries before builds or live accounts.
- Review the exact container and authentication route.
- Treat Docker socket access as root-equivalent.
- Do not copy the complete Codex home.
- Record MBX cache and object provenance before compilation.

The `unprivileged_exec_design` owner is still working. The preliminary security review is complete, but execution remains gated.

## CI workflow review

Disposition: NOT APPROVED.

The review used Jackin base `0aa821a088e1bacf3d4d85a4c9faaa67faa85132`. It compared the pinned Velnor release source `c57c700459bbe1549fe7eedcb7d8689585c38986` with Velnor main `0c40d077fcad5450521351f497ce003c69915eff`, dated `2026-10-04T23:43:56Z`.

Velnor main later moved to `ad73ae9f0500ddd02d64aad142bbecb2122c0617`. A direct `git ls-remote` check recorded it at `2026-10-05T00:39:47Z`. Recheck the review against this newer generator head.

### Accepted findings

- Generated CI lacks release, native, non-Rust, excluded Rust-test, and security-policy coverage.
- `.velnor/config.toml` excludes native, documentation, Docker, fuzz, lint, brand, build-metadata, picker, and PR-trailer paths.
- `.github/workflows/ci.yml` is 500,239 bytes. No unconditional size guard was found.
- The workflow is NOT APPROVED. Do not merge it as a completed coverage result.

### Rejected cache-writer claim

The claim that pull requests write these caches is rejected by the workflow conditions.

Cargo and Mise cache-save steps require `success()` and `github.event_name == 'push'`. See [.github/workflows/ci.yml](../../.github/workflows/ci.yml), including the Cargo save condition at line 111.

MBX uses `ACTIONS_CACHE_MODE` with `write` only for push events. Other events use `read`. The workflow shows this at lines 429, 701, and later crate jobs.

This is a static finding at Jackin base. Check current generator output before finalizing the disposition.

## Architect integration review

The Architect repository is [jackin-the-architect](https://github.com/jackin-project/jackin-the-architect). Its base is `2cf461e2fed1b95d9fd1e7ba74c10d4d8b1c685d`.

PR [#479](https://github.com/jackin-project/jackin-the-architect/pull/479) is a draft on `fix/architect-role-manifest-v1alpha7`. Its exact head is `0592d0deeaeaa5b785fa67a43d23d3b627552720`.

The change updates only `jackin.role.toml`. It changes the manifest from v1alpha5 to v1alpha7 and removes six obsolete Claude and OpenCode provider tables. Static validation and exact-head Sol review PASS.

DCO, Actionlint, Plan, Required, and Sonar pass at that head. Publish baseline is skipped by its main-only condition. This is not merge approval.

`jackin-role validate`, repository validation, and the live role route remain NOT RUN. Validation awaits reviewed MBX. Maintained CI support for real roles belongs to the generator.

## MBX provenance

The activation worker reports immutable MBX `1.21.0` release commit `201b9df3d18e8e96831bee631035f6b7c7ae20e0` and GNU checksum `1ed3fd18da0decc106a6242d1b724e6a4b8d0f6173b8abe4d4d68c929ed47120`.

No local provenance transcript was saved. Artifact installation and activation remain IN PROGRESS. Security review must finish before compilation.

## Review owners

| Owner | Work | State |
|---|---|---|
| `preflight_security_review` | Initial security gate | Initial review complete; follow-up pending |
| `unprivileged_exec_design` | Execution boundary | IN PROGRESS |
| `codex_schema_runtime` | Agent settings confirmation | IN PROGRESS |
| `branches` | Fetch passed; full diff review | IN PROGRESS |
| `execution_crosscheck` | Independent gate crosscheck | Complete; implementation acceptance NOT RUN |

The `architect_manifest_fix` owner supplied the exact-head review. The `velnor_recon` owners supplied generator history and workflow evidence.

See [checklist](checklist.md), [build results](build-results.md), and [Debian results](debian-results.md).
