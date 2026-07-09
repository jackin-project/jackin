# AGENTS.md — jackin-pr-trailers

Standalone CLI that extracts git trailers from a PR's commits for squash-merge attribution.

## Rules (this crate)

- Trailer order is fixed: `Signed-off-by` first, then `Co-authored-by`, then others in first-appearance order. Do not reorder without updating the squash-merge guidance in `.github/AGENTS.md`.
- Use `gh` for PRs; fall back to local `git log` only when no PR exists. Delegate trailer parsing to `git interpret-trailers` — do not hand-roll a parser.

Workspace rules: [../AGENTS.md](../AGENTS.md). Repo rules: [../../AGENTS.md](../../AGENTS.md).
