# jackin-dev

The `jackin-dev` PR-verification and contributor tooling binary — installed locally and used to prepare an isolated checkout for reviewing/verifying a PR (`jackin-dev pr sync`, `jackin-dev pr path`, env setup). A developer/agent tool, not part of the jackin❯ runtime.

## What this crate owns

- The `jackin-dev` CLI (`main`): PR-sync, PR-path resolution, env-script emission, and related contributor workflows.
- Its own unit tests (`tests`).

## Architecture tier and allowed dependencies

**Standalone binary (tooling).** No workspace dependencies — it bootstraps an environment *before* the workspace is necessarily built, so it must not depend on jackin❯ runtime crates. Third-party deps are kept minimal.

## Structure

- `src/main.rs` — the `jackin-dev` CLI
- `src/tests.rs` — unit tests

## Public API

The `jackin-dev` binary surface (`pr sync <N>`, `pr path <N>`, etc.) documented in `.github/AGENTS.md` and the PR-verification workflow. Not a library.

## How to verify

```sh
cargo nextest run -p jackin-dev
cargo clippy -p jackin-dev --all-targets -- -D warnings
jackin-dev --version
```

See [../AGENTS.md](../AGENTS.md) for workspace-wide Rust rules and [../../AGENTS.md](../../AGENTS.md) for repo rules.
