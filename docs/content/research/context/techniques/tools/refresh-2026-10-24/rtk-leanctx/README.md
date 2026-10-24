# RTK and LeanCTX verification record

Research source cutoff: **2026-10-24T01:15:00Z**. Executable verification and final identity checks continued on October 24 against those frozen releases. This directory contains authored harnesses, bounded outputs and provenance, not upstream source copies, caches or binaries. Product decisions are in `tools/03-rtk-design.mdx` and `tools/04-leanctx-design.mdx`.

## Frozen components

| Component | Source | Release archive SHA-256 |
|---|---|---|
| RTK full 1.0.1 | `rtk-ai/rtk`, `4ef8730cf3a0dea11c8d64a9b90bc0616fe52100` | `5b919c3bccb799b67a61866d0e617410a4fcdf173e2614440e59de2fa2003436` |
| RTK OSS 1.0.1 | Same tag/commit, default features disabled upstream | `a44e8ffc43981100b4ddf1710399a1649a6c874591aa6d0c0727fe6cbd983c41` |
| LeanCTX 3.11.4 | `yvgude/lean-ctx`, `3afe2bbffe0834ec58517ffd493e5a51c5c1ca57` | `f7a4d677b77266bfb60883485e4ee7ad3fd7e129d3f935c08dee2d50a8b1df71` |

All three archives target Linux x86_64 musl; hashes matched GitHub release asset digests. Both RTK editions report `rtk 1.0.1`; LeanCTX reports 3.11.4. Full RTK archive includes Apache and engine BUSL license texts, while the OSS archive has the Apache text. LeanCTX's native archive contains the executable; its release offers notices/SBOM separately. This establishes artifact identity, not byte-for-byte source reproducibility or other-platform correctness.

An independent reviewer verified that the executed OSS binary matches the archive member, SHA-256 `38b915f2a52ce4b7ef7d6dc3966aaeccb9f754731b2933571bf85026bcba8677`; see [identity cross-check](../representation/independent-review/identity-crosscheck.json).

`source-register.json` pins each inspected code path and its SHA-256, including confirmation that the inspected worktree bytes match the frozen tag. `registry-checks.json` records official registry responses. RTK 1.0.1 and rtk-engine were absent from crates.io; LeanCTX engine 3.11.4 and npm lean-ctx-bin 3.11.4 were present; Rust lean-ctx-embed was absent. `release-metadata` files retain the official asset inventory, without reproducing release prose. LeanCTX's release `target_commitish=main` is not substituted for its resolved tag commit.

`release-recheck-final.json` records the final public latest-stable/tag/asset metadata recheck at **2026-10-24T08:18:35Z**. Both resolved release identities and the three executed Linux archive digests remained unchanged. It does not replace the frozen cutoff or imply another binary/task execution. `heading-dispositions.json` maps every old product03/04 heading—**23 rows, S0782–S0804**—to its current anchor and evidence; material claim changes remain in `claim-dispositions.json`.

## Environment and authorization

See `environment.json`: Linux x86_64/glibc 2.39, Python 3.12.14, Git 2.51.1 and Rust/cargo 1.99.1. Rust was available in a pre-existing read-only toolchain directory but not initially on PATH. Build caches/targets were local to research. All source and release downloads were public; no paid API calls, subscription sessions, model downloads, global installers or production optimizer activation occurred.

The harnesses use explicit product/XDG state paths and do not redefine HOME or CODEX_HOME. RTK telemetry is disabled. LeanCTX disables telemetry, proxy, update checks, automatic rule/skill/hook management and uses headless startup with two worker threads and one index thread. It still advertises executable/edit tools; the probe invokes only read tools. A production read-only integration needs dispatcher/OS enforcement, not this probe's voluntary call selection.

The initial LeanCTX sandbox invocation could initialize MCP but tool execution failed because its process identity could not be established from the exposed `/proc` PID. The same scoped harness executed successfully in the host execution environment without disabling LeanCTX's registration check. Those initial failures are not treated as returned empty reads or a product fidelity result.

## Recorded build outcomes

