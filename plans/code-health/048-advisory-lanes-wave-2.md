# Plan 048: Advisory lanes wave 2 — hyperfine cold-start, rust-analyzer cleanliness, per-crate build-time measurement

> **Executor instructions**: Follow this plan step by step. Run every
> verification command and confirm the expected result before moving to the
> next step. If anything in the "STOP conditions" section occurs, stop and
> report — do not improvise. When done, update the status row for this plan
> in `plans/code-health/README.md`.
>
> **Drift check (run first)**: `git diff --stat fabe88406..HEAD -- .github/workflows/hygiene.yml mise.toml`
> Plans 022/035/046 append jobs to hygiene.yml — expected drift; append beside
> whatever landed, never restructure. Other hygiene.yml changes: compare the
> job-shape excerpt below before proceeding.

## Status

- **Priority**: P3
- **Effort**: S-M
- **Risk**: LOW (scheduled, advisory, artifact-only — no PR gate, no production code)
- **Depends on**: none (independent of plan 035's lanes; same append-only discipline)
- **Category**: dx / perf
- **Planned at**: commit `fabe88406`, 2026-07-09

## Why this matters

Three roadmap items are S-sized measurement lanes parked behind blockers that do not apply: (1) Phase 4 item 10's hyperfine cold-start lane was coupled to iai-callgrind adoption, but hyperfine measures wall-clock of a built binary while iai measures bench instruction counts — orthogonal; (2) Phase 6 item 8's rust-analyzer cleanliness lane ("the workspace loads without proc-macro or config errors") is S and owned by no plan; (3) Phase 6 item 3's feedback-loop budget needs a *measurement* half first — the existing CI `--timings` artifacts are HTML, not budget-parseable, so no per-crate baseline exists for the later ratchet. All three are "scheduled/advisory first" by the roadmap's own ratchet principle, and all three are the same shape: one hygiene.yml job producing an artifact. Bundle them.

## Current state

Verified at `fabe88406`.

- `.github/workflows/hygiene.yml` — the job shape to copy (`scheduled-hygiene`, :49-92): pinned `actions/checkout`, rustup toolchain cache keyed on `rust-toolchain.toml`, `jdx/mise-action@…` with `version: "2026.6.11"`, `cache_key_prefix: "mise-v2"`, `install_args: "cargo-binstall rust cargo:cargo-deny …"`, `github_token: ${{ secrets.GH_READONLY_TOKEN }}`, the `./.github/actions/cache-cargo-registry` composite, `Swatinem/rust-cache` with a job-specific `shared-key`. Cron `"23 11 * * *"` + `workflow_dispatch` at the workflow level.
- Workflow rules that bind (from `.github/AGENTS.md`): ALL tools via mise (`install_args`), never setup-* actions; mise-action gets `GH_READONLY_TOKEN`, same-repo API reads use `${{ github.token }}`; env vars at job level; artifact uploads via the pinned `actions/upload-artifact` already used in `ci.yml` (find the pinned SHA there and reuse it).
- hyperfine is NOT in `mise.toml` (grep clean). mise's registry carries `hyperfine` as a first-class tool — install per-job via `install_args: "hyperfine"` (do not add to `mise.toml`: it is not a local dev dependency).
- rust-analyzer: no `rust-analyzer.toml` / RA config exists anywhere; the workspace has no proc-macro crates (recorded DX finding), so `rust-analyzer analysis-stats .` is expected to be clean and fast. RA installs via `rustup component add rust-analyzer` (a rustup component, not a mise tool — the `.github/AGENTS.md` mise rule carves out rustup components explicitly: "mise does not install components; add a `rustup component add` step").
- Build-time facts: `ci.yml:507` (`cargo clippy --timings`) and `:678` (`cargo check --timings`) upload `cargo-timings-*.html` — human-readable only. The five agent-hot crates the roadmap names: jackin-runtime, jackin-capsule, jackin-console, jackin-term, jackin-config.
- Operator-perceived cold-start targets: `jackin --help` (pure CLI parse) and `jackin console --help` (console front). A console *first frame* needs a TTY — out of scope here (the input-latency harness half stays deferred; this lane measures process cold-start only).

## Commands you will need

| Purpose | Command | Expected on success |
|---|---|---|
| Validate workflow syntax | `gh workflow view Hygiene` after push | job listed |
| Dispatch smoke | `gh workflow run Hygiene --ref <branch>` | new jobs green |
| Local hyperfine dry run | `hyperfine --warmup 3 --export-json /tmp/cold.json 'target/release/jackin --help'` | JSON written |
| Local RA dry run | `rust-analyzer analysis-stats . 2>&1 \| tail -5` | stats, no errors |

## Scope

**In scope**:
- `.github/workflows/hygiene.yml` — up to three appended jobs (or one job with three step groups if runner cost matters; prefer separate jobs so one failure doesn't mask the others): `cold-start-bench`, `rust-analyzer-clean`, `build-time-measure`
- `TESTING.md` — one row noting the lanes + artifact names
- Roadmap Phase 4 item 10 / Phase 6 items 3+8 status notes

**Out of scope** (do NOT touch):
- Any threshold/gate/budget — these lanes MEASURE; plan 017's ratchet engine budgets later (maintenance note).
- The input-to-frame latency harness (needs a headless TUI driver that does not exist — stays deferred).
- `mise.toml`, existing hygiene jobs, ci.yml.
- iai-callgrind (separate, still sequenced behind plan 014's benches).

## Git workflow

- Branch off `main`: `ci/advisory-lanes-wave-2`.
- Conventional Commits (`ci: …`), `-s`, push per commit. PR to `main`; do not merge. Schedule-gated jobs → dispatch-smoke via `gh workflow run Hygiene --ref <branch>` before ready (`.github/AGENTS.md` rule); paste run URL in the PR body.

## Steps

### Step 1: `cold-start-bench` job

Append a job copying the `scheduled-hygiene` setup shape (checkout, rustup cache, mise-action with `install_args: "cargo-binstall rust hyperfine"`, registry cache, rust-cache `shared-key: cold-start-v1`). Steps: `cargo build --release --locked -p jackin`, then `hyperfine --warmup 3 --min-runs 10 --export-json cold-start.json 'target/release/jackin --help' 'target/release/jackin console --help'`, then upload `cold-start.json` as artifact `cold-start-bench` (pinned upload-artifact action from ci.yml). Also append the two mean times to `$GITHUB_STEP_SUMMARY` (pattern: the `cache-usage` job :22-47 writes summaries).

**Verify**: dispatch run → job green, artifact contains both commands' stats.

### Step 2: `rust-analyzer-clean` job

Same setup shape (mise `install_args: "cargo-binstall rust"`), then `rustup component add rust-analyzer`, then `rust-analyzer analysis-stats . > ra-stats.txt 2>&1` and a check step that greps the output for hard failure markers (`error` lines other than known-benign — start strict: fail on `grep -i '^error' ra-stats.txt`; if RA's output format makes that noisy, record what it prints and gate on exit code only). Upload `ra-stats.txt`. `continue-on-error: true` for the grep step initially (advisory posture) — flip to hard later once a few runs prove stable.

**Verify**: dispatch run → job green, artifact shows the workspace loaded (crate count in stats output).

### Step 3: `build-time-measure` job

Same setup shape. For each of the five crates (runtime, capsule, console, term, config): `cargo clean -p <crate>` then time a fresh `cargo build -p <crate> --locked` (seconds via `date +%s` around the command — cargo's machine-readable timings are unstable/nightly; shell timing is enough for a trend artifact), then `touch` one `src/lib.rs` and time the incremental rebuild. Emit `build-times.json` (`{"crate": {"clean_s": N, "incremental_s": N}}` via a small `jq -n` or printf assembly) + a `$GITHUB_STEP_SUMMARY` table. Upload as artifact `build-time-measure`. Cache note: use a rust-cache `shared-key: build-time-v1` distinct from other jobs, and run the clean-build timings AFTER `cargo fetch` so network noise stays out of the number.

**Verify**: dispatch run → artifact has 5 crates × 2 numbers; summary table renders.

### Step 4: Docs + dispositions

TESTING.md row (three lanes, artifact names, "advisory — no gate"). Roadmap: Phase 4 item 10 → cold-start half shipped (input-latency harness still deferred, reason restated); Phase 6 item 8 → RA lane shipped; Phase 6 item 3 → measurement half shipped, budget half stays SEQ(017).

**Verify**: `cargo xtask docs repo-links && cargo xtask roadmap audit` → pass; `cargo xtask ci --fast` → `ci gate OK`.

## Test plan

No Rust tests — the deliverable is CI jobs. The dispatch-run URL(s) in the PR body are the verification record; each artifact must be downloaded once and its shape confirmed (paste a 3-line excerpt of each JSON/summary into the PR body).

## Done criteria

- [ ] Three hygiene jobs appended, each green on a dispatch run (URLs in PR body)
- [ ] Artifacts: `cold-start.json` (2 commands), `ra-stats.txt`, `build-times.json` (5×2)
- [ ] Step summaries render tables/numbers
- [ ] No PR-time behavior change (jobs are schedule/dispatch-only); existing hygiene jobs untouched
- [ ] TESTING.md + roadmap dispositions updated; `plans/code-health/README.md` row updated

## STOP conditions

Stop and report back if:

- `rust-analyzer analysis-stats` fails to load the workspace (that is a real Phase-6 finding — report its output; do not mark the lane green around it).
- Runner wall-clock for `build-time-measure` exceeds ~30 min (five clean builds may be heavy on cold cache) — report and propose trimming to clean-build-only or fewer crates rather than silently shrinking.
- hyperfine is absent from the mise registry at the pinned mise version (install fails) — report; do not hand-download a binary.
- hygiene.yml has been restructured (not just appended to) since the excerpt.

## Maintenance notes

- Plan 017's ratchet engine later consumes `build-times.json` + `cold-start.json` as budget metric sources (families reserved there) — keep the JSON shapes stable.
- The RA grep-strictness flip (advisory → hard) is a one-line follow-up after ~2 weeks of green runs.
- The input-to-frame latency harness remains the one deliberately-deferred half of Phase 4 item 10; it needs a headless TUI driver (candidate after plan 025's test-support crate matures).
