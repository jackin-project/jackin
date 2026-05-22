# `.github/` rules — agent reference

This file is the canonical home for rules covering everything an agent does on the surfaces under `.github/`: opening pull requests, copying the PR template, iterating on review feedback, merging, and authoring CI workflows under `.github/workflows/` plus composite actions under `.github/actions/`.

Discovery flow the agent follows:

1. Operator asks for a PR. Agent reads [`PULL_REQUESTS.md`](../PULL_REQUESTS.md) at the repo root for the shared flow + template-body shape (the shared surface humans read too).
2. `PULL_REQUESTS.md` points at [`.github/PULL_REQUEST_TEMPLATE.md`](PULL_REQUEST_TEMPLATE.md) as the canonical body shape.
3. The template's preamble points at this file. The accompanying `.github/CLAUDE.md` is `@AGENTS.md`, so Claude Code auto-loads this file whenever the working directory is under `.github/`.

The split: `PULL_REQUESTS.md` is the **shared** PR flow (humans + agents). This file is the **agent-extras** — rules that govern agent-specific behavior (merge authorization, force-push policy, smoke-test mandates, squash-commit format, shell-quoting in body construction). Humans can read this file; nothing is secret. The agent-facing language is preserved because the agent is the canonical reader.

---

# Pull request rules for agents

## Per-PR merge authorization (hard rule)

**Agents must never merge a pull request without explicit per-PR confirmation from the human operator.**

- Open the PR, share the URL, and stop. The default response after creating a PR is "PR URL — ready for your review" — not a merge command in the same turn.
- Prior "just do it" / "don't wait for me" / "proceed autonomously" / "merge silently" authorizations apply only to the specific workstream the operator was discussing when they issued them. They do not carry forward to later PRs in the same session or to new sessions. Treat each PR as a fresh approval gate.
- `--admin` / branch-protection bypass is a privilege, not a default. Use it only when the operator explicitly authorizes merging *this specific PR*.
- Phrasing that does NOT authorize merge (ask anyway): "proceed", "don't wait for me", "do everything autonomously", "looks good". Phrasing that does: "merge it", "merge this one", "you can merge now", "ship it" (still prefer to confirm "ship = merge now?" for high-blast-radius PRs).
- Bounded authorization: if the operator says "merge all the PRs we just discussed" or similar, merge only the named set — not unrelated PRs that exist or that you open later.

If you are uncertain whether authorization applies to the PR in front of you, ask. The cost of pausing is ~30 seconds; the cost of merging something the operator wasn't ready for is much higher.

## Base branch for agent-created PRs

All pull requests created by agents must target `main` as the base branch unless the operator explicitly names a different target branch in the same request. Checking out, researching, or stacking on another branch does not imply that the opened pull request should target that branch. If a change depends on an unmerged branch, open a `main`-targeted PR only after the dependency is merged or explicitly ask the operator how they want the dependency represented.

## Force-push authorization

Agents must never rewrite an existing remote branch (`git push --force`, `git push --force-with-lease`, `git push +<ref>`) without explicit per-action operator approval. The full rule lives in [`BRANCHING.md`](../BRANCHING.md). Approval applies to the specific force-push the operator authorized, not to subsequent ones in the same session or branch.

A normal follow-up commit is acceptable during review unless the operator has explicitly asked to keep the PR as one amended commit.

## PR-body refresh policy

PR-body refreshes during iteration are **operator-triggered, not commit-triggered.** Do not rewrite the body after every follow-up commit. The operator may iterate on a PR for many commits before deciding the shape is right; auto-updating the body each time wastes attention and produces churn. Refresh the body only when:

