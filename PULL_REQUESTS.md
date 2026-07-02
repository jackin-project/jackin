# PULL_REQUESTS.md

Canonical guide: how PRs created, iterated, reviewed, merged. Applies to AI agents + humans.

Read before opening, updating, merging PR.

## Two reading surfaces

PR rules split by audience, avoid duplication:

- **This file** — **shared** PR flow: body-shape spec, Verify-locally policy, mandatory isolation env-var rule, docs-only PR requirements, review rules, roadmap-retirement procedure. Humans + agents start here.
- `AGENTS.md` under `.github/` holds **agent-only extras** — per-PR merge auth, base-branch requirement, force-push policy, body-construction shell-quoting, iteration-vs-merge-readiness, CI-green-before-merge, title/description reconciliation, squash-merge format, `jackin-capsule` smoke-test mandate. Plus GitHub Actions workflow authoring (mise-only installs, env scope, publish gating). Auto-loads when agent works under `.github/`.

When agent-only + shared rules cover same topic (e.g. "include Verify-locally section"), shared rule states *what*, agent-only states agent-specific *how/when/who*.

## Canonical body shape — see `.github/PULL_REQUEST_TEMPLATE.md`

Template at [`.github/PULL_REQUEST_TEMPLATE.md`](.github/PULL_REQUEST_TEMPLATE.md). Copy as start for every new PR body; fill placeholders. GitHub auto-loads when PR opened via web UI.

Sections, in order (drop optional when N/A — template comments say which):

1. **Summary** — one paragraph: what PR for: shipped feature/behavior, who benefits, how flow changes. Detail belongs later sections. Cross-reference other docs by name (no `/reference/...` links).
2. **What ships** — feature-level bullets grouped by user-visible or contributor-visible outcome: capabilities, behavior, config surfaces, docs, validation outcomes. Not function/struct inventory or file-by-file changelog.
3. **Behavior changes** *(optional — only when adds signal beyond "What ships")* — bullets naming changed defaults, validation, errors, migration, docs, CI, launch/runtime, cleanup semantics.
4. **What this addresses** — bullets naming practical problem, roadmap gap, regression, or operator pain now resolved. Explain what changed in reality, not implementation mechanics.
5. **Hard rule / impact callout** *(optional — only when PR introduces or honours non-trivial cross-cutting rule)* — one paragraph naming rule, what it blocks, where full rationale lives.
6. **Not included** *(optional — only when scope boundaries or deferred work worth calling out)* — bullets: explicit out-of-scope items, follow-up PRs, research-stage work, related behavior intentionally left unchanged.
7. **Verify locally** — copy-pasteable steps operator runs, structured by intent. Start from template sections; keep only applicable blocks.
8. **Migration notes** — short. "None" valid during pre-release; drop section when nothing to say.

Template deliberately omits:

- File-by-file changelog (in diff).
- Function/struct/constant/fixture-count/file-path inventory repeating diff.
- Full test list (in test runner output).
- Design rationale for every sub-decision (in contributor doc PR adds/updates).
- Links to deployed docs URLs (break post-merge; see "[Never link deployed docs from the PR body](#pr-body--keep-it-tight-let-github-flow-the-text)").
- Mechanical CI-shaped checks (sidebar diffs, link audits, file-tree assertions belong in CI, not PR body). One exception: docs verification gate (**Docs Checks** block), which AGENTS.md requires docs authors run from `docs/` before merge.

## Include local checkout instructions in every PR

Every PR must include copy-pasteable "Verify locally" section in body. Agents creating PRs must also repeat same commands in final response after sharing PR URL (agent-specific rule, governed by rules under `.github/`).

