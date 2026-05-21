# Goal: Implement Phase 3 of `jackin-container` — the in-container PTY multiplexer

## What to do

Implement the **entire Phase 3 rewrite** of `jackin-container` as defined in the roadmap, in **one pull request**, **without merging it**, **without pausing to ask the operator any decisions**, and **without waiting for approval between sub-phases**. The roadmap is the spec; this prompt only governs the *process* of executing that spec end-to-end in a single autonomous run.

Make every architectural, naming, and library-version decision yourself, using best practices and the project rules. If a decision is genuinely not addressable from the roadmap, AGENTS.md, PULL_REQUESTS.md, COMMITS.md, BRANCHING.md, or TESTING.md, write a one-paragraph note in the PR body identifying the question and your chosen answer, then proceed. Never block on the operator.

## Read these first, in this order, before touching code

1. `AGENTS.md` and the files it links (`PULL_REQUESTS.md`, `COMMITS.md`, `BRANCHING.md`, `TESTING.md`, `PROJECT_STRUCTURE.md`). These define the project rules for commits, attribution, sign-off, PR bodies, push policy, and "never mutate the host silently".
2. The Phase 3 roadmap item at `docs/src/content/docs/reference/roadmap/jackin-container-binary.mdx`. Read it **end-to-end**. The load-bearing sections for this work are:
   - **Multiplexer architecture (Phase 3)** — the whole design spec
   - **Why the first attempt is being rewritten** — the five defect categories you must fix
   - **Architectural reference: zellij** — the structural model to copy at small scope
   - **Concepts to borrow from herdr (license-safe restatement)** — UI shape jackin keeps
   - **Tab and pane model** — strict tmux subset, no `window`, no `workspace`
   - **VT state via `vt100`** — the crate replacing the hand-rolled emulator
   - **Render model: hot path and cold path** — what runs every PTY chunk vs every UI change
   - **Input model: prefix key** — state machine + default bindings table
   - **Wire protocol** — control channel (NDJSON) and attach channel (binary tag+length)
   - **Resize, detach, reattach** — `SIGWINCH` propagation, persistent server, single-client takeover
   - **Status bar** — row-0 ownership, `jackin'` pill, tab strip, prefix-mode hint
   - **Initial state** — empty vs default vs Shell fallback
   - **Module map** — file layout of the rewrite
   - **Sub-phases of the Phase 3 rewrite** — the 3a/3b/3c/3d split you execute below
   - **Tests required** — the regression set you write
   - **Terminal compatibility: tmux setting parity** — extended keys, focus events, OSC passthrough, `escape-time 0`, mouse motion — each one has an explicit jackin equivalent in that section
   - **Ghostty compatibility** — kitty keyboard protocol, true colour, kitty graphics, bracketed paste, BSU/ESU, OSC 52, OSC 8, sixel
   - **What does not change** — the contract surface you must not break
3. The existing crate under `crates/jackin-container/` — every `.rs` file plus `Cargo.toml` and `build.rs`. Understand the first attempt before deleting parts of it.
4. The host-side launch and attach paths: `src/runtime/launch.rs`, `src/runtime/attach.rs`, `src/bin/build_jackin_container.rs`, and the derived image under `docker/`. The rewrite should not need host-side changes; confirm that by reading.
5. Zellij as the structural reference (Apache-2.0): https://github.com/zellij-org/zellij. Read enough of its client-server split, per-pane VT state, and IPC framing to internalise the shape. **Do not copy its code.** Reproduce the equivalents in jackin's much smaller scope.

If, while reading, you find a detail the rewrite needs that the roadmap does not yet describe, **update the roadmap first**, in its own commit (`docs(roadmap): …`), with a one-sentence rationale, before writing the affected code. The roadmap is the spec — keep it ahead of the code.

## Branch + commit policy

