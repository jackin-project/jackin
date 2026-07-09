# Plan 051: Machine-readable gate output core — shared human|json|github reporter for xtask gates

> **Executor instructions**: Follow this plan step by step. Run every
> verification command and confirm the expected result before moving to the
> next step. If anything in the "STOP conditions" section occurs, stop and
> report — do not improvise. When done, update the status row for this plan
> in `plans/code-health/README.md`.
>
> **Drift check (run first)**: `git diff --stat fabe88406..HEAD -- crates/jackin-xtask/src .github`
> Plans 010/011/017/036/050 add/modify xtask modules — expected drift; the
> reporter must serve whatever gates exist at HEAD. Compare the two exemplar
> gates' shapes before converting them.

## Status

- **Priority**: P2
- **Effort**: S-M
- **Risk**: LOW (output-format layer; gate decisions unchanged)
- **Depends on**: none (plan 010 sets the `--format json` pattern for its health command if it lands first — reuse its serde shapes then; otherwise this plan sets the pattern and 010 reuses it. Coordinate, don't duplicate.)
- **Category**: dx
- **Planned at**: commit `fabe88406`, 2026-07-09

## Why this matters

The roadmap's "Diagnostics are prompts" principle and Phase 6 item 6: every first-party gate must offer `--format json` plus GitHub problem-matcher annotations "so agents parse results instead of scraping logs." The measured baseline is 0 of the xtask gates emit structured output or `::error file=…` annotations — an agent reading a red CI run regexes prose. Full rollout across every gate was fairly deferred as L (three of the gates don't exist yet); but the reusable CORE — one reporter module, one problem-matcher, two exemplar gates converted — is S-M and unblocks every later gate (and plans 010/011/017/050 name it as their output pattern). This plan builds the core so rollout becomes mechanical.

## Current state

Verified at `fabe88406`.

- `crates/jackin-xtask/src/` gate modules: `lint.rs` (file-size gate, 11.2K — `bail!`-based failures, e.g. `lint.rs:210` a `bail!` with path + guidance; `--print-budget` flag exists), `test_layout.rs` (10.9K, `--print-allowlist`), `agent_files.rs`, `agent_links.rs`, `arch.rs`, `docs.rs` (33.5K), `schema.rs`, plus `ci.rs` orchestration. Failure style today: human prose via `bail!`/eprintln — no JSON, no `::error` annotations, no shared violation type.
- The repo's best-message exemplar (per the recorded DX finding): `cargo xtask lint tests` states the exact rerun command + fix. The reporter must preserve that quality in `human` mode — JSON is additive, never a downgrade of prose.
- GitHub annotations: workflow commands (`::error file=<f>,line=<l>,title=<t>::<msg>`) print to stdout in Actions; a `.github/problem-matchers/*.json` registration is the alternative for tool-native output. Direct workflow commands need no matcher registration — prefer them (simpler, no workflow edit per gate).
- CI runs gates via `cargo xtask …` in `ci.yml` and locally via `cargo xtask ci` (`ci.rs`). Actions sets `GITHUB_ACTIONS=true` — auto-enable annotation mode there.
- Conventions: modules self-named + sibling `tests.rs`; serde available in xtask (check `crates/jackin-xtask/Cargo.toml` — if `serde`/`serde_json` absent, adding them to an xtask-only crate is acceptable dependency surface; note it in the PR).

## Commands you will need

| Purpose | Command | Expected on success |
|---|---|---|
| xtask tests | `cargo nextest run -p jackin-xtask` | all pass |
| Gate human mode | `cargo xtask lint file-size` (exact name per main.rs) | unchanged prose |
| Gate json mode | `cargo xtask lint file-size --format json` | valid JSON on stdout |
| Clippy | `cargo clippy -p jackin-xtask --all-targets -- -D warnings` | exit 0 |
| Full gate | `cargo xtask ci --fast` | `ci gate OK` |

## Scope

**In scope**:
- `crates/jackin-xtask/src/report.rs` (new) + `report/tests.rs`
- Two exemplar gate conversions: the file-size gate (`lint.rs`) and the agents gate (`agent_files.rs`) — chosen because both emit per-file violations with obvious file anchors
- `crates/jackin-xtask/src/main.rs` — `--format <human|json|github>` plumbing for the two converted gates (default `human`; `github` auto-selected when `GITHUB_ACTIONS=true` unless overridden)
- `crates/jackin-xtask/README.md`, TESTING.md gate rows
- Roadmap Phase 6 item 6 status note