Use template's `jackin-dev pr sync <PR_NUMBER>` checkout flow with real PR number + verification commands. `jackin-dev` creates or refreshes `$HOME/Projects/jackin-project/test/pr-<PR_NUMBER>/jackin`, prepares isolated config/state under the same PR bundle, checks out the PR's real head branch, builds the local binary, builds and exports a local capsule when the diff changes `jackin-capsule` or a workspace package in its dependency closure, writes `env.sh`, and prints the next commands. The bundle starts from a PR-specific test directory so operator can inspect multiple PRs at once without checkout collisions. Uses PR number, not branch name, for directory; branch prompt still shows the PR's actual head branch.

Split verification into named blocks only when each block contains meaningful commands. Always include checkout instructions. Add Static Checks only when local check worth running beyond CI + GitHub's diff UI. Add Rust tests only when relevant Rust test command exists. Add Docs checks only when relevant automated docs command exists; use template's docs gate rather than restating here. Keep Rust tests + Docs checks separate blocks; docs tests validate published documentation surface + docs tooling, not Rust project. Add User Smoke only when operator can exercise changed behavior locally (CLI, runtime, workspace, Docker, TUI, operator-flow changes). No placeholder sections saying no test applies; no commands that only print files for review. For CLI/runtime smoke, run local checkout's `jackin` binary + exercise behavior touched by PR. When behavior reachable from jackin❯ console, User Smoke block must lead with console command from template — operator's most intuitive end-to-end validation path. Follow with exact keys/clicks, setup commands, expected state needed to make changed behavior visible. Direct subcommand invocations belong after console smoke as faster repeat checks, or as primary smoke path only when changed behavior has no meaningful console route. Prose like "open the console and verify the tab" incomplete unless preceded by command operator pastes + state-seeding commands needed for UI to show changed behavior. For subcommands without `--debug`, include closest supported debug command in same smoke block + explain gap in one sentence.

### jackin-capsule PRs

Any PR touching `crates/jackin-capsule/` requires Checkout block to build + export capsule binary before any `jackin` smoke command, plus dedicated `### jackin-capsule smoke` block:

1. Checkout block uses `jackin-dev pr sync <PR_NUMBER>`, then sources generated env file. **Must stay in Checkout, before `### User smoke` and `### jackin-capsule smoke`.** Every `jackin console` / `jackin load` after it consumes whichever binary `ensure_available` resolves first — without capsule export first, launches use cached or preview-release binary + silently skip PR's container-side changes.
2. `### jackin-capsule smoke` uses template's launch + in-container verify checklist. Does not repeat capsule export; Checkout block exports `JACKIN_CAPSULE_BIN` for capsule-affecting PRs.

`jackin-dev pr sync` cannot mutate parent shell directly. It writes `JACKIN_CAPSULE_BIN` into generated `env.sh` only when the PR requires a local capsule; Checkout block must source that file before any smoke command. If PR also needs a local construct image, `jackin-dev` detects construct inputs from the diff and writes `JACKIN_CONSTRUCT_IMAGE` into the same env file.

Full rule — `ensure_available` resolution order, why hand-rolled `target/<triple>/release/...` exports forbidden, required verify checklist, prefix-surface opt-in — lives under `## jackin-capsule PRs (hard rule)` section of rules under `.github/`. PR template at [`.github/PULL_REQUEST_TEMPLATE.md`](.github/PULL_REQUEST_TEMPLATE.md) ships checkout command + smoke block in correct order; copy rather than rewriting build invocation.

A `crates/jackin-capsule/` PR that puts a `jackin` launch before Checkout block's `jackin-dev pr sync` step is incomplete. Unit tests passing necessary but not sufficient.

### Documentation-only PRs

Documentation PRs (changes under `docs/**` only — `.mdx` files, `astro.config.ts` sidebar, theme/CSS) must verify by running docs site **locally** in addition to checkout, not by pointing operator at GitHub Files-changed tab.

Files-changed tab shows raw MDX. Does not show how Starlight renders page, whether `<RepoFile />` resolves, whether sidebar entry lands right group, whether internal `[link](/path/)` references resolve, whether tables/Asides render, or whether page reachable through navigation. Docs PR that "looks right in diff" can render visibly broken on site.

