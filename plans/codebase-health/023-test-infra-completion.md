# Plan 023: Test infrastructure — consolidate duplicated fakes, first property tests, wire protocol fuzz into CI

> **Executor instructions**: Follow step by step; verify each step; STOP conditions binding. Update status row in `plans/codebase-health/README.md` when done.
>
> **Drift check (run first)**: `git diff --stat 846038946..HEAD -- crates/jackin-test-support/ crates/jackin-host/src/caffeinate/ crates/jackin/tests/common.rs crates/jackin-protocol/fuzz/ .github/workflows/hygiene.yml`
> Mismatch with "Current state" = STOP.

## Status

- **Priority**: P2
- **Effort**: M
- **Risk**: LOW (test/CI only)
- **Depends on**: none
- **Category**: tests
- **Planned at**: commit `846038946`, 2026-07-14

## Why this matters

Three Characterization-section gaps. Item 1 (shared test support "only after identifying real duplicate consumers"): real duplicates exist — `jackin-host` re-implements `FakeDockerClient`/`FakeRunner` (`crates/jackin-host/src/caffeinate/tests.rs:24,99`) despite canonical versions in `crates/jackin-test-support/src/{docker.rs:14,runner.rs:12}`, and `crates/jackin/tests/common.rs:122` carries another private `FakeRunner`; each copy drifts independently from the Docker/Runner contract. Item 2's property-test half is entirely absent: zero `proptest`/`quickcheck`/`arbitrary` usage anywhere (fuzz targets exist and are good). Item 6's protocol requirement — "Run the protocol decode target in PR and scheduled fuzz CI, wire it into cache, commit a seed corpus" — is unmet: `crates/jackin-protocol/fuzz/src/decode_frames.rs` is a complete libfuzzer target wired into NO CI lane, with no committed corpus (the other four fuzzed crates have `corpus/` dirs).

## Current state

- Canonical fakes: `crates/jackin-test-support/src/docker.rs` (`FakeDockerClient` — queue/`inspect_state_by_name` features), `runner.rs` (`FakeRunner`). Tier: test-support is T3 (`crates/jackin-xtask/src/arch.rs:70`); host is T4 (`:73`) — the dependency is legal.
- Duplicates: `jackin-host/src/caffeinate/tests.rs:24` + `:99`; `crates/jackin/tests/common.rs:122`; console `StubRunner`s (`crates/jackin-console/src/tui/op_picker/tests.rs:35`, `input/auth/tests.rs:709,834`, `global_mounts/tests.rs:675`) — smaller-surface, migrate only if trivially compatible.
- Scheduled fuzz lane: `.github/workflows/hygiene.yml:97-113` runs `damage_grid_process`, `config_migrate`, `workspace_migrate`, `manifest_migrate`, `manifest_validate`, `env_resolve` — not `decode_frames`; hygiene is schedule/dispatch-only (`hygiene.yml:5-8`), so NO fuzz runs at PR time at all.
- Protocol golden fixtures already exist as normal tests: `crates/jackin-protocol/tests/corpus_decode.rs` + `tests/corpus/`.
- Property-test targets named by the roadmap: manifest validation, environment resolution, config/manifest migrations, terminal parsing, protocol decoding; policies to cover: invalid-input non-panics, migration idempotence + validity, unknown-field policy, reserved-key handling (fuzz already covers the non-panic + idempotence half in its bodies).

## Commands you will need

