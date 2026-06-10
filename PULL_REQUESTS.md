# PULL_REQUESTS.md

Canonical guide for how pull requests are created, iterated on, reviewed, and merged in this repository. Applies to AI agents and human contributors. Linked from [`AGENTS.md`](AGENTS.md).

Read this file before opening, updating, or merging a pull request.

## Two reading surfaces

PR rules are split by audience to avoid duplication:

- **This file** is the **shared** PR flow — body-shape spec, Verify-locally policy, mandatory isolation env-var rule, docs-only PR requirements, review rules, roadmap-retirement procedure. Both humans and agents start here.
- [`.github/AGENTS.md`](.github/AGENTS.md) is the **agent-only extras** — per-PR merge authorization, base-branch requirement, force-push policy, body-construction shell-quoting rules, iteration-vs-merge-readiness behavior, CI-green-before-merge, title/description reconciliation, squash-merge format, and the `jackin-capsule` smoke-test mandate. Also covers GitHub Actions workflow authoring (mise-only installs, env scope, publish gating). Agents read this in addition to the shared file; the `.github/CLAUDE.md` include makes Claude Code auto-load it whenever working under `.github/`.

When agent-only and shared rules cover the same topic (e.g. "include a Verify-locally section"), the shared rule states the *what* and the agent-only rule states the agent-specific *how/when/who*.

## Canonical body shape — see `.github/PULL_REQUEST_TEMPLATE.md`

The reference template lives at [`.github/PULL_REQUEST_TEMPLATE.md`](.github/PULL_REQUEST_TEMPLATE.md). Copy it as the starting point for every new PR body and fill in the placeholders. GitHub also auto-loads it when a PR is opened through the web UI.

