# Workflow Rules

Apply these rules to every workflow under this directory. They define the repository's GitHub Actions policy; do not duplicate them in individual workflows.

## Toolchain

- Install every CI tool with `jdx/mise-action`; `mise.toml`, `mise.lock`, and `rust-toolchain.toml` are the version sources. Do not add language-specific setup actions.
- Add Rust components and cross-compilation targets after mise. Use `cargo:<crate>` keys directly in `mise.toml` and `install_args` for Cargo tools.
- The MSRV job reads `Cargo.toml`'s `rust-version` at runtime; it never hardcodes a version.

## Caches

- Add or widen a cache only after measuring cache usage and workflow timing. Specify its owner, invalidation inputs, restore source, and write policy.
- `main` owns durable cache state. PRs may restore default-branch state but should not write duplicate caches without a measured repeated-PR benefit.
- Every `jdx/mise-action` reachable from `pull_request` sets `cache_save: ${{ github.event_name != 'pull_request' }}`.
- The shared Cargo registry cache verifies restored fallback content with `cargo fetch --locked --offline` for the workspace and fuzz manifests; fetch and save only when that verification fails.
- `main` writes the Construct registry BuildKit cache with `mode=max`; PR-scoped GHA BuildKit caches use `mode=min`.
- BuildKit commands must stream inherited stdout and stderr with `--progress=plain`; GitHub Actions logs must show cache resolution and layer progress while an image build is running.

## Tokens and Scope

- Use `${{ github.token }}` for same-repository reads and Actions cache access. Reserve `${{ secrets.GH_READONLY_TOKEN }}` for cross-repository reads, including `jdx/mise-action` downloads and private sibling repositories.
- Declare third-party CLI selection variables such as `BUILDX_BUILDER`, `GH_TOKEN`, and `RUSTUP_TOOLCHAIN` at job scope. Workflow-level `env` is only for in-house naming with no tool side effect.

## Runner Capacity

- PR CI and Construct bootstrap jobs (path classification and lane selection) run on Velnor so hosted-runner admission cannot delay the matrix decision. Keep them dependency-free and short.
- Construct PR builds use the Velnor runner lane, where the persistent BuildKit cache makes the two platform jobs fast. Keep the high-fan-out Rust CI matrix on GitHub-hosted runners unless Velnor capacity is scaled to match it; otherwise a small self-hosted pool serializes one PR's matrix.

## Publishing and Parity

- Derive one `is_publish` value and gate every external write on `main`. Feature branches may build and test but never publish.
- Cancel stale runs per publish stream. Serialize only the job that writes a shared external resource, with a non-cancelling job-level concurrency group.
- A green PR must predict a green `main`: provide a read-only PR equivalent for every main-only invariant, and do not make required checks depend on transient third-party network state.
- If a change affects a push-only, main-only, dispatch-only, or `workflow_run` job, dispatch it against the PR branch with `gh workflow run` before merging.