Required pattern for docs-only PRs:

1. **Checkout block** — same as any other PR.
2. **Docs checks** — run automated docs verification gate from template before manual walk.
3. **Run docs site locally** — use `### Documentation` block from template.
4. **Direct links to every changed page** — for each affected docs page, include localhost URL operator can click into. For new pages, also tell operator which sidebar group entry should appear under, so they confirm navigation lands right.

A `.mdx`-only PR that omits Docs checks gate or local-render step incomplete. Files-changed tab last-resort fallback, not primary review surface.

### Verify-locally section policy

Exact copy-paste commands live only in [`.github/PULL_REQUEST_TEMPLATE.md`](.github/PULL_REQUEST_TEMPLATE.md). Do not duplicate here; update template when command changes. This file describes when each block required + invariants each protects.

Checkout step in template splits into two code fences: one paste disables `tirith` paste scanner for rest of session, second paste performs checkout. Keep that split in PR bodies.

Checkout recipe must keep these properties:

- Fetches real PR head branch into PR-numbered test directory.
- Force-updates remote-tracking ref so operator verification survives authorized force-pushes.
- Bootstraps working repository before invoking repo-local build commands, so operator can start from empty test directory.
- Builds local `jackin` binary + prepends `target/debug` so smoke commands exercise PR checkout, not previously installed binary.
- Exports PR-scoped `JACKIN_CONFIG_DIR="$HOME/Projects/jackin-project/test/pr-<PR_NUMBER>/state/config"` and `JACKIN_HOME_DIR="$HOME/Projects/jackin-project/test/pr-<PR_NUMBER>/state/home"` so schema migrations, workspace writes, role caches, runtime state don't touch operator's live config/state.
- Auto-builds and exports the local capsule before any `jackin console` or `jackin load` smoke command when the changed package is in the `jackin-capsule` dependency closure.

For non-trivial code changes, structure PR's "Verify locally" section by intent:

- **Checkout** — copy-pasteable commands to fetch + check out PR.
- **Static Checks** — only checks relevant + expected to run locally.
- **Rust Tests** — focused or full Rust test commands validating changed behavior.
- **Schema Migration Smoke** — only for PRs bumping versioned schema. Config/workspace migrations copy only operator's real `~/.config/jackin` into PR-scoped config dir, keep `JACKIN_HOME_DIR` empty + PR-scoped, then run PR binary against copied config. Role manifest migrations copy role repo into PR test directory, then migrate copy.
- **Docs Checks** — automated `bun` commands from `docs/` validating rendered docs project, repo links, TypeScript, docs test suite.
- **User Smoke** — manual validation steps when behavior visible in CLI/TUI/runtime.

No generic commands that don't materially validate PR. Particularly, no `git diff --check` unless PR specifically about whitespace, patch hygiene, generated diffs, or another issue that command catches.

For console/TUI changes, workspace flows, runtime behavior manually verifiable through jackin❯, put console smoke first then list keys/clicks operator walks. If TUI change depends on config or workspace state, seed that state in PR body before console command using template's isolated env-var pattern. For CLI subcommand changes, include exact subcommand invocation + expected output or persisted file change.

#### Isolation env vars

Three env vars let operator test PR without touching live config or state:

| Var | Default | Overrides |
|-----|---------|-----------|
| `JACKIN_CONFIG_DIR` | `~/.config/jackin` | config.toml, workspaces/ |
| `JACKIN_HOME_DIR` | `~/.jackin` | data/, roles/, cache/ |
| `JACKIN_CONSTRUCT_IMAGE` | `projectjackin/construct:trixie` | construct image used for role validation and launch |

`JACKIN_CONFIG_DIR` and `JACKIN_HOME_DIR` mandatory in Checkout block for every PR, including docs-only + pure-refactor PRs. Operator may paste same checkout block before deciding which smoke commands to run, and schema/state writes can happen from surprising places like first-load config sync. `jackin-dev` writes them under the PR-numbered test bundle so every PR gets one removable copy of config + runtime state.

