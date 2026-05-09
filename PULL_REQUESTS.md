# PULL_REQUESTS.md

Canonical guide for how pull requests are created, iterated on, reviewed, and merged in this repository. Applies to AI agents and human contributors. Linked from [`AGENTS.md`](AGENTS.md).

Read this file before opening, updating, or merging a pull request.

PR-body refreshes during iteration are **operator-triggered, not commit-triggered.** Do not rewrite the body after every follow-up commit. The operator may iterate on a PR for many commits before deciding the shape is right; auto-updating the body each time wastes attention and produces churn. Refresh the body only when:

1. The operator explicitly asks for it ("refresh the PR body", "update the description", "the body is out of date").
2. The PR is moving to merge-readiness — see "[Verify PR title and description before merging](#verify-pr-title-and-description-before-merging)" for the merge-time reconciliation step.
3. The current body has become *actively misleading* for a reviewer landing on the PR right now (e.g. the body claims a feature that was descoped, or a test count the runner now contradicts).

When the operator does ask for a refresh, re-read the full diff (`gh pr diff <PR>` + `git log` on the branch) and rewrite the affected sections so they match what's currently shipped. Surface the changes briefly in your reply.

## Canonical body shape — see `.github/PULL_REQUEST_TEMPLATE.md`

The reference template lives at [`.github/PULL_REQUEST_TEMPLATE.md`](.github/PULL_REQUEST_TEMPLATE.md). Copy it as the starting point for every new PR body and fill in the placeholders. GitHub also auto-loads it when a PR is opened through the web UI.

