# Plan 045: Corpus closure — protocol golden frames + capability-skew test, terminal fuzz seeds, env fuzz target, unknown-field migration assert

> **Executor instructions**: Follow this plan step by step. Run every
> verification command and confirm the expected result before moving to the
> next step. If anything in the "STOP conditions" section occurs, stop and
> report — do not improvise. When done, update the status row for this plan
> in `plans/code-health/README.md`.
>
> **Drift check (run first)**: `git diff --stat fabe88406..HEAD -- crates/jackin-protocol/src crates/jackin-term/fuzz crates/jackin-env/src crates/jackin/tests/migration_fixtures.rs`
> Plans 009/013 landing IS expected drift (this plan builds beside them — see
> the per-step branches). Any other in-scope change: compare excerpts, STOP on
> mismatch.

## Status

- **Priority**: P2
- **Effort**: M
- **Risk**: LOW (test/fixture-only; no production code changes)
- **Depends on**: none (coordinates with 009/013 if landed — each step says how)
- **Category**: tests
- **Planned at**: commit `fabe88406`, 2026-07-09

## Why this matters

Roadmap Phase 3 asks for a committed golden frame corpus for jackin-protocol ("every frame kind plus adversarial truncations … decode it in both directions"), an explicit version-negotiation test making "old capsule, new host" a tested state, committed minimized fuzz corpora per target, env-resolution fuzz coverage, and an unknown-field-policy assertion in the migration replay. The coverage index credited plan 009 with the corpus/negotiation halves, but 009 explicitly de-scopes them to follow-ups; the pre-existing `jackin-term` fuzz target has no committed corpus at all; env resolution's pure entry points have no fuzz/property target; and the migration harness asserts parse+version only. This plan closes all five recorded test-coverage gaps in one test-only PR.

## Current state

All excerpts verified by direct read at `fabe88406`.

- `crates/jackin-protocol/src/attach.rs` — the wire surface: `ClientFrame` (:455), `ServerFrame` (:497), `encode_server` (:523), `decode_client` (:988), `decode_server` (:1201). The handshake is **capability negotiation**, not a numeric version: the module doc (:2) says "Attach protocol handshake: initial capability negotiation and session-ID …"; per-capability source-of-truth flags live on the Hello frame (e.g. `handshake_identity`, :195). Existing tests: `attach/tests.rs` (round-trips) and `src/tests.rs` — no corpus, no skew test (grep `corpus|negotiation` → nothing).
- `crates/jackin-term/fuzz/` — fuzz crate with target `damage_grid_process` (run by the scheduled hygiene lane, `.github/workflows/hygiene.yml:89-92`, `-max_total_time=300`); **no `corpus/` directory is committed** (`git ls-files crates/jackin-term/fuzz` shows only src/.gitignore/Cargo.toml/Cargo.lock). Every CI fuzz run starts cold.
- `crates/jackin-env/src/resolve.rs:18` — `pub fn validate_reserved_names(config: &AppConfig) -> anyhow::Result<()>` (pure; delegates to `jackin_core::env_model::is_reserved`); `crates/jackin-env/src/env_resolver.rs:79/:86` — `resolve_env` / `resolve_env_with_overrides` (pure string substitution; no `Command`/spawn in the module). Reserved-name example tests exist at `resolve/tests.rs:223,235`. No fuzz/property target covers either.
- `crates/jackin/tests/migration_fixtures.rs:80-123` — the per-version migration replay asserts: migrated file parses (:98), hand-written after.toml parses (:102), version stamps match (:104-122). It does **not** diff `actual_after` against `expected_after`, does not re-migrate for idempotence (plan 013 Step 3 adds those two), and never asserts the unknown-field policy (an unknown key surviving or being dropped is untested).
- Coordination: plan 009 (TODO) creates the protocol fuzz crate + truncation/skew *unit tests*; its follow-ups section defers the corpus + negotiation test to this plan. Plan 013 (TODO) adds config/manifest fuzz targets with seed corpora and the idempotence/golden asserts. Neither blocks this plan; steps below say what to do in each landed/not-landed case.
- Conventions: tests in sibling `tests.rs` files only (crates/AGENTS.md); fixture layout exemplar `crates/jackin/tests/fixtures/migrations/<kind>/from-<version>/{before,after,meta}.toml`.

## Commands you will need

