# AGENTS.md — jackin-pr-trailers

Standalone CLI that extracts git trailers from a PR's commits for squash-merge attribution. A developer/agent tool for the merge process — not part of the jackin❯ runtime.

## Hard rules (this crate)

- **Tier & dependencies:** standalone binary. No workspace dependencies. Keep it minimal — `clap` + `anyhow` + std; delegate trailer parsing to `git interpret-trailers`, do not hand-roll a trailer parser.
- **Keep `README.md` current:** update it when the CLI surface, behavior, or integration flow changes (see `crates/AGENTS.md`).
- **`Signed-off-by` first, then `Co-authored-by`, then others in first-appearance order.** Do not reorder without updating the squash-merge guidance in `.github/AGENTS.md`.
- **Use `gh` for PRs; fall back to local `git log` only when no PR exists.** Do not add network/GitHub logic beyond what `gh` provides.

## What lives here vs elsewhere

- This crate owns: trailer extraction, dedup, ordering, the `--pr`/`--body-file`/`--repo` CLI.
- Squash-merge *workflow* and DCO/attribution policy live in `.github/AGENTS.md` and `CONTRIBUTING.md`.

Workspace rules: [../AGENTS.md](../AGENTS.md). Repo rules: [../../AGENTS.md](../../AGENTS.md).
