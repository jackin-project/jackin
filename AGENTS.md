# AGENTS.md

This repository uses `main` as its primary branch. This file is the canonical home for rules and restrictions that apply only to AI agents. Rules that apply equally to human contributors and agents live in topic-specific files linked under **Shared conventions** below.

## Project status: pre-release (agent-only)

Jackin has no released version — it is a proof-of-concept. **Breaking changes are expected and acceptable.** When schemas change (config TOML, on-disk state layout, CLI flags, role manifests, role/workspace/agent shapes), do not write migration code, compatibility shims, fallback parsers for old field names, "tolerant ignore + warn" handlers, or deprecation warnings. Make the new shape the only shape; let stale configs fail with the standard parser error.

Do not memorialize old shapes in code comments ("formerly named X", "old location was Y") or in documentation files outside the changelog. The git history is the record of what changed; the code should describe only the current shape.

This rule retires when jackin ships its first tagged release.

## Changelog (agent-only)

**Do not add entries to `CHANGELOG.md` until the first tagged release.**

The changelog exists to communicate breaking changes and new features to *users of released software*. Before a first release there are no such users, and every change is implicitly "unreleased" — adding entries now creates noise that will need to be cleaned up before the release and may give a false impression that the project follows a stable release cadence.

When the first release is being cut, the operator will explicitly ask for the changelog to be populated. Until then, leave `CHANGELOG.md` unchanged.

## Pull Request Merging (agent-only)

**Agents must never merge a pull request without explicit per-PR confirmation from the human operator.**

- Open the PR, share the URL, and stop. The default response after creating a PR is "PR URL — ready for your review" — not a merge command in the same turn.
- Prior "just do it" / "don't wait for me" / "proceed autonomously" / "merge silently" authorizations apply only to the specific workstream the operator was discussing when they issued them. They do not carry forward to later PRs in the same session or to new sessions. Treat each PR as a fresh approval gate.
- `--admin` / branch-protection bypass is a privilege, not a default. Use it only when the operator explicitly authorizes merging *this specific PR*.
- Phrasing that does NOT authorize merge (ask anyway): "proceed", "don't wait for me", "do everything autonomously", "looks good". Phrasing that does: "merge it", "merge this one", "you can merge now", "ship it" (still prefer to confirm "ship = merge now?" for high-blast-radius PRs).
- Bounded authorization: if the operator says "merge all the PRs we just discussed" or similar, merge only the named set — not unrelated PRs that exist or that you open later.

If you are uncertain whether authorization applies to the PR in front of you, ask. The cost of pausing is ~30 seconds; the cost of merging something the operator wasn't ready for is much higher.

### Include local checkout instructions in every PR

Every pull request created by an agent must include a copy-pasteable "Verify locally" section in the PR body, and the agent's final response should repeat the same commands after sharing the PR URL.

Use the real PR number, repository URL, branch name, and verification commands for the change. Start from a separate test directory so the operator can inspect the PR without disturbing their normal working tree. The clone step must be idempotent: reuse the folder if it already exists, otherwise clone it.

Template:

```sh
mkdir -p "$HOME/Projects/jackin-project/test"
cd "$HOME/Projects/jackin-project/test"

if [ ! -d jackin/.git ]; then
  git clone https://github.com/jackin-project/jackin.git
fi

cd jackin
git fetch origin pull/<PR_NUMBER>/head:pr-<PR_NUMBER>
git checkout pr-<PR_NUMBER>

# Run the checks relevant to this PR, for example:
# cd docs
# bun install --frozen-lockfile
# bun run dev
```

If the PR needs a different validation flow, replace the final example commands with the exact commands the operator should run. When those commands invoke `jackin`, include `--debug` as required by "Walking the operator through local validation".

### CI must be green before merging

**Never merge a pull request unless all required CI checks pass.** This is non-negotiable regardless of how the operator phrases the merge request.

Before invoking the merge command:

1. **Check CI status**: run `gh pr checks <PR> --repo <owner/repo>` and confirm every required check shows `pass`. A check in `pending` or `fail` state means do not merge — wait or fix first.
2. **Do not force-merge to bypass failures**: do not use `--admin` or other bypass flags to override failing checks unless the operator explicitly names the specific failing check and states it is safe to bypass for an articulated reason.
3. **Always use `gh` (GitHub CLI) for all GitHub interactions**: PR creation, review, status checks, and merging must go through `gh`, not raw `git push` to protected branches or direct API calls. This keeps the audit trail consistent and ensures branch-protection rules are respected.

If CI is red when the operator says "merge it", respond: "CI is failing on `<check name>` — I won't merge until it's green. Fix the failure and then I'll merge." If the operator insists on merging anyway, ask them to explicitly acknowledge the specific failing check.

Why this rule exists: a red main branch blocks the whole team. The cost of one bad merge far exceeds the cost of pausing to fix CI.

### Verify PR title and description before merging

When the operator confirms a PR can be merged, verify the PR's title and description still match the actual code being merged **before invoking the merge**.

- Read the current metadata: `gh pr view <PR>`.
- Read the actual diff being merged: `gh pr diff <PR>` (and `git log` on the PR branch if the diff is large).
- Compare. The metadata is stale if any of these are true: commits added scope that the title/body doesn't reflect; a feature was descoped after the PR opened; the test plan is wrong relative to what was actually verified; file paths cited in the body have moved or been renamed; the title still says "design doc only" / "WIP" / etc. while the PR now contains implementation.
- If stale, update the title and/or body via `gh pr edit <PR>` *before* running the merge. Squash-merge writes the PR title verbatim into the commit message; merging with stale metadata bakes the drift into history permanently.

