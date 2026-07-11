# jackin-xtask

The workspace's `cargo xtask` automation — CI lanes, lint gates, docs checks, release/construct helpers, schema checks, and PR tooling. The single entry point for "run the project's checks locally": `cargo xtask ci`, `cargo xtask ci --fast`, `cargo xtask ci --e2e`.

## What this crate owns

- CI orchestration (`ci`) and the lint gates (`lint` — file-size budget, and the lanes planned under it).
- Docs checks (`docs` — repo-links, brand prose, spec↔test citations), schema checks (`schema`), profile/feature matrix (`profile_matrix`), and the agent-file symlink gate (`agent_files`, including per-crate README presence).
- Architecture/structure tooling (`arch`), test-layout gate (`test_layout`), PTY fixture extraction (`pty_fixture`), construct helpers (`construct`), release verification (`release_verify`), and PR tooling (`pr`).

## Architecture tier and allowed dependencies

**Build/CI tooling (xtask).** No workspace dependencies — it inspects the workspace from the outside (files, `cargo metadata`, running cargo) and must not link against runtime crates. Runs on the host toolchain only.

## Structure

| Module | Owns | Tests |
|---|---|---|
| [`main.rs`](src/main.rs) | `cargo xtask` dispatcher | — |
| [`ci.rs`](src/ci.rs) · [`ci/`](src/ci) | CI orchestration | [`tests.rs`](src/ci/tests.rs) |
| [`lint.rs`](src/lint.rs) · [`lint/`](src/lint) | file-size lint gate (`--format human\|json\|github`) | [`tests.rs`](src/lint/tests.rs) |
| [`agent_files.rs`](src/agent_files.rs) · [`agent_files/`](src/agent_files) | agent-file symlink gate (`--format human\|json\|github`) | [`tests.rs`](src/agent_files/tests.rs) |
| [`report.rs`](src/report.rs) · [`report/`](src/report) | shared gate reporter (human/json/github) | [`tests.rs`](src/report/tests.rs) |
| [`agent_links.rs`](src/agent_links.rs) · [`agent_links/`](src/agent_links) | no-cross-ref gate (README/AGENTS) | [`tests.rs`](src/agent_links/tests.rs) |
| [`container_paths_gate.rs`](src/container_paths_gate.rs) · [`container_paths_gate/`](src/container_paths_gate) | residual `/jackin` literal shrink-only gate | [`tests.rs`](src/container_paths_gate/tests.rs) |
| [`arch.rs`](src/arch.rs) · [`arch/`](src/arch) | tier-graph dependency-direction gate (`TIERS` table; prod edges must descend; dev-cycle allowlist) | [`tests.rs`](src/arch/tests.rs) |
| [`test_layout.rs`](src/test_layout.rs) · [`test_layout/`](src/test_layout) | test-layout gate | [`tests.rs`](src/test_layout/tests.rs) |
| [`schema.rs`](src/schema.rs) · [`schema/`](src/schema) | schema check | [`tests.rs`](src/schema/tests.rs) |
| [`docs.rs`](src/docs.rs) · [`docs/`](src/docs) | docs repo-links / brand / specs / roadmap / research | [`tests.rs`](src/docs/tests.rs), brand/specs unit tests |
| [`pr.rs`](src/pr.rs) · [`pr/`](src/pr) | PR tooling | [`tests.rs`](src/pr/tests.rs) |
| [`profile_matrix.rs`](src/profile_matrix.rs) | feature-profile matrix | — |
| [`pty_fixture.rs`](src/pty_fixture.rs) · [`pty_fixture/`](src/pty_fixture) | PTY fixture extraction | [`tests.rs`](src/pty_fixture/tests.rs) |
| [`construct.rs`](src/construct.rs) · [`construct/`](src/construct) | construct image helpers | [`tests.rs`](src/construct/tests.rs) |
| [`release_verify.rs`](src/release_verify.rs) · [`release_verify/`](src/release_verify) | release verification | [`tests.rs`](src/release_verify/tests.rs) |
| [`health.rs`](src/health.rs) · [`health/`](src/health) | report-only code-health dashboard (Phase 0) | [`tests.rs`](src/health/tests.rs) |

## Public API

The `cargo xtask <lane>` CLI. Merge-readiness is `cargo xtask ci` (or `--fast` / `--e2e`). New checks are added as lanes here so they are discoverable from one command.

## How to verify

```sh
cargo nextest run -p jackin-xtask
cargo clippy -p jackin-xtask --all-targets -- -D warnings
cargo xtask docs brand
cargo xtask docs specs
cargo xtask lint agents
cargo xtask lint agents --format json
cargo xtask lint files --format json
cargo xtask ci --fast
```

