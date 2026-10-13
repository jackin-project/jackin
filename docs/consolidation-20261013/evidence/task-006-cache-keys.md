# TASK-006 step 5 — semantic cache-key reuse

**ABSENT**

This note does not approve TASK-006. The one-minute warm-run target was not measured. Cargo was not run. The integration worktree was not edited.

Integration tree: `/Users/donbeave/Projects/jackin-project/wt-integration-20261013`, branch `integration/branch-consolidation-20261013`, HEAD `274665c0f5f06061920326a76a6ec0007deb1f36`. Policy tip compared: `/Users/donbeave/Projects/jackin-project/wt-baseline-P` at `fba69d60c2373156dc0dcecee8e085c83776966f`. `.github-gen` is absent. The only workflow is `.github/workflows/ci.yml`. `TESTING.md` cites `.github/workflows/ci-pr.yml` and `.github/ci/project.toml`; neither file is in this tree.

These blobs match the policy tip exactly: `crates/tools/jackin-xtask/src/affected_crates.rs`, `ci_result.rs`, `ci_target.rs`, `ci_route.rs`, `ci_audit.rs`, `ci_stage.rs`, `.github/workflows/ci.yml`, `TESTING.md`, `.velnor/config.toml`. There is no stricter cache-key gate on `fba69d60` to port.

## What CI reuses

The generated workflow restores Cargo sources with a prefix fallback. The primary key is the lockfile hash only. A prefix hit that still satisfies `cargo metadata --locked --offline` is logged as a sources hit and skips fetch. A prefix miss downloads in that same job. The same restore-and-fetch pair is repeated on the plan job and on each crate job (`restore-keys:` occurs 28 times). That is not an offline fan-out from one warmup owner.

```yaml
key: velnor-v1-sources-x86_64-unknown-linux-gnu-1.98.1-${{hashFiles('Cargo.lock')}}
restore-keys: velnor-v1-sources-x86_64-unknown-linux-gnu-1.98.1-
```

`.github/workflows/ci.yml` lines 95–97 and 109. The fetch script treats a successful offline metadata check as a hit even when the restored key was the prefix.

`ACTIONS_CACHE_MODE` is `write` on `push` and `read` otherwise, at 27 sites (first at line 429). Every push crate job that reaches the Mr. Boxington object cache can write. That is not one canonical writer.

The workflow does not call `affected-crates`, `ci-route`, `ci-result`, `ci-target`, or `ci-audit`. `.velnor/config.toml` has no cache-key, forward-closure, or canonical-writer fields. The plan job regenerates `.github` with `velnor-actions` and `diff`s it, so a hand edit of `ci.yml` is not a durable gate.

`sccache`, `RUSTC_WRAPPER`, and `SCCACHE_GHA_ENABLED` are absent from `ci.yml`. `assert_no_sccache_env` in `crates/tools/jackin-xtask/src/ci/tests.rs` checks the generated workflow and the installer prefix. Do not restore them. Task-scoped `run_with_sccache` is not in this tree.

## Pieces that are not the CI gate

`WorkspaceGraph::cache_keys` (`affected_crates.rs`, from line 333) hashes the resolved forward closure: workspace git object ids, external package ids, enabled features, the workspace manifest contract, and `rust-toolchain.toml` / `.cargo` object ids. `affected_with_dependencies` (from line 257) walks reverse dependents and fails safe to every crate on workspace-wide or unknown inputs. Nothing in `.github` or `.velnor` passes those keys to `actions/cache` or to job selection. The workflow jobs are static.

`has_exact_local_target` (`ci_target.rs`, line 204) refuses a local exact hit when `.ci-source-key` disagrees. `ci_target::find` (line 282) still looks up `ci-crate-target-…-{package}` and a `-warmup` alias without putting `cache_key` in the artifact name, and it emits `known_exact: false` unless a supplied map says otherwise. `restore` extracts that archive either way. The workflow does not call this command.

`ci_audit::run` bails on `expect_clean` when `clean_gate_total` is nonzero (`ci_audit.rs`, lines 252–254), which counts prefix restores, cache misses, downloads, and non-exact reported layers. No workflow step and no Velnor task invokes `ci-audit` or `--expect-clean`. An exact warm run is therefore not failed by CI for a forbidden download or recompile. That absence is not a timing result.

Push-head ledger qualification stays as ported in `86693632`: `build_rollup` requires `DenominatorBoundary::PushHead`, and `require_qualified` rejects `Bootstrap`. This note does not redo that. It is not a cache-key binding.

## Nearest function

`ci_result::find` is the nearest reuse decision. An empty `cache_key` skips the forward-dependency artifact name. A miss of that name falls through to `sha_name`, which carries `source_sha` and omits `cache_key`. `result_names` substitutes a fixed contract digest when `common_contract_key` is empty. `hit` is then true for any non-expired artifact of either name. `ci_route::run` treats that hit as reuse and does not schedule the crate. The generated workflow never calls either command.

```rust
fn find(args: FindArgs) -> Result<()> {
    let names = result_names(&args);
    let artifact = if args.cache_key.is_empty() {
        Lookup::Missing
    } else {
        lookup_artifact(&args.repository, args.run_id, &names.name, &args.package)
    };
    let artifact = match artifact {
        Lookup::Missing => lookup_artifact(
            &args.repository,
            args.run_id,
            &names.sha_name,
            &args.package,
        ),
        result => result,
    };
    // ...
    let hit = artifact_id.is_some() && args.package != args.refresh_package;
```

`crates/tools/jackin-xtask/src/ci_result.rs`, `find` at line 128 and `result_names` at line 164.

## Follow-up

No workflow or `ci_result` edit was made. Deleting `restore-keys` or the `sha_name` fallback would not create the forward-closure key, the single writer, and stale-head rejection together. `.github/workflows/ci.yml` was not hand-edited: the plan job regenerates it and diffs the result, and the generator inputs in this tree do not name those cache fields. Warm timing remains unmeasured.

## Why this is not a port

Stopping the `sha_name` fallback, or deleting `restore-keys`, does not bind CI reuse to forward-dependency keys, reverse-dependency selection, one canonical writer, and stale-head rejection together. Both edits are already the behavior of this tree and of `fba69d60`. The workflow generator source is not in the tree, and `.velnor/config.toml` does not name those inputs. No safe port of the step 5 fail-closed contract is present to copy.
