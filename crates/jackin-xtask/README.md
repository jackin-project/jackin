# jackin-xtask

The workspace's `cargo xtask` automation — CI lanes, lint gates, docs checks, release/construct helpers, schema checks, and PR tooling. The single entry point for "run the project's checks locally": `cargo xtask ci`, `cargo xtask ci --fast`, `cargo xtask ci --e2e`.

## What this crate owns

- CI orchestration (`ci`) and the lint gates (`lint` — file-size budget, and the lanes planned under it).
- Docs checks (`docs` — repo-links), schema checks (`schema`), profile/feature matrix (`profile_matrix`), and the agent-file symlink gate (`agent_files`).
- Architecture/structure tooling (`arch`), test-layout gate (`test_layout`), PTY fixture extraction (`pty_fixture`), construct helpers (`construct`), release verification (`release_verify`), and PR tooling (`pr`).

## Architecture tier and allowed dependencies

**Build/CI tooling (xtask).** No workspace dependencies — it inspects the workspace from the outside (files, `cargo metadata`, running cargo) and must not link against runtime crates. Runs on the host toolchain only.

## Structure

- `src/main.rs` — the `cargo xtask` dispatcher
- `src/ci.rs` / `src/ci/`, `src/lint.rs` / `src/lint/` — CI orchestration + lint gates
- `src/docs.rs`, `src/schema.rs`, `src/test_layout.rs` / `src/test_layout/`, `src/agent_files.rs` — docs/schema/layout/agent-file gates
- `src/arch.rs`, `src/profile_matrix.rs`, `src/pty_fixture.rs` / `src/pty_fixture/`, `src/construct.rs`, `src/release_verify.rs`, `src/pr.rs` — architecture, profiling, fixtures, construct, release, PR tooling

## Public API

The `cargo xtask <lane>` CLI. Merge-readiness is `cargo xtask ci` (or `--fast` / `--e2e`). New checks are added as lanes here so they are discoverable from one command.

## How to verify

```sh
cargo nextest run -p jackin-xtask
cargo clippy -p jackin-xtask --all-targets -- -D warnings
cargo xtask ci --fast
```

See [../AGENTS.md](../AGENTS.md) for workspace-wide Rust rules and [../../AGENTS.md](../../AGENTS.md) for repo rules.
