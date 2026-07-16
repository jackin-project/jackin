# Workflow Rules

Apply these rules to every workflow under this directory. They define the repository's GitHub Actions policy; do not duplicate them in individual workflows.

## Toolchain

- Install every CI tool with `jdx/mise-action`; `mise.toml`, `mise.lock`, and `rust-toolchain.toml` are the version sources. Do not add language-specific setup actions.
- Add Rust components and cross-compilation targets after mise. Use `cargo:<crate>` keys directly in `mise.toml` and `install_args` for Cargo tools.
- CI uses only the newest pinned stable Rust toolchain. Keep `Cargo.toml`'s `rust-version` aligned with `rust-toolchain.toml`; do not add older-compiler lanes or compatibility caches.

## Caches

- Add or widen a cache only after measuring cache usage and workflow timing. Specify its owner, invalidation inputs, restore source, and write policy.
- `main` owns durable cache state. PRs may restore default-branch state but should not write duplicate caches without a measured repeated-PR benefit.
- Every `jdx/mise-action` sets `cache_save: ${{ github.ref == 'refs/heads/main' }}`. Branches, tags, and PRs restore but never write its cache.
- The shared Cargo registry cache verifies restored fallback content with `cargo fetch --locked --offline` for the workspace and fuzz manifests. A non-main ref fetches only when verification fails and never saves; `main` saves only after such a miss.
- Release-preview target caches restore on every ref but only save on `main`; tagged releases and branch dispatches must not create isolated archive caches.
- `main` writes the Construct registry BuildKit cache with `mode=max`; PR-scoped GHA BuildKit caches use `mode=min`.
- BuildKit commands must stream inherited stdout and stderr with `--progress=plain`; GitHub Actions logs must show cache resolution and layer progress while an image build is running.
- Runner selection and change classification are metadata jobs: cap them at five minutes. Keep build, test, publish, and network-heavy jobs at a measured, explicit timeout.

## Tokens and Scope

- Use `${{ github.token }}` for same-repository reads and Actions cache access. Reserve `${{ secrets.GH_READONLY_TOKEN }}` for cross-repository reads, including `jdx/mise-action` downloads and private sibling repositories.
- Declare third-party CLI selection variables such as `BUILDX_BUILDER`, `GH_TOKEN`, and `RUSTUP_TOOLCHAIN` at job scope. Workflow-level `env` is only for in-house naming with no tool side effect.

## Runner Capacity

- GitHub-hosted runners are the default and required PR path. Every runner-selectable pipeline exposes a `workflow_dispatch` `lanes` choice with `github`, `velnor`, and `both`; omitted input resolves to `github`. `velnor` runs only when explicitly selected, never from automatic PR or push triggers.

## Semantic Boundaries

- Never split one crate's tests, one artifact's bytes, or one conceptual command into numbered shards, batches, jobs, steps, or parts. One affected crate owns one complete test job; one cache artifact is one archive with one publish and one restore operation.
- Split jobs and steps only when they have distinct meaning, ownership, or failure diagnosis. Transport mechanics are not semantic boundaries.
- Improve runtime through result reuse, local or remote caches, prebuilt artifacts, and faster transport. Do not trade readability or independently attributable results for parallel fragments of the same work.

## Publishing and Parity

- Derive one `is_publish` value and gate every external write on `main`. Feature branches may build and test but never publish.
- Cancel stale runs per publish stream. Serialize only the job that writes a shared external resource, with a non-cancelling job-level concurrency group.
- Scope workflow concurrency by ref for validation and branch build-only dispatches. Only `main` publishing runs may share a repository-wide concurrency group.
- A green PR must predict a green `main`: provide a read-only PR equivalent for every main-only invariant, and do not make required checks depend on transient third-party network state.
- If a change affects a push-only, main-only, dispatch-only, or `workflow_run` job, dispatch it against the PR branch with `gh workflow run` before merging.
