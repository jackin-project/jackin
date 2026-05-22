<!--
PR body template. The shape and rules below are documented in
PULL_REQUESTS.md at the repo root — read that before authoring.

Rules in one line each:
- One paragraph per section, no hard-wrap (GitHub flows the text).
- No design rationale narration here — link out to a contributor doc instead.
- No file-by-file changelog (use the diff). No full test list (use the runner output).
- No deployed-docs URLs (they break post-merge). Refer to docs by name only.
- No mechanical CI-shaped checks (sidebar diffs, link audits). Those belong in CI.
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
mkdir -p "$HOME/Projects/jackin-project/test"
cd "$HOME/Projects/jackin-project/test"

if [ ! -d jackin/.git ]; then
  git clone https://github.com/jackin-project/jackin.git
fi

cd jackin
mise trust
git fetch -f origin <BRANCH_NAME>:refs/remotes/origin/<BRANCH_NAME>
git checkout -B <BRANCH_NAME> refs/remotes/origin/<BRANCH_NAME>
mise install
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

### Tests

```sh
cargo nextest run -E '<SCOPED_TEST_FILTER>'
cargo nextest run --all-features
```

<One sentence describing what the tests cover — provisioning, parser, error
paths, etc. Skip this paragraph when the test set is small enough that the
filter speaks for itself.>

### User smoke

```sh
cargo run --bin jackin -- console --debug
```

<List the in-container commands or UI steps the operator should walk, with
expected output where it disambiguates a pass/fail. Replace this block with the
narrower path when the PR has one (e.g. `cargo run --bin jackin -- load
<role> <target> --debug`).>

### jackin-container smoke

<Drop this whole subsection when the PR does NOT touch `crates/jackin-container/`.
Include it whenever daemon.rs, client.rs, session.rs, terminal.rs, layout.rs,
dialog.rs, statusbar.rs, input.rs, pid1.rs, or any other file under
`crates/jackin-container/src/` is changed.>

Build the binary and point `ensure_available` at it in one shot:

```sh
eval "$(cargo run --bin build-jackin-container -- --export)"
```

`build-jackin-container` invokes `cargo zigbuild`, writes the cross-compiled
Linux artifact to the host cache, and `--export` prints
`export JACKIN_CONTAINER_BIN=<path>` for `eval` to consume. The eval form is
required (not optional): hand-rolled `target/<triple>/release/jackin-container`
exports silently break when the operator switches architectures. First build
takes ~2-3 min via cargo-zigbuild; subsequent builds are incremental. Editing
any file under `crates/jackin-container/src/` does NOT auto-invalidate the
binary on disk — re-run the eval to rebuild. To purge the cache entirely:
`rm -rf ~/.jackin/cache/jackin-container/`.

Then smoke:

```sh
cargo run --bin jackin -- load the-architect . --debug
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
cd docs
bun install --frozen-lockfile
bun run dev
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