For construct image PRs, `jackin-dev pr sync <PR_NUMBER>` detects construct inputs from the diff, builds the local construct image, and points jackin❯ at it for Dockerfile validation + role container launch instead of published one. The same sync command builds and exports the local capsule only when the PR also affects the capsule dependency closure.

No `JACKIN_CONSTRUCT_IMAGE` in PRs that don't touch construct image — isolation pattern scopes test risk, not exhaustively listing every env var.

## PR body — keep it tight, let GitHub flow the text

PR body read in GitHub's renderer, which wraps long lines at viewport width. Treat as source of truth for line breaks:

- **Do not hard-wrap prose at ~70 columns.** Write each paragraph as single long line in source. GitHub wraps at display time. Source-side breaks at column 70 produce output where every other line ends mid-sentence — harder to read. Exception: code fences + bullet contents that already encode meaningful line breaks.
- **Feature detail, not implementation inventory.** PR body explains *what shipped*, *what changed in reality*, *how to verify*. Use **What ships** for feature-level outcomes: new operator flows, capabilities, config surfaces, docs, validation coverage. Use **Behavior changes** for changed defaults, validation, errors, migrations, launch/runtime effects, cleanup semantics, docs behavior, CI behavior. No function names, struct names, constants, raw fixture counts, or every touched file unless name itself public surface operator uses.
- **No verbosity, no duplication.** PR body does not duplicate design rationale (in contributor doc PR adds/updates), file-by-file changelog (in PR diff), or test list (in test-runner output). Trim every sentence existing in two places. Default 100–200 lines for substantial PR; 400+ lines a smell.
- **Never link deployed docs from the PR body.** Operator-facing docs URLs, roadmap pages, any deployed docs link can move, rename, or 404 after merge. PR body becomes permanent commit attribution after squash-merge, so broken link permanent. Use localhost render URL shape from template inside **Verify locally → Documentation** block — those links valid only at verification time + obviously local. Refer to other docs by name, not URL: write *"the GitHub CLI authentication strategy roadmap doc"*, not a link.
- **No mechanical / CI-shaped checks in the PR body.** Anything fully deterministic — sidebar diffs, link audits, file-tree assertions, "did you update the changelog" greps — belongs in CI, not checklist operator copy-pastes. PR body for operator-facing verification path: build, test, run binary, render docs. If mechanical check missing from CI today, file follow-up to add it; don't promote into every PR body meanwhile. One exception: docs verification gate from template. Single sanctioned copy-paste mechanical check because parts of docs gate have no CI backstop today, so operator running them locally is only gate; AGENTS.md requires gate before docs-touching PR merge-ready.
- **Verify-locally documentation block: one block per page.** Use URL-and-description shape from template for each page operators walk. No headings for URLs; don't bury URL in prose with description tail-trailing on same wrapped line.

For agent-side body construction (shell quoting, `gh pr create --body-file` vs `--body`, heredoc pattern), see `## Author the PR body so it renders correctly on GitHub` section of rules under `.github/`.

## Reviewing a PR

Three cross-cutting rules apply to every PR review (manual, agent-driven, automated) before output ships:

### Versioned-schema migration check

Missing or stale fixtures under `tests/fixtures/migrations/` break smooth-migration guarantee for operators upgrading from older versions. When diff touches struct serialized into `config.toml`, `~/.config/jackin/workspaces/<name>.toml`, or `jackin.role.toml`, verify PR ships all five required artifacts: version bump, migration step, new fixture directory, re-baked `after.toml` files for every existing `from_version`, new entry in `schema-versions.mdx` timeline. Full rule lives in [`PRERELEASE.md`](PRERELEASE.md).

### Accepted-exceptions catalog

