# jackin-pr-trailers

Small CLI helper to extract git trailers (Signed-off-by, Co-authored-by, and any other trailer parsed by `git interpret-trailers`) from all commits in a GitHub PR.

## Purpose

When performing squash merges (especially for PRs that mix human and agent commits), the final squash commit body should include the trailers that were present in the individual commits inside the PR. This preserves attribution history.

This tool automates the extraction and deduplication so the merge process can reliably append them.

## Usage

```sh
# Extract trailers for a PR
cargo run -p jackin-pr-trailers -- --pr 550

# For a different repo
cargo run -p jackin-pr-trailers -- --pr 123 --repo some-org/some-repo

# Auto-detect the PR for the current branch, verify local HEAD matches origin/<branch>,
# then extract via gh. Falls back to local git log only when no PR exists.
cargo run -p jackin-pr-trailers -- --body-file "$BODY_FILE"
```

Output is the list of unique trailers, with `Signed-off-by` first, then `Co-authored-by`, then others (in order of first appearance).

## Integration in squash merge flow

See the squash-merge guidance under `.github/` ("PR squash merge messages" and the "Trailer extraction helper" section).

Typical pattern:

```sh
BODY_FILE=$(mktemp)
# write prose summary of the PR ...
cargo run -p jackin-pr-trailers -- --pr "$PR" >> "$BODY_FILE"
gh pr merge "$PR" --squash --body-file "$BODY_FILE"
rm "$BODY_FILE"
```

## Implementation

- `src/main.rs`: CLI with clap, calls `gh pr view --json commits` for PRs and `git log --format=%B%x00` only when no PR exists.
- Trailer parsing is delegated to `git interpret-trailers --parse --only-trailers --unfold`, then deduplicated and ordered.
- Minimal dependencies, includes unit tests for the parser.

This is a developer/ agent tool for the merge process, not part of the main jackin runtime.