| Purpose | Command | Expected on success |
|---|---|---|
| Protocol tests | `cargo nextest run -p jackin-protocol` | all pass |
| Env tests | `cargo nextest run -p jackin-env` | all pass |
| Migration fixtures | `cargo nextest run -p jackin -E 'test(migration)'` | all pass |
| Term fuzz smoke (local) | `cd crates/jackin-term && cargo fuzz run --sanitizer none damage_grid_process -- -max_total_time=30` | exits 0, no crash |
| Env fuzz build | `cd crates/jackin-env && cargo fuzz build` | exit 0 |
| Full local gate | `cargo xtask ci --fast` | `ci gate OK` |

(cargo-fuzz needs nightly locally; the hygiene lane already installs `cargo:cargo-fuzz` via mise — see `.github/workflows/hygiene.yml:71`.)

## Scope

**In scope**:
- `crates/jackin-protocol/tests/` or `src/attach/tests.rs` additions + a new committed fixture dir `crates/jackin-protocol/tests/corpus/` (exact home per Step 1)
- `crates/jackin-term/fuzz/corpus/damage_grid_process/` (committed seeds)
- `crates/jackin-env/fuzz/` (new fuzz crate, mirroring `crates/jackin-term/fuzz/` shape) + `.github/workflows/hygiene.yml` (append the env target to the existing fuzz step)
- `crates/jackin/tests/migration_fixtures.rs` (unknown-field assertion)
- `TESTING.md` (corpus + fuzz-target rows)

**Out of scope** (do NOT touch):
- Any production code in jackin-protocol/jackin-env/jackin-term.
- Plan 009's fuzz crate/truncation tests and plan 013's idempotence/golden asserts — do not duplicate; if they landed, extend, never rewrite.
- PR-time CI lanes (fuzz smoke stays scheduled; this plan only seeds it).

## Git workflow

- Branch off `main`: `test/corpus-closure`.
- Conventional Commits (`test(protocol): …`, `test(env): …`), `-s`, push per commit. PR to `main`; do not merge. Protocol is in the capsule closure → capsule smoke block verbatim in the PR body.

## Steps

### Step 1: Protocol golden frame corpus, decoded in both directions

Create `crates/jackin-protocol/tests/corpus/` holding one binary fixture per frame tag: generate each via the crate's own `encode_server`/client-side encoders (write a small `#[test] fn regenerate_corpus()` guarded behind `#[ignore]` that writes the files — the committed bytes are the artifact; the ignored test is the regenerator). Enumerate every `ClientFrame` (:455) and `ServerFrame` (:497) variant — the corpus is complete when a match on both enums has no uncovered arm (write the enumeration as an exhaustive `match` so a new variant fails compilation until its fixture exists — that is the drift-guard). Add adversarial fixtures: each golden frame truncated at 1 byte, mid-header, and mid-payload (generate in-test from the goldens, no extra files needed).

Add `crates/jackin-protocol/tests/corpus_decode.rs`: for every committed fixture, `decode_client`/`decode_server` must return Ok with the expected variant; every truncation must return a clean `Err`/incomplete (never panic). If plan 009 landed, reuse its truncation helpers instead of writing new ones.

**Verify**: `cargo nextest run -p jackin-protocol` → all pass, including one test per frame kind (count them against the enum arms in the PR body).

### Step 2: Capability-skew ("old capsule, new host") test

The protocol has no numeric version — skew safety lives in the Hello capability flags (attach.rs:195 area) and unknown-tag handling. Add tests asserting the two skew directions the roadmap names: (a) a Hello frame encoded WITHOUT the newest capability field(s) (simulate an older peer by re-encoding a Hello with defaults / truncated optional tail — read how the Hello decoder handles missing trailing fields first) decodes cleanly with the capability defaulted off; (b) an unknown frame tag (pick `0xFE`-style value not in the tag space — confirm unused by reading the tag match arms in `decode_client` :988 / `decode_server` :1201) produces the decoder's documented unknown-tag behavior (clean error or skip — assert whichever the code does, and state it in the test name). If the Hello decoder hard-fails on short frames, that IS the finding — STOP and report rather than asserting the failure as intended.

**Verify**: `cargo nextest run -p jackin-protocol -E 'test(skew)'` → new tests pass.

### Step 3: Commit terminal fuzz seeds