**Out of scope** (do NOT touch):
- Converting the other gates (arch, docs, schema, test-layout, …) — mechanical follow-ups once the core exists; list them in the maintenance note.
- Changing any gate's PASS/FAIL decisions or thresholds.
- A rustc/clippy problem-matcher (cargo already emits GitHub-renderable diagnostics under Actions' default matchers; out of scope unless trivially true — do not go down this hole).

## Git workflow

- Branch off `main`: `feature/xtask-gate-reporter`.
- Conventional Commits (`feat(xtask): …`), `-s`, push per commit. PR to `main`; do not merge.

## Steps

### Step 1: The reporter module

`report.rs`:

```rust
pub enum Format { Human, Json, Github }
#[derive(serde::Serialize)]
pub struct Violation {
    pub rule: &'static str,      // e.g. "file-size"
    pub file: String,            // repo-relative
    pub line: Option<u64>,
    pub why: String,             // one sentence: the rule's reason
    pub fix: String,             // the exact clearing edit/command
    pub rerun: String,           // narrowest rerun command
}
pub struct Report { pub gate: &'static str, pub violations: Vec<Violation> }
```

`Report::emit(&self, format) -> anyhow::Result<()>`: `Human` → the current prose quality (rule, why, fix, rerun per violation — port the best existing wording, never flatten it); `Json` → `serde_json::to_writer(stdout, …)` of `{gate, ok, violations: […]}`; `Github` → one `::error file=…,line=…,title=<rule>::<why> Fix: <fix>` line per violation (escape `%`, `\r`, `\n` per workflow-command rules — encode `%` as `%25`, newline as `%0A`) plus the human block to stderr so logs stay readable. Non-empty violations → the gate returns Err after emitting (exit-code behavior unchanged). `Format::detect(cli_flag)` — flag wins; else `GITHUB_ACTIONS=true` → Github; else Human.

Tests: JSON round-trip shape; Github escaping (`%`, newline in why); detect() matrix.

**Verify**: `cargo nextest run -p jackin-xtask` → new tests pass.

### Step 2: Convert the file-size gate

Rework `lint.rs`'s failure path to build `Violation`s (file, why = over-budget by N lines, fix = split guidance + budget-file pointer — port the existing prose, rerun = the gate's own command) and emit via the reporter. Human output must remain byte-comparable in content (not necessarily formatting) to today's — capture before/after in the PR body.

**Verify**: `cargo xtask lint <file-size subcommand> --format json` on a deliberately-violating temp state — create a scratch oversized file under a crate in a THROWAWAY worktree, not the working tree, OR use the gate's tests: extend `lint/tests.rs` to assert the JSON shape from the pure violation-building path. Green suite; human mode unchanged on a clean tree (`cargo xtask lint …` → pass).

### Step 3: Convert the agents gate

Same treatment for `agent_files.rs` (violations: missing AGENTS.md / missing CLAUDE.md symlink; fix = the two-line creation commands from crates/AGENTS.md).

**Verify**: `cargo nextest run -p jackin-xtask` → pass; `cargo xtask lint agents --format json` on the clean tree → `{"ok":true,…}`.

### Step 4: Wire + document

`--format` flag on the two subcommands (clap arg, match main.rs's existing arg style). ci.yml: no change needed (Github mode auto-detects; confirm one CI log shows `::error` by pushing a deliberate violation to the PR branch once, screenshot/log-link in PR body, then revert it). README/TESTING rows; roadmap item 6 note ("core shipped; per-gate rollout mechanical, N gates remaining").

**Verify**: `cargo xtask ci --fast` → `ci gate OK`; the one-commit violation push showed an annotation in the PR checks UI (link in PR body).

## Test plan

- `report/tests.rs`: serialization, escaping, detection (≥6 cases).
- Extended `lint/tests.rs` + `agent_files/tests.rs`: violation-building paths produce rule/why/fix/rerun non-empty.
- The live annotation check (Step 4) is the end-to-end proof.

## Done criteria

- [ ] `report.rs` with `Violation{rule,file,line,why,fix,rerun}` + 3 formats + auto-detect
- [ ] file-size + agents gates emit via it; human prose quality preserved (before/after in PR body)
- [ ] JSON valid + tested; Github annotations proven live once (link in PR body)
- [ ] Gate exit codes/decisions unchanged; full suite + `ci --fast` green
- [ ] README/TESTING/roadmap updated; `plans/code-health/README.md` row updated

## STOP conditions

Stop and report back if:

- Plan 010 landed with a different `--format json` schema for its health command — reconcile with ITS shape (one schema across xtask), do not ship a second.
- The gates' failure paths are so interleaved with I/O that violation-building can't be factored pure for tests without behavior risk — convert one gate only and report.
- clap arg plumbing conflicts with an in-flight 036 restructure of main.rs — coordinate per drift check.

## Maintenance notes

- Rollout order for remaining gates (each a small follow-up): test-layout → arch → docs trio → schema → (new) 010/011/017/050 gates adopt at birth.
- The JSON schema is now load-bearing for agents — version it informally (a `"schema": 1` field) so a future field change is detectable.
- Reviewer scrutiny: workflow-command escaping (an unescaped newline in `why` silently truncates annotations) and that human output lost nothing.
