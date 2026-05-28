<!--
PR body template. Two surfaces describe how to use it:

  - PULL_REQUESTS.md at the repo root — shared PR flow + body-shape
    spec + Verify-locally template + isolation env vars + review and
    roadmap-retirement rules. Both humans and agents start here.
  - .github/AGENTS.md (next to this template) — agent-only extras:
    merge authorization, body-construction shell quoting, force-push
    policy, jackin-capsule smoke-test mandate, squash-commit format.
    Claude Code auto-loads it via .github/CLAUDE.md when working
    under .github/.

Read both before authoring the body if you are an agent; the shared
file alone if you are a human contributor.

Rules in one line each:
- One paragraph per section, no hard-wrap (GitHub flows the text).
- No design rationale narration here — link out to a contributor doc instead.
- No file-by-file changelog (use the diff). No full test list (use the runner output).
- No deployed-docs URLs (they break post-merge). Refer to docs by name only.
- No mechanical CI-shaped checks (sidebar diffs, link audits). Those belong in CI.
  Exception: the docs verification gate (`### Docs checks`) is the one sanctioned
  copy-paste block — AGENTS.md requires docs authors run it before merge.
- Verify-locally URLs use http://localhost:4321/... only — never deployed.
- Each verify-locally docs page: bolded URL on its own line, soft-break (two
  trailing spaces), description on the next line, blank line between blocks.
- Drop the headings you don't need. "Related pull requests" is only when the PR
  spans multiple repos. "Hard rule" is only when the PR introduces or honours a
  non-trivial cross-cutting rule. "What's deferred" is only for the first slice
  of a longer plan. "Migration notes" can read "None" during pre-release.
-->

## Related pull requests

<When this PR is part of a coordinated set spanning multiple repos (jackin,
role repos, construct image, CI actions), list every PR here — just the link,
no description. The reader follows the link for details. Drop this section
entirely when the PR stands alone.>

- <https://github.com/org/repo/pull/N>
- <https://github.com/org/repo/pull/N>

## Summary

<One paragraph: what shipped, who benefits, how it changes their flow. No file
list, no rationale narration. Cross-references to other docs by name (no
`/reference/...` links).>

## Hard rule: <name of the rule, when relevant>

<One paragraph naming the rule, what it blocks, and where the full rationale
lives. Drop this section entirely when the PR doesn't introduce or honour a
non-trivial cross-cutting rule.>

## What's deferred (follow-up PRs)

<Bulleted list of explicit follow-up items so reviewers know what's intentionally
out of scope. Drop this section entirely when the PR is the whole feature, not
the first slice of a plan.>

- <follow-up 1>
- <follow-up 2>

## Verify locally

### Checkout

Paste this first to bypass the `tirith` paste scanner for the rest of the session:

```sh
export TIRITH=0
```

Then paste the checkout block:

```sh
mkdir -p "$HOME/Projects/jackin-project/test/pr-<PR_NUMBER>"
cd "$HOME/Projects/jackin-project/test/pr-<PR_NUMBER>"

if [ ! -d jackin/.git ]; then
  git clone https://github.com/jackin-project/jackin.git
fi

cd jackin
mise trust
git fetch -f origin <BRANCH_NAME>:refs/remotes/origin/<BRANCH_NAME>
git checkout -B <BRANCH_NAME> refs/remotes/origin/<BRANCH_NAME>
mise trust
mise install
cargo build --bin jackin
export PATH="$PWD/target/debug:$PATH"
which jackin
```

<Capsule fence — keep ONLY for PRs touching `crates/jackin-capsule/`, drop it
entirely otherwise. It is a separate paste, not a line appended to the block
above, so the operator can run it on its own. It must still come before any
`### User smoke` / `### jackin-capsule smoke` step, since every later `jackin`
launch consumes whichever capsule binary `ensure_available` resolves first.>

Then build and export the jackin-capsule binary so the smoke steps below use it:

```sh
eval "$(cargo run --bin build-jackin-capsule -- --export)"
```

### Isolation