Sections, in order (drop the optional ones when they don't apply — the template comments call out which):

1. **Summary** — one paragraph: what shipped, who benefits, how it changes their flow. No file list, no rationale narration. Cross-references to other docs by name (no `/reference/...` links).
2. **Hard rule / impact callout** *(optional — only when the PR introduces or honours a non-trivial cross-cutting rule)* — one paragraph naming the rule, what it blocks, where the full rationale lives.
3. **What's deferred** *(optional — only when the PR is the first slice of a longer plan)* — bulleted list of explicit follow-up items so reviewers know what's intentionally out of scope.
4. **Verify locally** — copy-pasteable steps the operator runs, structured by intent (Checkout / Static checks / Tests / User smoke / Documentation). One block per intent, with the exact commands and the expected output.
5. **Migration notes** — short. "None" is a valid answer during pre-release; drop the section entirely when there's nothing to say.

What the template deliberately omits:

- File-by-file changelog (visible in the diff).
- Full test list (visible in the test runner output).
- Design rationale for every sub-decision (lives in the contributor doc the PR adds or updates).
- Links to deployed docs URLs (those break post-merge; see "[Never link deployed docs from the PR body](#pr-body--keep-it-tight-let-github-flow-the-text)").
- Mechanical CI-shaped checks (sidebar diffs, link audits, file-tree assertions belong in CI, not in the PR body).

## Pull Request Merging (agent-only)

**Agents must never merge a pull request without explicit per-PR confirmation from the human operator.**

- Open the PR, share the URL, and stop. The default response after creating a PR is "PR URL — ready for your review" — not a merge command in the same turn.
- Prior "just do it" / "don't wait for me" / "proceed autonomously" / "merge silently" authorizations apply only to the specific workstream the operator was discussing when they issued them. They do not carry forward to later PRs in the same session or to new sessions. Treat each PR as a fresh approval gate.
- `--admin` / branch-protection bypass is a privilege, not a default. Use it only when the operator explicitly authorizes merging *this specific PR*.
- Phrasing that does NOT authorize merge (ask anyway): "proceed", "don't wait for me", "do everything autonomously", "looks good". Phrasing that does: "merge it", "merge this one", "you can merge now", "ship it" (still prefer to confirm "ship = merge now?" for high-blast-radius PRs).
- Bounded authorization: if the operator says "merge all the PRs we just discussed" or similar, merge only the named set — not unrelated PRs that exist or that you open later.

If you are uncertain whether authorization applies to the PR in front of you, ask. The cost of pausing is ~30 seconds; the cost of merging something the operator wasn't ready for is much higher.

## Include local checkout instructions in every PR

Every pull request created by an agent must include a copy-pasteable "Verify locally" section in the PR body, and the agent's final response should repeat the same commands after sharing the PR URL.

Use the real PR number, repository URL, branch name, and verification commands for the change. Start from a separate test directory so the operator can inspect the PR without disturbing their normal working tree. The clone step must be idempotent: reuse the folder if it already exists, otherwise clone it. Prefer the actual head branch name over GitHub's synthetic `pull/<PR_NUMBER>/head` ref for same-repository PRs; use the synthetic PR ref only when the branch cannot be fetched directly, such as a fork PR without an added fork remote.

Split verification into named blocks only when each block contains meaningful commands. Always include checkout instructions. Add Static Checks only when there is a local check worth running beyond CI and GitHub's diff UI. Add Tests only when there is a relevant automated test command. Add User Smoke only when the operator can exercise changed behavior locally, such as CLI, runtime, workspace, Docker, TUI, or operator-flow changes. Do not add placeholder sections that say no test applies, and do not add commands that only print files for review. For CLI/runtime smoke, run the local checkout's `jackin` binary and exercise the behavior touched by the PR. If the PR has no narrower manual path, use the console as the baseline smoke command: `cargo run --bin jackin -- console --debug`. For launch/runtime flows, prefer a command that hits the changed path, such as `cargo run --bin jackin -- load <role> <target> --debug`. For subcommands that do not support `--debug`, include the closest supported `jackin --debug` command in the same smoke block and explain the gap in one sentence.

### Documentation-only PRs

Documentation PRs (changes under `docs/**` only — `.mdx` files, `astro.config.ts` sidebar, theme/CSS) must verify by running the docs site **locally** in addition to checkout, not by pointing the operator at the GitHub Files-changed tab.

The Files-changed tab shows raw MDX. It does not show how Starlight renders the page, whether `<RepoFile />` resolves, whether the sidebar entry lands in the right group, whether internal `[link](/path/)` references resolve, whether tables and Asides render correctly, or whether the page is even reachable through navigation. A docs PR that "looks right in the diff" can render visibly broken on the site.

Required pattern for docs-only PRs:

1. **Checkout block** — same as any other PR.
2. **Run the docs site locally** — `cd docs && bun install --frozen-lockfile && bun run dev`. Astro serves at `http://localhost:4321/`.
3. **Direct links to every changed page** — for each affected MDX file, include a localhost URL the operator can click straight into. Map `docs/src/content/docs/<path>.mdx` to `http://localhost:4321/<path>/`. For new pages, also tell the operator which sidebar group the entry should appear under, so they can confirm the navigation lands in the right place.

A `.mdx`-only PR that omits the local-render step is incomplete. The Files-changed tab is the operator's last-resort fallback, not the primary review surface.

### Template

The Checkout step is split into two separate code fences. The first block disables the `tirith` paste scanner for the rest of the session; the second block is the actual checkout. Splitting them lets the operator paste the bypass once on its own line and then paste the multi-line checkout block without triggering the scanner mid-way.

#### Checkout

Paste this first to bypass the `tirith` paste scanner for the rest of the session:

```sh
export TIRITH=0
```

Then paste the checkout block:

```sh
mkdir -p "$HOME/Projects/jackin-project/test"
cd "$HOME/Projects/jackin-project/test"

if [ ! -d jackin/.git ]; then
  git clone https://github.com/jackin-project/jackin.git
fi

cd jackin
mise trust
git fetch -f origin <BRANCH_NAME>:refs/remotes/origin/<BRANCH_NAME>
git checkout -B <BRANCH_NAME> refs/remotes/origin/<BRANCH_NAME>
```

The `-f` (`--force`) on `git fetch` is required, not optional. Agent-authored PR branches may have been force-pushed after explicit operator approval (DCO amend, rebase onto fresh `main`, body-only fix-ups). Without `-f`, every force-push breaks the operator's verify recipe with `! [rejected] <branch> -> origin/<branch> (non-fast-forward)`, and the local `refs/remotes/origin/<branch>` stays pinned to the pre-force-push tip. The `git checkout -B` rewrites the local branch unconditionally, but only against whatever the remote-tracking ref points at - so the fetch must update that ref through force-pushes to be useful. Equivalent recipe: `git fetch origin '+<BRANCH_NAME>:refs/remotes/origin/<BRANCH_NAME>'`. Prefer the `-f` form for readability.

#### Static Checks

```sh
cargo fmt --check
cargo clippy --lib
```

#### Tests

```sh
cargo test <RELEVANT_TEST>
```

#### User Smoke

```sh
# Adapt this to the behavior changed by the PR.
cargo run --bin jackin -- console --debug
```

If the PR needs a different validation flow, replace the final example commands with the exact commands the operator should run. When those commands invoke `jackin`, include `--debug` as required by "Walking the operator through local validation" in `AGENTS.md`.

For non-trivial code changes, structure the PR's "Verify locally" section by intent:

- **Checkout** — copy-pasteable commands to fetch and check out the PR.
- **Static Checks** — only checks that are relevant and expected to be run locally.
- **Tests** — focused or full test commands that validate the changed behavior.
- **User Smoke** — manual validation steps when behavior is visible in the CLI/TUI/runtime.

Do not add generic commands that do not materially validate the PR. In particular, do not include `git diff --check` unless the PR is specifically about whitespace, patch hygiene, generated diffs, or another issue that command is meant to catch.

For console/TUI changes that can be manually verified in jackin itself, prefer:

```sh
cargo run --bin jackin -- console --debug
```

## Author the PR body so it renders correctly on GitHub

The PR body is Markdown — what the operator sees on GitHub is what matters. Two recurring failure modes when an agent constructs the body inside a shell command:

1. **Do not escape backticks or `$`.** Triple-backtick fences must be literal `` ``` ``, not `\`\`\``. Variable references inside fenced code blocks (e.g. `$HOME`, `$PR_NUMBER`) must be literal `$`, not `\$`. Escaping them produces visibly broken output like `\`\`\`sh` and `\$HOME` in the rendered PR.
2. **Use `gh pr create --body-file <file>` (not `--body "..."`)** when the body contains code fences, dollar signs, or anything else that interacts with shell quoting. Write the body to a temp file with a single-quoted `<<'EOF'` heredoc — single quotes already disable shell expansion and command substitution, so no manual escaping is needed inside the heredoc. The pattern is:

   ~~~sh
   cat > /tmp/pr-body.md <<'EOF'
   ## Summary

   ```sh
   echo "$HOME"
   ```
   EOF
   gh pr create --body-file /tmp/pr-body.md ...
   ~~~

   Then immediately verify the rendered body with `gh pr view <PR> --json body -q .body`. If you see `\`` or `\$` anywhere, the body is broken — fix it with `gh pr edit <PR> --body-file <file>` before moving on.

## PR body — keep it tight, let GitHub flow the text

The PR body is read in GitHub's renderer, which already wraps long lines at the viewport width. Treat that as the source of truth for line breaks and follow these rules:

- **Do not hard-wrap prose at ~70 columns.** Write each paragraph as a single long line in the source. GitHub will wrap it at display time. Source-side line breaks at column 70 produce output where every other line ends mid-sentence, which is much harder to read than a flowing paragraph. The exception is code fences and bullet contents that already encode meaningful line breaks.
- **No verbosity, no duplication.** A PR body explains *what shipped* and *how to verify it*. It does not duplicate the design rationale (that lives in the contributor doc the PR adds or updates), the file-by-file changelog (visible in the PR diff), or the test list (visible in the test-runner output). Trim every sentence that exists in two places. Default to 100–200 lines for a substantial PR; 400+ lines is a smell.
- **Never link deployed docs from the PR body.** Operator-facing docs URLs, roadmap pages, and any `https://jackin.example/...` link can move, rename, or 404 after the PR merges. The PR body becomes a permanent commit attribution after squash-merge, so a broken link is permanent. Use localhost render URLs (`http://localhost:4321/...`) inside the **Verify locally → Documentation** block — those are valid only at verification time and are obviously local. Refer to other docs by name, not URL: write *"the GitHub CLI authentication strategy roadmap doc"*, not a link to it.
- **No mechanical / CI-shaped checks in the PR body.** Anything fully deterministic — sidebar diffs, link audits, file-tree assertions, "did you remember to update the changelog" greps — belongs in CI, not in a checklist the operator has to copy-paste. The PR body is for the operator-facing verification path: build, test, run the binary, render the docs. If a mechanical check is missing from CI today, file a follow-up to add it; do not promote it into every PR body in the meantime.
- **Verify-locally documentation block: one block per page.** Each page operators should walk gets its own block: the URL bolded on its own line, the description on the next line with no blank line in between (use a trailing two-space line break for the soft break), and a blank line between blocks. Like:

  ```md
  **http://localhost:4321/getting-started/design-principles/**␣␣
  NEW page. Repo-wide design rules — never mutate the host, …

  **http://localhost:4321/guides/github-cli-auth/**␣␣
  NEW page. Dedicated `gh` auth flow — modes, launch-summary samples, …
  ```

  Do not use `####` or `###` for the URLs — that creates extra vertical space and pushes the description away from its link. Do not put the URL in plain prose with the description tail-trailing it on the same wrapped line — that hides the link inside a paragraph. The bold-URL + soft-break + description pattern keeps each entry one visual block while still flowing inside GitHub's renderer.

## Reviewing a PR

Two cross-cutting rules apply to every PR review (manual, agent-driven, or automated) before output ships:

### Accepted-exceptions catalog

Do not flag items listed under "Accepted exceptions" on the [Open review findings](docs/src/content/docs/reference/roadmap/open-review-findings.mdx) roadmap catalog. Those items are retained intentionally and have been reviewed.

The catalog itself is a forward-looking backlog — consult it on demand when a review task calls for it. It is not operational context and should not be loaded at session start.

### Always check the PR against jackin's design principles

Every PR review must explicitly verify the change against jackin's [design principles](docs/src/content/docs/getting-started/design-principles.mdx). Read that page before producing review output. If a change appears to contradict any principle (most commonly: *never mutate the host machine silently*, *operator-only configuration boundaries*, *container is the trust boundary, not the prompt*), flag it loudly in the review with a specific reference to which principle is at risk.

Don't silently let a principle violation pass because the diff is small or the operator seemed to want the shortcut. The whole point of the principles is that operators rely on them across every feature — a quietly-merged exception erodes that contract for every future PR.

When you flag a possible violation, surface it like this:

> **Design-principle check:** *<principle name>* — *<one-sentence summary of what the diff does that risks the principle>*. Operator decision required: keep the change as proposed (and update the principle / add an explicit opt-in), narrow the change to stay inside the principle, or drop it.

The operator's call decides the outcome — the agent's job is to make sure the question is asked, not to silently approve or block. New principles or principle changes happen at design-principles-page level (with a PR), not inside an unrelated feature PR.

## Applying review fixes to an open PR

When the operator asks for code review fixes on a PR that has **not yet been merged**, commit the fixes directly to the PR's existing branch — do not create a new branch or open a new PR unless the operator explicitly requests it.

- Check out the PR branch (`gh pr checkout <PR>` or `git checkout <branch>`) before making changes.
- Commit fixes to that branch and push; the open PR picks up the new commits automatically.
- Creating a separate PR on top of an unmerged PR fragments review history and forces an extra merge step — avoid it.

## Iterating on operator feedback for an open PR

When the operator gives design or behavior feedback on an open PR, treat it as an iteration step unless they explicitly say the PR is ready for final verification, merge preparation, or review handoff.

During iteration:

- Make the requested code changes on the PR branch.
- It is okay to run a narrow, targeted test or command that directly exercises the code just changed, especially when it catches obvious local breakage cheaply.
- Do **not** run broad/final verification by default during iteration. In particular, do not run `cargo fmt -- --check`, `cargo clippy -- -D warnings`, `cargo nextest run`, or GitHub Actions polling unless the operator explicitly asks for verification/final prep or the PR is moving to merge-readiness.
- If a small targeted run reveals a formatting or clippy issue, fix the obvious local cause when it is part of the changed code, but do not escalate into the full formatting + clippy + full-suite pipeline unless the operator asks.
- Do not update the PR body after every iteration unless the operator asks for it or the PR description has become actively misleading for someone reviewing right now.
- Do not amend, force-push, or wait for GitHub Actions as a reflex after every small feedback pass. Force-pushes require explicit operator approval per [BRANCHING.md](BRANCHING.md). If the branch already has a PR open, a normal follow-up commit is acceptable during review unless the operator asked to keep the PR as one amended commit.
- Summarize what changed and tell the operator what lightweight local check, if any, was run. Then stop so the operator can validate the UI/behavior.

Move to merge-readiness only when the operator gives a clear signal such as "this is correct", "prepare it", "ready for review", "run the full checks", or "now we can merge". At that point run the full verification suite, reconcile the PR body with the final diff, push/update the branch, and check CI.

Why this rule exists: the operator often needs several UI/behavior iterations before deciding the shape is right. Running formatting, clippy, the full test suite, PR body updates, and CI checks on every intermediate pass wastes time and tokens before the operator has validated the design.

## CI must be green before merging

**Never merge a pull request unless all required CI checks pass.** This is non-negotiable regardless of how the operator phrases the merge request.

Before invoking the merge command:

1. **Check CI status**: run `gh pr checks <PR> --repo <owner/repo>` and confirm every required check shows `pass`. A check in `pending` or `fail` state means do not merge — wait or fix first.
2. **Do not force-merge to bypass failures**: do not use `--admin` or other bypass flags to override failing checks unless the operator explicitly names the specific failing check and states it is safe to bypass for an articulated reason.
3. **Always use `gh` (GitHub CLI) for all GitHub interactions**: PR creation, review, status checks, and merging must go through `gh`, not GitHub connectors, raw `git push` to protected branches, or direct API calls. This keeps the audit trail consistent and ensures branch-protection rules are respected.

If CI is red when the operator says "merge it", respond: "CI is failing on `<check name>` — I won't merge until it's green. Fix the failure and then I'll merge." If the operator insists on merging anyway, ask them to explicitly acknowledge the specific failing check.

Why this rule exists: a red main branch blocks the whole team. The cost of one bad merge far exceeds the cost of pausing to fix CI.

## Verify PR title and description before merging

When the operator confirms a PR can be merged, verify the PR's title and description still match the actual code being merged **before invoking the merge**.

- Read the current metadata: `gh pr view <PR>`.
- Read the actual diff being merged: `gh pr diff <PR>` (and `git log` on the PR branch if the diff is large).
- Compare. The metadata is stale if any of these are true: commits added scope that the title/body doesn't reflect; a feature was descoped after the PR opened; the test plan is wrong relative to what was actually verified; file paths cited in the body have moved or been renamed; the title still says "design doc only" / "WIP" / etc. while the PR now contains implementation.
- If stale, update the title and/or body via `gh pr edit <PR>` *before* running the merge. Squash-merge writes the PR title verbatim into the commit message; merging with stale metadata bakes the drift into history permanently.

Don't ask the operator for permission to bring the metadata into agreement with the diff — they've authorized merging the *content*, and reconciling the description is part of finishing the merge cleanly. *Do* surface the discrepancy briefly in your reply ("title was 'docs(specs):' but the PR now ships the feature too — updated to 'feat(cli):' before merging") so the operator can object if your interpretation is wrong. Only pause for confirmation if the metadata rewrite would represent a meaningful change the operator might not have noticed (e.g. the PR has grown from "fix bug" into "rewrite module" — flag it and confirm before both updating and merging).

Why this rule exists: the operator relies on PR titles and bodies as the long-term navigable record of what shipped. Drift between description and diff is the single most common cause of "what does this PR actually do?" archaeology after the fact.

## Workflow / CI changes (agent-only)

CI workflow files (`.github/workflows/*.yml`, `.github/actions/*/action.yml`) have failure modes that are invisible to PR-time CI because most gated jobs do not run on a `pull_request` event. Two rules apply when an agent modifies these files.

### Scope third-party-CLI env vars to the consuming job

Environment variables that a third-party CLI reads as a default-selection — most notably `BUILDX_BUILDER` for `docker buildx`, `DOCKER_BUILDKIT`, `GH_TOKEN` / `GITHUB_TOKEN`, `KUBECONFIG`, `AWS_PROFILE`, `RUSTUP_TOOLCHAIN`, `npm_config_*` — MUST be declared at the job level, not at the workflow level. Setting such a variable at the workflow level leaks it into every job in the file; a job that did not opt into the corresponding tool setup will then fail at runtime when the CLI dereferences the variable against state that does not exist for that job.

Workflow-level `env:` is reserved for in-house naming and paths (`DIGEST_DIR`, `REGISTRY_IMAGE`, internal labels) where the value has no runtime effect on third-party tooling and leaking into all jobs is harmless.

The break in [jackin-project/jackin#266](https://github.com/jackin-project/jackin/pull/266) is the canonical example: a refactor hoisted `BUILDX_BUILDER: jackin-construct` to workflow level. The `publish-manifest` job intentionally creates no buildx builder because `docker buildx imagetools create` / `inspect` are registry-side operations, but with the env var leaked in, every `docker buildx` invocation tried to look up a builder by that name and exited with `ERROR: no builder "jackin-construct" found`. Fixed by moving the env var into the `build` job's `env:` block where the matching `setup-buildx-action` actually creates that builder.

### Hard-gate registry / production publishing to main

Every workflow that writes to a public registry, a tag, a release, a Homebrew formula, or any other production artifact MUST gate the actual publish step on `main`. PRs and dispatches from feature branches are allowed to *build* and *test* but are forbidden from publishing. The canonical pattern in jackin is the `is_publish` flag on the `Construct Image` workflow's `changes` job: it is true only when `event_name == 'push'` (already main-by-construction because `on.push.branches: [main]`) or when `event_name == 'workflow_dispatch' && ref == 'refs/heads/main'`. Login, push-by-digest, digest upload, and the multi-platform manifest publish all gate on `is_publish == 'true'`; the local-only build path gates on `is_publish != 'true'` and runs from any branch.

Equivalent contracts apply to `publish-preview` (Publish Homebrew Preview, hard-gated to dispatch-from-main and to `workflow_run.head_branch == 'main'`), `deploy` (Docs, gated to push-to-main and dispatch-from-main), and `build-validator` (CI, gated to push-to-main and dispatch). A PR-time run of any of these workflows must never produce a registry- or release-visible side effect.

When introducing a new publishing workflow or step, mirror this shape: derive a single `is_publish` (or analogous) boolean once, in the `changes` job, and gate every side-effect step on it. Do not restate the conditions inline at multiple steps — the duplication is exactly how a reviewer or refactor accidentally widens the gate.

### Smoke-test push-only / main-only jobs before requesting merge

Jobs gated to `push to main`, `workflow_dispatch && ref == 'refs/heads/main'`, or `workflow_run.conclusion == 'success' && head_branch == 'main'` do not run on `pull_request` events. Their runtime path is therefore untested by PR-time CI. Examples in jackin today: `build-validator` (CI), `publish-manifest` (Construct Image), `deploy` (Docs), `publish-preview` (Publish Homebrew Preview).

Before requesting merge on a PR that touches such a job — the job's `if:` clause, its `needs:` chain, any env var it consumes, the recipe it ultimately runs, or any composite/reusable action it depends on — the agent must:

1. Trigger the workflow against the PR's feature branch with `gh workflow run <workflow.yml> --ref <branch>`. Every workflow in jackin already declares `workflow_dispatch:` for exactly this purpose.
2. Wait for the dispatched run with `gh run watch <run-id>` and confirm the touched job succeeded.
3. Note the run URL in the PR's "Verify locally" section so a reviewer can audit it.

When the gated job's safety rails forbid running it from a feature branch (for example `publish-preview` is hard-gated to dispatch from `main` only because it publishes to a public Homebrew tap), the PR description must explicitly call out the gap and what manual verification was performed instead — at minimum, walking the code path against the production state and naming the assumptions that could not be exercised.

PR-time CI is necessary but not sufficient for workflow-file changes. The smoke-test step closes the largest remaining hole.

## PR squash merge messages

When an agent merges a pull request, the resulting squash commit must preserve the GitHub PR reference and enough attribution to make the shipped history auditable.

- Always use squash merge. Agents must not use merge commits or rebase merges for jackin pull requests.
- Use `gh pr merge <PR> --squash --body-file <file>` for the merge operation; never use a GitHub connector or direct API call to merge.
- The squash commit title must be the final PR title with the PR number suffix: `type(scope): summary (#PR_NUMBER)`.
- Prefer GitHub's default squash title when it already matches that format.
- If overriding the commit title, manually append `(#PR_NUMBER)`.
- For Codex `gh` merges: do not pass a custom title unless necessary; if one is passed, it must include `(#PR_NUMBER)`.
- Before merging, explicitly check the exact title that will be written to
  history. If using GitHub's default, confirm it already includes `(#PR_NUMBER)`.
  If passing `--subject`, build it from the final PR title plus the PR suffix
  and read it back before running the merge command.
- Generate the squash commit body at merge time in a temporary file. Do not pollute the visible PR description with commit-only trailer footers just to influence GitHub's default squash message.
- The generated squash commit body must summarize what actually shipped in clear prose. Use the PR title/body, diff, and commit messages as source material, but do not paste the full PR body, local verification instructions, checklists, or raw commit list into the final commit.
- The generated body can be one paragraph for small PRs or a few concise paragraphs for larger PRs. It should be detailed enough to explain the change when reading `git log`, but free of process noise.
- Extract trailers from the PR commits with `gh pr view <PR> --json commits` and carry them into the generated squash body. Include the operator's `Signed-off-by` trailer when present/required and one `Co-authored-by` trailer for each AI agent that materially contributed to the PR. Include multiple agent trailers when multiple agents contributed.
- Keep trailers at the very end of the generated squash body so Git parses them as trailers. De-duplicate repeated trailers from multi-commit PRs.

Good squash body:

```text
Prefer real branch names for same-repo PR verification, omit placeholder verification sections, and require meaningful local jackin --debug smoke commands for CLI/runtime behavior changes.

Signed-off-by: Alexey Zhokhov <alexey@zhokhov.com>
Co-authored-by: Codex <codex@openai.com>
```

Good squash titles:

```text
docs: include mise trust in PR verification (#232)
docs: improve landing hero nav and PR guidance (#231)
chore(deps): update taiki-e/install-action action to v2.77.1 (#222)
refactor!: relocate host→container handoff under /jackin/, drop ~/.claude bind mount (#229)
```

Good squash trailers for a Codex-authored PR:

```text
Signed-off-by: Alexey Zhokhov <alexey@zhokhov.com>
Co-authored-by: Codex <codex@openai.com>
```

Good squash trailers for a PR with multiple AI agents:

```text
Signed-off-by: Alexey Zhokhov <alexey@zhokhov.com>
Co-authored-by: Codex <codex@openai.com>
Co-authored-by: Claude <noreply@anthropic.com>
```

This keeps commit history, GitHub commit pages, and local `git log --oneline` visibly linked back to the PR.