Run the existing target locally to build a minimized corpus: `cd crates/jackin-term && cargo fuzz run --sanitizer none damage_grid_process -- -max_total_time=120`, then `cargo fuzz cmin damage_grid_process`. Commit the minimized `corpus/damage_grid_process/` (check `crates/jackin-term/fuzz/.gitignore` — it likely ignores `corpus/`; carve out the target's corpus dir). Seed it richer than random: add a handful of hand-picked inputs — plain text, CSI cursor moves, SGR runs, OSC 8 links, alt-screen switches (byte strings crafted from the escape sequences the term crate parses).

**Verify**: `git ls-files crates/jackin-term/fuzz/corpus | wc -l` → >0; the 30s local fuzz smoke runs from the seeds without crash.

### Step 4: Env fuzz target over the pure resolution surface

Create `crates/jackin-env/fuzz/` mirroring `crates/jackin-term/fuzz/`'s Cargo layout: one target `env_resolve` that (a) builds arbitrary env maps/strings from the fuzz input, (b) calls `resolve_env`/`resolve_env_with_overrides` (env_resolver.rs:79/:86) and `validate_reserved_names` (resolve.rs:18, via a minimal `AppConfig` built from fuzzed key/value pairs — read the `AppConfig` construction used in `resolve/tests.rs:223` and reuse its builder), asserting: never panics; reserved names are consistently rejected (if `is_reserved(k)` then validate errors — the roadmap's "reserved env keys preserved or rejected consistently" invariant). Append the target to the hygiene fuzz step (`.github/workflows/hygiene.yml:89-92`) as a second `cargo fuzz run … env_resolve -- -max_total_time=120` line, and commit a small seed corpus (a valid map, a cycle-inducing map, a reserved-key map).

**Verify**: `cd crates/jackin-env && cargo fuzz build` → exit 0; 30s local run clean.

### Step 5: Unknown-field assertion in the migration replay

In `migration_fixtures.rs`, add one fixture-driven assertion of the unknown-field policy: inject a synthetic unknown key (`x_unknown_probe = "1"`) into a copy of each `before.toml`, run the migration, and assert the policy the schema documents — read the policy first (grep `deny_unknown_fields` in `crates/jackin-config/src` and `crates/jackin-manifest/src`; the schema-versions doc `docs/content/docs/**/schema-versions.mdx` states intent). Assert whichever is documented: preserved verbatim, dropped, or rejected — one behavior per file kind, asserted explicitly. If the observed behavior contradicts the documented policy, STOP and report (that is a real finding, not a test to write around). If plan 013 landed, put this beside its idempotence assert in the same loop.

**Verify**: `cargo nextest run -p jackin -E 'test(migration)'` → pass, including the new unknown-field cases.

### Step 6: TESTING.md + gate

Add corpus/fuzz rows to TESTING.md (where the corpora live, how to regenerate, how to run each target locally). Full gate.

**Verify**: `cargo xtask docs repo-links` → pass; `cargo xtask ci --fast` → `ci gate OK`.

## Test plan

Steps 1-5 ARE tests. Patterns: `attach/tests.rs` for protocol round-trips; `crates/jackin-term/fuzz` for fuzz-crate layout; `resolve/tests.rs` for env builders; the existing fixture loop for migrations.

## Done criteria

- [ ] One committed golden fixture per Client/Server frame tag; exhaustive-match drift-guard compiles
- [ ] Truncation + skew tests pass; unknown-tag behavior asserted by name
- [ ] `crates/jackin-term/fuzz/corpus/damage_grid_process/` committed (>0 files); hygiene lane seeds from it
- [ ] `crates/jackin-env/fuzz` target builds; hygiene fuzz step includes it; no-panic + reserved-consistency asserted
- [ ] Unknown-field policy asserted per file kind in the migration replay
- [ ] `cargo xtask ci --fast` → `ci gate OK`; `plans/code-health/README.md` row updated

## STOP conditions

Stop and report back if:

- The Hello decoder panics or hard-errors on a short/older frame (Step 2a) — that is a live skew bug; report with the failing bytes.
- The unknown-field behavior contradicts the documented schema policy (Step 5).
- `cargo fuzz` cannot run locally (no nightly) AND the hygiene lane can't be exercised — commit the corpus + tests and report the fuzz-run verification as blocked, do not fake it.
- The frame-tag space has no free value for the unknown-tag test.

## Maintenance notes

- New frame variants: the exhaustive match forces a fixture; reviewers should demand the corpus file in the same PR as any `ClientFrame`/`ServerFrame` change (capsule AGENTS already requires host+capsule alignment).
- Fuzz finds: promote crashing inputs into the golden corpus (roadmap Phase 3 item 5); `cargo fuzz cmin` before committing corpus growth.
- 013's idempotence assert + this plan's unknown-field assert belong in the same fixture loop — whichever lands second merges them.
