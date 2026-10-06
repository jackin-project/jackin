# S1a result — desktop check-profile `mise run` auto-install kill (2026-09-21)

Slice implementer record. Parent integrates (no commit by implementer).

## Outcome

`MISE_TASK_RUN_AUTO_INSTALL=false` + `MISE_NOT_FOUND_AUTO_INSTALL=false` added to the
`desktop-merge` and `desktop-scheduled` `[check_profile.env]` sections in the generation
input only. Tools-coverage audit finds **gap = empty**: no profile tool-list extension
needed. Regen via the pinned published renderer; rendered diff is exactly the two env
vars in the two desktop jobs (+ state hashes). All gates green.

## 1. Audit: what `mise run desktop-merge` activates vs what the graph needs

Source of truth for the auto-installed set: job log of desktop-merge main run
35542103447/job 106161493034 (wall 38.4min, `/tmp/s1-desktop-merge.log`, 4335 lines).

- `Set up Mise tools`: cache HIT, `installing 9 tools`, 11.2s (only `rust` reinstalled).
- `Run desktop-merge`: `mise … installing 26 tools` → `26/26 · installed 26 tools in 576.5s`
  (9.6min, unconditional — the mise-action cache saves inline before this step, so the
  entry can never contain these tools; same root cause as run 35517379563's 573s).

Exact 26 (unique `✓` lines, log lines 379–2119):

```text
actionlint  aqua:apple/container  bun  cargo:cargo-audit  cargo:cargo-deny
cargo:cargo-dylint  cargo:cargo-fuzz  cargo:cargo-hack  cargo:cargo-hakari
cargo:cargo-llvm-cov  cargo:cargo-mutants  cargo:cargo-shear  cargo:cargo-zigbuild
cargo:codebook-lsp  cargo:dylint-link  cosign  github:open-telemetry/weaver
hyperfine  node  periphery  pipx:reuse  python  shellcheck  syft  uv  zig
```

Note: that run predates #1015. `hk = "2.0.1"` entered `mise.toml` at HEAD `799deeef`,
so post-#1015 runs would auto-install **27** (26 + `hk`). The audit below covers all
36 lock entries at HEAD (9 profile + 26 + `hk`).

### 1a. Disposition of each auto-installed tool (merge graph + scheduled delta)

Method: every binary the graph can invoke was traced from `mise.toml` task scripts,
`crates/jackin-xtask/src/desktop.rs` (`cmd::command(`/`which(` — plain `Command`, no
mise wrapping), `crates/jackin-xtask/src/desktop/*.rs`, `native/Scripts/run-ui-tests.sh`,
workspace `build.rs` files, and `.cargo/config.toml`.

| Tool | Invoked by desktop-merge / desktop-scheduled? | Evidence |
|---|---|---|
| actionlint | No | zero refs in graph sources |
| aqua:apple/container | No | construct tasks only |
| bun | No | string literal in docs-contract hash only (`docs/contract.rs:190`), never executed; desktop never calls `contract::` |
| cargo:cargo-audit | No | zero refs |
| cargo:cargo-deny | No | zero refs |
| cargo:cargo-dylint | No | zero refs |
| cargo:cargo-fuzz | No | zero refs |
| cargo:cargo-hack | No | zero refs |
| cargo:cargo-hakari | No | zero refs |
| cargo:cargo-llvm-cov | No | zero refs |
| cargo:cargo-mutants | No | zero refs |
| cargo:cargo-shear | No | zero refs |
| cargo:cargo-zigbuild | No | Linux cross only; zero refs in graph |
| cargo:codebook-lsp | No | zero refs |
| cargo:dylint-link | No | build-time lib for `jackin-lints`; graph compiles `jackin-usage`/`-ffi`/xtask only; no binary invocation |
| cosign | No | release attest only; zero refs in graph |
| github:open-telemetry/weaver | No | zero refs |
| hk (HEAD only) | No | hooks only; zero refs in graph |
| hyperfine | No | zero refs |
| node | No | same as bun (contract-hash literal only) |
| periphery | Merge: no. Scheduled: **yes** (`desktop-deadcode`) — **already in scheduled profile** | `mise.toml` `desktop-deadcode` task |
| pipx:reuse | No | zero refs |
| python | No | zero `python3` refs in graph sources and all 7 workspace `build.rs`; no PATH-shadowing dependent |
| shellcheck | No | zero refs |
| syft | No | zero refs |
| uv | No | zero refs |
| zig | No | zero refs |

**Enumerated gap: none.** No auto-install-off failure is possible from these 27:
nothing in either desktop graph resolves to them.

### 1b. Positive coverage: every invoked binary traces to profile tools or system

Profile tools (merge 9; scheduled 10 = 9 + `periphery`):

| Profile tool | Provides | Consumer (required?) |
|---|---|---|
| `rust` | `cargo`, `rustup` | all `cargo xtask` + `cargo nextest`; `rustup target add` in `build_xcframework` (required) |
| `aqua:nextest-rs/nextest/cargo-nextest` | `cargo-nextest` | `cargo nextest run -p jackin-usage -p jackin-usage-ffi` in `run_desktop_tests` (required) |
| `cargo:boltffi_cli` | `boltffi` | `bindings-check`, `build_xcframework` via `which("boltffi")` (required) |
| `xcodegen` | `xcodegen` | `desktop-generate` task + `build_app` via `which("xcodegen")` (required) |
| `swiftlint` | `swiftlint` | `desktop-lint` task (required) |
| `xcbeautify` | `xcbeautify` | `run-ui-tests.sh` JUnit reports (required) |
| `ripgrep` | `rg` | `run-ui-tests.sh` test discovery (required) |
| `periphery` (scheduled only) | `periphery` | `desktop-deadcode` task (required iff scheduled) |
| `cargo-binstall` | install helper | not invoked by graph; keep (cache-hit, ~0s) |
| `cargo:sccache` | `sccache` | currently unwired (no `RUSTC_WRAPPER`); **keep — S2 wires it** |

System/Xcode binaries (not mise tools, unaffected by mise env):
`swift`, `xcodebuild`, `xcrun`, `plutil`, `lipo`, `ditto`, `codesign`, `spctl`,
`stapler`, `unzip`, `/usr/libexec/PlistBuddy`, `dwarfdump`, `vtool`, `otool`,
`xattr`, `open`, `pgrep`/`pkill`, `which`, `git`, `sh`/`bash` + coreutils.
(`spctl`/`stapler`/`unzip` sit on the `--release` verify path, not run by
`desktop-ci`; `gh`/`op`/`openssl`/`base64`/`shasum` are release-graph-only
in `desktop/{bootstrap,release_state,sign_notarize}.rs`.)
`cargo xtask` is a `.cargo/config.toml` alias (`run --locked -p jackin-xtask`), no tool.

## 2. Input diff (`.github-gen/velnor-workflow.toml` only)

Both desktop `[check_profile.env]` sections gain, with an explanatory comment:

```toml
MISE_TASK_RUN_AUTO_INSTALL = "false"
MISE_NOT_FOUND_AUTO_INSTALL = "false"
```

- `MISE_TASK_RUN_AUTO_INSTALL=false`: kills the exact logged mechanism (upfront
  whole-toolset install on `mise run`, incl. nested `mise run` via env inheritance).
- `MISE_NOT_FOUND_AUTO_INSTALL=false`: shims stay on PATH (see job log); a shim
  miss must fail fast, not slow-install the tool back and silently reintroduce
  minutes. Matches the unit-job trio precedent (`ci-unit-*.yml`).
- Tool lists **unchanged** (gap empty; §1). `cargo-binstall`/`sccache` kept
  deliberately (zero-cost cache hits; `sccache` is S2's wire-up target).
- Declined: `MISE_FRESHEN_CACHE` — **no such mise setting exists**
  (`mise settings --all` on mise 2026.9.12 lists only `task.source_freshness_*`,
  which govern task-output caching, not tool installs).
- Declined: `MISE_AUTO_INSTALL`/`MISE_EXEC_AUTO_INSTALL` — no `mise exec`/`mise x`
  path exists in the graph; the reviewed perf-design S1 names exactly the two vars set.

## 3. Rendered diff summary (pinned regen, no hand-edits)

Renderer: published `velnor-workflow-runtime-v1-af140ad4d8d84326` macOS-ARM64,
SHA `7bee5aab…` verified against release `manifest.json`;
`--revision 4fa7a3a85f141a6bb95bc9bdf0eef9e3ddde165d` = declared pin,
`--closure af140ad4d8d84326…` = pinned closure. (Local `~/.cargo/bin/velnor-workflow`
is rev `10483370…` — NOT used.)

Changed files (3 of 19 outputs; dry-run predicted exactly these):
- `.github/workflows/desktop-merge.yml`: `+ MISE_NOT_FOUND_AUTO_INSTALL: "false"`,
  `+ MISE_TASK_RUN_AUTO_INSTALL: "false"` in job `env` (renderer sorts keys).
- `.github/workflows/desktop-scheduled.yml`: same two lines.
- `.github/ci/.github-actions-generator-state`: `config` hash + the two desktop
  output hashes only (`scan` unchanged; all other 16 outputs byte-identical).

## 4. Gate evidence (all observed this session)

| Gate | Command | Result |
|---|---|---|
| Pre-change cleanliness | `velnor-workflow-macOS-ARM64 --plain --check .` @ clean `799deeef` | `Generated files are current`, exit 0 |
| Dry-run scoping | `--plain --dry-run .` after input edit | 3 files would change (2 desktop + state), 16 unchanged |
| Regen | `--plain generate . --output . --force` (force needed: ownership guard) | `Generated 19 files` |
| Post-change check | `--plain --check .` | `Generated files are current`, exit 0 |
| Byte-identical re-regen | `--plain --dry-run .` after regen | 19/19 `= unchanged`, `0 files would change` |
| Workflow lint | `mise exec -- actionlint desktop-merge.yml desktop-scheduled.yml` (pinned 1.7.12) | exit 0, no findings |
| Mechanism proof (live) | renovate-upstream-sources run 35545317857 (success; same renderer path, `MISE_TASK_RUN_AUTO_INSTALL=false` in job env) | job log contains **0** `installing N tools` lines |

Not run (deliberately): the 38-min desktop graph itself — no runner provisioned for
this slice; merge-time `desktop-merge` on main is the live verification
(expect: green, `Run desktop-merge` reaches `[desktop-ci]` in <60s, ≈576s saved).

## 5. Follow-up (NOT done here)

- **S1b (unit-job mise env)**: unit checks already carry the generator-emitted
  `MISE_AUTO_INSTALL/EXEC/NOT_FOUND=false` trio; any further unit-side change needs
  **generator support** (Velnor-owned) — out of scope for input-only slices.
- **`[release.job.env]`**: perf-design S1 also names both release jobs for the same
  two vars (release drill pays the same ~8min). Excluded by this slice's delegation
  (desktop profiles only); needs its own audit (release graph additionally invokes
  `gh`, `openssl`, `security`, `codesign`, `xcrun stapler`, `git`) + regen.
- **S2 (sccache wire-up)**: `RUSTC_WRAPPER=sccache` + `SCCACHE_GHA_ENABLED=true` +
  `CARGO_INCREMENTAL=0` on desktop profiles / swift unit; needs Velnor `ir.rs` gap
  closure for the structural fix (perf-design §S2).
- **Velnor structural follow-up**: emit `MISE_*=false` in scheduled-checks run steps
  by default (parity with unit checks), so future profiles inherit S1a.
- Live verification on next main `desktop-merge`: assert log lacks
  `installing 26 tools` and step-time delta ≈ −576s.