- Land all work on the existing Phase 3 feature branch. Identify it from `gh pr list --state open` or from the currently-checked-out branch if it is already the PR branch. **Do not** create a new branch. **Do not** rebase or force-push without explicit operator approval; only fast-forward pushes.
- One commit per sub-phase, plus one test commit, plus one roadmap-fix commit per gap (if any). Commit messages follow Conventional Commits as defined in `COMMITS.md`. Suggested titles:
  - `feat(jackin-container): replace hand-rolled VT with vt100 crate`
  - `feat(jackin-container): tmux-style prefix-key input model`
  - `feat(jackin-container): persistent server and binary attach channel`
  - `feat(jackin-container): resize, mouse, focus events, OSC passthrough, status bar polish`
  - `test(jackin-container): VT, prefix, persistence, reattach, resize, mouse, OSC passthrough`
- Every commit carries the DCO `Signed-off-by:` trailer matching the local `git config user.email`, and exactly one `Co-authored-by: Claude <noreply@anthropic.com>` trailer per the `AGENTS.md` attribution rule.
- **Push every commit immediately** after creating it. No local-only commits. The operator watches progress on the GitHub PR; commits not pushed are invisible.
- **Never `--no-verify`**, never `--no-gpg-sign`, never skip hooks. If a hook fails, fix the underlying issue and make a new commit; do not amend the previous one.

## Pre-push verification (run before every push)

- `cargo fmt --check`
- `cargo clippy --all-targets --all-features -- -D warnings`
- `cargo nextest run` (or `cargo test` if `nextest` is not configured)
- For docs commits only: `bun run --cwd docs build`, `bun run --cwd docs check:repo-links`, `bun --cwd docs test`

If clippy or tests fail, fix the underlying cause. Do not introduce blanket `#[allow(...)]` attributes. Do not weaken lints. Do not stub tests.

## PR body refresh

After the **first** push and the **last** push of this run, refresh the PR body to the shape required by `PULL_REQUESTS.md`:

- **Summary** (2–3 bullets describing the rewrite as a whole, not per-commit)
- **Hard-rule callout**: confirm the PR makes **no** host-side mutations; the socket mount is unchanged from prior phases
- **What's deferred**: Phase 4 (daemon integration, desktop bridge) and `session.attach` over a non-multiplex socket, if applicable
- **Verify locally**: include the `export TIRITH=0` block, the checkout block, and an operator smoke recipe using `cargo run --bin jackin -- console --debug` (and `cargo run --bin jackin -- load the-architect . --debug` as the alternative form). Every `jackin` invocation in the recipe **must** include `--debug` per `AGENTS.md`.
- **Migration notes**: pre-release; no migration shim

Do not link to deployed docs. Do not reference other open PRs by number. Do not narrate CI-shaped checks.

Do **not** refresh the PR body after every intermediate commit — only after the first and last pushes of this run.

## Do-not list

- **Do not merge.** No `gh pr merge`. No "ready for review" requests. The operator merges by hand after smoke-testing.
- **Do not change host-side files** (anything outside `crates/jackin-container/`, `docs/`, the PR body, and possibly `Cargo.lock`) unless the rewrite genuinely requires it. If it does, call it out in the PR body and add host-side tests.
- **Do not edit `CHANGELOG.md`** (pre-release rule in `AGENTS.md`).
- **Do not add migration shims, deprecation warnings, or backwards-compat parsers** (pre-release rule in `AGENTS.md`).
- **Do not memorialise old code shapes in comments** ("previously did X", "renamed from Y"). Git history is the record.
- **Do not pause to ask the operator.** Make the decision and proceed. The exception is the explicit operator-confirmation cases in `AGENTS.md` "Executing actions with care": force-push, dropping uncommitted local work, anything destructive. None of those should arise in this work.

## When done

When all four sub-phases plus the test commit have landed and CI is green:

1. Refresh the PR body one final time.
2. Write one short PR comment: `Phase 3 rewrite complete; ready for operator smoke.` Nothing else.
3. Stop. Do not merge. Do not request review. Do not start Phase 4.

The operator will smoke-test locally (`jackin console --debug` and `jackin load the-architect . --debug`) and merge by hand.
