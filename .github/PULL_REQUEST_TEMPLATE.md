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
- Drop the headings you don't need. "Hard rule" is only when the PR introduces
  or honours a non-trivial cross-cutting rule. "What's deferred" is only for the
  first slice of a longer plan. "Migration notes" can read "None" during
  pre-release.
-->

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
