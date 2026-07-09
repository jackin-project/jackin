# Plan 023: Phase 5 — documented-command drift gate: parse every `jackin …` doc invocation against the real clap tree

> **Executor instructions**: Follow this plan step by step. Run every
> verification command and confirm the expected result before moving to the
> next step. If anything in the "STOP conditions" section occurs, stop and
> report — do not improvise. When done, update the status row for this plan
> in `plans/code-health/README.md`.
>
> **Drift check (run first)**: `git diff --stat e80d5cc0a..HEAD -- crates/jackin/src/cli.rs crates/jackin/tests/ docs/content/docs/`
> On a mismatch with the "Current state" excerpts (in particular the `Cli`
> struct shape), treat it as a STOP condition.

## Status

- **Priority**: P2
- **Effort**: M
- **Risk**: MED (the extractor's false-positive handling is the whole difficulty; mitigations below)
- **Depends on**: none
- **Category**: docs
- **Planned at**: commit `e80d5cc0a`, 2026-07-09

## Why this matters

Roadmap Phase 5 item 11: "Extract `jackin …` invocations from docs code fences and `try_parse` them against the real clap command tree in a test, so a renamed flag fails docs CI instead of an operator." Measured blast radius: ~206 fence-scoped `jackin `-prefixed invocation lines across the docs MDX (a looser line-anchored grep catches 366 including prose false positives — e.g. `creating-roles.mdx:178` "jackin adds each declared marketplace…" is prose, which is exactly why the extractor must be fence-aware). Today a renamed subcommand or removed flag breaks operators, not CI: 121 existing `try_parse_from` call sites in the CLI's own tests all use hardcoded arg arrays; none reads the docs. The clap tree is centralized and derive-based, so the gate is one fence-aware extractor plus `Cli::try_parse_from` in a test.

## Current state

Verified at the planning commit.

- Clap root: `crates/jackin/src/cli.rs` — `pub struct Cli` (~line 74: `#[command(subcommand)] pub command: Option<Command>`, flattened `ConsoleArgs`, global `--debug` with `env = "JACKIN_DEBUG"`); `pub enum Command` (~line 110: `Load(LoadArgs)`, `Hardline(HardlineArgs)`, `Eject(EjectArgs)`, `Exile`, `Purge(PurgeArgs)`, … all tuple variants). Existing parse-test pattern: `try_parse_from` call sites like `crates/jackin/src/cli/config/tests.rs:57` (read one before writing the harness — match its assertion style).
- Doc invocation shape (samples, fence lines from `docs/content/docs/(role-authoring)/developing/creating-roles.mdx`):

  ```
  jackin role create ChainArgos/Rustacean "$HOME/Projects"
  jackin role validate .
  jackin load chainargos/rustacean . --debug
  jackin role migrate .
  jackin workspace create my-app --workdir ~/Projects/my-app --default-agent amp
  ```

  Known content shapes the extractor must handle: leading `$ ` prompts; shell line continuations (`\`); env-var prefixes (`JACKIN_TELEMETRY_LEVEL=trace jackin console --debug` — TESTING.md shows the pattern); placeholders (`<PR_NUMBER>`, `<run-id>`); quoted args with `$HOME`; `~` paths; here-and-there `jackin-dev` (a DIFFERENT binary — must not be parsed against `jackin`'s tree); prose lines inside fences are rare but prose OUTSIDE fences starting with "jackin " is common (the 366-vs-206 delta).
- Distribution: 53 MDX files carry candidates; heaviest are the commands/ pages (`workspace` 57, `config` 36+, `prewarm` 19, `load` ~30, `status`/`prune` 13 each, `hardline` 11).
- Where the test lives: `crates/jackin/tests/` already hosts cross-cutting harnesses (`migration_fixtures.rs` reads fixture trees via `env!("CARGO_MANIFEST_DIR")` — same technique reaches `docs/` at `../../docs/content/docs` from the crate root; verify the relative depth). A plain `#[test]` integration file `crates/jackin/tests/docs_commands.rs` is the shape.
- Repo conventions: tests never `cargo test` (nextest); the docs tree is stable ASCII MDX; do not hard-wrap prose when editing docs; `jackin console` is the canonical TUI name in docs.

## Commands you will need

| Purpose | Command | Expected on success |
|---|---|---|
| Run the new gate | `cargo nextest run -p jackin -E 'binary(docs_commands)'` | all pass |
| Candidate census | `rg -n '^\s*(\$ )?jackin ' docs/content/docs -g '*.mdx' \| wc -l` | ~366 raw (fence filter shrinks it) |
| Crate suite | `cargo nextest run -p jackin` | all pass |
| Docs gates | `cargo xtask roadmap audit && cargo xtask docs repo-links` | pass |
| Full local gate | `cargo xtask ci --fast` | `ci gate OK` |

## Scope

**In scope**:
- `crates/jackin/tests/docs_commands.rs` (create — extractor + harness + skip-ledger)
- Whatever genuinely-broken documented commands the first run finds: fix the DOCS (the clap tree is the source of truth per docs/CLAUDE.md "code is source of truth") — each doc fix is a separate commit listing the stale invocation
- `crates/jackin/README.md` (verify section mentions the new harness) — only if the README lists test binaries
- Roadmap Phase 5 item 11 status; `plans/code-health/README.md` row

**Out of scope**:
- The config-key half of roadmap item 11 (docs tables vs schema artifacts) — separate ledger item
- Changing any clap definition to make a doc parse (never; docs follow code)
- `jackin-dev` command documentation (different binary; extractor skips it)
- MDX component-rendered commands (only fenced code blocks are parsed)

## Git workflow

- Branch off `main`: `test/docs-command-drift-gate`.
- Commits: harness first, then one `docs(...)` commit per stale-command fix batch. `-s`, push each. PR to `main`; do not merge.

## Steps

### Step 1: Fence-aware extractor

In `crates/jackin/tests/docs_commands.rs`, write `fn extract_invocations(mdx: &str) -> Vec<(usize, String)>` (line number + command line):

1. Track fenced blocks only: lines between ``` fences whose info string is empty, `sh`, `bash`, `shell`, `console`, or `text` — skip fences tagged as other languages and everything outside fences.
2. Within a fence, a candidate line: optional leading whitespace, optional `$ ` prompt, then optional `NAME=value ` env-prefix pairs (strip them), then the literal `jackin ` (word-boundary — NOT `jackin-dev`, `jackin-capsule`, `jackin❯`).
3. Join `\`-continued lines before matching.
4. Normalize: strip trailing comments (` # …`), replace `<placeholder>` tokens and `~`/`$VAR`/quoted-`$VAR` args with the literal `x` (clap only validates structure, not path existence), keep flags verbatim.
5. Tokenize shell-style (a minimal splitter honoring double/single quotes is enough — no full shell grammar; if a line contains `|`, `&&`, `>`, `$(`, or backticks, SKIP it and count it, these are compound shell lines not plain invocations).

Unit-test the extractor inside the same file (`#[cfg(test)]`-style plain `#[test]` fns are fine in an integration-test binary) against inline fixture strings covering every shape in Current state: prompt, env-prefix, continuation, placeholder, prose-outside-fence, `jackin-dev` exclusion, compound-line skip.

**Verify**: `cargo nextest run -p jackin -E 'binary(docs_commands)'` → extractor unit tests pass.

### Step 2: The parse harness

Main test: walk `docs/content/docs/**/*.mdx` (relative to `env!("CARGO_MANIFEST_DIR")`; sort the walk for determinism — repo convention: no filesystem-order dependence), extract, and for each invocation run `jackin::cli::Cli::try_parse_from(tokens)` — confirm `Cli` and its parse entry are reachable from the test (they are `pub` in the lib target per cli.rs; if the crate exposes no lib target for tests, STOP and report — do not restructure the crate). Failures collect into one report listing `file:line`, the original line, and clap's error, then a single assert at the end (so ONE run reports ALL drift — diagnostics-are-prompts).

Add a skip-ledger at the top of the file: `const SKIP: &[(&str, u32, &str)] = &[/* (file, line, reason) */];` for invocations that are deliberately illustrative and cannot parse (expect a handful). Every entry needs a reason string; the test fails on stale entries (skip row whose file:line no longer yields a candidate) — shrink-only, same philosophy as the repo's other ledgers.

**Verify**: first full run — expect failures; capture the list. This list is Step 3's worklist, not a defect in the harness.

### Step 3: Fix the drift

For each failing invocation: check against `jackin --help`/the clap definitions; fix the DOC (rename the flag/subcommand to current reality). If a documented command reveals the DOC was right and the CLI renamed something operators depend on — report it in the PR body (that is a product regression discovered, not a doc fix; do not change the CLI). Genuinely-illustrative unparseables go into `SKIP` with reasons. Batch doc fixes by page in `docs(...)` commits.

**Verify**: `cargo nextest run -p jackin -E 'binary(docs_commands)'` → all pass with `SKIP.len()` ≤ 10; `cargo xtask docs repo-links && cargo xtask roadmap audit` → pass (doc edits can touch linked files).

### Step 4: Roadmap + wire-up

The test runs wherever `cargo nextest run -p jackin` runs — already every CI test lane; no workflow edit needed (confirm the integration test binary isn't filtered out by `.config/nextest.toml`'s `default-filter = 'not binary(dind_e2e)'` — it isn't, different binary name). Roadmap Phase 5 item 11: command half shipped; config-key half open.

**Verify**: `cargo nextest run -p jackin` → all pass including docs_commands; `cargo xtask ci --fast` → `ci gate OK`.

## Test plan

- Extractor unit tests (Step 1, ≥7 shape cases).
- The harness IS the test; its first green run over ~200 invocations with ≤10 skips is the deliverable.
- Negative probe: temporarily rename a documented flag in one MDX fence (e.g. `--workdir` → `--work-dir`), run → harness fails naming that file:line; revert.

## Done criteria

- [ ] `crates/jackin/tests/docs_commands.rs` exists; extractor unit tests pass
- [ ] Harness parses every fence invocation; ≤10 skip-ledger entries, each reasoned; stale-skip detection works
- [ ] All discovered doc drift fixed in `docs(...)` commits (list in PR body)
- [ ] Negative probe demonstrated and reverted
- [ ] Roadmap item 11 updated; `plans/code-health/README.md` row updated
- [ ] `cargo nextest run -p jackin` green; `cargo xtask ci --fast` → `ci gate OK`

## STOP conditions

Stop and report back if:

- `Cli` is not importable from an integration test (no lib target exposing it) — do not add a lib target or make items pub without reporting.
- The first run yields > 40 failures (drift far beyond expectation — the docs or the extractor are systematically off; report the breakdown before fixing).
- A failing invocation implies a CLI regression (docs documented a flag operators still need) — product call, not yours.
- More than 10 invocations genuinely need skipping (the extractor's normalization needs improving instead).

## Maintenance notes

- New docs pages with command fences are covered automatically; authors see failures locally via `cargo nextest run -p jackin -E 'binary(docs_commands)'` — consider adding that line to the TESTING.md matrix when next edited.
- The config-key drift half (roadmap item 11's second sentence) can reuse this file's walking/fence machinery — keep `extract_invocations` private but factor the fence-walker into a small helper when that lands.
- Reviewer should scrutinize: the compound-line skip counter (silent skips would hollow the gate — the test should log how many lines were skipped as compound vs parsed, and the parsed count should be ≥150).
