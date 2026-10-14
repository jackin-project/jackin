# TASK-016 gap check

Not an approval. No product edits, no cargo, no commit.

Integration `7a05f3aebda4a73e183b665702f00bb7a38ce36f` (`7a05f3ae` is HEAD). Policy P `fba69d60c2373156dc0dcecee8e085c83776966f`. Checked-in workflows: `.github/workflows/ci.yml` only. Its workflow name is `CI` (`.github/workflows/ci.yml:2`). The file says it is generated from `.velnor/config.toml` (`.github/workflows/ci.yml:1`).

`native/README.md` is unchanged from P. The false lines below are still the current tree.

Already landed, and not repeated as PORT: `TESTING.md` crate and `ci.yml` path rewrites (`59453729`), citation rewrites in that same commit, MBX-only compiler-cache banners (`c3bb10fa`), and the reported RepoFile scan of 1239 paths with 0 missing. This note did not rerun that scan.

## Verdict

PORT

Three current operating sentences are false on this tree. Historical sections with an explicit historical banner are not in the edit list.

## Command names removed workflows

PORT

`native/README.md:39-40` is the current forward-lane exit, not a historical table. It says the absent Xcode 27 lane is added at `ci-unit-swift.yml`, dispatched via `group-swift` in generated `ci-pr.yml`, then regenerated. Neither file is in the tree. The only checked-in workflow is `.github/workflows/ci.yml`.

`native/Design/UnifiedAgentUsage/SwiftProjectReadiness.md:25` is in the remediation ledger, above the preserved-audit banner at `SwiftProjectReadiness.md:44`. It says current xtask contract tests prove that generated `ci-pr.yml` only delegates the native lane. `crates/tools/jackin-xtask/src/ci/tests.rs:128-134` says that desktop cadence contract test was removed when `desktop-merge.yml`, `ci-pr.yml`, and `ci-unit-swift.yml` were deleted, and that no surviving workflow invokes desktop tasks.

`TESTING.md:384` is a current command: `` `gh workflow run Hygiene` ``. `TESTING.md:366` names Hygiene job `dind-chaos`, `TESTING.md:370` names a `workflow_dispatch` input `chaos_seed`, and `TESTING.md:375` names Hygiene job `dhat-allocation`. `ci.yml` contains none of those names. `59453729` rewrote `ci-pr.yml` path citations and left this command in place.

Not defects, explicit historical banner:

- `native/Design/UnifiedAgentUsage/SwiftProjectSetupAudit.md:1` covers the `ci-pr.yml` / `ci-unit-swift.yml` / `release.yml` sentences at `SwiftProjectSetupAudit.md:57-63` and `SwiftProjectSetupAudit.md:77-80`.
- `SwiftProjectReadiness.md:44` and `SwiftProjectReadiness.md:147` cover the implementation steps at `SwiftProjectReadiness.md:129-131` and the `release.yml:469-479` citation at `SwiftProjectReadiness.md:165`.
- Dated September run ledgers under `plans/cicd-reliability/` name `desktop-merge.yml` as past run evidence. They are not current operator commands.

## Global sccache install presented as current

ALREADY_PRESENT

`docs/content/research/engineering/build-performance/shared-ci-compiler-cache.mdx:8-12` and `docs/content/research/engineering/build-performance/rust-build-cache-hygiene.mdx:8-11` say MBX is the only compiler cache and that the sccache and kache setup below is historical and must not be applied. `brew install sccache` at `rust-build-cache-hygiene.mdx:309` sits under that banner. `mise.toml` has no `sccache` pin. Do not reopen those pages.

## Release workflow claimed to exist

PORT

Docs that describe the current publisher already say it is absent: `docs/content/research/engineering/desktop-verification/commands.mdx:83-85`, `research/agent-usage-platform/08-surfaces-release-tooling.md:105`, and `native/README.md:153` ("Publication | Not automated by a checked-in workflow").

The remediation ledger still states the opposite as closed current evidence. `SwiftProjectReadiness.md:26` says release CI uploads the dSYM and a compressed unstripped `.a` with 90-day retention. `ci.yml` has no `dSYM` upload and there is no `release.yml`.

`crates/tools/jackin-xtask/src/desktop/bootstrap.rs:150` prints `gh workflow run release.yml --ref main -f mode=publish -f lanes=github`. That is product code, not a docs edit. This note does not add it to the edit list and does not approve changing it. Do not hand-edit `.github/workflows/ci.yml` to create that workflow.

## Edit list

1. `native/README.md:39-40`: state that the forward lane is absent and that the only checked-in workflow is `.github/workflows/ci.yml`. Remove `ci-unit-swift.yml` and `ci-pr.yml` as the place to add the lane. Do not hand-edit `ci.yml`.
2. `native/Design/UnifiedAgentUsage/SwiftProjectReadiness.md:25`: delete the claim that current tests prove generated `ci-pr.yml` delegates the native lane. The mise cadence graph can stay; the removed contract is `crates/tools/jackin-xtask/src/ci/tests.rs:128-134`.
3. `native/Design/UnifiedAgentUsage/SwiftProjectReadiness.md:26`: delete the claim that release CI uploads the dSYM and unstripped archive. No checked-in workflow does that.
4. `TESTING.md:366-384`: stop telling the operator to run `gh workflow run Hygiene` and stop naming `dind-chaos`, `chaos_seed`, and `dhat-allocation` as current CI. Do not add a Hygiene workflow.

No other docs edit. This note does not approve TASK-016.
