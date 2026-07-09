# Plan 013: Phase 3 — flake detection, suite timing artifacts, migration idempotence, parser fuzz targets

> **Executor instructions**: Follow this plan step by step. Run every
> verification command and confirm the expected result before moving to the
> next step. If anything in the "STOP conditions" section occurs, stop and
> report — do not improvise. When done, update the status row for this plan
> in `plans/code-health/README.md`.
>
> **Drift check (run first)**: `git diff --stat 47dd5fca0..HEAD -- .config/nextest.toml .github/workflows/rust-nextest.yml crates/jackin/tests/migration_fixtures.rs crates/jackin-term/fuzz/`
> If any in-scope file changed since this plan was written, compare the
> "Current state" excerpts against the live code before proceeding; on a
> mismatch, treat it as a STOP condition.

## Status

- **Priority**: P2
- **Effort**: M-L
- **Risk**: LOW-MED (CI-config and test-only changes; the fuzz targets touch no production code)
- **Depends on**: none
- **Category**: tests
- **Planned at**: commit `47dd5fca0`, 2026-07-09

## Why this matters

Roadmap Phase 3 ("Flakes, corpora, and suite hygiene" + property/fuzz items, lines 163-196): today a test that fails then passes on rerun is invisible (nextest has no retries configured, so flakes surface as hard failures someone reruns by hand — or worse, as noise that erodes trust), no timing artifact exists to feed the Phase 7 wall-time budget, and the migration fixture harness — the executable half of the schema-versioning hard rule — never checks idempotence and never diffs the migrated output against the committed `after.toml` golden. Fuzz coverage is a single target (`jackin-term`'s `damage_grid_process`) while the surfaces that ingest untrusted operator/role-repo input — manifest validation, env resolution, config/manifest migration — have none; "invalid input never panics" is unasserted exactly where the workspace-wide `indexing_slicing` adoption (a later wave) will matter most.

## Current state

- `.config/nextest.toml` (22 lines, read in full at `47dd5fca0`): profiles `default` (with `default-filter = 'not binary(dind_e2e)'` and `slow-timeout = { period = "60s", terminate-after = 3, grace-period = "5s" }`) and `docker-e2e`; a `docker-e2e` test-group with `max-threads = 1`; **no `retries`, no `[profile.ci]`, no junit config, no `final-status-level`**.
- `.github/workflows/rust-nextest.yml` — sharded runner. The run step (lines 95-123) builds `args` per matrix group (`jackin`, `jackin-capsule`, `jackin-runtime`, `jackin-tui`, `small-crates`) and ends with `cargo nextest run "${args[@]}"` (line 123). The only artifact is sccache stats (lines 124-129). No `--profile`, no junit upload, no timing summary.
- `crates/jackin/tests/migration_fixtures.rs` — fixture harness (read in full). `walk_fixtures` (lines 64+) per fixture dir: reads `before.toml`, `expected_after` from `after.toml`, `meta.toml`; migrates a temp copy; asserts the migrated file parses (line 98), the expected file parses (line 102), and both declare `meta.target_version` (lines 105-119). **Never** (a) re-runs the migration on the migrated output (idempotence), (b) compares `actual_after` to `expected_after` (the golden is parsed but not enforced). Migration entry points used by the harness — these are the public APIs the fuzz targets also drive: `jackin_config::migrate_config_file_if_needed` (line 39), `jackin_config::migrate_workspace_file_if_needed` (line 46), `jackin_manifest::migrations::migrate_manifest_file` (line 53). Fixture corpus exists per schema version: `crates/jackin/tests/fixtures/migrations/{config,workspace,manifest}/<version>/{before,after,meta}.toml`.
- Fuzz: exactly one target, `crates/jackin-term/fuzz/src/damage_grid_process.rs` (libfuzzer via cargo-fuzz; `cargo-fuzz` pinned in `mise.toml:7`). CI smoke runs it in ci.yml's `fuzz` job (job starts line 805); the scheduled long run is `hygiene.yml:89-92`: `cd crates/jackin-term && cargo fuzz run --sanitizer none --target x86_64-unknown-linux-gnu damage_grid_process -- -max_total_time=300`. No committed corpora anywhere (`git ls-files | grep -i corpus` is empty).
- No proptest/quickcheck/turmoil/madsim/fail anywhere in the workspace (audit-confirmed) — property-based and sim testing are recorded next-wave items, not this plan.
- Conventions: tests in sibling `tests.rs`; workflows install tools via mise only; PR/main check parity is a hard rule (`.github/CLAUDE.md`) — everything added here runs identically at PR time.

## Commands you will need

| Purpose | Command | Expected on success |
|---|---|---|
| Workspace tests | `cargo nextest run --workspace --all-features --locked` | all pass |
| Migration harness only | `cargo nextest run -p jackin -E 'binary(migration_fixtures)'` | all pass |
| Fuzz build (per target) | `cd crates/<crate> && cargo fuzz build <target>` | exit 0 |
| Fuzz smoke (per target) | `cd crates/<crate> && cargo fuzz run <target> -- -max_total_time=30` | exit 0, no crash |
| Workflow lint | `actionlint .github/workflows/rust-nextest.yml .github/workflows/ci.yml .github/workflows/hygiene.yml` | exit 0 |
| Full local gate | `cargo xtask ci --fast` | `ci gate OK` |

## Scope

**In scope**:
- `.config/nextest.toml`
- `.github/workflows/rust-nextest.yml` (profile flag, junit + timing artifact), `.github/workflows/ci.yml` fuzz job + `.github/workflows/hygiene.yml` long-fuzz step (new targets)
- `crates/jackin/tests/migration_fixtures.rs`
- New fuzz crates/targets under `crates/jackin-config/fuzz/`, `crates/jackin-manifest/fuzz/` (+ their corpus seed dirs)
- `flaky-tests.toml` (create, repo root) — quarantine ledger (may start empty)
- `TESTING.md` (one short subsection: flake policy + fuzz lanes)
- Roadmap Phase 3 status notes

**Out of scope**:
- Fixing any flaky test found (file a row in the ledger + report)
- Property tests, turmoil/fail/proptest adoption, chaos E2E lane (recorded next wave)
- The protocol decoder fuzz targets — that is plan 009 (already written); do not duplicate
- Production code changes of any kind (if a fuzz target immediately crashes production code, STOP — that is a bug report, not a silent fix)
- `jackin-env` fuzzing IF its resolution API requires live process/op side effects (see STOP conditions)

## Git workflow

- Branch off `main`: `test/flakes-fuzz-idempotence`.
- Conventional Commits (`test(...)`/`ci(...)` types), `-s`, push after every commit. PR to `main`; do not merge.

## Steps

### Step 1: Nextest CI profile with flake detection

In `.config/nextest.toml` add:

```toml
[profile.ci]
# Detect flakes: a pass-on-retry is reported as FLAKY, never silently absorbed
# (codebase-health-enforcement Phase 3). The quarantine ledger is
# flaky-tests.toml at the repo root; an unquarantined flake fails review.
retries = { backoff = "fixed", count = 2, delay = "1s" }
failure-output = "immediate-final"
final-status-level = "flaky"

[profile.ci.junit]
path = "junit.xml"
```

`[profile.ci]` inherits `default`'s `default-filter` and slow-timeout automatically per nextest's profile inheritance — verify by running it locally. Create `flaky-tests.toml` at the root: a header comment (shrink-only ledger, one `[[test]]` entry per quarantined flake with `name`, `owner`, `reason`, `since`) and an empty list.

**Verify**: `cargo nextest run --workspace --locked --profile ci` → all pass; `ls target/nextest/ci/junit.xml` → exists.

### Step 2: Wire the profile + timing artifact into the sharded workflow

In `.github/workflows/rust-nextest.yml`:
1. Line 119's `args+=(--no-tests=pass --color=always --locked --offline)` — add `--profile ci`.
2. After the run step, add two `if: always()` steps:

```yaml
      - name: Upload junit + timing
        if: always()
        uses: actions/upload-artifact@<pin the same SHA-pinned version used elsewhere in this workflow file>
        with:
          name: nextest-junit-${{ matrix.group }}-${{ matrix.config.lane }}
          path: target/nextest/ci/junit.xml
          if-no-files-found: ignore
      - name: Slowest tests summary
        if: always()
        shell: bash
        run: |
          if [ -f target/nextest/ci/junit.xml ]; then
            echo "### Slowest tests (${{ matrix.group }})" >> "$GITHUB_STEP_SUMMARY"
            grep -o '<testcase name="[^"]*"[^>]*time="[0-9.]*"' target/nextest/ci/junit.xml \
              | sed -E 's/<testcase name="([^"]*)".*time="([0-9.]*)"/\2 \1/' \
              | sort -rn | head -10 | awk '{printf "- %ss `%s`\n", $1, $2}' >> "$GITHUB_STEP_SUMMARY"
          fi

Also append per-shard TOTAL wall time to the same summary (sum the junit `time` attributes, or the workflow step duration). Then, once this PR's CI has run, seed the Phase 0 baseline: if `code-health-baseline.toml` exists (plan 010), replace its commented `suite-wall-time` pointer row with the measured per-shard totals from this PR's run (a hand-seeded dated entry — the roadmap's fifth baseline family; the Phase 7 engine later automates the recompute). If 010 has not landed, record the totals in the PR body and this plan's README row instead.
```

3. Add a flake gate to the run step: nextest marks flaky tests in the junit as `flaky="true"` retries; after the run, fail if any flaky test is not listed in `flaky-tests.toml`:

```yaml
      - name: Fail on unquarantined flakes
        if: always()
        shell: bash
        run: |
          [ -f target/nextest/ci/junit.xml ] || exit 0
          flaky=$(grep -o 'flaky="true"[^>]*' target/nextest/ci/junit.xml || true)
          if [ -n "$flaky" ]; then
            echo "::error::flaky test(s) detected and not quarantined in flaky-tests.toml"; echo "$flaky"; exit 1
          fi
```

(If the junit output does not carry a `flaky` attribute on the pinned nextest 0.9.136, use `final-status-level = "flaky"` console output instead: tee the run output and grep for `FLAKY`. Pick whichever the local run in Step 1 demonstrates; note the choice in the commit message.)

**Verify**: `actionlint .github/workflows/rust-nextest.yml` → exit 0.

### Step 3: Migration idempotence + golden enforcement

In `crates/jackin/tests/migration_fixtures.rs`, inside `walk_fixtures` after the existing version assertions (line ~119), add:

```rust
        // Golden: the migration must produce exactly the committed after.toml.
        assert_eq!(
            actual_after, expected_after,
            "fixture {name}: migrated output differs from after.toml golden"
        );

        // Idempotence: migrating an already-current file must be a no-op.
        migrate(&target).unwrap_or_else(|e| panic!("re-migrating {name}: {e:#}"));
        let after_second = fs::read_to_string(&target).unwrap();
        assert_eq!(
            after_second, actual_after,
            "fixture {name}: migration is not idempotent (second run changed the file)"
        );
```

If the golden assertion fails on any fixture, the committed `after.toml` and the migrator genuinely disagree — STOP and report which fixture (do not regenerate goldens to make it pass; that decision is the operator's).

**Verify**: `cargo nextest run -p jackin -E 'binary(migration_fixtures)'` → all 3 tests pass.

### Step 4: Fuzz targets for config/manifest migration + manifest validation

Model on the existing `crates/jackin-term/fuzz/` layout (read its `Cargo.toml` and target first; replicate the structure). Create:

1. `crates/jackin-config/fuzz/` with targets:
   - `config_migrate`: write the fuzz input bytes to a temp file named `config.toml`, call `jackin_config::migrate_config_file_if_needed(&path)`, ignore the `Result` — the invariant is *no panic*. Second half of the target: if migration returned `Ok`, run it again and assert the file content is unchanged (idempotence under arbitrary input).
   - `workspace_migrate`: same shape via `migrate_workspace_file_if_needed`.
2. `crates/jackin-manifest/fuzz/` with targets:
   - `manifest_migrate`: same shape via `jackin_manifest::migrations::migrate_manifest_file`.
   - `manifest_validate`: locate the public manifest parse/validate entry point (check `crates/jackin-manifest/src/lib.rs` re-exports and `validate.rs`; the harness must call parse-then-validate on arbitrary bytes/str). If no side-effect-free public entry exists, STOP and report the API shape you found.

Seed corpora: create `corpus/<target>/` inside each fuzz crate, seeded from the real fixtures — copy every `before.toml` and `after.toml` from `crates/jackin/tests/fixtures/migrations/{config,workspace}/` into the config targets' corpora and `.../manifest/` into the manifest ones. Commit the seeds (they are small TOML files).

Wire CI: in ci.yml's `fuzz` job add a 30-second smoke run per new target next to the existing `damage_grid_process` invocation (mirror its command shape, adjusting the `cd` directory); in `hygiene.yml` extend the long-fuzz step with a 120s run per new target. Point every `cargo fuzz run` at the committed corpus dir (cargo-fuzz uses `fuzz/corpus/<target>` by default when it exists — verify with one local run).

**Verify**: for each of the 4 new targets: `cargo fuzz build <target>` exit 0 and `cargo fuzz run <target> -- -max_total_time=30` completes without crash; `actionlint` on both workflows → exit 0.

### Step 5: TESTING.md + roadmap

- `TESTING.md`: add a short `## Flakes and fuzz` subsection — flaky tests are detected via the CI profile's retries and must be quarantined in `flaky-tests.toml` (shrink-only, owner+reason per entry) or fixed; list the five fuzz targets and the smoke/long lanes.
- Roadmap Phase 3: mark flake-ratchet, suite-timing artifact, migration-idempotence, and the fuzz-target gap as shipped; leave property tests, sim/fault-injection, chaos lane, snapshot standardization, coverage-map gate as open (the coverage-map *report* ships in plan 010's dashboard).

**Verify**: `cargo xtask roadmap audit && cargo xtask docs repo-links` → pass; `cargo xtask ci --fast` → `ci gate OK`.

## Test plan

- Step 3 IS a test change; the three migration_fixtures tests must pass with the two new assertions.
- Fuzz targets: each runs 30s locally without a crash; corpora committed.
- No production code changes ⇒ no new unit tests beyond the harness edits.

## Done criteria

- [ ] `[profile.ci]` with retries + junit exists; sharded workflow runs `--profile ci`, uploads junit, prints slowest-10, fails on unquarantined flakes
- [ ] `flaky-tests.toml` committed (empty is fine)
- [ ] `migration_fixtures.rs` asserts golden equality + idempotence; suite green
- [ ] 4 new fuzz targets build and smoke-run clean; corpora seeded from fixtures and committed; CI smoke + hygiene long lanes cover all 5 targets
- [ ] TESTING.md + roadmap updated
- [ ] `cargo xtask ci --fast` → `ci gate OK`; `plans/code-health/README.md` row updated

## STOP conditions

Stop and report back if:

- Step 3's golden assertion fails on any existing fixture (migrator vs committed golden disagreement — operator decision).
- A fuzz target crashes production code within the 30s smoke (report the reproducer path; do not fix production code in this plan).
- `jackin-manifest` has no side-effect-free public validate entry point, or `jackin_config` migration fns spawn processes / touch the network (they should be pure file transforms — verify by reading them before fuzzing).
- nextest 0.9.136's junit lacks any flaky marker AND its console output lacks a greppable FLAKY line (then the flake gate needs a design change — report).
- Any test in the workspace turns out to be actually flaky under retries during your runs — record it in `flaky-tests.toml` with `owner = "unassigned"` and report it; do not chase the flake.

## Maintenance notes

- The wall-time budget (Phase 7 ratchet) consumes the junit artifacts this plan starts uploading; keep artifact names stable (`nextest-junit-<group>-<lane>`).
- New parser-ish surfaces (protocol changes, new config sections) should get a fuzz target as part of their own PR — the four targets here are the backfill, plan 009 covers protocol.
- Reviewer should scrutinize: the flake-gate grep (false-positive risk on the junit format) and that corpus seeds don't bloat the repo (they are small TOMLs; keep it that way).
