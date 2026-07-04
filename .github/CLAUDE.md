# `.github/` rules — agent reference

Canonical home for rules covering everything an agent does on surfaces under `.github/`: opening PRs, copying the PR template, iterating on review feedback, merging, authoring CI workflows under `.github/workflows/` plus composite actions under `.github/actions/`.

Discovery flow the agent follows:

1. Operator asks for a PR. Agent reads [`PULL_REQUESTS.md`](../PULL_REQUESTS.md) at repo root for shared flow + template-body shape (shared surface humans read too).
2. `PULL_REQUESTS.md` points at [`.github/PULL_REQUEST_TEMPLATE.md`](PULL_REQUEST_TEMPLATE.md) as canonical body shape.
3. Template's preamble points at this file, which harness auto-loads whenever working directory is under `.github/`.

The split: `PULL_REQUESTS.md` is **shared** PR flow (humans + agents). This file is **agent-extras** — rules governing agent-specific behavior (merge authorization, force-push policy, smoke-test mandates, squash-commit format, shell-quoting in body construction). Humans can read this file; nothing is secret. Agent-facing language preserved because agent is canonical reader.

---

# Pull request rules for agents

## Per-PR merge authorization (hard rule)

**Agents must never merge a PR without explicit per-PR confirmation from the human operator.**

- Open PR, share URL, stop. Default response after creating a PR is "PR URL — ready for your review" — not a merge command in same turn.
- Prior "just do it" / "don't wait for me" / "proceed autonomously" / "merge silently" authorizations apply only to the specific workstream operator was discussing when issued. They don't carry forward to later PRs in same session or to new sessions. Treat each PR as a fresh approval gate.
- `--admin` / branch-protection bypass is a privilege, not a default. Use only when operator explicitly authorizes merging *this specific PR*.
- Phrasing that does NOT authorize merge (ask anyway): "proceed", "don't wait for me", "do everything autonomously", "looks good". Phrasing that does: "merge it", "merge this one", "you can merge now", "ship it" (still prefer to confirm "ship = merge now?" for high-blast-radius PRs).
- Bounded authorization: if operator says "merge all the PRs we just discussed" or similar, merge only the named set — not unrelated PRs that exist or you open later.

If uncertain whether authorization applies to the PR in front of you, ask. Pausing costs ~30 seconds; merging something operator wasn't ready for costs much more.

## Base branch for agent-created PRs

All PRs created by agents must target `main` as base branch unless operator explicitly names a different target in same request. Checking out, researching, or stacking on another branch does not imply opened PR targets that branch. If a change depends on an unmerged branch, open a `main`-targeted PR only after dependency merged, or explicitly ask operator how they want dependency represented.

## Force-push authorization

Agents must never rewrite an existing remote branch (`git push --force`, `git push --force-with-lease`, `git push +<ref>`) without explicit per-action operator approval. Full rule lives in [`BRANCHING.md`](../BRANCHING.md). Approval applies to the specific force-push operator authorized, not subsequent ones in same session or branch.

A normal follow-up commit is acceptable during review unless operator explicitly asked to keep PR as one amended commit.

## PR-body refresh policy

PR-body refreshes during iteration are **operator-triggered, not commit-triggered.** Do not rewrite body after every follow-up commit. Operator may iterate many commits before deciding shape is right; auto-updating body each time wastes attention + produces churn. Refresh body only when:

1. Operator explicitly asks ("refresh the PR body", "update the description", "the body is out of date").
2. PR moving to merge-readiness — see [Verify PR title and description before merging](#verify-pr-title-and-description-before-merging) for merge-time reconciliation step.
3. Current body has become *actively misleading* for a reviewer landing on PR right now (e.g. body claims a descoped feature, or a test count runner now contradicts).

When operator asks for a refresh, re-read full diff (`gh pr diff <PR>` + `git log` on branch) and rewrite affected sections to match what's currently shipped. Surface changes briefly in reply.

## Include local checkout instructions in every PR

Every PR created by an agent must include copy-pasteable "Verify locally" section in body, and agent's final response should repeat same commands after sharing PR URL.

Use template's `jackin-dev pr sync <PR_NUMBER>` checkout flow with real PR number and verification commands. `jackin-dev` starts from PR-specific test directory `$HOME/Projects/jackin-project/test/pr-<PR_NUMBER>`, creates or refreshes the checkout under `jackin/`, checks out the PR's real head branch, prepares isolated config/state under `state/`, builds the local binary, builds and exports a local capsule when the diff affects `jackin-capsule` or a workspace package in its dependency closure, writes `env.sh`, and auto-builds local construct artifacts when the diff requires them. Uses PR number, not branch name, for this directory so test bundles remain unique and stable while the Git prompt still shows the PR's actual branch.

Checkout block must source env file generated by `jackin-dev pr sync <PR_NUMBER>` for every PR, even docs-only or pure-refactor PRs. That env file exports `PATH`, `JACKIN_CONFIG_DIR="$HOME/Projects/jackin-project/test/pr-<PR_NUMBER>/state/config"`, `JACKIN_HOME_DIR="$HOME/Projects/jackin-project/test/pr-<PR_NUMBER>/state/home"`, `JACKIN_CAPSULE_BIN` only for capsule-affecting PRs, and any auto-detected local construct override. Do not leave isolation as a separate optional section: a PR binary can migrate config or write runtime state before operator notices, and shared live config can make older PR branches unusable after a newer schema branch is tested.

Canonical body shape, intent-split block list, mandatory isolation env-var rule live in [`PULL_REQUESTS.md`](../PULL_REQUESTS.md) — read first. Agent-specific extras below override or extend that shared content.

## `jackin-capsule` PRs (hard rule)

`jackin-capsule` is the in-container Capsule control-plane binary at `crates/jackin-capsule/`. Any PR touching any file under `crates/jackin-capsule/` requires Checkout block to run `jackin-dev pr sync <PR_NUMBER>`, source generated env file, include a dedicated smoke block:

1. Capsule prepare — runs `jackin-dev pr sync <PR_NUMBER>` in Checkout section + sources generated env file. **MUST come before `### User smoke` and `### jackin-capsule smoke`.** Every `jackin console` / `jackin load` after it consumes whichever binary `ensure_available` resolves first — without export first, launches use cached or preview-release binary and silently skip PR's container-side changes. Reviewers must reject any `crates/jackin-capsule/` PR whose Verify-locally puts a `jackin console` / `jackin load` step before Checkout block's sync command, regardless of how body is otherwise structured.
2. `### jackin-capsule smoke` — runs `jackin load the-architect . --debug` + in-container verify checklist. Unit tests + CI alone not sufficient — multiplexer only works end-to-end running as PID 1 inside a container, and the only way to verify status bar, input routing, pane splits, session switching is a live `jackin load`. Do not rebuild capsule here; Checkout already exported `JACKIN_CAPSULE_BIN` for capsule-affecting PRs.

### How `ensure_available` picks the binary

`ensure_available` in `src/capsule_binary.rs` resolves binary in this priority order:

1. **`JACKIN_CAPSULE_BIN=/path` env override.** Used directly, no cache, no download. Set when iterating on `crates/jackin-capsule/` source — path should point at a Linux build produced by `cargo run --bin build-jackin-capsule`.
2. **Cache hit** at `~/.jackin/cache/jackin-capsule/<version>/linux-<arch>/jackin-capsule`. Cache key is `JACKIN_VERSION`. Local non-CI builds use the package version to keep this cache stable across commits; CI, preview, construct, and release builds run with `CI` set and keep the SHA suffix.
3. **Download** from `preview` rolling GitHub Release tag (for `-dev` / `-preview.` versions) or `v<version>` tag (for tagged releases). Cached after first successful download.

Host does **not** auto-rebuild `crates/jackin-capsule/` on source edits outside the PR sync flow. To pick up local capsule-affecting changes, operator must re-run `jackin-dev pr sync <PR_NUMBER>` — why Checkout block's sync step is mandatory.

### Required Checkout prepare for jackin-capsule PRs

Use this command in Checkout block for any PR touching `crates/jackin-capsule/`:

```sh
jackin-dev pr sync <PR_NUMBER>
```

`build-jackin-capsule` invokes `cargo zigbuild` (not Docker) to cross-compile Linux binary, writes artifact to host cache, and `--export` flag prints `export JACKIN_CAPSULE_BIN=<path>`. When the PR diff affects `jackin-capsule` or a workspace package in its dependency closure, `jackin-dev` captures that export + writes it into the PR env file so sourcing `env.sh` points `ensure_available` at the freshly built binary. This path is required for capsule-affecting PRs because hand-rolled `target/<triple>/release/jackin-capsule` exports silently break when operator switches architectures or moves checkouts. First build ~2-3 minutes via cargo-zigbuild; subsequent builds incremental. Editing any file under `crates/jackin-capsule/src/` does NOT auto-invalidate the binary on disk — re-run `jackin-dev pr sync <PR_NUMBER>` to rebuild. To purge cache entirely (e.g. switching between published + locally built binaries): `rm -rf ~/.jackin/cache/jackin-capsule/`.

`jackin-dev` cannot export into parent shell directly. Review that Checkout block sources `$(jackin-dev pr path <PR_NUMBER>)/env.sh` after `jackin-dev pr sync <PR_NUMBER>`; only then does `JACKIN_CAPSULE_BIN` exist in operator's environment for capsule-affecting PRs.

If build step prints a `cargo zigbuild` error, operator should paste full `--debug` output (`cargo-zigbuild` and `zig` must be on `PATH`; install via `mise install zig cargo:cargo-zigbuild`).

This line is positionally load-bearing for capsule-affecting PRs: must stay in Checkout, **before** `### User smoke` and `### jackin-capsule smoke` (and before any other block running `jackin console` / `jackin load`). Without `jackin-dev pr sync` first, every subsequent `jackin` invocation resolves cached or downloaded binary instead of the freshly built capsule, so PR's container-side changes silently absent from every launch in verify recipe.

### Required `### jackin-capsule smoke` launch + verify list

Launch command must hit changed surface — usually:

```sh
jackin load the-architect . --debug
```

or `jackin console --debug` for console-side changes. Do not rebuild capsule here; Checkout already exported `JACKIN_CAPSULE_BIN`.

Inside container, operator must verify:

- Row 0 status bar is visible: `jackin❯  [<agent-name>]`
- Agent TUI starts and renders correctly below status bar
- `Ctrl+\` opens command palette (override with `JACKIN_PALETTE_KEY`)
- Mouse clicks, arrow keys, paste reach agent unmodified
- The specific behavior changed by the PR was observed to work — one sentence (e.g. "Split pane rendered after `Ctrl+\ → Split pane │`", "Session switch preserved agent output")

PRs touching tmux-style prefix surface (`Ctrl+B Space` palette, `Ctrl+B "` / `Ctrl+B %` splits, `Ctrl+B d` detach) must opt in before launching + call out surface in verify list:

```sh
export JACKIN_PREFIX=C-b
```

A `crates/jackin-capsule/` PR without this block is incomplete. Unit tests passing is necessary but not sufficient. PR template at [`.github/PULL_REQUEST_TEMPLATE.md`](PULL_REQUEST_TEMPLATE.md) ships this block under `### jackin-capsule smoke` — copy verbatim rather than rewriting build invocation.

## Author the PR body so it renders correctly on GitHub

PR body is Markdown — what operator sees on GitHub is what matters. Two recurring failure modes when an agent constructs body inside a shell command:

1. **Do not escape backticks or `$`.** Triple-backtick fences must be literal `` ``` ``, not `\`\`\``. Variable references inside fenced code blocks (e.g. `$HOME`, `$PR_NUMBER`) must be literal `$`, not `\$`. Escaping them produces visibly broken output like `\`\`\`sh` and `\$HOME` in rendered PR.
2. **Use `gh pr create --body-file <file>` (not `--body "..."`)** when body contains code fences, dollar signs, or anything else interacting with shell quoting. Write body to a temp file with a single-quoted `<<'EOF'` heredoc — single quotes already disable shell expansion + command substitution, so no manual escaping needed inside heredoc. Pattern:

   ~~~sh
   cat > /tmp/pr-body.md <<'EOF'
   ## Summary

   ```sh
   echo "$HOME"
   ```
   EOF
   gh pr create --body-file /tmp/pr-body.md ...
   ~~~

   Then immediately verify rendered body with `gh pr view <PR> --json body -q .body`. If you see `\`` or `\$` anywhere, body is broken — fix with `gh pr edit <PR> --body-file <file>` before moving on.

## Applying review fixes to an open PR

When operator asks for code review fixes on a PR **not yet merged**, commit fixes directly to PR's existing branch — do not create a new branch or open a new PR unless operator explicitly requests it.

- Check out PR branch (`gh pr checkout <PR>` or `git checkout <branch>`) before making changes.
- Commit fixes to that branch + push; open PR picks up new commits automatically.
- Creating a separate PR on top of an unmerged PR fragments review history + forces an extra merge step — avoid it.

## Iterating on operator feedback for an open PR

When operator gives design or behavior feedback on an open PR, treat as an iteration step unless they explicitly say PR is ready for final verification, merge preparation, or review handoff.

During iteration:

- Make requested code changes on PR branch.
- OK to run a narrow, targeted test or command that directly exercises just-changed code, especially when it catches obvious local breakage cheaply.
- Do **not** run broad/final verification by default during iteration. In particular, do not run `cargo fmt -- --check`, `cargo clippy -- -D warnings`, `cargo nextest run`, or GitHub Actions polling unless operator explicitly asks for verification/final prep or PR is moving to merge-readiness.
- If a small targeted run reveals a formatting or clippy issue, fix the obvious local cause when part of changed code, but don't escalate into full formatting + clippy + full-suite pipeline unless operator asks.
- Do not update PR body after every iteration unless operator asks or description has become actively misleading for someone reviewing right now.
- Do not amend, force-push, or wait for GitHub Actions as a reflex after every small feedback pass. Force-pushes require explicit operator approval per [`BRANCHING.md`](../BRANCHING.md). If branch already has a PR open, a normal follow-up commit is acceptable during review unless operator asked to keep PR as one amended commit.
- Summarize what changed + tell operator what lightweight local check, if any, was run. Then stop so operator can validate UI/behavior.

Move to merge-readiness only when operator gives a clear signal such as "this is correct", "prepare it", "ready for review", "run the full checks", or "now we can merge". At that point run full verification suite, reconcile PR body with final diff, push/update branch, check CI.

Rationale: operator often needs several UI/behavior iterations before deciding shape is right; running formatting, clippy, full test suite, PR body updates, CI checks on every intermediate pass wastes time + tokens before validation.

## CI must be green before merging (hard rule)

**Never merge a PR unless all required CI checks pass.** Non-negotiable regardless of how operator phrases merge request.

Before invoking merge command:

1. **Check CI status**: run `gh pr checks <PR> --repo <owner/repo>` and confirm every required check shows `pass`. A check in `pending` or `fail` state means do not merge — wait or fix first.
2. **Do not force-merge to bypass failures**: do not use `--admin` or other bypass flags to override failing checks unless operator explicitly names the specific failing check + states it is safe to bypass for an articulated reason.
3. **Always use `gh` (GitHub CLI) for all GitHub interactions**: PR creation, review, status checks, merging must go through `gh`, not GitHub connectors, raw `git push` to protected branches, or direct API calls. Keeps audit trail consistent + ensures branch-protection rules respected.

If CI is red when operator says "merge it", respond: "CI is failing on `<check name>` — I won't merge until it's green. Fix the failure and then I'll merge." If operator insists anyway, ask them to explicitly acknowledge the specific failing check.

Rationale: a red main branch blocks the whole team; one bad merge costs far more than pausing to fix CI.

## Verify PR title and description before merging

When operator confirms a PR can be merged, verify PR's title + description still match actual code being merged **before invoking the merge**.

- Read current metadata: `gh pr view <PR>`.
- Read actual diff being merged: `gh pr diff <PR>` (and `git log` on PR branch if diff is large).
- Check whether PR ships, advances, defers, or invalidates any roadmap item under `docs/content/docs/reference/roadmap/`. If roadmap stale, update roadmap item + `docs/content/docs/reference/roadmap/index.mdx`, refresh PR description, push that change, only then continue toward merge. A merge request is the final freshness gate, even if earlier review missed the roadmap update.
- Compare. Metadata is stale if any are true: commits added scope title/body doesn't reflect; a feature was descoped after PR opened; test plan is wrong relative to what was verified; file paths cited in body have moved or been renamed; title still says "design doc only" / "WIP" / etc. while PR now contains implementation.
- If stale, update title and/or body via `gh pr edit <PR>` *before* running merge. Squash-merge writes PR title verbatim into commit message; merging with stale metadata bakes drift into history permanently.

Don't ask operator for permission to bring metadata into agreement with diff — they've authorized merging the *content*, and reconciling description is part of finishing merge cleanly. *Do* surface the discrepancy briefly in reply ("title was 'docs(specs):' but PR now ships the feature too — updated to 'feat(cli):' before merging") so operator can object if your interpretation is wrong. Only pause for confirmation if metadata rewrite would represent a meaningful change operator might not have noticed (e.g. PR grew from "fix bug" into "rewrite module" — flag + confirm before both updating and merging).

Rationale: operator relies on PR titles + bodies as the long-term navigable record of what shipped; drift between description and diff is the top cause of "what does this PR actually do?" archaeology after the fact.

## PR squash merge messages

When an agent merges a PR, resulting squash commit must preserve GitHub PR reference so shipped history is auditable.

- Always use squash merge. Agents must not use merge commits or rebase merges for jackin PRs.
- Use `gh pr merge <PR> --squash --body-file <file>`; never use a GitHub connector or direct API call to merge.
- Squash commit title must be final PR title with PR number suffix: `type(scope): summary (#PR_NUMBER)`.
- Prefer GitHub's default squash title when it already matches that format.
- If overriding commit title, manually append `(#PR_NUMBER)`.
- For Codex `gh` merges: do not pass a custom title unless necessary; if passed, must include `(#PR_NUMBER)`.
- Before merging, explicitly check exact title that will be written to history. If using GitHub's default, confirm it already includes `(#PR_NUMBER)`. If passing `--subject`, build from final PR title plus PR suffix + read it back before running merge command.
- Generate squash commit body at merge time in a temporary file. Do not pollute visible PR description with commit-only footers.
- Generated squash commit body must summarize what actually shipped in clear prose. Use PR title/body, diff, commit messages as source material, but do not paste full PR body, local verification instructions, checklists, or raw commit list into final commit.
- Generated body can be one paragraph for small PRs or a few concise paragraphs for larger PRs. Detailed enough to explain change when reading `git log`, but free of process noise.
- Use `jackin-pr-trailers` helper (see below) to reliably extract + deduplicate all trailers (`Signed-off-by`, `Co-authored-by`, any others) from PR's commits and append at end of body.

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

Keeps commit history, GitHub commit pages, local `git log --oneline` visibly linked back to PR.

## Trailer extraction helper (`jackin-pr-trailers`)

For reliable extraction of trailers from all commits in a PR (to include in squash body), use the small dedicated CLI:

```sh
cargo run -p jackin-pr-trailers -- --pr <PR_NUMBER> [--repo owner/repo]
```

Shells out to `gh pr view`, pipes each commit message through `git interpret-trailers --parse --only-trailers --unfold`, deduplicates trailers, prints them in a useful order (Signed-off-by first, then Co-authored-by, then others). If `--pr` omitted, auto-detects PR for current branch after verifying local HEAD matches `origin/<branch>`; falls back to local `git log --format=%B%x00` extraction only when no PR exists.

Example usage when preparing the body file (run from within feature branch; --pr can be omitted to auto-detect from branch name + remote sync check):

```sh
BODY_FILE=$(mktemp)
# ... write the prose summary to $BODY_FILE ...

# Auto-detect branch/PR (if not provided), verify local == remote (push first if not),
# extract trailers from the PR or branch commits, and append the block to the body file.
cargo run -p jackin-pr-trailers -- --body-file "$BODY_FILE"

gh pr merge "$PR" --squash --body-file "$BODY_FILE"
rm "$BODY_FILE"
```

Source in `crates/jackin-pr-trailers/`. Minimal dependencies + tests for the git-native trailer path.

---

# GitHub Actions workflow authoring rules

Rules for writing + maintaining workflows under `.github/workflows/` and composite actions under `.github/actions/`. Apply to all contributors — human + AI.

## Tool installation: always use mise (hard rule)

**All tools — in CI and locally — must be installed through mise. Never add `actions-rust-lang/setup-rust-toolchain`, `dtolnay/rust-toolchain`, `actions/setup-node`, `actions/setup-go`, `actions/setup-python`, `extractions/setup-just`, or any other language- or tool-specific setup action to a workflow.**

`mise.toml` is single source of truth for tool versions. Gives local development + CI identical environments, one place to bump versions, one mental model for every contributor + agent.

**In GitHub Actions workflows:**
- Use `jdx/mise-action` for every tool installation — Rust, Node, Bun, Zig, cargo tools, everything.
- **Rust toolchain version**: channel declared in `rust-toolchain.toml`. mise reads it automatically via `idiomatic_version_file` — no version pin in `install_args` needed. mise does **not** install `components` from `rust-toolchain.toml`; add a `rustup component add <components>` step after mise when a job needs non-default components (e.g. `rustfmt`, `clippy`).
- **Cross-compilation targets**: run `rustup target add <target>` after mise step; `actions-rust-lang/setup-rust-toolchain`'s `target:` parameter not available.
- **Cargo-registry tools used across all jobs** (e.g. `cargo-nextest`): declare in `mise.toml` with a pinned version (`"cargo:cargo-nextest" = "0.9.136"`). Tools needed by only one job (e.g. `cargo-zigbuild`, `cross`) can use `install_args: "cargo:<crate>"` instead.
- **MSRV override** (the `msrv` CI job only): read version from `Cargo.toml`'s `rust-version` field at job runtime — never hardcode. Use `install_args: "rust@${{ steps.msrv.outputs.version }}"` and pin cargo step with `RUSTUP_TOOLCHAIN: ${{ steps.msrv.outputs.version }}`.
- **Multiple tools in one step**: space-separate in `install_args: "rust zig cargo:cargo-zigbuild"`. Use a GHA expression when set is matrix-conditional: `install_args: "${{ matrix.zigbuild && 'rust zig cargo:cargo-zigbuild' || 'rust' }}"`.

**Locally:** `mise install` from repo root installs every tool at version CI uses.

## Env-var scope: job level, not workflow level

Environment variables a third-party CLI reads as a default-selection (`BUILDX_BUILDER`, `DOCKER_BUILDKIT`, `GH_TOKEN`, `RUSTUP_TOOLCHAIN`, `AWS_PROFILE`, etc.) MUST be declared at **job** level, not workflow level. Workflow-level `env:` leaks into every job; a job that didn't opt into corresponding tool setup fails at runtime when CLI dereferences a missing resource.

Workflow-level `env:` is reserved for in-house naming (`DIGEST_DIR`, internal labels) where value has no runtime side-effect on third-party tooling.

See canonical break in [jackin-project/jackin#266](https://github.com/jackin-project/jackin/pull/266) — `BUILDX_BUILDER` hoisted to workflow level blew up every job that didn't create that builder.

## Publishing steps must gate on `main`

Every workflow that writes to a public registry, tag, release, or Homebrew formula MUST gate actual publish step on `main`. PRs + dispatches from feature branches may build + test but must never publish. Derive a single `is_publish` boolean once (in `changes` job), gate every side-effect step on it — do not restate branch conditions inline at multiple steps.

## Publish concurrency: latest wins, shared writers serialize

Publish workflows MUST protect freshness first. Any workflow that publishes a rolling channel, mutable formula, release alias, or "latest" install target must cancel stale runs from the same stream: use a workflow-level `concurrency` group scoped to that stream with `cancel-in-progress: true`. This includes preview-style commit channels and SemVer channels such as `jackin-dev` when multiple source commits can queue before the tap catches up. Otherwise a slower older run can finish after a newer run and overwrite the formula back to an older source. After any overlapping publish activity, each Homebrew formula must point at the newest valid source for that formula: latest preview commit for preview, latest committed dev version/source for jackin-dev.

Shared external writers are a separate concern. When multiple freshness-cancelled workflows write the same protected resource, such as the Homebrew tap, do not put all workflows in one cancelling top-level group. That lets one stream cancel another stream before it publishes. Instead keep each workflow's top-level freshness group separate and add a job-level mutex only around the final shared write, with `cancel-in-progress: false`, so the writers serialize without dropping the other stream.

Pattern:

```yaml
concurrency:
  group: homebrew-preview-publish
  cancel-in-progress: true

jobs:
  publish-preview:
    concurrency:
      group: homebrew-tap-publish
      cancel-in-progress: false
```

Use distinct top-level freshness groups for distinct publish streams (`homebrew-preview-publish`, `jackin-dev-publish`, etc.). Use the same job-level shared-writer group only for the step/job that mutates the shared resource. Never queue whole publish workflows as a freshness strategy; queued whole workflows can publish an older artifact after a newer one. If the shared writer cannot prove the source is still publishable, add an explicit freshness check before writing rather than disabling cancellation for the whole workflow.

## Smoke-test push-only jobs before merging

Jobs gated to `push to main`, `workflow_dispatch && ref == main`, or `workflow_run` events do not run on `pull_request`. If a PR modifies such a job, smoke-test via `gh workflow run --ref <feature-branch>` before merging — PR-time CI never exercises it.

## PR/main check parity (hard rule)

**Every invariant that can fail a push-to-main run must be evaluated identically at PR time, against the same inputs. A green PR must mean a green main.** Cost of a violation is a red `main` no PR could have caught — exactly the failure mode branch protection exists to prevent.

Two anti-patterns produce a PR-green/main-red gap; both forbidden:

1. **Main-only validation.** A check that runs only when `is_publish == 'true'` (push to main / dispatch from main) and is skipped or stubbed on PRs cannot gate the PR. When a publish step enforces an invariant (a version tag must not already exist, an artifact must be well-formed, a manifest must resolve), that invariant must also run on `pull_request` in a read-only form. Registry *reads* of public images need no credentials, so guard is runnable at PR time even when publish *write* is not — see `construct-assert-version-unpublished` mise task (a `jackin-xtask` subcommand), shared between `publish-manifest` (main) and `publish-manifest-rehearsal` (PR). A publish job whose failure mode has no PR-time mirror is incomplete.

2. **Non-deterministic required checks.** A required check depending on live third-party network state (external link liveness, an upstream API's rate-limit mood, a remote registry's transient 5xx) can pass on PR + fail on main, or vice versa, with no code change between. Caches keyed by `github.ref_name` make this worse: PR branch's warm cache hides a failure the cold main run then hits. Required checks must be deterministic — key shared caches without ref, authenticate API calls so they aren't rate-limited (host-scoped tokens, not unauthenticated remaps that strip auth), move genuinely-flaky external liveness checks to a non-blocking or scheduled job rather than gating merges on them.

When adding or modifying any workflow, ask: *what runs on push-to-main that does not run on this PR, and what makes a green PR here not guarantee a green main?* If answer names a job, an invariant, or a network dependency, close the gap in same PR.
