# Branching

All new features and bug fixes developed on a dedicated feature branch. Never commit directly to `main`.

- Create a branch from `main` before starting work: `git checkout -b feature/<short-description>`
- Use prefixes matching change type: `feature/`, `fix/`, `refactor/`, `chore/`
- Keep branch names short, lowercase, hyphen-separated
- Merge back to `main` via pull request after review

## Stay on the active branch (agent rule)

**Never create a new branch when an existing feature branch or open PR is already in scope for the session.**

- At session start, check `git branch --show-current` and `gh pr list --head <current-branch>`. If there is an open PR, all work goes on that branch for entire session.
- If current branch is `main`, work must not be committed there. Before making any change, choose a short, lowercase, hyphen-separated branch name following prefix rules above (`feature/`, `fix/`, `refactor/`, or `chore/`), then ask operator to confirm: "This is on `main`. I suggest `<branch-name>` for this work. Should I create it?" Do not proceed until operator confirms that name or provides a replacement.
- If you believe work belongs on a *different* branch from active one, stop and ask: "This feels like it belongs on a separate branch — should I create one, or keep it on `<current-branch>`?" Default to staying on active branch unless operator says otherwise.
- Never push to a remote branch other than the one active local branch should track. If local branch name differs from remote PR branch (e.g. local `pr-435` vs remote `fix/capsule-agent-wheel-scroll`), resolve tracking with `git push origin HEAD:<remote-branch-name>` — do not create a new remote branch.
- If you accidentally create a wrong remote branch, delete it with `git push origin --delete <wrong-branch>` immediately after correcting the push.

## Syncing with main: always rebase, never merge

When a feature branch needs new commits from `main`, always rebase — never a merge commit:

```bash
git fetch origin
git rebase origin/main
git push --force-with-lease origin <branch>   # force-push requires operator approval
```

A merge commit (`git merge main`) creates a merge commit in PR history pulling in main's commits as branch commits. Causes DCO, review-history, and squash-merge problems: Renovate and other bot commits appear in PR diff and must be signed off separately.

Rebase replays only branch's unique commits on top of updated main, keeping PR history clean and DCO check scoped to work branch author actually produced.

Force-push after rebase still requires operator approval per rule below.

## Force pushes

Force pushes rewrite shared review history. Agents must never run `git push --force`, `git push --force-with-lease`, or any equivalent history-rewriting GitHub operation unless operator explicitly approved that force push in current conversation for specific branch or pull request.

Normal pushes adding new commits to an agent branch are fine without extra approval. If a history rewrite seems useful — for example, amending a missed DCO sign-off, rebasing an open PR, or squashing review commits before merge — ask first and name branch/PR plus reason. Prefer a normal follow-up commit unless operator asks for rewritten history.

This rule does not prohibit `git fetch -f` in verification recipes. Forced fetch updates only local remote-tracking ref; does not rewrite GitHub's remote branch.
