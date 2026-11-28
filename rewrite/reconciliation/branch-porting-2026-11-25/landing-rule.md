# Landing rule (user directive, overrides "open a DRAFT PR, do NOT merge")

The user requires: merge every PR we create. Never leave PRs sitting unmerged. Never leave local changes uncommitted, unpushed or unmerged.

Each agent lands its OWN PRs, end to end:
1. Commit early with `git commit -s` (DCO; the ruleset requires no cryptographic signature) and push over HTTPS: `git -c url."https://github.com/".insteadOf="git@github.com:" push`. Don't leave work uncommitted in a worktree between steps.
2. Once the independent review verdict is APPROVE (after fixes) and G-FAST passes, mark the PR ready (`gh pr ready <n>`).
3. Bring it up to date with main by MERGING origin/main into the branch (no rebase, no force-push). Resolve conflicts, re-run the affected Nextest plus G-FAST (throttled per load-note.md), and push.
4. Wait for the required CI checks (`gh pr checks <n> --watch`). Fix real failures. Never bypass rules, never use `--admin`, never disable checks.
5. Merge with `gh pr merge <n> --squash`, following the repo's merge policy, and confirm the state is MERGED. Then start your next task from fresh origin/main.
6. If a PR truly can't merge (a live proof is blocked, or a required review needs a human), say so explicitly in the PR and your report, with the exact blocker. That is the only allowed reason to leave one open.
Shared contracts: if another open PR touches the same functions, merge main first and re-run tests. The orchestrator settles any conflict between agents.

## UPDATE: single merge queue (the main ruleset requires PRs to be up to date with main before merging)
Every merge puts all other PRs behind main again, so racing them wastes CI. From now on:
- PR OWNERS take their PR to "ready to land": review APPROVE recorded, review threads resolved, G-FAST passed, CI green at least once, `gh pr ready <n>`, everything committed and pushed. Then append ONE line `<pr-number> <owner-task-id>` to scratchpad/merge-queue.txt (`echo "1312 p06" >> .../merge-queue.txt`) and move on to your next task. Don't keep re-syncing your PR with main.
- The LANDER is the only agent that does the final step. It takes queue entries in order, merges origin/main into the branch (no rebase, no force-push), pushes, enables auto-merge (`gh pr merge <n> --squash --auto`), waits until MERGED, then takes the next entry. Trivial conflicts it resolves itself. Non-trivial conflicts go back to the owner by message, and the next entry goes ahead.
- If CI fails on the synced head, the lander tells the owner, and the owner fixes it and re-queues.

## Known load blocker in G-FAST
`jackin-image::jackin_start::jackin_start_applies_only_the_bytes_it_validated` takes about 135s even when run alone and hits the 180s nextest limit under the current load (about 110). If it is the ONLY G-FAST failure AND the PR touches nothing in crates/jackin-image:
- Record G-FAST honestly as "every step passed except this one test, which timed out under load (NOT a pass)".
- Run that test on its own and record the result.
- Queue the PR. The required CI check ("Required") must be green before merge; the lander enforces this.
Never describe this as "G-FAST passed". Any other failure is real and must be fixed.

## USER DIRECTIVES (latest, override everything above)
1. NEVER DEFER. Every review comment and thread (from a bot or a person, on our PRs) is addressed IN THAT PR: either fixed with a commit, or, if it is factually wrong, answered with evidence (file:line, test output). "Out of scope", "follow-up", "pre-existing" and "deferred" are not allowed answers. Pre-existing gaps that a reviewer finds get fixed in the same PR. Resolve each thread after replying.
2. BATCH BRANCHES INTO ONE PR. The lander lands queued work in batches: create `integrate/batch-N` from fresh origin/main, `git merge --no-ff` each ready branch into it in queue order (resolve conflicts keeping main's newer safeguards and each task's invariant), run the affected crates' Nextest plus one throttled G-FAST, push, and open ONE PR. Its body lists every included task: ID, source PR, invariant, red/green test names, reviewer verdict. Wait for CI and threads resolved, squash-merge, then close each source PR with the comment "Landed in #<batch PR> (<merge sha>)". Every unresolved thread on a source PR must be addressed before it goes into a batch.
3. Workers may also stack their paired task onto the same branch/PR (P02+P11, P03+P16, P05+P15, P06+P07), with a separate review for each task. This is preferred over separate PRs.

## EFFORT CAP (user directive)
Never use `--effort xhigh` or `--effort max` anywhere. Use high at most.

## MERGE COMMAND (user directive, overrides `gh pr merge`)
Merge ONLY with the tailrocks pr-merge script, always passing `--accept-risk` (the user's standing authorization for the high-risk-path check):
```bash
  M="<claude-plugins>/tailrocks/tailrocks-repository-skills/0.5.1/skills/tailrocks-pr-merge/scripts/merge.ts"
  bun run "$M" merge "$PR_NUMBER" --repo jackin-project/jackin --git-dir "<workspace>/jackin" --accept-risk          # plan, exits 4
  bun run "$M" merge "$PR_NUMBER" --repo jackin-project/jackin --git-dir "<workspace>/jackin" --accept-risk --confirm "$PR_NUMBER"
```
Set `PR_NUMBER` to the pull request number and replace the quoted path placeholders before running the commands.

Exit 0 = merged. 10 = waiting/queued: then run `bun run "$M" watch "$PR_NUMBER" --repo jackin-project/jackin --confirm "$PR_NUMBER"` once. 20 = blocked: fix the blockers it prints (threads, checks, sync), then retry. 3 = refused for a reason other than risk: report it. Never use `gh pr merge`, `--admin`, or `pkill`/`pkill -f`. Kill only the recorded PID.
