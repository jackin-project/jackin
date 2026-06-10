<!--
PR body template. Two surfaces describe how to use it:

  - PULL_REQUESTS.md at the repo root — shared PR flow + body-shape
    spec + Verify-locally policy + isolation env vars + review and
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
- Explain the shipped feature shape, not every implementation detail.
- No design rationale narration here — link out to a contributor doc instead.
- No file-by-file changelog (use the diff). No function/struct inventory. No full test list (use the runner output).
- No deployed-docs URLs (they break post-merge). Refer to docs by name only.
- No mechanical CI-shaped checks (sidebar diffs, link audits). Those belong in CI.
  Exception: the docs verification gate (`### Docs checks`) is the one sanctioned
  copy-paste block — AGENTS.md requires docs authors run it before merge.
- Verify-locally URLs use http://localhost:3000/... only — never deployed.
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

<One paragraph answering: what is this pull request for? Name the shipped
feature or behavior, who benefits, and how it changes their flow. Keep this
short; the feature-level detail goes in the next two sections. Cross-references
to other docs by name only (no `/reference/...` links).>

## What changed

<Feature-level bullets grouped by user-visible or contributor-visible outcome.
This is the place for "what ships" detail. Describe capabilities, behavior,
configuration surfaces, docs, and verification coverage in plain terms. Avoid
function names, struct names, raw fixture counts, file lists, and anything that
is only useful because the diff already shows it. For large roadmap items,
phase headings are fine when they help the reader understand the shipped shape.>

- <Capability or behavior that now exists>
- <Configuration, documentation, or workflow change operators can rely on>
- <Regression coverage or validation added, stated as an outcome rather than a
  test inventory>

## What this addresses

<Bullets naming the practical problem, roadmap gap, regression, or operator pain
that is now resolved. This should answer "what in reality is addressed?" rather
than restating the implementation. If the PR completes or advances a roadmap
item, say that by name without linking to deployed docs.>

- <Problem or gap addressed>
- <Operator-visible or maintainer-visible outcome>

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
export JACKIN_PR_TEST_DIR="$HOME/Projects/jackin-project/test/pr-<PR_NUMBER>"
mkdir -p "$JACKIN_PR_TEST_DIR"
cd "$JACKIN_PR_TEST_DIR"

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
export JACKIN_CONFIG_DIR="$JACKIN_PR_TEST_DIR/.config/jackin"
export JACKIN_HOME_DIR="$JACKIN_PR_TEST_DIR/.jackin"
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

<For construct image PRs only, also add:>

```sh
mise run construct-build-local
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

### Schema migration smoke

<Keep this subsection when the PR bumps `CURRENT_CONFIG_VERSION`,
`CURRENT_WORKSPACE_VERSION`, or `CURRENT_MANIFEST_VERSION`; drop it otherwise.
For config/workspace migrations, copy only the operator's real
`~/.config/jackin` into the PR-scoped config dir from Checkout first, then run
the PR's later smoke/test commands against that copy. Keep `JACKIN_HOME_DIR`
empty and PR-scoped so the smoke path cannot read or mutate live `~/.jackin`
state.>

```sh
if [ -d "$HOME/.config/jackin" ]; then
  cp -a "$HOME/.config/jackin" "$JACKIN_CONFIG_DIR"
else
  mkdir -p "$JACKIN_CONFIG_DIR"
fi

mkdir -p "$JACKIN_HOME_DIR"
```

Expected: the operator's real config is copied into the PR-scoped config dir,
and `JACKIN_HOME_DIR` exists as an empty PR-scoped state dir. The operator's
live `~/.config/jackin` is only read for the initial copy, and live
`~/.jackin` is not copied or read; later commands run with the Checkout block's
PR-scoped env vars.

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

Vite serves at `http://localhost:3000/`. Pages to walk:

**http://localhost:3000/<path>/**  
<NEW page | UPDATED ...>. <One-sentence description of what to look at on this
page. Include the sidebar group and surrounding entries when adding or moving
sidebar items.>

**http://localhost:3000/<path>/**  
<Same shape — bold URL line, soft-break, description on next line, blank line
between blocks.>

## Migration notes

<"None." during pre-release is fine. Otherwise: one paragraph naming what
operators have to do — schema rename, env-var addition, on-disk path move,
etc. Skip the section entirely when it would just say "None.">