Do not flag items listed under "Accepted exceptions" on the [Open review findings](docs/content/docs/reference/roadmap/open-review-findings.mdx) roadmap catalog. Those items retained intentionally + reviewed.

Catalog forward-looking backlog — consult on demand when review task calls for it. Not operational context; don't load at session start.

### Always check the PR against the jackin❯ design principles

Every PR review must explicitly verify change against jackin❯ [design principles](docs/content/docs/getting-started/design-principles.mdx). Read that page before producing review output. If change appears to contradict any principle (most commonly: *never mutate the host machine silently*, *operator-only configuration boundaries*, *container is the trust boundary, not the prompt*), flag loudly in review with specific reference to which principle at risk.

Don't silently let principle violation pass because diff small or operator seemed to want shortcut. Operators rely on principles across every feature — quietly-merged exception erodes that contract for every future PR.

Surface possible violation like this:

> **Design-principle check:** *<principle name>* — *<one-sentence summary of what the diff does that risks the principle>*. Operator decision required: keep the change as proposed (and update the principle / add an explicit opt-in), narrow the change to stay inside the principle, or drop it.

Operator's call decides outcome — agent's job to ask question, not silently approve or block. New principles or principle changes happen at design-principles-page level (with a PR), not inside unrelated feature PR.

### Always check TUI changes against the TUI design decisions

Every PR review touching console, capsule, or any terminal UI surface must explicitly verify change against jackin❯ [TUI design decisions](docs/content/docs/reference/tui/index.mdx). Read those pages before producing review output. Reviewers must reject or flag TUI changes that miss documented interaction cues: long-running or background work needs explicit in-surface progress/status state; clickable targets need distinct resting style, visible hover style change, pointer-shape feedback where supported; active keys need footer hints; focus + scroll geometry must use shared rules.

For every TUI action that can wait on I/O, Docker, git, network, background worker, token generation, or any noticeably slow operation, review must answer: after operator commits action, what visible state tells them work happening before result appears? If answer "screen stays unchanged until it finishes," PR violates TUI design decisions + must be fixed before landing.

Surface TUI issues like this:

> **TUI design-decision check:** *<rule name>* — *<one-sentence summary of what the diff does that risks the rule>*. Required fix: align the implementation with the TUI design decisions page, or update that page first if the design contract is intentionally changing.

## Solo-maintainer review model

jackin❯ has exactly one human contributor — operator. No second reviewer available, and GitHub does not let PR author approve own PR. Shapes pre-merge confidence model:

- Branch protection on `main` does **not** require approving review (`required_approving_review_count = 0` in `jackin-github-terraform`). Don't propose raising it without concrete plan for how second human reviews every PR.
- "Get second pair of eyes" not available pre-merge. Pre-merge confidence comes from CI, path-aware aggregator status checks, strict up-to-date branch policy, agent following rules in this file — not human reviewer operator does not have.
- Multi-agent review (running `code-reviewer` / `comment-analyzer` / `silent-failure-hunter` / etc. in parallel before requesting merge) substitutes for missing second human. Treat those passes as load-bearing, not optional polish.
- For irreversible or high-blast-radius changes, prefer asking operator to confirm once more over assuming green CI sufficient. Pausing 30 seconds cheaper than bad merge absent second reviewer would have caught.

Practices designed for multi-developer teams (CODEOWNERS, mandatory second-human review, pair programming conventions, team-oriented workflow tooling) should not be proposed without concrete plan for how second human participates. This rule retires when project gains additional human reviewers.

## Roadmap freshness — check before marking any PR ready

Before marking any PR ready to land, and again whenever operator asks to merge PR, check whether change ships, advances, defers, or invalidates anything under `docs/content/docs/reference/roadmap/`. If yes, update roadmap item's `**Status**`, related files, implementation notes in same PR, then update `docs/content/docs/reference/roadmap/index.mdx` so item appears only in correct overview section.