| Command / log | Status | What may be claimed |
|---|---|---|
| `cargo test --locked -p rtk-engine`; `logs/rtk-engine-test.log` | Exit 101 after unit/integration phases, because `rustdoc` was not on PATH | Initial environment failure; not a complete pass. |
| Same command with toolchain bin directory on PATH; `logs/rtk-engine-test-corrected.log` | Exit 0; 286 unit, 1 guide integration, 0 doc tests | The pinned engine's selected package suite passed on this toolchain/platform. |
| Selected full CLI regression attempts; `logs/rtk-regressions*.log` | Build-script refusal before tests; undeclared development-only `ng_cmd.rs` remained after checkout | No CLI suite pass or release build failure claimed. The complete test selector was not retained, so these attempts are diagnostic setup evidence only. |
| `cargo check --locked -p lean-ctx-embed`; `logs/leanctx-embed-check.log` | Dependency compilation progressed, then interrupted with exit 130 during shared disk exhaustion | **Incomplete**. No completed facade build, integration test, actual minimum-Rust result or upstream compilation defect established. |

The research environment exposed default-branch-only untracked files after a detached tag checkout, and those files reappeared across execution boundaries after attempted removal. Tracked source files in the register matched the stable tags. A clean `git archive` extraction is the required setup for a future full build. Large local targets/caches were removed after logs were preserved to release disk space; another facade build was not started concurrently.

## Reproduce the executable fixtures

Obtain the exact official release archives from the URLs in the metadata files, verify their digests above, and extract to separate local directories. Use a fresh output directory for every invocation: fixtures create Git commits and branches, and are not intended to run over user repositories.

```bash
python3 probe_rtk.py /absolute/full-rtk /absolute/new-full-results /absolute/rust-toolchain-bin
python3 probe_rtk.py /absolute/oss-rtk /absolute/new-oss-results /absolute/rust-toolchain-bin
python3 probe_rtk_boundaries.py /absolute/oss-rtk /absolute/new-diff-results
python3 probe_leanctx.py /absolute/lean-ctx /absolute/new-mcp-results
python3 probe_leanctx.py /absolute/lean-ctx /absolute/new-cold-results --cold-only
```

`probe_rtk.py` intentionally executes only harmless local fixture commands twice to compare native and RTK arms. A transformation failure never triggers a producer retry. Captured stdout/stderr, exits and hashes are retained. It creates a dependency-free Rust crate and runs Cargo offline. Its signal producer is a controlled shim, clearly separate from the real Cargo fixtures.

`probe_rtk_boundaries.py` generates a Git patch with header-like body lines and a missing final newline, then compares `rtk diff -` and `rtk pipe --filter git-diff`. The malformed variant cuts a hunk before its declared budget is satisfied. It also tests invalid UTF-8, non-diff input, the 10 MiB input limit and a missing recall record. Large fallback output is represented by byte length, SHA-256 and a labeled excerpt rather than a multi-megabyte file. An initial harness call omitted the required `-` operand; only the corrected invocation is retained as product evidence.

`probe_leanctx.py` speaks MCP 2025-03-26 over JSONL stdio. Main cases vary full/map/auto mode, unknown/custom/Claude-style identity and aggressive/safe cache policy. Auto cases seed a full delivery before requesting deduplication. It writes a host-style compaction marker, actually checks out another local Git branch, and restarts with the same/different explicit consumer. The separate cold arm has never seen a compaction marker, avoiding the false inference that a restart after a marker exercises every persisted-stub path. `mcp-wire/` retains requests, responses and elapsed call times.

The checked-in main MCP result was captured before the final harness revision added the separate no-marker cold cases to its default run. The matching cold results and wire logs are checked in separately. The final script runs both when `--cold-only` is omitted. The source fixture and case logic are unchanged; this is an additive case, not a replacement result.

## Executed observations

### RTK full and OSS

Each edition completed 25 fixture records: 15 shell comparisons, three byte-sensitive file diffs, four stdin cases, two real Rust producers and one signal producer. All 15 shell cases preserved exits, including tested `&&`/`||`, environment/cwd, quoted paths, nested Bash, redirects, pipes and command substitution. Deferred rewrites are successful boundary preservation, not coverage of transformed output.

Standalone porcelain output lost its trailing newline. Real cargo-check success diagnostics survived without a recall hint. The failing test kept exit 101, panic/expected-value detail and a working recall record. This does not establish that the separate native baseline and wrapper execution emitted identical bytes: Cargo child IDs and timings differ across executions. SIGTERM preserved the shim's pre-signal diagnostic and the wrapper exited by signal; other platforms, process-tree cleanup and arbitrary partial streams are untested.