Sections, in order (drop the optional ones when they don't apply — the template comments call out which):

1. **Summary** — one paragraph answering what the PR is for: the shipped feature or behavior, who benefits, and how it changes their flow. Keep the detail for the next sections. Cross-references to other docs by name (no `/reference/...` links).
2. **What ships** — feature-level bullets grouped by user-visible or contributor-visible outcome: capabilities, behavior, configuration surfaces, docs, and validation outcomes. It is not a function/struct inventory or a file-by-file changelog.
3. **Behavior changes** *(optional — only when it adds signal beyond "What ships")* — bullets naming changed defaults, validation, errors, migration behavior, docs behavior, CI behavior, launch/runtime behavior, or cleanup semantics.
4. **What this addresses** — bullets naming the practical problem, roadmap gap, regression, or operator pain that is now resolved. This should explain what in reality changed for the project, not restate implementation mechanics.
5. **Hard rule / impact callout** *(optional — only when the PR introduces or honours a non-trivial cross-cutting rule)* — one paragraph naming the rule, what it blocks, where the full rationale lives.
6. **Not included** *(optional — only when scope boundaries or deferred work are useful to call out)* — bulleted list of explicit out-of-scope items, follow-up PRs, research-stage work, or related behavior intentionally left unchanged.
7. **Verify locally** — copy-pasteable steps the operator runs, structured by intent. Start from the sections in the template and keep only the blocks that apply.
8. **Migration notes** — short. "None" is a valid answer during pre-release; drop the section entirely when there's nothing to say.

What the template deliberately omits:

- File-by-file changelog (visible in the diff).
- Function, struct, constant, fixture-count, or file-path inventory that only repeats what the diff shows.
- Full test list (visible in the test runner output).
- Design rationale for every sub-decision (lives in the contributor doc the PR adds or updates).
- Links to deployed docs URLs (those break post-merge; see "[Never link deployed docs from the PR body](#pr-body--keep-it-tight-let-github-flow-the-text)").
- Mechanical CI-shaped checks (sidebar diffs, link audits, file-tree assertions belong in CI, not in the PR body). The one exception is the docs verification gate (the **Docs Checks** block), which AGENTS.md requires docs authors run from `docs/` before merge.

## Include local checkout instructions in every PR

Every pull request must include a copy-pasteable "Verify locally" section in the PR body. Agents creating PRs must also repeat the same commands in their final response after sharing the PR URL (agent-specific rule — see [`.github/AGENTS.md`](.github/AGENTS.md)).

Use `cargo xtask pr prepare <PR_NUMBER>` from the template with the real PR number and verification commands for the change. The xtask starts from a PR-specific test directory (`$HOME/Projects/jackin-project/test/pr-<PR_NUMBER>`) so the operator can inspect multiple PRs at once without checkout collisions. It uses the PR number instead of the branch name for this directory: PR numbers are unique and stable, while branch names can contain slashes, be reused, or change during iteration. The clone/fetch step is idempotent, force-updates the fetched ref, prefers the actual head branch name for same-repository PRs, and falls back to GitHub's synthetic PR ref for fork PRs.

Split verification into named blocks only when each block contains meaningful commands. Always include checkout instructions. Add Static Checks only when there is a local check worth running beyond CI and GitHub's diff UI. Add Rust tests only when there is a relevant Rust test command for the project. Add Docs checks only when there is a relevant automated docs command; use the template's docs gate rather than restating it here. Keep Rust tests and Docs checks in separate blocks; docs tests validate the published documentation surface and docs tooling, not the Rust project itself. Add User Smoke only when the operator can exercise changed behavior locally, such as CLI, runtime, workspace, Docker, TUI, or operator-flow changes. Do not add placeholder sections that say no test applies, and do not add commands that only print files for review. For CLI/runtime smoke, run the local checkout's `jackin` binary and exercise the behavior touched by the PR. When the behavior is reachable from jackin' console, the User Smoke block must lead with the console command from the template because it is the operator's most intuitive end-to-end validation path. Follow it with the exact keys/clicks, setup commands, and expected state needed to make the changed behavior visible. Direct subcommand invocations belong after the console smoke as faster repeat checks, or as the primary smoke path only when the changed behavior has no meaningful console route. Prose like "open the console and verify the tab" is incomplete unless it is preceded by the command the operator should paste and the state-seeding commands needed for the UI to show the changed behavior. For subcommands that do not support `--debug`, include the closest supported debug command in the same smoke block and explain the gap in one sentence.

### jackin-capsule PRs

Any PR touching `crates/jackin-capsule/` requires the Checkout block to build and export the capsule binary before any `jackin` smoke command, plus a dedicated `### jackin-capsule smoke` block:

1. The Checkout block uses `cargo xtask pr prepare <PR_NUMBER> --capsule`, then sources the generated env file. **It must stay in Checkout, before `### User smoke` and `### jackin-capsule smoke`.** Every `jackin console` / `jackin load` invocation after it consumes whichever binary `ensure_available` resolves first — so without the capsule export first, the launches use the cached or preview-release binary and silently do not exercise the PR's container-side changes.
2. `### jackin-capsule smoke` uses the template's launch and in-container verify checklist. It does not repeat the capsule export; the Checkout block already exported `JACKIN_CAPSULE_BIN`.

The full rule — `ensure_available` resolution order, why hand-rolled `target/<triple>/release/...` exports are forbidden, the required verify checklist, prefix-surface opt-in — lives in [`.github/AGENTS.md`](.github/AGENTS.md) under `## jackin-capsule PRs (hard rule)`. The PR template at [`.github/PULL_REQUEST_TEMPLATE.md`](.github/PULL_REQUEST_TEMPLATE.md) ships the checkout command and smoke block in the correct order; copy them rather than rewriting the build invocation.

A `crates/jackin-capsule/` PR that puts a `jackin` launch before the Checkout block's `--capsule` prepare step, or omits `--capsule` entirely, is incomplete. Unit tests passing is necessary but not sufficient.

### Documentation-only PRs

Documentation PRs (changes under `docs/**` only — `.mdx` files, `astro.config.ts` sidebar, theme/CSS) must verify by running the docs site **locally** in addition to checkout, not by pointing the operator at the GitHub Files-changed tab.

The Files-changed tab shows raw MDX. It does not show how Starlight renders the page, whether `<RepoFile />` resolves, whether the sidebar entry lands in the right group, whether internal `[link](/path/)` references resolve, whether tables and Asides render correctly, or whether the page is even reachable through navigation. A docs PR that "looks right in the diff" can render visibly broken on the site.

Required pattern for docs-only PRs:

1. **Checkout block** — same as any other PR.
2. **Docs checks** — run the automated docs verification gate from the template before the manual walk.
3. **Run the docs site locally** — use the `### Documentation` block from the template.
4. **Direct links to every changed page** — for each affected docs page, include a localhost URL the operator can click straight into. For new pages, also tell the operator which sidebar group the entry should appear under, so they can confirm the navigation lands in the right place.

A `.mdx`-only PR that omits the Docs checks gate or the local-render step is incomplete. The Files-changed tab is the operator's last-resort fallback, not the primary review surface.

### Verify-locally section policy

The exact copy-paste commands live only in [`.github/PULL_REQUEST_TEMPLATE.md`](.github/PULL_REQUEST_TEMPLATE.md). Do not duplicate those commands here; update the template when a command changes. This file describes when each block is required and the invariants each block protects.

The Checkout step in the template is split into two separate code fences: one paste disables the `tirith` paste scanner for the rest of the session, and the second paste performs the checkout. Keep that split in PR bodies.

The checkout recipe must keep these properties:

- It fetches the real PR head branch into a PR-numbered test directory.
- It force-updates the remote-tracking ref so operator verification survives authorized force-pushes.
- It builds the local `jackin` binary and prepends `target/debug` so smoke commands exercise the PR checkout, not a previously installed binary.
- It exports PR-scoped `JACKIN_CONFIG_DIR="$HOME/.config/jackin-pr-<PR_NUMBER>"` and `JACKIN_HOME_DIR="$HOME/.jackin-pr-<PR_NUMBER>"` so schema migrations, workspace writes, role caches, and runtime state do not touch the operator's live config/state.
- For `crates/jackin-capsule/` PRs, it uses `--capsule` before any `jackin console` or `jackin load` smoke command.

For non-trivial code changes, structure the PR's "Verify locally" section by intent:

- **Checkout** — copy-pasteable commands to fetch and check out the PR.
- **Static Checks** — only checks that are relevant and expected to be run locally.
- **Rust Tests** — focused or full Rust test commands that validate the changed behavior.
- **Schema Migration Smoke** — only for PRs that bump a versioned schema. Config/workspace migrations copy only the operator's real `~/.config/jackin` into the PR-scoped config dir, keep `JACKIN_HOME_DIR` empty and PR-scoped, then run the PR binary against the copied config. Role manifest migrations copy a role repo into the PR test directory, then migrate the copy.
- **Docs Checks** — automated `bun` commands from `docs/` that validate the rendered docs project, repo links, TypeScript, and docs test suite.
- **User Smoke** — manual validation steps when behavior is visible in the CLI/TUI/runtime.

Do not add generic commands that do not materially validate the PR. In particular, do not include `git diff --check` unless the PR is specifically about whitespace, patch hygiene, generated diffs, or another issue that command is meant to catch.

For console/TUI changes, workspace flows, and runtime behavior that can be manually verified through jackin' itself, put the console smoke first and then list the keys/clicks the operator should walk. If the TUI change depends on config or workspace state, seed that state in the PR body before the console command using the template's isolated env-var pattern. For CLI subcommand changes, include the exact subcommand invocation and the expected output or persisted file change.

#### Isolation env vars

Three env vars let the operator test a PR without touching their live config or state:

| Var | Default | Overrides |
|-----|---------|-----------|
| `JACKIN_CONFIG_DIR` | `~/.config/jackin` | config.toml, workspaces/ |
| `JACKIN_HOME_DIR` | `~/.jackin` | data/, roles/, cache/ |
| `JACKIN_CONSTRUCT_IMAGE` | `projectjackin/construct:trixie` | construct image used for role validation and launch |

`JACKIN_CONFIG_DIR` and `JACKIN_HOME_DIR` are mandatory in the Checkout block for every PR, including docs-only and pure-refactor PRs. The operator may paste the same checkout block before deciding which smoke commands to run, and schema/state writes can happen from surprising places such as first-load config sync. The xtask writes them as PR-numbered home directories so every PR gets a removable copy of config and runtime state.

For construct image PRs, use `cargo xtask pr prepare <PR_NUMBER> --construct` from the template. It builds a local construct image and points jackin' at that image for Dockerfile validation and role container launch instead of the published one.

Do not include `JACKIN_CONSTRUCT_IMAGE` in PRs that do not touch the construct image — the isolation pattern is about scoping test risk, not about exhaustively listing every available env var.

## PR body — keep it tight, let GitHub flow the text

The PR body is read in GitHub's renderer, which already wraps long lines at the viewport width. Treat that as the source of truth for line breaks and follow these rules:

- **Do not hard-wrap prose at ~70 columns.** Write each paragraph as a single long line in the source. GitHub will wrap it at display time. Source-side line breaks at column 70 produce output where every other line ends mid-sentence, which is much harder to read than a flowing paragraph. The exception is code fences and bullet contents that already encode meaningful line breaks.
- **Feature detail, not implementation inventory.** A PR body explains *what shipped*, *what changed in reality*, and *how to verify it*. Use the **What ships** section for feature-level outcomes: new operator flows, capabilities, configuration surfaces, docs, and validation coverage. Use **Behavior changes** for changed defaults, validation, errors, migrations, launch/runtime effects, cleanup semantics, docs behavior, or CI behavior. Do not list function names, struct names, constants, raw fixture counts, or every touched file unless the name itself is the public surface the operator uses.
- **No verbosity, no duplication.** A PR body does not duplicate the design rationale (that lives in the contributor doc the PR adds or updates), the file-by-file changelog (visible in the PR diff), or the test list (visible in the test-runner output). Trim every sentence that exists in two places. Default to 100–200 lines for a substantial PR; 400+ lines is a smell.
- **Never link deployed docs from the PR body.** Operator-facing docs URLs, roadmap pages, and any deployed docs link can move, rename, or 404 after the PR merges. The PR body becomes a permanent commit attribution after squash-merge, so a broken link is permanent. Use the localhost render URL shape from the template inside the **Verify locally → Documentation** block — those links are valid only at verification time and are obviously local. Refer to other docs by name, not URL: write *"the GitHub CLI authentication strategy roadmap doc"*, not a link to it.
- **No mechanical / CI-shaped checks in the PR body.** Anything fully deterministic — sidebar diffs, link audits, file-tree assertions, "did you remember to update the changelog" greps — belongs in CI, not in a checklist the operator has to copy-paste. The PR body is for the operator-facing verification path: build, test, run the binary, render the docs. If a mechanical check is missing from CI today, file a follow-up to add it; do not promote it into every PR body in the meantime. The one exception is the docs verification gate from the template. It is the single sanctioned copy-paste mechanical check because parts of the docs gate have no CI backstop today, so the operator running them locally is the only gate; AGENTS.md requires the gate before a docs-touching PR is merge-ready.
- **Verify-locally documentation block: one block per page.** Use the URL-and-description shape from the template for each page operators should walk. Do not use headings for the URLs, and do not bury the URL in prose with the description tail-trailing it on the same wrapped line.

For agent-side body construction (shell quoting, `gh pr create --body-file` vs `--body`, heredoc pattern), see [`.github/AGENTS.md`](.github/AGENTS.md) under `## Author the PR body so it renders correctly on GitHub`.

## Reviewing a PR

Three cross-cutting rules apply to every PR review (manual, agent-driven, or automated) before output ships:

### Versioned-schema migration check

Missing or stale fixtures under `tests/fixtures/migrations/` break the smooth-migration guarantee for operators upgrading from older versions. When the diff touches a struct serialized into `config.toml`, `~/.config/jackin/workspaces/<name>.toml`, or `jackin.role.toml`, verify the PR ships with all five required artifacts: version bump, migration step, new fixture directory, re-baked `after.toml` files for every existing `from_version`, and a new entry in the `schema-versions.mdx` timeline. The full rule lives in `AGENTS.md` under "Project status: pre-release."

### Accepted-exceptions catalog

Do not flag items listed under "Accepted exceptions" on the [Open review findings](docs/content/docs/reference/roadmap/open-review-findings.mdx) roadmap catalog. Those items are retained intentionally and have been reviewed.

The catalog itself is a forward-looking backlog — consult it on demand when a review task calls for it. It is not operational context and should not be loaded at session start.

### Always check the PR against the jackin' design principles

Every PR review must explicitly verify the change against the jackin' [design principles](docs/content/docs/getting-started/design-principles.mdx). Read that page before producing review output. If a change appears to contradict any principle (most commonly: *never mutate the host machine silently*, *operator-only configuration boundaries*, *container is the trust boundary, not the prompt*), flag it loudly in the review with a specific reference to which principle is at risk.

Don't silently let a principle violation pass because the diff is small or the operator seemed to want the shortcut. The whole point of the principles is that operators rely on them across every feature — a quietly-merged exception erodes that contract for every future PR.

When you flag a possible violation, surface it like this:

> **Design-principle check:** *<principle name>* — *<one-sentence summary of what the diff does that risks the principle>*. Operator decision required: keep the change as proposed (and update the principle / add an explicit opt-in), narrow the change to stay inside the principle, or drop it.

The operator's call decides the outcome — the agent's job is to make sure the question is asked, not to silently approve or block. New principles or principle changes happen at design-principles-page level (with a PR), not inside an unrelated feature PR.

### Always check TUI changes against the TUI design decisions

Every PR review that touches console, capsule, or any other terminal UI surface must explicitly verify the change against the jackin' [TUI design decisions](docs/content/docs/reference/tui-design-decisions.mdx). Read that page before producing review output. Reviewers must reject or flag TUI changes that miss the documented interaction cues: long-running or background work needs an explicit in-surface progress/status state; clickable targets need a distinct resting style, a visible hover style change, and pointer-shape feedback where supported; active keys need footer hints; focus and scroll geometry must use the shared rules.

For every TUI action that can wait on I/O, Docker, git, network, a background worker, token generation, or any other noticeably slow operation, the review must answer: after the operator commits the action, what visible state tells them work is happening before the result appears? If the answer is "the screen stays unchanged until it finishes," the PR violates the TUI design decisions and must be fixed before it lands.

Surface TUI issues like this:

> **TUI design-decision check:** *<rule name>* — *<one-sentence summary of what the diff does that risks the rule>*. Required fix: align the implementation with the TUI design decisions page, or update that page first if the design contract is intentionally changing.

## Retire fully-resolved roadmap items in the same PR

When a PR ships the last remaining piece of a roadmap item — every feature, sub-phase, and follow-up tracked by the page is now implemented — delete the roadmap `.mdx` file in that same PR rather than leaving it behind as a `Status: Resolved` page. The retirement steps:

1. **Confirm there is no remaining work.** Re-read the page top to bottom. Any "Remaining Work", "Future Work", "Phase N — open", or open question that is not actually shipped is a remaining-work signal — keep the page and update its status to `Partially implemented` instead.
2. **Confirm no load-bearing inbound links.** `rg "roadmap/<slug>" docs/` from the repo root. References from the roadmap overview and the sidebar config are expected and get cleaned up below; references from *open* roadmap items mean the page is acting as an internal contract for unfinished work — keep it, or repoint those references first.
3. **Audit every detail on the page and place it in its long-term home.** Operator behaviour goes to a `guides/` or `commands/` page so users can learn the feature without reading internals; design decisions, on-disk layout, struct/enum/function names, and architecture trade-offs go to `reference/architecture.mdx`, `reference/configuration.mdx`, `reference/codebase-map.mdx`, or another internals page so the next contributor reads accurate internals. The git history is the long-term archive of design rationale; the roadmap directory is not. Apply the **Documentation as the source of truth** rule in `AGENTS.md` for the audience split — never inline TOML schemas, on-disk paths, or struct names on the user-facing pages, and never put `jackin foo --bar` operator instructions on internals pages.
4. **Replace the page with a single bullet in the Completed section** of `docs/content/docs/reference/roadmap/index.mdx`. The bullet names the feature in plain prose and links to the canonical user-facing or contributor-facing doc that now describes the shipped behaviour. No link back to a deleted roadmap page.
5. **Repoint inbound references.** Update any open roadmap item, goal prompt, or contributor doc that linked to the deleted page; point them at the canonical home from step 3 instead.
6. **Run the sidebar and overview audits** documented in `docs/AGENTS.md`. The sidebar audit must show no diff after deleting the entry from `docs/astro.config.ts`. The overview audit must continue to pass (every roadmap file is reachable from `roadmap.mdx` or covered by a parent program entry).
7. **Run the docs verification gate.** Use the template's Docs Checks block. A retirement that breaks the build or repo-link references is incomplete.

A `Status: Resolved` roadmap page that still sits in the directory is a smell, not a shipping target. The only legitimate reasons to keep one are (a) genuine remaining work tracked on the same page, or (b) load-bearing inbound links from open roadmap items that still treat the page as an internal contract. Anything else gets retired in the PR that ships the last piece — not deferred to a later cleanup PR, because every later contributor reading the resolved page treats it as authoritative until it is gone.

## Agent-only rules — see `.github/AGENTS.md`

The following rules apply only to agents and live in [`.github/AGENTS.md`](.github/AGENTS.md). Read that file before opening, iterating on, or merging a PR as an agent:

- **Per-PR merge authorization** — agents never merge without explicit "merge it" confirmation; prior session authorizations don't carry forward.
- **Base branch** — agent-created PRs target `main` unless the operator explicitly names a different target.
- **Force-push authorization** — agents never rewrite an existing remote branch without explicit operator approval.
- **PR-body refresh policy** — refresh on operator request or at merge-readiness, not after every iteration commit.
- **Body construction** — `gh pr create --body-file` (not `--body "..."`), single-quoted heredoc, no escaped backticks or `$`.
- **Applying review fixes** — commit to the existing PR branch, not a new one.
- **Iterating on operator feedback** — narrow targeted checks during iteration; full verification suite only at merge-readiness.
- **CI must be green before merging** — `gh pr checks` confirmation before every merge; no `--admin` bypass without explicit per-failure authorization.
- **Verify PR title/description before merging** — reconcile metadata with the diff before invoking `gh pr merge`.
- **PR squash merge messages** — squash-only, `(#PR_NUMBER)` suffix, `Signed-off-by` + `Co-authored-by` trailers at the end.
- **`jackin-capsule` PRs** — the `--capsule` prepare path, the verify checklist, the prefix-surface opt-in.

## Workflow / CI changes — see `.github/AGENTS.md`

All rules for authoring and modifying CI workflow files live in [`.github/AGENTS.md`](.github/AGENTS.md). Read that file before modifying any workflow. It covers:

- **mise-only tool installation** — no language-specific setup actions; `jdx/mise-action` everywhere.
- **Env-var scope** — third-party-CLI env vars at job level, never workflow level.
- **Publishing gates** — registry / release / Homebrew steps must hard-gate on `main`.
- **Smoke-testing push-only jobs** — `gh workflow run --ref <branch>` before merge for jobs that don't run on `pull_request`.
