# Headroom and LemonCrow: source and reproduction record

Snapshot cutoff: **2026-10-24T01:15:00Z**. Public-release recheck: **2026-10-24T01:28:58Z**, unchanged. These are focused component and packaging checks, not an accepted-task benchmark. No paid inference, subscription session, global installer, provider proxy or model download was used.

## Pinned treatments

| Product/treatment | Release/source | Executed environment |
|---|---|---|
| Headroom Rust core, ML disabled | v0.41.0, `76133cff59b71284399bf760c5578dc373c97231` | Linux x86_64, Rust 1.99.1, locked Cargo graph |
| Headroom Python agent/densify and fake-mask reconstruction | Installed `headroom-ai==0.41.0` wheel; key source files matched that commit | Python 3.12.14, pytest 9.0.3; offline/beacon-off environment |
| LemonCrow public Runtime Lite | v0.8.0, `d96bad0e59dbc799d54ac5ca1a6a4fe9b8a8ab8d` | Python 3.12.14, source client; package correspondence independently checked |

The Headroom product version is not the Rust crate's internal `0.1.0` version. LemonCrow's delivered Runtime wheel is named `lemoncrow-runtime-client`, while the source project is named `lemoncrow-client`. The current public Lite runtime and historical fuller engine/persona benchmark are different treatments.

## Evidence inventory

- [release-register.json](release-register.json): repository identities, publication times, release asset digests, Python/npm records and unsuccessful registry queries.
- [release-recheck.json](release-recheck.json): final latest-release identity check. LemonCrow's release `target_commitish` is `main`; the inspected tag resolves to the immutable commit recorded above.
- [source-register.json](source-register.json): 42 primary source/issue anchors and claim scopes; source files have SHA-256 digests.
- [issue-resolution-register.json](issue-resolution-register.json): issue #3099 closure and PR #46's closed-but-not-publicly-merged status.
- [command-results.json](command-results.json): structured command argv, cwd, explicit environment overrides, results and test-input hashes. This preserves the operation without treating tool-wrapper formatting as a shell command.
- [headroom-no-ml-feasibility.json](headroom-no-ml-feasibility.json): normal dependency inventory/MSRV declarations. A declared minimum is not an independently tested minimum.
- [lemoncrow-artifact-correspondence.json](lemoncrow-artifact-correspondence.json): all 87 compared client files, archive hash and wheel metadata. It does not claim every source file is packaged or every release artifact is bit-reproducible.
- [coverage-dispositions.json](coverage-dispositions.json): reconciliation of the previous product-page headings and numerical claims.
- `logs/`: captured outputs, including the explicitly explained initial Python import-environment failure. `probes/`: our small executable fixtures. Downloaded archives, source clones, virtual environments, databases and compiled targets are deliberately excluded.

## Findings that affect the decision

| Check | Outcome | Qualification limit |
|---|---|---|
| Headroom no-default-features core check | Passed; 185 normal package/version nodes including root; no ort/fastembed/magika | Native C/SQLite/tokenizer/network dependencies remain; no other platform tested |
| Headroom Rust JSON fixture | Duplicate key's earlier value removed; Unicode escapes normalized; malformed JSON rejected | Exact raw evidence needs a stronger domain/byte-preservation contract |
| Headroom SQLite fault | Removing its table caused `put` to return unit and later `get` to return no value | Schema-loss fault, not disk-full simulation; no successful-write acknowledgement |
| Headroom selected Python tests | 18 passed in 0.31 seconds against matched installed wheel | Four use a fake keep mask; no ML inference or complete task |
| Headroom Python/Rust shell boundaries | Python fix and four reconstruction tests confirmed; Rust still splits whitespace and joins with spaces | Rust Kompress was source-inspected, not executed with model weights |
| LemonCrow Runtime Lite selected tests | 45 passed in 6.41 seconds | One upstream test file; not the full engine suite |
| LemonCrow real Git-worktree fixtures | Parallel explicit roots correct; relative request stayed with launch root; explicit other-root selection worked; mixed roots refused | Direct library API, not every host/process/resume topology |
| LemonCrow exposure boundary | Five tools advertised; unadvertised `grep` still callable | A read-only adapter must deny calls in dispatch, not just hide schema |
| LemonCrow repeated-shell spill fault | `unchanged` output remained after spill creation failed, with no recovery path | Concrete output-replacement blocker; retrieval-only adapter can exclude shell |
| LemonCrow release package correspondence | All 87 packaged client files matched source bytes; archive digest matched release | Does not establish full artifact build reproducibility |

## Reproduction

Use an empty disposable directory, an existing suitable Rust toolchain and a Python 3.12 interpreter. Dependencies require ordinary public registry downloads during preparation. The selected execution paths do not require model weights or paid calls. Paths below are variables for reproducing the structured commands; recorded absolute paths are in `command-results.json`.

