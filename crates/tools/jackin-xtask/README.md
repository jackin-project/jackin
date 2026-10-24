# jackin-xtask

Workspace automation for CI, lints, docs, releases, schemas, and PRs. Merge-readiness entry points are `mise exec -- mbx +1.99.1 xtask ci`, `mise exec -- mbx +1.99.1 xtask ci --fast`, and `mise exec -- mbx +1.99.1 xtask ci --e2e`.

## What this crate owns

- CI orchestration (`ci`) and the lint gates (`lint` — file-size budget, and the lanes planned under it).
- Docs checks (`docs` — repo-links, brand prose, spec↔test citations), schema checks (`schema`), profile/feature matrix (`profile_matrix`), and the agent-file symlink gate (`agent_files`, including per-crate README presence).
- Architecture/structure tooling (`arch`), test-layout gate (`layout_lint`), PTY fixture extraction (`pty_fixture`), construct helpers (`construct`), unsigned release packaging (`release_package`, `release_archive`, `release_sbom`), and PR tooling (`pr`).

## Architecture tier and allowed dependencies

**Build/CI tooling (xtask).** It inspects the workspace from the outside (files, `cargo metadata`, running cargo) and uses the foundational `jackin-process` transport for subprocess execution. It must not link against runtime crates. Runs on the host toolchain only.

## Structure

| Module | Owns | Tests |
|---|---|---|
| [`main.rs`](src/main.rs) | MBX `xtask` dispatcher | — |
| [`lib.rs`](src/lib.rs) · [`workflow_yaml.rs`](src/workflow_yaml.rs) | One bounded workflow parser shared by the binary and integration checks | [`tests.rs`](src/workflow_yaml/tests.rs), run once in the library |
| [`ci.rs`](src/ci.rs) | CI orchestration | [`tests.rs`](src/ci/tests.rs) |
| [`ci_audit.rs`](src/ci_audit.rs) · [`ci_cargo_audit.rs`](src/ci_cargo_audit.rs) · [`ci_target.rs`](src/ci_target.rs) | workflow audit, advisory-cache selection, and target transport | sibling `tests.rs` files |
| [`ci_doc_examples.rs`](src/ci_doc_examples.rs) | nextest-only documentation-example gate | [`tests.rs`](src/ci_doc_examples/tests.rs) |
| [`lint.rs`](src/lint.rs) | file-size lint gate (adapter; budgets also in `ratchet.toml`) | [`tests.rs`](src/lint/tests.rs) |
| [`ratchet.rs`](src/ratchet.rs) | unified shrink-only ratchet engine (`lint ratchet`) | [`tests.rs`](src/ratchet/tests.rs) |
| [`agent_files.rs`](src/agent_files.rs) | agent-file symlink gate (`--format human\|json\|github`) | [`tests.rs`](src/agent_files/tests.rs) |
| [`report.rs`](src/report.rs) | shared gate reporter (human/json/github) | [`tests.rs`](src/report/tests.rs) |
| [`agent_links.rs`](src/agent_links.rs) | no-cross-ref gate (README/AGENTS) | [`tests.rs`](src/agent_links/tests.rs) |
| [`container_paths_gate.rs`](src/container_paths_gate.rs) | container-path gate | [`tests.rs`](src/container_paths_gate/tests.rs) |
| [`suppressions.rs`](src/suppressions.rs) | lint-suppression gate | [`tests.rs`](src/suppressions/tests.rs) |
| [`arch.rs`](src/arch.rs) | tier-graph dependency-direction gate (`TIERS` table; prod edges must descend; dev-cycle allowlist) | [`tests.rs`](src/arch/tests.rs) |
| [`readme_freshness.rs`](src/readme_freshness.rs) | structural src change ⇒ README same-PR gate | [`tests.rs`](src/readme_freshness/tests.rs) |
|  [`layout_lint.rs`](src/layout_lint.rs) | test-layout gate | [`tests.rs`](src/layout_lint/tests.rs) |
| [`schema.rs`](src/schema.rs) | schema check | [`tests.rs`](src/schema/tests.rs) |
| [`docs.rs`](src/docs.rs) | docs repo-links / brand / specs / roadmap / research and semantic CI cache contracts | [`tests.rs`](src/docs/tests.rs), contract/brand/specs unit tests |
| [`telemetry_registry.rs`](src/telemetry_registry.rs) | closed-registry Weaver validation and namespace/privacy gates | [`tests.rs`](src/telemetry_registry/tests.rs) |
| [`telemetry_bench.rs`](src/telemetry_bench.rs) | telemetry performance capture and 5% comparison gate | [`tests.rs`](src/telemetry_bench/tests.rs) |
| [`pr.rs`](src/pr.rs) | PR tooling | [`tests.rs`](src/pr/tests.rs) |
| [`profile_matrix.rs`](src/profile_matrix.rs) | feature-profile matrix | — |
| [`pty_fixture.rs`](src/pty_fixture.rs) | PTY fixture extraction | [`tests.rs`](src/pty_fixture/tests.rs) |
| [`construct.rs`](src/construct.rs) | construct image helpers | [`tests.rs`](src/construct/tests.rs) |
| [`release_package.rs`](src/release_package.rs) · [`release_package/`](src/release_package) | Native channel payload selection, handoff and assembly | [`tests.rs`](src/release_package/tests.rs) |
| [`release_archive.rs`](src/release_archive.rs) · [`release_sbom.rs`](src/release_sbom.rs) | Unsigned product archives and their build evidence | [`tests.rs`](src/release_archive/tests.rs) |
| [`health.rs`](src/health.rs) | report-only code-health dashboard (Phase 0) | [`tests.rs`](src/health/tests.rs) |
| [`fs_util.rs`](src/fs_util.rs) | deterministic `read_dir_sorted` for gate code (plan 027) | [`tests.rs`](src/fs_util/tests.rs) |
| [`desktop.rs`](src/desktop.rs) | jackin❯ desktop assembly: bindings, XCFramework, build/verify/run, sign-notarize, release-state, bootstrap-secrets | [`tests.rs`](src/desktop/tests.rs) |

## Public API

The `mise exec -- mbx +1.99.1 xtask <lane>` CLI. Merge-readiness is `mise exec -- mbx +1.99.1 xtask ci` (or `--fast` / `--e2e`). New checks are added as lanes here so they are discoverable from one command.

`mise exec -- mbx +1.99.1 xtask lint ratchet` checks every configured family. CI jobs that own one
artifact use repeatable `--only <family>` arguments so they do not measure
unrelated families or launch nested build work. Artifact-backed families skip
when their artifact is absent; the job that produces an artifact must run the
matching scoped ratchet after generation.

## Native release packaging

`mise run release:package` consumes Velnor's `velnor.release-task.v1` handoff. Leaves build four CLI and two Linux capsule payloads; assembly validates captured archives, SBOMs, and the capsule manifest without rebuilding. Separate jobs sign and publish immutable previews by SHA and stable versions. Builds receive no publishing credentials.

## How to verify

```sh
mise exec -- mbx +1.99.1 nextest run -p jackin-xtask
mise exec -- mbx +1.99.1 clippy -p jackin-xtask --all-targets -- -D warnings
mise exec -- mbx +1.99.1 xtask docs brand
mise exec -- mbx +1.99.1 xtask docs specs
mise exec -- mbx +1.99.1 xtask lint agents
mise exec -- mbx +1.99.1 xtask lint agents --format json
mise exec -- mbx +1.99.1 xtask lint files --format json
mise exec -- mbx +1.99.1 xtask ci --fast
```