<Include when the PR touches config/state layout, path resolution, versioned schemas, runtime state under ~/.jackin/, or the construct image. Drop this section entirely for docs-only, roadmap, CI, or pure-refactor PRs. See PULL_REQUESTS.md § "Isolation env vars" for the full decision rule.>

```sh
export JACKIN_CONFIG_DIR="$HOME/.config/jackin-pr-<PR_NUMBER>"
export JACKIN_HOME_DIR="$HOME/.jackin-pr-<PR_NUMBER>"
```

<For construct image PRs only, also add:>

```sh
just construct-build-local
export JACKIN_CONSTRUCT_IMAGE="jackin-local/construct:trixie"
```

### Static checks

```sh
cargo fmt --check
cargo clippy --all-targets --all-features -- -D warnings
```

### Rust tests

```sh
cargo nextest run -E '<SCOPED_TEST_FILTER>'
cargo nextest run --all-features
```

<Drop this whole subsection when the PR has no Rust test coverage to run.
One sentence describing what the tests cover — provisioning, parser, error
paths, etc. Skip this paragraph when the test set is small enough that the
filter speaks for itself.>

### Docs checks

<Drop this whole subsection when the PR does not touch `docs/` or docs tooling.
Keep these automated docs checks separate from Rust tests so operator-facing
docs validation does not get mixed into the Rust project test surface. The
per-page localhost render walk still goes in `### Documentation` below.>

```sh
(
  cd docs
  bun install --frozen-lockfile
  bun run build
  bun run check:repo-links
  bunx tsc --noEmit
  bun test
)
```

### User smoke

```sh
jackin console --debug
```

<Keep the console command first whenever the changed behavior is reachable from
jackin' console; it is the preferred operator smoke path. List the clicks, keys,
workspace state, in-container commands, and expected output that disambiguate a
pass/fail. Add narrower repeat checks after the console flow when helpful, e.g.
`jackin load <role> <target> --debug`. Replace the console
command only when the changed behavior has no meaningful console route. For PRs
touching `crates/jackin-capsule/`, keep the capsule build eval at the end of
the Checkout block — otherwise the console launches with a stale binary.>

### jackin-capsule smoke

<Drop this whole subsection when the PR does NOT touch `crates/jackin-capsule/`.
Include it whenever any file under `crates/jackin-capsule/src/` is changed.
This block assumes the Checkout block's capsule build eval has already run — do
not repeat the eval here.>

```sh
jackin load the-architect . --debug
```

Inside the container, verify:

- Row 0 status bar is visible: `jackin'  [<agent-name>]`
- Agent TUI starts and renders correctly below the status bar
- `Ctrl+\` opens the command palette (override with `JACKIN_PALETTE_KEY`)
- Mouse clicks, arrow keys, and paste reach the agent unmodified
- <One sentence specific to what this PR changed — e.g. "Split pane rendered
  after `Ctrl+\ → Split pane │`" or "Session switch preserved agent output">

<For PRs touching the tmux-style prefix surface (`Ctrl+B Space` palette,
`Ctrl+B "` / `Ctrl+B %` splits, `Ctrl+B d` detach), opt in before launching
and call it out in the verify list:>

```sh
export JACKIN_PREFIX=C-b
```

### Documentation

<Drop this whole subsection if the PR didn't touch `docs/`.>

```sh
(
  cd docs
  bun install --frozen-lockfile
  bun run dev
)
```

Astro serves at `http://localhost:4321/`. Pages to walk:

**http://localhost:4321/<path>/**  
<NEW page | UPDATED ...>. <One-sentence description of what to look at on this
page. Include the sidebar group and surrounding entries when adding or moving
sidebar items.>

**http://localhost:4321/<path>/**  
<Same shape — bold URL line, soft-break, description on next line, blank line
between blocks.>

## Migration notes

<"None." during pre-release is fine. Otherwise: one paragraph naming what
operators have to do — schema rename, env-var addition, on-disk path move,
etc. Skip the section entirely when it would just say "None.">
