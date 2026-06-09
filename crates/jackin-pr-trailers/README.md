# jackin-pr-trailers

Small CLI helper to extract git trailers (Signed-off-by, Co-authored-by, and any other `Key: Value` or `Key #value` trailers) from all commits in a GitHub PR.

## Purpose

When performing squash merges (especially for PRs that mix human and agent commits), the final squash commit body should include the trailers that were present in the individual commits inside the PR. This preserves attribution history.

This tool automates the extraction and deduplication so the merge process can reliably append them.

## Usage

```sh
# Use cargo
cargo build -p jackin-pr-trailers --release

# Extract trailers for a PR
jackin-pr-trailers --pr 550

# For a different repo
jackin-pr-trailers --pr 123 --repo some-org/some-repo
```

Output is the list of unique trailers, with `Signed-off-by` first, then `Co-authored-by`, then others (in order of first appearance).

## Integration in squash merge flow

See the guidance in `.github/AGENTS.md` under "PR squash merge messages" and the "Trailer extraction helper" section.

Typical pattern:

```sh
BODY_FILE=$(mktemp)
# write prose summary of the PR ...
jackin-pr-trailers --pr "$PR" >> "$BODY_FILE"
gh pr merge "$PR" --squash --body-file "$BODY_FILE"
rm "$BODY_FILE"
```

## Implementation

- `src/main.rs`: CLI with clap, calls `gh pr view --json commits`, parses each commit's messageHeadline + messageBody.
- `extract_trailers`: Simple robust parser that walks from the end of the message, collects consecutive trailer lines, deduplicates.
- Minimal dependencies, includes unit tests for the parser.

This is a developer/ agent tool for the merge process, not part of the main jackin runtime.