Do this check even when PR mostly code, tests, CI, or rule changes. Roadmap operator-facing source of truth, not retrospective cleanup task. Feature landing without moving its roadmap item leaves stale planning docs behind + should be treated as incomplete. If merge request reveals stale roadmap state, stop before merging, update roadmap + PR description, only then continue normal merge verification.

Run sidebar + overview audits documented in rules under `docs/` after any roadmap status or file movement. If roadmap item partially shipped, keep it in **Partially implemented** with remaining phases named; don't duplicate same item under **Planned**.

Roadmap pages for planned, researched, designed, deferred, or remaining work. Once behavior ships, move operator details to normal docs (`guides/`, `commands/`, `reference/`) + replace roadmap detail with short status plus canonical-doc links. No long copied implementation walkthroughs in roadmap items after feature documented elsewhere.

## Documentation as the source of truth — check before marking any PR ready

**The published docs site is the spec.** Every feature jackin❯ ships must be described from two angles, both kept current in same PR that lands change:

- **User-facing docs** (the *Operator* and *Role Authoring* sidebar groups: `getting-started/`, `guides/`, `commands/`, `developing/`) describe **what jackin❯ does from outside the binary**. They answer "if I run this command or set this config, what will happen?" without naming on-disk paths operator never edits, internal Rust types, or implementation steps. Reader following only user-facing docs must use feature successfully.
- **Contributor-facing docs** (the *Internals* sidebar group: `reference/architecture.mdx`, `reference/configuration.mdx`, `reference/codebase-map.mdx`, `reference/claude-token-orchestrator.mdx`, `reference/schema-versions.mdx`, `reference/tui/`, plus active items under `reference/roadmap/`) describe **how jackin❯ is built**. On-disk layout, struct/enum/function names, design decisions, trade-offs, file paths under `src/`, links into source tree all live here.

Both surfaces load-bearing. If operator-visible behaviour ships without user-facing docs update, feature not shipped — operators can't learn it exists or how to invoke it. If internal change ships without contributor-facing docs update, next agent reading internals page debugs against stale spec.

**Before marking any PR ready to merge — and again whenever operator asks to merge it — re-verify every change against published docs and update both surfaces in same PR:**

1. Walk diff and ask, for each change: does this change what operator sees, types, or relies on? If yes, matching `guides/`, `commands/`, `getting-started/`, or `developing/` page must be updated in this PR.
2. Walk diff again and ask: does this change struct, enum, function name, on-disk path, schema version, design decision, or any other detail internals page describes? If yes, matching `reference/` page must be updated in this PR.
3. Apply **Roadmap freshness** rule above: status updates, sidebar/overview audits, retire-when-fully-resolved.
4. Run `bun run build`, `cargo xtask docs repo-links`, `bunx tsc --noEmit`, `bun test`. Docs change that doesn't compile or breaks repo-file references incomplete.

Do not split feature PR from its docs PR by default. Docs land with code that makes them true; landing later means docs wrong for the gap, and the gap exactly when other agents + operators read them. Exception: explicit "docs-only follow-up" pattern named above, which operator authorizes per case.

**Audience-correct placement not optional.** Wanting to put TOML schema fragment, on-disk path, or struct name on user-facing page → placement wrong; that detail goes on matching internals page, user-facing page links to it. Wanting to write `jackin foo --bar` operator instructions on internals page → that block belongs in `commands/` page, internals page links out. This audience split permanent + does not retire at first release; full three-audience classification lives in rules under `docs/`.

## Retire fully-resolved roadmap items in the same PR

When PR ships last remaining piece of roadmap item — every feature, sub-phase, follow-up tracked by page now implemented — delete roadmap `.mdx` file in that same PR rather than leaving `Status: Resolved` page. Retirement steps:

