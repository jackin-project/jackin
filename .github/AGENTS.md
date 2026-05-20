# GitHub Actions Workflow Authoring Rules

Rules for writing and maintaining workflows under `.github/workflows/` and composite actions under `.github/actions/`. These apply to all contributors — human and AI.

## Tool installation: always use mise (hard rule)

**All tools — in CI and locally — must be installed through mise. Never add `actions-rust-lang/setup-rust-toolchain`, `dtolnay/rust-toolchain`, `actions/setup-node`, `actions/setup-go`, `actions/setup-python`, or any other language-specific setup action to a workflow.**

`mise.toml` is the single source of truth for tool versions. This gives local development and CI identical environments, one place to bump versions, and one mental model for every contributor and agent.

**In GitHub Actions workflows:**
- Use `jdx/mise-action` for every tool installation — Rust, Node, Bun, Zig, cargo tools, everything.
- **Rust toolchain version**: channel declared in `rust-toolchain.toml`. mise reads it automatically via `idiomatic_version_file` — no version pin in `install_args` needed. mise does **not** install `components` from `rust-toolchain.toml`; add a `rustup component add <components>` step after mise when a job needs non-default components (e.g. `rustfmt`, `clippy`).
- **Cross-compilation targets**: run `rustup target add <target>` after the mise step; `actions-rust-lang/setup-rust-toolchain`'s `target:` parameter is not available.
- **Cargo-registry tools** (nextest, zigbuild, cross, etc.): pass as `install_args: "cargo:<crate>"`.
- **MSRV override** (the `msrv` CI job only): read the version from `Cargo.toml`'s `rust-version` field at job runtime — never hardcode it. Use `install_args: "rust@${{ steps.msrv.outputs.version }}"` and pin the cargo step with `RUSTUP_TOOLCHAIN: ${{ steps.msrv.outputs.version }}`.
- **Multiple tools in one step**: space-separate in `install_args: "rust zig cargo:cargo-nextest"`. Use a GHA expression when the set is matrix-conditional: `install_args: "${{ matrix.zigbuild && 'rust zig cargo:cargo-zigbuild' || 'rust' }}"`.

**Locally:** `mise install` from the repo root installs every tool at the version CI uses.

## Env-var scope: job level, not workflow level

Environment variables that a third-party CLI reads as a default-selection (`BUILDX_BUILDER`, `DOCKER_BUILDKIT`, `GH_TOKEN`, `RUSTUP_TOOLCHAIN`, `AWS_PROFILE`, etc.) MUST be declared at the **job** level, not the workflow level. Workflow-level `env:` leaks into every job; a job that didn't opt into the corresponding tool setup will fail at runtime when the CLI dereferences a missing resource.

Workflow-level `env:` is reserved for in-house naming (`DIGEST_DIR`, internal labels) where the value has no runtime side-effect on third-party tooling.

See the canonical break in [jackin-project/jackin#266](https://github.com/jackin-project/jackin/pull/266) — `BUILDX_BUILDER` hoisted to workflow level blew up every job that didn't create that builder.

## Publishing steps must gate on `main`

Every workflow that writes to a public registry, tag, release, or Homebrew formula MUST gate the actual publish step on `main`. PRs and dispatches from feature branches may build and test but must never publish. Derive a single `is_publish` boolean once (in the `changes` job), gate every side-effect step on it — do not restate the branch conditions inline at multiple steps.

## Smoke-test push-only jobs before merging

Jobs gated to `push to main`, `workflow_dispatch && ref == main`, or `workflow_run` events do not run on `pull_request`. If a PR modifies such a job, smoke-test it via `gh workflow run --ref <feature-branch>` before merging — PR-time CI will never exercise it.
