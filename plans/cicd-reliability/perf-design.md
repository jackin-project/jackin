# Perf slice design (agent 15, 2026-09-21, read-only)

All evidence gathered. No repo writes made (scratch only under `/tmp/perf-design-1`: downloaded job logs + notes).

# Perf slice design — Jackin CI/CD 120s budget

## 1. Critical-path model (measured, 2026-09-20 runs)

All links `https://github.com/jackin-project/jackin/actions/runs/<id>`, jobs `/job/<jobid>`.

### PR unit — CI/PR, scope=affected but selects 40/40

- [run 35537475111](https://github.com/jackin-project/jackin/actions/runs/35537475111) (PR #1007 branch, wall **23m59s** 21:03:08→21:27:07): plan 25s (queue 6s) → critical job `Swift·native` [106149045008](https://github.com/jackin-project/jackin/actions/runs/35537475111/job/106149045008) **22m15s**: runner-setup 15s, mise **211s (cache MISS → boltffi source build)**, checks **1097s** = `cargo xtask` compile 55s + `boltffi pack` **16m16s** (cold `desktop-release` = release+thin-LTO+codegen-units=1) + `swift build/test` 65s. VELNOR_CI_REPORT: `tool_bootstrap 211, checks_wall 1097, mbx cold, cargo null` (no cache configured). Second-slowest `rust-jackin` 4m09s. `ci-required` 2s.
- [run 35539395149](https://github.com/jackin-project/jackin/actions/runs/35539395149) (PR #1014 head, in progress): **queue 6m30s** before plan (21:39:52→21:46:22, contention), plan 23s, selection **units=40/40, full_units=40/40**.
- Selection root cause (both sampled PRs 40/40): `select_affected` falls back to full on any `.github/` path (`FULL_SELECTION_PREFIXES`, Velnor pin `reuse.rs:54`) or any unmatched path (`reuse.rs:241-300`). Every regen'd branch commits `.github/ci/.github-actions-generator-state` → **affected-selection is effectively dead**; plus `plans/*.md` unmatched → same fallback.

### Main — CI/Main, full 40 units, wall ~15–16min

- [run 35517379726](https://github.com/jackin-project/jackin/actions/runs/35517379726) (wall **15m03s**): plan 15s (queue 6s) → critical `Swift·native` **13m33s** (mise HIT 14s + checks 13m06s). `rust-jackin` [job](https://github.com/jackin-project/jackin/actions/runs/35517379726/job/106095550010) **8m31s**: setup 32s (rustup/mold/cargo all `exact`, mbx **MISS** — log: `No mbx cache found`), checks **352s** = fmt 0.4s + nextest 243s (cold compile 3m49s, 641 crates) + clippy 109s (recompile 1m48s, clippy-driver refingerprint), **mbx post-export 124s** (`export 10.1 GiB` + upload 2.15GB compressed, then `Saved mbx cache …`). VELNOR report: `runner_setup 4, tool_bootstrap 17, cache_prep 7, checks_wall 352, mbx 0 hits / 8.4 GiB stored locally`.
- [run 35521080097](https://github.com/jackin-project/jackin/actions/runs/35521080097): attempt-1 wall ~16.5min + flake rerun (C1, attempt-2 green).

### Desktop-merge — single macOS job, wall 34–42min, zero cargo cache

- [run 35521079960](https://github.com/jackin-project/jackin/actions/runs/35521079960) (wall **41m41s**, job started 15:55:33): checkout 6s → mise setup **212s (MISS**, boltffi source build) → **`mise run` auto-installs 26 more tools, 448s** → bindings-check **7m44s** (xtask compile 35s + `boltffi generate` 7m09s cold desktop-release) → generate/lint/format ~12s → desktop-test **12m57s** (nextest compile 2m56s + 316 tests **3.2s** + xcframework **8m22s** cold + 5 harnesses 90s) → desktop-build 1m24s (xcframework incremental 19s + xcodebuild ~60s) → test-swift 1m22s → verify 5s → **test-ui 6m18s**.
- [run 35517379563](https://github.com/jackin-project/jackin/actions/runs/35517379563) (wall **33m46s**): mise **HIT** (9 tools, 12s) yet **26 tools still rebuilt in 573s**. Root cause proven: mise-action saves the cache **inline** (log: `installed 9 tools` 15:59:18 → `Cache saved` 15:59:22 → `installing 26 tools` 15:59:23), so the entry can never contain auto-installed tools. Every desktop run pays 448–573s unconditionally.

### Desktop-scheduled — never runs

- [Workflow 360371404](https://github.com/jackin-project/jackin/actions/workflows/desktop-scheduled.yml): **0 runs** via API. Model by composition only: merge graph + `periphery` deadcode (unmeasured, est. +2–5min).

### Release — drill mirrors desktop cost

- [run 35212777349](https://github.com/jackin-project/jackin/actions/runs/35212777349) (dispatch drill): build job **41m09s**, sign skipped. Publish mode (sign/notarize/attest) unmeasured in window.

### Cache audit ( settles several debates)

- Actions cache: **30 entries / 11.14 GB = 111% of the 10GB cap** → LRU eviction churn. mbx blobs dominate (rust-jackin 2.15GB, capsule 1.13GB×2, …); ~15 mbx entries fit, rest evicted. Registry caches healthy (5×238MB, exact hits).
- Eviction proven: desktop mise key `d76f…` HIT 14:44 → **MISS 15:55** (same key) → re-saved 15:59.
- sccache: **installed but unwired, mechanism confirmed** — zero `SCCACHE_*/RUSTC_WRAPPER` in the generated tree. Generator emits sccache env only when `tools_for_unit` yields `Sccache` (`ir.rs:6153`), which happens for Rust **iff not mbx** (Jackin rust = all mbx → excluded) and **never for Swift** (`UnitKind::Swift` arm has no transport). Check-profiles path emits no sccache env at all.
- Phase summary: **test execution is trivial everywhere** (nextest 3s, swift ~1min); cost = cold compile (macOS release+LTO 7–16min ×2–3 per push; Linux test-profile ×2 nextest+clippy) + tool installs (3.5–13min) + mbx post-export (~2min/job) + queue (normally <10s; 6.5min contention observed).

## 2. Ranked slice plan

### S1 — Kill the 26-tool auto-install on check-profiles (desktop-merge, desktop-scheduled, release) — ~8min/run, Jackin-config-only

- **Mechanism**: set `MISE_TASK_RUN_AUTO_INSTALL=false` (plus `MISE_NOT_FOUND_AUTO_INSTALL=false` for nested `mise run`) in `[check_profile.env]` for `desktop-merge`, `desktop-scheduled`, and `[release.job.env]` for both jobs. The 9 (10) `install_args` tools already cover the task closure: `cargo` (rust), `boltffi`, `xcodegen`, `swiftlint`, `xcbeautify`, `cargo-nextest`, `ripgrep`, `sccache`, (`periphery`); everything else (`xcrun`, `xcodebuild`, `plutil`, `lipo`) is system-provided. The 26 rebuilt tools (`cargo-audit`, `cargo-dylint`, `codebook-lsp`, …) are never invoked by the desktop graph.
- **Owned files**: Jackin `.github-gen/velnor-workflow.toml` (`[check_profile.env]`, `[release.job.env]`); regen. Precedent in same file: `renovate-upstream-sources` profile already sets it (L124–128); unit checks set the `MISE_*=false` trio (`ci-unit-rust.yml:443-445`). Velnor follow-up (structural): emit `MISE_*=false` in scheduled-checks run steps by default (parity with unit checks).
- **Correctness/invalidation**: no cache involved — pure skip of unneeded installs. Closure audit: `mise_tasks` scripts + `which()` calls in `crates/jackin-xtask/src/desktop.rs` (boltffi L650/737, xcodegen L812, plutil/lipo) + `native/Scripts/run-ui-tests.sh`.
- **Saving**: 448–573s → ~0 (both desktop logs). Same ~8min on release drill.
- **Verification**: next desktop-merge run green; log lacks `installing 26 tools`; `Run desktop-merge` reaches `[desktop-ci]` in <60s; step-time delta in job API.

### S2 — Wire sccache (GHA backend) on all cargo-invoking macOS jobs — 5–15min warm, Jackin env + Velnor gap

- **Mechanism**: `CARGO_INCREMENTAL=0`, `RUSTC_WRAPPER=sccache`, `SCCACHE_GHA_ENABLED=true` as job env on `swift-package-native` unit + desktop/release profiles. `cargo:sccache` is already in all three tool lists; only env is missing. First run populates; subsequent runs share across swift-unit/desktop/release (same OS/arch/lock).
- **Owned files**: Jackin `.github-gen/velnor-workflow.toml` (`[check_profile.env]`, `[release.job.env]`, and `[[units]]` env for `swift-package-native` if schema-1 supports unit env — verify, else declare via Velnor); Velnor `ir.rs` (`tools_for_unit` Swift arm + scheduled-checks renderer) for the structural fix. Bonus: `RUSTC_WRAPPER` also accelerates mise `cargo:` source builds (boltffi).
- **Correctness/invalidation**: sccache content-hashes compiler inputs — automatic, profile/flag-sensitive; `CARGO_INCREMENTAL=0` required for hits (generator already pairs them, `ir.rs:1833-1844`). No manual key management, no staleness class.
- **Saving**: cold desktop-release FFI compiles (bindings 7m09s, pack 8m22s/16m16s, overlapping dep graphs) → warm ~1–2min. Basis: measured cold times; sccache shares what per-job caches cannot.
- **Verification**: two consecutive desktop runs on same lock: `Compiling` lines → ~0, `checks_wall_seconds` delta in VELNOR_CI_REPORT; `sccache --show-stats` (or `SCCACHE_LOG`) shows hits>0; tree-identical output (bindings byte-compare still gates).

### S3 — Fix Rust compile-cache economics: switch mbx→sccache transport (or bound mbx) — ~5min/job + budget recovery, Velnor-owned

- **Mechanism (preferred)**: flip Rust units from `MrBoxington` to `Sccache` transport (generator already supports both; `tools_for_unit`, `ir.rs:6175-6181`). sccache entries are content-addressed and **shared across all 40 units** (dep crates compile once), vs per-unit freshness-keyed 2GB mbx blobs that miss on every commit and LRU-evict each other. Fallback if mbx must stay: key mbx on dependency-hash only, cap per-entry size, skip save when restore missed twice.
- **Owned files**: Velnor `ir.rs` transport selection + a repo-visible toggle (Jackin `.github-gen/velnor-workflow.toml [workflow]`); regen. Must first verify `mbx` command-prefix carries no isolation semantics CI depends on (else keep prefix, add sccache under it — generator calls them mutually exclusive, so this needs a design decision).
- **Correctness/invalidation**: sccache automatic via hashing; shared entries cannot go stale. mbx-fallback correctness must prove prefix-restores are sound (they are the current design; just never hit).
- **Saving**: rust-jackin checks 352s cold → ~60s warm; **mbx post-export 124s/job → 0**; cache drops from 11.14GB toward <4GB, ending the eviction churn that causes random MISSes everywhere (incl. S1/S2-adjacent mise keys). Basis: VELNOR report `compiling_lines 641`, post-step timestamps, cache-list audit.
- **Verification**: no-op-rebuild run shows `compiling_lines≈0` in VELNOR_CI_REPORT; `gh actions-caches` shows no new `velnor-mbx-*` blobs and total <10GB; unit p50 checks time.

### S4 — Scope affected-selection for non-code paths — full-matrix PRs → narrow, Velnor-owned

- **Mechanism**: stop `fallback_full` for provably non-code changes: (a) extend a cheap unit's watch (e.g. docs/bun) to cover `plans/**`, `*.md`, research docs so markdown PRs select it instead of falling back on unmatched paths; (b) narrow `FULL_SELECTION_PREFIXES` handling of `.github/ci/.github-actions-generator-state` — a state-only regen commit carries no code change, and any scan-input change that matters is also in the diff selecting appropriately (Policy still gates tree-vs-render). Keep fail-closed full selection for genuinely unknown non-ignored paths.
- **Owned files**: Velnor `reuse.rs` (`select_affected`, `FULL_SELECTION_PREFIXES:54`, `effective_matches`) + planner logging of `fallback_full` reason (already in `explanations` — surface it); Jackin `project.toml` watch lists via regen.
- **Correctness/invalidation**: watch-glob extension is conservative (selects a fast unit rather than skipping); state-file narrowing is sound iff state is a pure function of scanned inputs (verify in generator: state must not embed behavior). Unmatched-path fallback stays for everything else.
- **Saving**: docs/regen-only PRs 16–24min → ~2min (plan + 1 narrow unit + policy). Basis: both sampled PRs ran 40/40 solely due to fallback triggers.
- **Verification**: synthetic PR touching only `plans/x.md` → plan log shows `fallback_full=false`, `units≈1`; wall <3min; ci-required green. Negative: PR touching `crates/*/src/**` still selects the unit + dependents.

### S5 — Prebuilt boltffi + align generate/pack fingerprints — 3–8min on cold macOS, Jackin/upstream

- **Mechanism**: (a) publish prebuilt `boltffi_cli` binaries so binstall hits (today: `Fallback to cargo-install is disabled` → ~3min source build on every mise MISS, ×3 macOS jobs/push); (b) align `boltffi generate` vs `boltffi pack` cargo fingerprints (same profile/flags/target/features) so the desktop job's two desktop-release FFI compiles share `target/` incrementally — today pack rebuilds 8m22s after generate's 7m09s with zero reuse.
- **Owned files**: (a) upstream boltffi release config (or a Jackin-pinned mirror); (b) Jackin `crates/jackin-xtask/src/desktop.rs` (`generate_bindings_into` L638, `build_xcframework` L713).
- **Correctness/invalidation**: binstall verifies checksums; fingerprint alignment changes no inputs, only reuse. Bindings byte-compare gate unchanged.
- **Saving**: (a) ~3min per mise-MISS macOS job; (b) up to ~8min/desktop run if full reuse achieved. Basis: measured source-build and pack durations.
- **Verification**: (a) MISS-run log shows `boltffi … Installed` via binstall in <30s; (b) pack after generate shows `Finished` with ~0 `Compiling` lines.

## 3. 120s verdicts per class (no timeout/coverage cuts)

- **PR unit: plausible, iff narrow+warm.** Structural requirements: S4 (selection must actually narrow — today every regen'd PR runs 40 units), S3 (shared warm compile cache; per-unit cold compile alone is 2–6min), S2 for Swift-touching PRs (cold pack is 13–18min; warm must be <90s), plan latency ≤25s (already met). Realistic warm narrow-PR budget: plan 25s + setup 15s + warm checks 30–60s + gates 5s ≈ 85–105s. Full-matrix PRs can never meet it.
- **Main: cannot meet as a full-verification gate.** 40-way fan-out with a slowest unit ≥2–4min even warm (Swift pack + release-profile link), plus plan+gates, floors at ~4–6min. Structural requirement: change what main *is* — PR attestation model (main runs Policy + selection-proof + smoke only; full matrix verified pre-merge). As an attestation gate 120s is easy; as re-verification it is structurally impossible.
- **Desktop-merge: cannot meet as one graph.** UI tests alone need ~6min (simulator + xcodebuild), xcodebuild ~1min, and even warm bindings/test can fill 2–3min. Structural requirement: split the PR-validating slice (format/lint/bindings-check warm) from the merge resilience tier (UI tests, harnesses) into parallel shards with separate budgets; UI tests move off the merge critical path (scheduled). Post-S1+S2 warm full graph ≈ 10–14min — better, but 120s needs decomposition, not just caching.
- **Desktop-scheduled: exempt by design.** It has never run; it is the slow lane (merge graph + deadcode). Verdict: keep it unbudgeted, never gate merge on it; it inherits S1/S2 savings for free.
- **Release: exempt.** Sign/notarize/attest depend on external Apple/GitHub services with unbounded latency; budgeting it at 120s is incoherent. Verdict: optimize for hermeticity/reliability (drill==publish path), not wall time; S1 applies to both jobs.

**Bottom line**: test execution is already fast (seconds); every minute is cold compile + tool install + cache churn. S1 is free money (~8min, config-only). S2+S3 replace a 0%-effective multi-GB cache economy with content-addressed sharing. S4 is the only thing that can make PRs fast. Main/desktop/release at 120s require redefining the gates, not speeding them up.