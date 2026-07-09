# Plan 027: Phase 4 — stream diagnostics JSONL into a typed borrowed record; stop double-parsing every line

> **Executor instructions**: Follow this plan step by step. Run every
> verification command and confirm the expected result before moving to the
> next step. If anything in the "STOP conditions" section occurs, stop and
> report — do not improvise. When done, update the status row for this plan
> in `plans/code-health/README.md`.
>
> **Drift check (run first)**: `git diff --stat b42c97d4c..HEAD -- crates/jackin-diagnostics/src/summary.rs`
> On a mismatch with the "Current state" excerpts, treat it as a STOP condition.

## Status

- **Priority**: P2
- **Effort**: M
- **Risk**: LOW-MED (pure parsing refactor with a strong characterization net: the summary's outputs over a fixed corpus must be identical)
- **Depends on**: plans/code-health/014-hot-path-bench-coverage.md (its `summarize_jsonl` bench measures this; create it per 014 Step 4 if not landed)
- **Category**: perf
- **Planned at**: commit `b42c97d4c`, 2026-07-09

## Why this matters

Roadmap Phase 4 item 7 verbatim: "Stream diagnostics JSONL into typed borrowed records and parse nested detail only for event kinds that need it." Measured behavior (first-wave PERF-diag-double-parse, re-verified): the run-summary path parses **every** line of a diagnostics run file into an owned `serde_json::Value` tree, immediately copies out up to six fields as fresh owned `String`s, and then — for the `detail` field — parses a **second** JSON document from the detail string on every line that has one, regardless of whether the event kind ever reads it. Diagnostics runs reach hundreds of MB (`--debug` captures every external command's output), and this path backs the operator-facing `jackin diagnostics` summary, so the waste is operator-perceived latency on exactly the "something went wrong, inspect the run" flow.

## Current state

Verified at the planning commit — `crates/jackin-diagnostics/src/summary.rs:153-207`:

```rust
    for (line_index, line) in reader.lines().enumerate() {
        let line = line.with_context(|| format!("reading diagnostics line {}", line_index + 1))?;
        if line.trim().is_empty() {
            continue;
        }
        let value: Value = serde_json::from_str(&line)
            .with_context(|| format!("parsing diagnostics JSONL line {}", line_index + 1))?;
        summary.event_count += 1;

        let kind = value.get("kind").and_then(Value::as_str).unwrap_or("unknown");
        *summary.event_counts.entry(kind.to_owned()).or_default() += 1;
        // … run_id / ts_ms / stage extraction, each .map(ToOwned::to_owned) …
        let message = value.get("message").and_then(Value::as_str).unwrap_or_default().to_owned();
        let detail_raw = value.get("detail").and_then(Value::as_str).map(ToOwned::to_owned);
        let detail_json = detail_raw
            .as_deref()
            .and_then(|detail| serde_json::from_str::<Value>(detail).ok());

        match kind {
            "stage_done" => { …
```

Facts that shape the fix:
- `reader.lines()` allocates a fresh `String` per line (fine to keep initially; the big wins are the `Value` tree and the unconditional second parse — see Step 2's ordering).
- `detail` is a **string field containing JSON** (JSON-in-JSON), so `&RawValue` alone does not defer its parse — the deferral is simply *not parsing the string* until a kind that needs it. Which kinds need it: read the full `match kind` arms below line 207 to enumerate (e.g. `stage_done` reads `duration_ms` from the top level; find which arms touch `detail_json`).
- The JSONL producer's field set: `JsonEvent` in `crates/jackin-diagnostics/src/run.rs` (fields include `ts_ms`, `run_id`, `trace_id`, `span_id`, `kind`, `event_name`, `stage`, `message`, `detail`, …) — the borrowed record must tolerate unknown/extra fields (`#[serde(default)]`, no `deny_unknown_fields`) because old run files and newer producers must both parse.
- serde_json is already the workspace dep (pinned `=1.0.150`); `serde` with derive is available in this crate (check its Cargo.toml features — `derive` is used by `JsonEvent` already).
- Error-handling convention in this function: `anyhow` with line-number context — keep it (this crate is one of the anyhow-permitted surfaces until the Phase 2 error-taxonomy program says otherwise).
- Callers of the summary entry point (grep `summarize` in `crates/jackin-diagnostics/src` and `crates/jackin/src/cli/diagnostics.rs`): the CLI diagnostics command. Its rendered output over a fixed input file is the characterization oracle.

## Commands you will need

| Purpose | Command | Expected on success |
|---|---|---|
| Crate tests | `cargo nextest run -p jackin-diagnostics` | all pass |
| Clippy | `cargo clippy -p jackin-diagnostics --all-targets -- -D warnings` | exit 0 |
| Bench | `cargo bench --bench summarize_jsonl -p jackin-diagnostics -- --quick` | after ≥2× faster than before on the mixed corpus |
| CLI consumer | `cargo nextest run -p jackin` | all pass |
| Full local gate | `cargo xtask ci --fast` | `ci gate OK` |

## Scope

**In scope**:
- `crates/jackin-diagnostics/src/summary.rs` + its sibling `summary/tests.rs`
- `crates/jackin-diagnostics/benches/summarize_jsonl.rs` (from 014; create per 014 Step 4 if absent)
- Roadmap Phase 4 item 7 status; `plans/code-health/README.md` (row + strike PERF-diag-double-parse)

**Out of scope**:
- The JSONL *writer* (`run.rs`) — format unchanged, old files must keep parsing
- Plan 018's trace-id changes to the same records (coordinate if both in flight — 018 touches run.rs, this touches summary.rs; disjoint files, same crate)
- memory-mapping / parallel parsing — measure the simple fix first
- Any change to the summary's OUTPUT (fields, counts, rendering) — byte-identical results are the contract

## Git workflow

- Branch off `main`: `perf/diagnostics-typed-streaming`.
- Commits: characterization corpus + test first; then the refactor; bench numbers in the PR body. `-s`, push each. PR to `main`; do not merge.

## Steps

### Step 1: Characterization first

Build a committed fixture corpus `crates/jackin-diagnostics/tests/fixtures/summary/mixed.jsonl` (~200 lines): synthesize from the real event shapes in `run.rs`'s `JsonEvent` — a realistic mix of `stage_started`/`stage_done` (with stages incl. one `hardline`), events with and without `detail` (some with large nested detail JSON), an empty line, an `unknown`-kind line, and lines with extra unknown fields. Add a test in `summary/tests.rs` (extend the existing file; read its current tests for the harness pattern) that runs the summary over the corpus and asserts the COMPLETE resulting summary struct (event_count, event_counts map, run_id, first/last/hardline ts, stage timings, whatever else the struct holds — read it fully). This test pins current behavior BEFORE any refactor; commit it and see it pass against the unmodified code.

**Verify**: `cargo nextest run -p jackin-diagnostics -E 'test(/summary/)'` → passes on the un-refactored code.

### Step 2: Typed borrowed record + deferred detail

Refactor the loop:

```rust
#[derive(serde::Deserialize)]
struct EventLine<'a> {
    #[serde(default)]
    kind: Option<&'a str>,
    #[serde(default)]
    run_id: Option<&'a str>,
    #[serde(default)]
    ts_ms: Option<u64>,
    #[serde(default)]
    stage: Option<&'a str>,
    #[serde(default, borrow)]
    message: Option<std::borrow::Cow<'a, str>>,
    #[serde(default, borrow)]
    detail: Option<std::borrow::Cow<'a, str>>,
}
```

— field list derived from what the loop + match arms actually read (Step 1's full read of the arms), not from `JsonEvent`'s full set. Notes that matter: `&'a str` borrows only work for strings without escapes — use `Cow<'a, str>` (with `borrow`) for fields that may contain escaped content (`message`, `detail` — command output definitely has escapes); plain `&'a str` is fine for `kind`/`run_id`/`stage` (machine tokens, but if any test shows escapes, Cow them too). Parse with `serde_json::from_str::<EventLine>(&line)`; keep the same line-number error context. Then: parse `detail` into a `Value` ONLY inside the match arms that consume it (move the `detail_json` computation into those arms; arms that never read it skip the second parse entirely). Owned copies (`to_owned`) happen only where the summary struct stores them — that set shrinks to what is stored, not everything read.

Keep `reader.lines()` for now (per-line String). If the bench in Step 3 shows the line allocation dominating after the typed switch, apply the follow-up: a reused `String` buffer with `read_line` (mechanical; only do it if measured).

**Verify**: the Step 1 characterization test passes UNCHANGED; `cargo clippy -p jackin-diagnostics --all-targets -- -D warnings` → exit 0 (watch for borrow-lifetime issues between `line` and `EventLine` — the record must not outlive the line; process fully within the iteration).

### Step 3: Measure

Bench before/after on the 014 `summarize_jsonl` bench (generated ~20MB corpus). Record both numbers in the PR body. Expected: ≥2× from skipping the `Value` tree + unconditional detail parse on detail-heavy corpora. If <1.3×, run the `read_line` buffer follow-up and re-measure; if still <1.3×, STOP and report the profile (the assumption about where time goes was wrong — do not land a refactor without its win).

**Verify**: bench numbers in PR body meeting the ≥1.3× floor (target 2×).

### Step 4: Docs + ledger

Roadmap Phase 4 item 7 → shipped. Ledger: strike PERF-diag-double-parse → this plan. `cargo nextest run -p jackin` (CLI consumer) green.

**Verify**: `cargo xtask roadmap audit` → pass; `cargo nextest run -p jackin-diagnostics -p jackin` → all pass; `cargo xtask ci --fast` → `ci gate OK`.

## Test plan

- The Step 1 characterization test (complete-struct assertion over the committed mixed corpus) is the load-bearing net — written and green BEFORE the refactor, untouched through it.
- Add two adversarial cases to the corpus after the refactor: a line where `detail` contains invalid JSON (current behavior: `.ok()` swallows it — preserve exactly), and a line with a non-string `detail` (current: `and_then(Value::as_str)` yields None — the typed record must also treat it as absent; if serde rejects the whole line instead, type `detail` as `Option<serde_json::Value>`-tolerant via a custom shape and STOP if that spirals).
- Existing summary tests green unchanged.

## Done criteria

- [ ] `summary.rs` parses each line once into a typed borrowed record; `rg -n 'from_str::<Value>' crates/jackin-diagnostics/src/summary.rs` shows detail parsing only inside kind arms that use it, and no top-level `Value` line parse
- [ ] Characterization test (full-struct, committed corpus) green before and after, byte-identical expectations
- [ ] Bench delta ≥1.3× (target 2×) recorded in the PR body
- [ ] jackin-diagnostics + jackin suites green; clippy clean
- [ ] Roadmap + ledger updated; `plans/code-health/README.md` row updated
- [ ] `cargo xtask ci --fast` → `ci gate OK`

## STOP conditions

Stop and report back if:

- The match arms below line 207 read MORE fields than a small typed record can carry cleanly (>10 fields incl. nested access) — the record design needs revisiting, report the field census.
- Non-string or escape-heavy fields make borrowed deserialization reject lines the old code accepted (behavior change) and the Cow/custom-shape fallback doesn't restore parity.
- The measured win misses the 1.3× floor after both steps.
- Any characterization assertion must change to make the refactor pass (that IS the failure).

## Maintenance notes

- Plan 018 (if landed) adds real trace/span ids to the SAME record family in run.rs — the summary's typed record ignores unknown fields, so it is forward-compatible; keep it that way (`no deny_unknown_fields`, all fields `#[serde(default)]`).
- New summary features must extend the typed record AND the characterization corpus together — reviewers should reject one without the other.
- Reviewer should scrutinize: the detail-parse deferral (every arm that used `detail_json` still gets it), and the corpus's coverage of the swallowed-invalid-detail path.
