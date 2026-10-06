# Branching

All new features and bug fixes belong on a dedicated branch. Never commit directly to `main`.

- Create a branch from `main` before starting work.
- Use a prefix matching the change type: `feature/`, `fix/`, `refactor/`, or `chore/`.
- Keep branch names short, lowercase, and hyphen-separated.
- Merge back to `main` through a pull request after review.

## Stay On The Active Branch

Never create a new branch when an existing feature branch or open PR is already in scope for the session.

- At session start, check `git branch --show-current` and `gh pr list --head <current-branch>`. If there is an open PR, all work goes on that branch for the entire session.
- If the current branch is `main`, work must not be committed there. Before making any change, choose a short, lowercase, hyphen-separated branch name following the prefix rules above, then ask the operator to confirm: "This is on `main`. I suggest `<branch-name>` for this work. Should I create it?" Do not proceed until the operator confirms that name or provides a replacement.
- If work appears to belong on a different branch from the active one, stop and ask whether to create a new branch or keep working on the current branch. Default to staying on the active branch unless the operator says otherwise.
- Never push to a remote branch other than the branch the local work should update. If the local branch name differs from the remote PR branch, use `git push origin HEAD:<remote-branch-name>` instead of creating a new remote branch.

## Sync With Main

When bringing `main` into an active PR branch, use a normal merge commit by default:

```sh
git fetch origin main
git merge --no-ff origin/main -m "chore(merge): sync main into <branch>"
git push
```

This preserves review history and avoids a force-push cycle. Do not rebase, amend, squash, or otherwise rewrite the branch unless the operator explicitly approves that rewrite for the branch.

If the merge has conflicts, resolve them in the merge commit and keep the subject conventional. Use `chore(merge): sync main into <branch>` unless a more specific non-release maintenance type is clearly better.

## Force Pushes

Force pushes rewrite shared review history. Agents must never run `git push --force`, `git push --force-with-lease`, `git push +<ref>`, or an equivalent history-rewriting GitHub operation unless the operator explicitly approves that force push in the current conversation for the specific branch or pull request.

Normal pushes adding new commits to a branch are fine without extra approval. If a history rewrite seems useful, such as amending a missed DCO sign-off, rebasing an open PR, or squashing review commits before merge, ask first and name the branch or PR plus the reason. Prefer a normal follow-up commit unless the operator asks for rewritten history.

This rule does not prohibit `git fetch -f` in verification recipes. A forced fetch updates only a local remote-tracking ref; it does not rewrite GitHub's remote branch.