| Purpose | Command | Expected |
|---|---|---|
| Touched crates | `cargo nextest run -p jackin-host -p jackin -p jackin-test-support` | pass |
| Property tests | `cargo nextest run -p jackin-manifest -p jackin-config -p jackin-env` | pass |
| Fuzz build | `cargo +nightly fuzz build` in `crates/jackin-protocol/fuzz` (mirror the hygiene lane's exact invocation — read it) | builds |
| Full | `cargo xtask ci --fast` | exit 0 |

## Scope

**In scope**: fake consolidation for jackin-host + `jackin/tests/common.rs` (console StubRunners only if drop-in); proptest dev-deps + first property suites for `jackin-manifest`, `jackin-config`, `jackin-env`; `hygiene.yml` fuzz step + a PR-time short fuzz smoke; committed `corpus/decode_frames/` seed corpus; a written promotion rule for minimized findings (comment in the workflow or TESTING.md).

**Out of scope**: new fuzz targets; turmoil/madsim (plan 017 evaluates); spec-gate work (024); test-layout re-organization beyond what consolidation touches.

## Git workflow

Branch `test/infra-completion`; Conventional Commits (`test:`/`ci:`); `git commit -s`; push per commit.

## Steps

### Step 1: Consolidate fakes

Point `jackin-host`'s caffeinate tests and `jackin/tests/common.rs` at `jackin_test_support::{FakeDockerClient, FakeRunner}` (add the dev-dependency where missing); delete the local copies, adapting call sites to the canonical API (if the canonical fake lacks a feature a duplicate had, ADD it to test-support rather than keeping the fork). Evaluate the console `StubRunner`s: migrate if drop-in, else record "kept: smaller-surface stub, different seam" in the PR.

**Verify**: `cargo nextest run -p jackin-host -p jackin -p jackin-test-support` → pass; `grep -rn "struct FakeDockerClient\|struct FakeRunner" crates | grep -v test-support` → none.

### Step 2: First property suites

Add `proptest` as dev-dependency to `jackin-manifest`, `jackin-config`, `jackin-env`. Write properties the fuzz bodies can't express cheaply:
- manifest: arbitrary-but-shaped manifests → validation never panics AND valid⊕error is total; unknown fields rejected per policy (`#[serde(deny_unknown_fields)]` sites at `crates/jackin-config/src/auth.rs:21,89`, `schema.rs:94` are the pattern).
- config/workspace migrations: migrate(migrate(x)) == migrate(x) (idempotence) and output always validates; reserved-key handling per the env-resolve rules.
- env resolution: resolution order invariants (read `crates/jackin-env/src/resolve.rs` docs for the declared ordering — encode it as a property).

Place per the test rules (module `tests.rs` or crate `tests/` dir matching existing layout).

**Verify**: `cargo nextest run -p jackin-manifest -p jackin-config -p jackin-env` → pass (bounded case counts so suite time stays sane; use `PROPTEST_CASES` default or a modest config).

### Step 3: Wire `decode_frames`

(a) Add `decode_frames` to the hygiene scheduled fuzz steps (mirror the existing step shape + cache wiring exactly). (b) Commit a seed corpus at `crates/jackin-protocol/fuzz/corpus/decode_frames/` — seed from the existing golden fixtures (`tests/corpus/` frames) so seeds are meaningful. (c) Add a short PR-time smoke (~30–60s) — either in the PR workflow with the same cache, or as an `cargo fuzz run decode_frames -- -max_total_time=45` step; match how maintainers gate cost elsewhere (the mutants job is scheduled-only; PR fuzz smoke is a new lane — keep it cheap and non-flaky with `-runs=` bounded). (d) Write the promotion rule where the other corpora document theirs (check sibling `corpus` READMEs or TESTING.md; if none exists, add the rule to TESTING.md: minimized reproducers from findings are committed as corpus entries + a regression test).

**Verify**: fuzz target builds via the lane's invocation; corpus committed; `actionlint` clean; TESTING.md rule present.

## Test plan

Steps 1–3 are all test work; the gates above are the verification. Total suite-time delta should stay small — note the measured delta in the PR (roadmap cares about suite time).

## Done criteria

- [x] No duplicate FakeDockerClient/FakeRunner outside test-support (console stubs resolved or recorded)
- [x] Property suites in manifest/config/env covering idempotence, validity, unknown-field, reserved-key, ordering invariants
- [x] `decode_frames` in scheduled fuzz + PR smoke + committed seed corpus + written promotion rule
- [ ] `cargo xtask ci --fast` exits 0; status row updated

## STOP conditions

- Canonical fake API diverges enough that host migration rewrites test semantics (not just plumbing) — report the contract gap.
- A property test finds a real bug (idempotence violation etc.) — STOP the suite work, file the bug finding with reproducer; fixing production code is out of this plan's scope.
- PR-time fuzz proves flaky in CI (nondeterministic timeouts) — drop to scheduled-only and record the decision; do not ship a flaky PR gate.

## Maintenance notes

- New fakes go to test-support when ≥2 consumers exist (the roadmap's own bar).
- Fuzz findings: minimize, commit to corpus, add regression test — the written rule from step 3d is the contract.

**Index deviation (audit 2026-07-15)**: demoted from DONE to IN PROGRESS — Done criteria not fully met; see implementer audit rollup.