The additional boundary probe shows the corrected region parser and Git compactor are not interchangeable. `diff -` preserved the no-newline marker on the producer patch; `pipe --filter git-diff` dropped it. A truncated patch fell back through `diff -` with a terminal newline added, while the Git filter returned condensed partial data without an incomplete-hunk warning. Explicit non-diff input to the Git filter produced empty success. Non-UTF-8 and over-cap filtering rejected with exit 1. Generic passthrough retained tested arbitrary bytes. Missing recall failed visibly.

`rtk-rewrite-lint.json` records a later bounded probe of the same OSS binary: `rtk rewrite 'npm run lint'` emitted `rtk lint` with rewrite/ask exit 3. Only the rewrite operation ran; no npm, linter or host approval flow was executed. Pinned source shows the empty lint arguments select ESLint rather than the project script, so script flags/chains need a separate equivalence check.

An [independent four-case probe](../representation/independent-review/rtk-independent-results.json) corroborated the malformed-diff and arbitrary-byte boundaries. Its additional **synthetic stdin** fixture combined 60 valid grep-style matches with `SCAN INCOMPLETE: protected directory unreadable`. `pipe --filter grep` returned 311 bytes and exit 0 while omitting that diagnostic. The probe did not run grep or induce a real permission failure; it demonstrates why preserved producer status and raw recovery remain caller responsibilities.

### LeanCTX

With steering rules off, 12 tools still occupied 10,608 bytes of compact JSON schema; the list includes ctx_patch, ctx_shell and shell. Initialization instructions were empty, but first full reads received a 202-byte nudge and second responses a 115-byte project overview. Schemas and annotation bytes are not provider tokens or bills.

Explicit full reads returned source; map returned selected structure. Auto rereads after full delivery returned a 55-byte unchanged stub for unknown/custom identities, but full content for the Claude process scope. A simulated compaction marker invalidated stubs; auto then returned current cognitive selection, not necessarily the full original. Branch checkout refreshed the changed function name. Raw benign CRLF/Unicode/no-final-newline content matched exactly.

Restart after the marker returned content. In the separate no-marker cold arm, the same custom consumer returned a persisted stub; a different consumer returned content. This supports current scope separation while demonstrating why a host must not reuse a custom scope across lost model context. It is not proof that Claude/Codex actually delivers every needed lifecycle event, or that concurrent worktrees are isolated.

## Evidence limits and next gates

- No live model observed these responses; these are the bytes the MCP client received, not proof of complete host/provider transcript integration.
- No task acceptance, provider usage, cached-token classes, subscription quotas or economic benefit was measured. Local size counters and toy timing differences cannot supply those quantities.
- No full CLI upstream suite, completed LeanCTX facade build, no-ML facade build, Windows/macOS run, concurrent worktree test, native read-before-edit integration or proxy/model routing trial completed.
- Actual disk exhaustion interrupted a build; it was **not** a controlled recovery-store disk-full fault test. No store crash/eviction integrity result is claimed.
- Probe timing samples are not randomized/interleaved performance estimates; RSS, CPU attribution, indexing/model warmup and long-run disk growth remain unmeasured.

The next Rust feasibility check should use a clean archive, explicit isolated target/cache, `CARGO_PROFILE_DEV_DEBUG=0`, `CARGO_INCREMENTAL=0` and two jobs, with adequate disk. The next correctness check should exercise two facade instances with different roots/state/epochs after a successful build. The next host check should create a stub, reset/compact/resume or spawn another consumer, and independently inspect actual delivered observations and raw recovery. Only then should complete-task arms be run at fixed model/effort with the canonical evaluation protocol.

`release-recheck.json` records the 02:19 UTC October 24 final metadata check. RTK remains v1.0.1 at `4ef8730cf3a0dea11c8d64a9b90bc0616fe52100`; LeanCTX remains v3.11.4, whose annotated tag resolves to `3afe2bbffe0834ec58517ffd493e5a51c5c1ca57`. No new binary or task run was substituted during that check. A separate minimal dependency-constraint probe and the incomplete full-facade offline metadata attempt are preserved in the architecture evidence pack; they are different from a successful embedded build.