1. **Confirm no remaining work.** Re-read page top to bottom. Any "Remaining Work", "Future Work", "Phase N — open", or open question not actually shipped is remaining-work signal — keep page + set status `Partially implemented`.
2. **Confirm no load-bearing inbound links.** `rg "roadmap/<slug>" docs/` from repo root. References from roadmap overview + sidebar config expected + cleaned up below; references from *open* roadmap items mean page acts as internal contract for unfinished work — keep it, or repoint those references first.
3. **Audit every detail on page + place it in its long-term home.** Operator behaviour goes to `guides/` or `commands/` page so users learn feature without reading internals; design decisions, on-disk layout, struct/enum/function names, architecture trade-offs go to `reference/architecture.mdx`, `reference/configuration.mdx`, `reference/codebase-map.mdx`, or another internals page so next contributor reads accurate internals. Git history is long-term archive of design rationale; roadmap directory is not. Apply **Documentation as the source of truth** rule above for audience split — never inline TOML schemas, on-disk paths, struct names on user-facing pages, never put `jackin foo --bar` operator instructions on internals pages.
4. **Replace page with single bullet in Completed section** of `docs/content/docs/reference/roadmap/index.mdx`. Bullet names feature in plain prose + links to canonical user-facing or contributor-facing doc now describing shipped behaviour. No link back to deleted roadmap page.
5. **Repoint inbound references.** Update any open roadmap item, goal prompt, or contributor doc that linked deleted page; point at canonical home from step 3.
6. **Run sidebar + overview audits** documented in `docs/AGENTS.md`. Sidebar audit must show no diff after deleting entry from `docs/astro.config.ts`. Overview audit must continue passing (every roadmap file reachable from `roadmap.mdx` or covered by parent program entry).
7. **Run docs verification gate.** Use template's Docs Checks block. Retirement that breaks build or repo-link references incomplete.

A `Status: Resolved` roadmap page still sitting in directory is smell, not shipping target. Only legitimate reasons to keep one: (a) genuine remaining work tracked on same page, or (b) load-bearing inbound links from open roadmap items still treating page as internal contract. Anything else gets retired in PR that ships last piece — not deferred to later cleanup PR, because every later contributor reading resolved page treats it as authoritative until gone.

## Agent-only rules

Following rules apply only to agents. Full text lives in `AGENTS.md` under `.github/`, which loads automatically when agent works under that directory; summaries here keep shared flow self-contained:

- **Per-PR merge authorization** — agents never merge without explicit "merge it" confirmation; prior session authorizations don't carry forward.
- **Base branch** — agent-created PRs target `main` unless operator explicitly names different target.
- **Force-push authorization** — agents never rewrite existing remote branch without explicit operator approval.
- **PR-body refresh policy** — refresh on operator request or at merge-readiness, not after every iteration commit.
- **Body construction** — `gh pr create --body-file` (not `--body "..."`), single-quoted heredoc, no escaped backticks or `$`.
- **Applying review fixes** — commit to existing PR branch, not new one.
- **Iterating on operator feedback** — narrow targeted checks during iteration; full verification suite only at merge-readiness.
- **CI must be green before merging** — `gh pr checks` confirmation before every merge; no `--admin` bypass without explicit per-failure authorization.
- **Verify PR title/description before merging** — reconcile metadata with diff before invoking `gh pr merge`.
- **PR squash merge messages** — squash-only, `(#PR_NUMBER)` suffix, `Signed-off-by` + `Co-authored-by` trailers at end.
- **`jackin-capsule` PRs** — the `jackin-dev pr sync` capsule auto-build path, verify checklist, prefix-surface opt-in.

## Workflow / CI changes

All rules for authoring + modifying CI workflow files live in `AGENTS.md` under `.github/`, which loads automatically when agent works on workflow files there. Covers:

- **mise-only tool installation** — no language-specific setup actions; `jdx/mise-action` everywhere.
- **Env-var scope** — third-party-CLI env vars at job level, never workflow level.
- **Publishing gates** — registry / release / Homebrew steps must hard-gate on `main`.
- **Smoke-testing push-only jobs** — `gh workflow run --ref <branch>` before merge for jobs that don't run on `pull_request`.
