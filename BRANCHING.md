# Branching

All new features and bug fixes must be developed on a dedicated feature branch.
Never commit directly to `main`.

- Create a branch from `main` before starting work: `git checkout -b feature/<short-description>`
- Use prefixes that match the type of change: `feature/`, `fix/`, `refactor/`, `chore/`
- Keep branch names short, lowercase, and hyphen-separated
- Merge back to `main` via pull request after review

## Force pushes

Force pushes rewrite shared review history. Agents must never run
`git push --force`, `git push --force-with-lease`, or any equivalent
history-rewriting GitHub operation unless the operator has explicitly approved
that force push in the current conversation for the specific branch or pull
request.

Normal pushes that add new commits to an agent branch are fine without extra
approval. If a history rewrite seems useful - for example, amending a missed
DCO sign-off, rebasing an open PR, or squashing review commits before merge -
ask first and name the branch/PR plus the reason. Prefer a normal follow-up
commit unless the operator asks for rewritten history.

This rule does not prohibit `git fetch -f` in verification recipes. Forced
fetch updates only the local remote-tracking ref; it does not rewrite GitHub's
remote branch.
