# Branching

All new features and bug fixes must be developed on a dedicated feature branch.
Never commit directly to `main`.

- Create a branch from `main` before starting work: `git checkout -b feature/<short-description>`
- Use prefixes that match the type of change: `feature/`, `fix/`, `refactor/`, `chore/`
- Keep branch names short, lowercase, and hyphen-separated
- Merge back to `main` via pull request after review

## Stay on the active branch (agent rule)

**Never create a new branch when an existing feature branch or open PR is already in scope for the session.**

- At session start, check `git branch --show-current` and `gh pr list --head <current-branch>`. If there is an open PR, all work goes on that branch for the entire session.
- If the current branch is `main`, work must not be committed there. Before making any change, choose a short, lowercase, hyphen-separated branch name that follows the prefix rules above (`feature/`, `fix/`, `refactor/`, or `chore/`), then ask the operator to confirm it: "This is on `main`. I suggest `<branch-name>` for this work. Should I create it?" Do not proceed until the operator confirms that name or provides a replacement.
- If you believe a piece of work belongs on a *different* branch from the active one, stop and ask: "This feels like it belongs on a separate branch — should I create one, or keep it on `<current-branch>`?" Default to staying on the active branch unless the operator says otherwise.
- Never push to a remote branch other than the one the active local branch should track. If the local branch name differs from the remote PR branch (e.g. local `pr-435` vs remote `fix/capsule-agent-wheel-scroll`), resolve the tracking with `git push origin HEAD:<remote-branch-name>` — do not create a new remote branch.
- If you accidentally create a wrong remote branch, delete it with `git push origin --delete <wrong-branch>` immediately after correcting the push.

## Syncing with main: always rebase, never merge

When a feature branch needs to incorporate new commits from `main`, always
use rebase — never a merge commit:

```bash
git fetch origin
git rebase origin/main
git push --force-with-lease origin <branch>   # force-push requires operator approval
```

A merge commit (`git merge main`) creates a merge commit in the PR history that
pulls in main's commits as branch commits. This causes DCO, review-history, and
squash-merge problems: Renovate and other bot commits appear in the PR diff and
must be signed off separately.

Rebase replays only the branch's unique commits on top of the updated main,
keeping the PR history clean and the DCO check scoped to work the branch author
actually produced.

Force-push after rebase still requires operator approval per the rule below.

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