Don't ask the operator for permission to bring the metadata into agreement with the diff — they've authorized merging the *content*, and reconciling the description is part of finishing the merge cleanly. *Do* surface the discrepancy briefly in your reply ("title was 'docs(specs):' but the PR now ships the feature too — updated to 'feat(cli):' before merging") so the operator can object if your interpretation is wrong. Only pause for confirmation if the metadata rewrite would represent a meaningful change the operator might not have noticed (e.g. the PR has grown from "fix bug" into "rewrite module" — flag it and confirm before both updating and merging).

Why this rule exists: the operator relies on PR titles and bodies as the long-term navigable record of what shipped. Drift between description and diff is the single most common cause of "what does this PR actually do?" archaeology after the fact.

### PR merge commit titles

When an agent merges a pull request, the resulting squash/merge commit title must preserve the GitHub PR reference.

- Always use squash merge unless the human operator explicitly requests a different merge method for that specific PR.
- Prefer GitHub's default squash/merge title when it already includes `(#PR_NUMBER)`.
- If overriding the commit title, manually append `(#PR_NUMBER)`.
- For Codex/GitHub connector merges: do not pass a custom `commit_title` unless necessary; if one is passed, it must include `(#PR_NUMBER)`.
- Example: `docs(roadmap): refine per-mount isolation design (#168)`

This keeps commit history, GitHub commit pages, and local `git log --oneline` visibly linked back to the PR.

## Commit Attribution (agent-only)

Every commit created by an AI agent in this repository must include **exactly one** `Co-authored-by` trailer identifying the agent that made the commit. The trailer identifies the **agent tool**, not the underlying model — **never stack multiple agent trailers on one commit** (for example, an Amp-generated commit must not also carry `Co-authored-by: Claude` or `Co-authored-by: Codex` just because Amp used one of those vendors' models under the hood).

Until the listed agents emit their trailers automatically, the trailer must be added by hand when creating or amending the commit.

**Trailers by agent:**

- **Claude** (Claude Code CLI, or any Claude-API coding agent used directly):

  ```text
  Co-authored-by: Claude <noreply@anthropic.com>
  ```

- **Codex** (OpenAI Codex CLI):

  ```text
  Co-authored-by: Codex <codex@openai.com>
  ```

- **Amp** (Sourcegraph Amp, regardless of underlying model):

  ```text
  Co-authored-by: Amp <amp@ampcode.com>
  ```

Amp may additionally emit an `Amp-Thread-ID:` metadata trailer; that is acceptable alongside the single `Co-authored-by: Amp` trailer because the thread ID identifies the conversation, not a second agent.

If you are uncertain which agent is creating the commit, ask — the trailer is how the operator tracks which agent produced which change, and wrong attribution is worse than no attribution.

## Code review & automated scanning (agent-only)

When performing code review or automated scanning on this repository, do not flag items listed under "Accepted exceptions" on the [Open review findings](docs/src/content/docs/reference/roadmap/open-review-findings.mdx) roadmap catalog. Those items are retained intentionally and have been reviewed.

The catalog itself is a forward-looking backlog — consult it on demand when a review task calls for it. It is not operational context and should not be loaded at session start.

### Applying review fixes to an open PR

When the operator asks for code review fixes on a PR that has **not yet been merged**, commit the fixes directly to the PR's existing branch — do not create a new branch or open a new PR unless the operator explicitly requests it.

- Check out the PR branch (`gh pr checkout <PR>` or `git checkout <branch>`) before making changes.
- Commit fixes to that branch and push; the open PR picks up the new commits automatically.
- Creating a separate PR on top of an unmerged PR fragments review history and forces an extra merge step — avoid it.

## Walking the operator through local validation (agent-only)

When walking the operator through manual validation of a jackin feature (smoke testing a PR, reproducing a bug, executing a PR test plan), every `jackin <subcommand>` invocation in the recipe MUST include `--debug`. That includes `cargo run --bin jackin -- <subcommand> --debug` while iterating from a checkout.

The `--debug` flag prints every external command jackin issues (`docker`, `git`, `id`, etc.) along with their captured output, plus jackin's own `[jackin debug ...]` instrumentation. This makes the operator's terminal output triage-able by the agent: when something doesn't behave as expected, the operator can paste the full debug log and the agent can localize the issue without guessing.

If the operator reports unexpected behavior from a clean (non-debug) run, the FIRST follow-up should be to ask them to rerun with `--debug` and paste the full output before proposing fixes.

This does not apply to:

- Inspection commands the operator runs (`pgrep`, `pmset`, `cat`, `ls`) — those aren't jackin invocations.
- Production recommendations or scripted automation (debug output is too noisy for those).

## Shared conventions

Rules in the files below apply to everyone working in the repo — human and agent:

- [RULES.md](RULES.md) — documentation-location convention (no project rules in tool-specific files).
- [BRANCHING.md](BRANCHING.md) — branch naming, feature-branch policy, what never to commit to `main`.
- [COMMITS.md](COMMITS.md) — Conventional Commits format, DCO sign-off, pre-commit verification commands.
- [TESTING.md](TESTING.md) — test runner setup and commands.
- [PROJECT_STRUCTURE.md](PROJECT_STRUCTURE.md) — navigational map of the codebase, documentation site, Docker assets, and CI workflows.
- [DEPRECATED.md](DEPRECATED.md) — ledger of deprecated APIs, CLIs, config values, and usage patterns that are still supported but should eventually be removed.
- [TODO.md](TODO.md) — small follow-up items (especially upstream dependencies waiting on a fix), the per-PR stale-docs checklist, and the convention for code-level `TODO(<topic>)` markers that link back to this file.
- [CONTRIBUTING.md](CONTRIBUTING.md) — contribution flow, DCO v1.1 text, and license terms for external contributors.