1. The operator explicitly asks for it ("refresh the PR body", "update the description", "the body is out of date").
2. The PR is moving to merge-readiness — see [Verify PR title and description before merging](#verify-pr-title-and-description-before-merging) for the merge-time reconciliation step.
3. The current body has become *actively misleading* for a reviewer landing on the PR right now (e.g. the body claims a feature that was descoped, or a test count the runner now contradicts).

When the operator does ask for a refresh, re-read the full diff (`gh pr diff <PR>` + `git log` on the branch) and rewrite the affected sections so they match what's currently shipped. Surface the changes briefly in your reply.

## Include local checkout instructions in every PR

Every pull request created by an agent must include a copy-pasteable "Verify locally" section in the PR body, and the agent's final response should repeat the same commands after sharing the PR URL.

Use the real PR number, repository URL, branch name, and verification commands for the change. Start from a separate test directory so the operator can inspect the PR without disturbing their normal working tree. The clone step must be idempotent: reuse the folder if it already exists, otherwise clone it. Prefer the actual head branch name over GitHub's synthetic `pull/<PR_NUMBER>/head` ref for same-repository PRs; use the synthetic PR ref only when the branch cannot be fetched directly, such as a fork PR without an added fork remote.

The canonical body shape, intent-split block list, and isolation-env-var decision rule live in [`PULL_REQUESTS.md`](../PULL_REQUESTS.md) — read them first. The agent-specific extras below override or extend that shared content.

## `jackin-container` PRs (hard rule)

`jackin-container` is the in-container multiplexer binary at `crates/jackin-container/`. Any PR that touches any file under `crates/jackin-container/` requires a `### jackin-container smoke` block in the Verify-locally section, in addition to the standard User Smoke block. Unit tests and CI alone are not sufficient — the multiplexer only works end-to-end when running as PID 1 inside a container, and the only way to verify the status bar, input routing, pane splits, and session switching is a live `jackin load`.

### How `ensure_available` picks the binary

`ensure_available` in `src/container_binary.rs` resolves the binary in this priority order:

1. **`JACKIN_CONTAINER_BIN=/path` env override.** Used directly, no cache, no download. Set this when iterating on `crates/jackin-container/` source — the path should point at a Linux build produced by `cargo run --bin build-jackin-container`.
2. **Cache hit** at `~/.jackin/cache/jackin-container/<version>/linux-<arch>/jackin-container`. The cache key is `JACKIN_VERSION` (commit SHA suffix included), so any `cargo build` of jackin invalidates it.
3. **Download** from the `preview` rolling GitHub Release tag (for `-dev` / `-preview.` versions) or the `v<version>` tag (for tagged releases). Cached after first successful download.

The host does **not** auto-rebuild `crates/jackin-container/` on source edits. To pick up local changes, the operator must re-run the build command — which is why the eval one-shot below is mandatory.

### Required smoke-block command

The block must lead with the canonical eval one-shot build invocation:

```sh
eval "$(cargo run --bin build-jackin-container -- --export)"
```

`build-jackin-container` invokes `cargo zigbuild` (not Docker) to cross-compile the Linux binary, writes the artifact to the host cache, and the `--export` flag prints `export JACKIN_CONTAINER_BIN=<path>` — wrapping in `eval` both builds and points `ensure_available` at the freshly built binary in one step. The eval form is required (not optional) because hand-rolled `target/<triple>/release/jackin-container` exports silently break when the operator switches architectures or moves checkouts. First build takes ~2-3 minutes via cargo-zigbuild; subsequent builds are incremental. Editing any file under `crates/jackin-container/src/` does NOT auto-invalidate the binary on disk — re-run the eval to rebuild. To purge the cache entirely (e.g. switching between published and locally built binaries): `rm -rf ~/.jackin/cache/jackin-container/`.

If the build step prints a `cargo zigbuild` error, the operator should paste the full `--debug` output (`cargo-zigbuild` and `zig` must be on `PATH`; install via `mise install zig cargo:cargo-zigbuild`).

### Required launch command + verify list

The launch command that follows must hit the changed surface — usually:

```sh
cargo run --bin jackin -- load the-architect . --debug
```

or `cargo run --bin jackin -- console --debug` for console-side changes.

Inside the container, the operator must verify:

- Row 0 status bar is visible: `jackin'  [<agent-name>]`
- Agent TUI starts and renders correctly below the status bar
- `Ctrl+\` opens the command palette (override with `JACKIN_PALETTE_KEY`)
- Mouse clicks, arrow keys, and paste reach the agent unmodified
- The specific behavior changed by the PR was observed to work — one sentence (e.g. "Split pane rendered after `Ctrl+\ → Split pane │`", "Session switch preserved agent output")

PRs touching the tmux-style prefix surface (`Ctrl+B Space` palette, `Ctrl+B "` / `Ctrl+B %` splits, `Ctrl+B d` detach) must opt in before launching and call out the surface in the verify list:

```sh
export JACKIN_PREFIX=C-b
```

A `crates/jackin-container/` PR without this block is incomplete. Unit tests passing is necessary but not sufficient. The PR template at [`.github/PULL_REQUEST_TEMPLATE.md`](PULL_REQUEST_TEMPLATE.md) ships this block under `### jackin-container smoke` — copy it verbatim rather than rewriting the build invocation.

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
- Do not amend, force-push, or wait for GitHub Actions as a reflex after every small feedback pass. Force-pushes require explicit operator approval per [`BRANCHING.md`](../BRANCHING.md). If the branch already has a PR open, a normal follow-up commit is acceptable during review unless the operator asked to keep the PR as one amended commit.
- Summarize what changed and tell the operator what lightweight local check, if any, was run. Then stop so the operator can validate the UI/behavior.

Move to merge-readiness only when the operator gives a clear signal such as "this is correct", "prepare it", "ready for review", "run the full checks", or "now we can merge". At that point run the full verification suite, reconcile the PR body with the final diff, push/update the branch, and check CI.

Why this rule exists: the operator often needs several UI/behavior iterations before deciding the shape is right. Running formatting, clippy, the full test suite, PR body updates, and CI checks on every intermediate pass wastes time and tokens before the operator has validated the design.

## CI must be green before merging (hard rule)

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
- Check whether the PR ships, advances, defers, or invalidates any roadmap item under `docs/src/content/docs/reference/roadmap/`. If the roadmap is stale, update the roadmap item and `docs/src/content/docs/reference/roadmap.mdx`, refresh the PR description, push that change, and only then continue toward merge. A merge request is the final freshness gate, even if earlier review missed the roadmap update.
- Compare. The metadata is stale if any of these are true: commits added scope that the title/body doesn't reflect; a feature was descoped after the PR opened; the test plan is wrong relative to what was actually verified; file paths cited in the body have moved or been renamed; the title still says "design doc only" / "WIP" / etc. while the PR now contains implementation.
- If stale, update the title and/or body via `gh pr edit <PR>` *before* running the merge. Squash-merge writes the PR title verbatim into the commit message; merging with stale metadata bakes the drift into history permanently.

Don't ask the operator for permission to bring the metadata into agreement with the diff — they've authorized merging the *content*, and reconciling the description is part of finishing the merge cleanly. *Do* surface the discrepancy briefly in your reply ("title was 'docs(specs):' but the PR now ships the feature too — updated to 'feat(cli):' before merging") so the operator can object if your interpretation is wrong. Only pause for confirmation if the metadata rewrite would represent a meaningful change the operator might not have noticed (e.g. the PR has grown from "fix bug" into "rewrite module" — flag it and confirm before both updating and merging).

Why this rule exists: the operator relies on PR titles and bodies as the long-term navigable record of what shipped. Drift between description and diff is the single most common cause of "what does this PR actually do?" archaeology after the fact.

## PR squash merge messages

When an agent merges a pull request, the resulting squash commit must preserve the GitHub PR reference and enough attribution to make the shipped history auditable.

- Always use squash merge. Agents must not use merge commits or rebase merges for jackin pull requests.
- Use `gh pr merge <PR> --squash --body-file <file>` for the merge operation; never use a GitHub connector or direct API call to merge.
- The squash commit title must be the final PR title with the PR number suffix: `type(scope): summary (#PR_NUMBER)`.
- Prefer GitHub's default squash title when it already matches that format.
- If overriding the commit title, manually append `(#PR_NUMBER)`.
- For Codex `gh` merges: do not pass a custom title unless necessary; if one is passed, it must include `(#PR_NUMBER)`.
- Before merging, explicitly check the exact title that will be written to history. If using GitHub's default, confirm it already includes `(#PR_NUMBER)`. If passing `--subject`, build it from the final PR title plus the PR suffix and read it back before running the merge command.
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

---

# GitHub Actions workflow authoring rules

Rules for writing and maintaining workflows under `.github/workflows/` and composite actions under `.github/actions/`. These apply to all contributors — human and AI.

## Tool installation: always use mise (hard rule)

**All tools — in CI and locally — must be installed through mise. Never add `actions-rust-lang/setup-rust-toolchain`, `dtolnay/rust-toolchain`, `actions/setup-node`, `actions/setup-go`, `actions/setup-python`, or any other language-specific setup action to a workflow.**

`mise.toml` is the single source of truth for tool versions. This gives local development and CI identical environments, one place to bump versions, and one mental model for every contributor and agent.

**In GitHub Actions workflows:**
- Use `jdx/mise-action` for every tool installation — Rust, Node, Bun, Zig, cargo tools, everything.
- **Rust toolchain version**: channel declared in `rust-toolchain.toml`. mise reads it automatically via `idiomatic_version_file` — no version pin in `install_args` needed. mise does **not** install `components` from `rust-toolchain.toml`; add a `rustup component add <components>` step after mise when a job needs non-default components (e.g. `rustfmt`, `clippy`).
- **Cross-compilation targets**: run `rustup target add <target>` after the mise step; `actions-rust-lang/setup-rust-toolchain`'s `target:` parameter is not available.
- **Cargo-registry tools used across all jobs** (e.g. `cargo-nextest`): declare in `mise.toml` with a pinned version (`"cargo:cargo-nextest" = "0.9.136"`). Tools needed by only one job (e.g. `cargo-zigbuild`, `cross`) can use `install_args: "cargo:<crate>"` instead.
- **MSRV override** (the `msrv` CI job only): read the version from `Cargo.toml`'s `rust-version` field at job runtime — never hardcode it. Use `install_args: "rust@${{ steps.msrv.outputs.version }}"` and pin the cargo step with `RUSTUP_TOOLCHAIN: ${{ steps.msrv.outputs.version }}`.
- **Multiple tools in one step**: space-separate in `install_args: "rust zig cargo:cargo-zigbuild"`. Use a GHA expression when the set is matrix-conditional: `install_args: "${{ matrix.zigbuild && 'rust zig cargo:cargo-zigbuild' || 'rust' }}"`.

**Locally:** `mise install` from the repo root installs every tool at the version CI uses.

## Env-var scope: job level, not workflow level

Environment variables that a third-party CLI reads as a default-selection (`BUILDX_BUILDER`, `DOCKER_BUILDKIT`, `GH_TOKEN`, `RUSTUP_TOOLCHAIN`, `AWS_PROFILE`, etc.) MUST be declared at the **job** level, not the workflow level. Workflow-level `env:` leaks into every job; a job that didn't opt into the corresponding tool setup will fail at runtime when the CLI dereferences a missing resource.

Workflow-level `env:` is reserved for in-house naming (`DIGEST_DIR`, internal labels) where the value has no runtime side-effect on third-party tooling.

See the canonical break in [jackin-project/jackin#266](https://github.com/jackin-project/jackin/pull/266) — `BUILDX_BUILDER` hoisted to workflow level blew up every job that didn't create that builder.

## Publishing steps must gate on `main`

Every workflow that writes to a public registry, tag, release, or Homebrew formula MUST gate the actual publish step on `main`. PRs and dispatches from feature branches may build and test but must never publish. Derive a single `is_publish` boolean once (in the `changes` job), gate every side-effect step on it — do not restate the branch conditions inline at multiple steps.

## Smoke-test push-only jobs before merging

Jobs gated to `push to main`, `workflow_dispatch && ref == main`, or `workflow_run` events do not run on `pull_request`. If a PR modifies such a job, smoke-test it via `gh workflow run --ref <feature-branch>` before merging — PR-time CI will never exercise it.