```bash
export HL_WORK="/absolute/path/to/disposable/headroom-lemoncrow"
export HL_EVIDENCE="/absolute/path/to/this/evidence/directory"
export HL_RUST_BIN="/absolute/path/to/rust-1.99.1/bin"
mkdir -p "$HL_WORK"
cd "$HL_WORK"
git clone https://github.com/headroomlabs-ai/headroom.git headroom
git -C headroom checkout --detach 76133cff59b71284399bf760c5578dc373c97231
git clone https://github.com/lemoncrow-lab/lemoncrow.git lemoncrow
git -C lemoncrow checkout --detach d96bad0e59dbc799d54ac5ca1a6a4fe9b8a8ab8d
git -C headroom status --porcelain --untracked-files=no
git -C lemoncrow status --porcelain --untracked-files=no
export CARGO_HOME="$HL_WORK/cargo-home"
export CARGO_TARGET_DIR="$HL_WORK/target"
export RUSTC="$HL_RUST_BIN/rustc"
export UV_CACHE_DIR="$HL_WORK/uv-cache"
uv venv --python python3.12 "$HL_WORK/venv"
uv pip install --python "$HL_WORK/venv/bin/python" -r "$HL_EVIDENCE/logs/python-environment.txt"
```

The executed test trees had no tracked modifications. The only added Headroom source-tree file was the example copied below. Do not diagnose an upstream failure from a checkout containing default-branch leftovers; a fresh checkout or clean `git archive` of the pinned commit avoids that ambiguity.

```bash
cd "$HL_WORK/headroom"
"$HL_RUST_BIN/cargo" check --locked -p headroom-core --no-default-features
"$HL_RUST_BIN/cargo" tree --locked -p headroom-core --no-default-features -e normal
mkdir -p crates/headroom-core/examples "$HL_WORK/probes"
cp "$HL_EVIDENCE/probes/headroom_fidelity.rs" crates/headroom-core/examples/jackin_research_fidelity.rs
"$HL_RUST_BIN/cargo" run --locked -p headroom-core --no-default-features --example jackin_research_fidelity -- "$HL_WORK/probes/headroom-ccr.sqlite3"
```

This probe creates and damages only its disposable CCR database. It does not run a shell command supplied by a model. Its final debug microtiming line has an incorrect hard-coded `47_bytes` label: the fixture is actually **44 ASCII bytes**. Keep the raw log for audit, but do not use that line as performance evidence or as a release-build/task latency measure.

Run Python tests outside the Headroom source package so they load the installed compiled wheel. Running directly from the clone initially shadowed the wheel with a source package lacking `_core`, causing nine densify failures. That harness mismatch was resolved before reporting the passing result.

```bash
cd "$HL_WORK"
mkdir -p probes/upstream-tests
cp headroom/tests/test_densify_agent_mode.py probes/upstream-tests/
cp headroom/tests/test_kompress_line_integrity.py probes/upstream-tests/
HEADROOM_OFFLINE=1 HEADROOM_BEACON=off HEADROOM_CCR_BACKEND=memory \
  "$HL_WORK/venv/bin/python" -m pytest -c /dev/null \
  probes/upstream-tests/test_densify_agent_mode.py \
  probes/upstream-tests/test_kompress_line_integrity.py -q
HEADROOM_OFFLINE=1 HEADROOM_BEACON=off HEADROOM_CCR_BACKEND=memory \
  "$HL_WORK/venv/bin/python" "$HL_EVIDENCE/probes/headroom_package_identity.py" "$HL_WORK/headroom"
cd "$HL_WORK/lemoncrow"
PYTHONPATH="$HL_WORK/lemoncrow/client/src:$HL_WORK/lemoncrow/src" \
  "$HL_WORK/venv/bin/python" -m pytest client/tests/test_runtime_lite.py -q
cd "$HL_WORK"
PYTHONPATH="$HL_WORK/lemoncrow/client/src:$HL_WORK/lemoncrow/src" \
  "$HL_WORK/venv/bin/python" "$HL_EVIDENCE/probes/lemoncrow_boundaries.py"
```

The LemonCrow probe creates a temporary Git repository and two registered worktrees, turns local tracing off and closes its servers. Repeated commands are a deliberately side-effect-free output fixture. Tiny cold/warm index timings are not task benchmarks. No native host permission contract is tested by direct library calls.

For archive verification, download the Runtime client archive identified by name/URL in `release-register.json`, then run:

```bash
"$HL_WORK/venv/bin/python" "$HL_EVIDENCE/probes/lemoncrow_artifact_correspondence.py" \
  /absolute/path/to/downloaded-runtime-client.tar.gz "$HL_WORK/lemoncrow"
```

The script reads archive members without extracting or installing them. Confirm SHA-256 `73bb7c7d062f1f622dff50a851d36a71b1c8efa9ebebd7017d1d09cf81529e94`. The separately tested Headroom Linux wheel digest is `f4db5e5119a73af3ed952b1c10caff2430831d899eb17d995a27574ae6e6b306`.

## Aborted work and unresolved qualification

A separate CCR-filtered Rust test attempt exited 101 during workspace disk exhaustion with an empty log. It produced no test result and is neither a pass nor an upstream-failure claim. Disposable build output was cleaned after retaining the useful results. No broader rebuild was needed for the demonstrated storage fault.

These checks do not establish real-provider billing, subscription quota, cache-adjusted task cost, accepted-task quality, arbitrary result fidelity, context-epoch recovery, cancellation or cross-platform lifecycle behavior. Neither product receives a whole-product adoption recommendation from this evidence. Current pages distinguish inspected source, executed fixtures, upstream benchmark claims and unmeasured next gates.
