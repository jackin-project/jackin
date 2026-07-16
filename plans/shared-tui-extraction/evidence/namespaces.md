# Namespace and extraction-tool revalidation

Checked 2026-07-15 during Stage 0.

## Names and owners

- `gh repo view tailrocks/termrock` returned “Could not resolve to a Repository”; the GitHub repository name is available.
- `cargo search termrock --limit 10` and `cargo search termrock-lookbook --limit 10` returned no exact matches; both crates.io names are available.
- Intended GitHub owner: the `tailrocks` organization.
- Intended crates.io owner: the operator's crates.io account. Registry publication is outside this program.
- Trademark disposition: the operator authorized this implementation to proceed with the approved TermRock naming family. This records the project naming decision, not trademark advice or a legal clearance claim.

## History tooling and attribution

`git-filter-repo` revision `31ebad4c8fb3` was verified from the upstream
`newren/git-filter-repo` repository with:

```text
PATH="$PWD/target/tools/git-filter-repo:$PATH" git filter-repo --version
```

Stage 1 will record the literal filter command and retained-path set. Filtering
preserves inherited authors, author/committer timestamps, commit messages, and
file-level SPDX notices. New commits after the recorded provenance boundary are
DCO-signed; inherited commits are not rewritten to add trailers retroactively.
