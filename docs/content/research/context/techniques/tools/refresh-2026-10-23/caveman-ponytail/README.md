# Pinned reproduction method and evidence limits — 2026-10-23

Evidence cutoff: **2026-10-23T15:31:31Z**. This page records pinned source identities, reproduction commands, curated component results, and their limits. This directory contains two small probes, curated semantic result JSON, and compact package correspondence used by product chapters 01 and 09. It contains no source clone, downloaded runtime, build cache, credential, or paid-model result. Original execution traces and full registry/release snapshots are retained as private provenance.

## Source identities and environments

| Local source directory used by probes | Identity | Toolchain |
|---|---|---|
| `caveman` | Product v4.0.1, `d262ab3882952a59312ff3537cd86d894523c5ab` | Node v24.19.0; Go 1.26.9 |
| `caveman-runtime` | Runtime bin-v4.0.2, `b4531dfc05a3ef0ba93b57e8bfa5f48564a4b4d6` | Go 1.26.10 |
| `ponytail` | v5.3.2, `16352ac229eb08a9598dcae48fe9f2b342567d86` | Node v24.19.0 |

Pinned source links are the Caveman product commit [d262ab3882952a59312ff3537cd86d894523c5ab](https://github.com/JuliusBrussee/caveman/tree/d262ab3882952a59312ff3537cd86d894523c5ab), runtime commit [b4531dfc05a3ef0ba93b57e8bfa5f48564a4b4d6](https://github.com/JuliusBrussee/caveman/tree/b4531dfc05a3ef0ba93b57e8bfa5f48564a4b4d6), and Ponytail commit [16352ac229eb08a9598dcae48fe9f2b342567d86](https://github.com/DietrichGebert/ponytail/tree/16352ac229eb08a9598dcae48fe9f2b342567d86). Python 3.12.14 drove the policy probe. Go toolchains, module cache, and build cache were downloaded into task scratch space. No global optimizer was installed. The probes did not call model providers. Source dependencies needed network access for the initial Go module download.

## Exact test selections and recorded outcomes

From the product `caveman` checkout:

```sh
node --test --test-reporter=tap tests/hooks/codex-sessionstart.test.mjs tests/hooks/host-session-start.test.mjs tests/hooks/subagent-start.test.mjs tests/hooks/caveman-config.test.mjs tests/hooks/headless-entrypoint.test.mjs
```

The captured selection reports **84 Node tests passed** in an isolated rerun. Initial captures included separate mode assertions. The available evidence does not establish why those initial assertions failed. The initial and rerun outcomes remain separate evidence and are not collapsed into one cause.

From `ponytail`:

```sh
node --test --test-reporter=tap tests/hooks.test.js tests/behavior.test.js tests/map.test.js tests/uninstall.test.js tests/activation-map.test.js tests/commands.test.js tests/band-import.test.js
```

The isolated selection reports **22 Node tests passed**, exit zero. Some files contain more than one internal assertion. Initial captures included child-process EPERM and separate mode and uninstall assertions. EPERM identifies an execution-sandbox restriction; the available evidence does not establish the cause of the other assertions. The separate outcomes do not establish a full upstream suite or cross-platform behavior.

Separately, from **each** Caveman checkout, using its corresponding toolchain and task-specific scratch caches:

```sh
GOTOOLCHAIN=local GOCACHE="$TASK_GO_CACHE" GOMODCACHE="$TASK_GO_MODULE_CACHE" "$TASK_GO_BIN" test -count=1 -short ./engine ./engine/ccr ./engine/tokens ./engine/pixel ./proxy/internal/config
```

The captured outcomes show all five selected packages passing for product v4.0.1 with Go 1.26.9 and independently for runtime bin-v4.0.2 with Go 1.26.10. The `-short` selection is not the full upstream suite or cross-platform verification.

## Reproducing the probes in a disposable workspace

Use a disposable directory with these exact source checkouts in sibling directories named `caveman`, `caveman-runtime`, and `ponytail`. Copy `probe_policy.py` and `probe_caveman.go` into their parent, then verify each `git rev-parse HEAD` against the source table. The scripts preserve the layout and command inputs used for the historical run; no downloaded source is vendored here. Run only from that disposable copy: generated output must not overwrite the curated JSON in this directory or be added to Git.

```sh
python probe_policy.py
```

The Python script creates temporary `CLAUDE_CONFIG_DIR`, `XDG_CONFIG_HOME` and `PLUGIN_DATA` paths. It invokes delivered JavaScript, disables the map for instruction/startup measurement, and writes generated instruction text plus `policy-probe-results.json` beside itself. It tests state behavior under explicit environment conditions, not a live host session. It does not claim all real Codex hosts share plugin-data paths.

From `caveman-runtime`, with Go 1.26.10 and scratch caches:

```sh
GOTOOLCHAIN=local GOCACHE="$TASK_GO_CACHE" GOMODCACHE="$TASK_GO_MODULE_CACHE" "$TASK_GO_BIN" run ../probe_caveman.go ../caveman-injected.txt ../ponytail-injected-full.txt ../caveman/skills/caveman/SKILL.md ../ponytail/skills/ponytail/SKILL.md > ../caveman-probe-results.json
```

The Go probe generates a 45,007-byte JSON array containing Unicode, CRLF and no final newline. It checks record passthrough, no-store passthrough, exact recovery after compression, malformed-JSON passthrough and a missing-handle error. The synthetic input is highly repetitive: its 14,002-to-139 o200k_base token reduction is not an accepted-task saving. The eleven in-memory compression timings began after tokenizer initialization; their individual values remain in private provenance and do not establish cold-start, disk-I/O, peak-memory, or whole-agent latency.

Instruction counts are estimated with Caveman's embedded o200k_base tokenizer: generated Caveman 1,248 tokens versus source-file 1,158; generated map-disabled Ponytail 639 versus source-file 802. Generated text is recreated from the pinned upstream sources rather than copied into this repository. These counts exclude other host-added instructions and tool schemas and are not provider-billed usage.

## Curated results and release/package provenance

The published <code>caveman-probe-results.json</code> is a curated semantic subset. It retains <code>default_record_exact</code>, <code>no_store_passes_original</code>, <code>malformed_json_passes_original</code>, and <code>missing_recovery_is_error</code>; fixture bytes, content type, token counts, and byte-exact CRLF/Unicode/no-final-newline recovery remain exact. It omits only the random recovery handle and the individual compression timing samples. <code>policy-probe-results.json</code> retains the three state-behavior booleans and the Caveman/Ponytail injection byte counts; it omits only the individual map-disabled activation timings. Full original values remain in private provenance.

The GitHub release cutoff is **2026-10-23T15:31:31Z**. The v4.0.1 Caveman product release was published 2026-10-13T18:34:04Z ([release](https://github.com/JuliusBrussee/caveman/releases/tag/v4.0.1)); Ponytail v5.3.2 was published 2026-10-16T23:15:15Z ([release](https://github.com/DietrichGebert/ponytail/releases/tag/v5.3.2)). The separately reviewed bin-v4.0.2 runtime release was published 2026-10-23T05:22:07Z ([release](https://github.com/JuliusBrussee/caveman/releases/tag/bin-v4.0.2)), before the cutoff. Tag object <code>62a906068603b51922e1aa609424f2548950cfab</code> resolves to runtime source commit <code>b4531dfc05a3ef0ba93b57e8bfa5f48564a4b4d6</code>. The Ponytail default-branch commit <code>09c434ab73ca43761c5d8b0c5d341926c2fba710</code> contains later benchmark cleanup and was not used to infer v5.3.2 behavior.

Registry snapshots were accessed at their recorded times: npm 2026-10-23T15:36:24.390237+00:00, package correspondence 2026-10-23T15:37:47.728900+00:00, and PyPI 2026-10-23T15:39:50.368258+00:00. These records preserve the selected versions from the frozen release cutoff.

| Registry package | Version | Recorded publication or upload | Recorded license label |
|---|---|---|---|
| npm @caveman-ai/cli | 4.0.1 | 2026-10-13T18:31:25.030Z | Apache-2.0 |
| npm @caveman-ai/pi | 0.3.1 | 2026-10-13T18:32:14.958Z | Apache-2.0 |
| npm @caveman-ai/sdk | 1.2.0 | 2026-09-30T22:27:05.469Z | Apache-2.0 |
| npm @caveman-ai/middleware | 1.0.1 | 2026-10-08T05:17:19.694Z | Apache-2.0 |
| npm caveman-installer | 0.0.0 | 2026-09-30T17:13:04.390Z | UNLICENSED |
| npm @dietrichgebert/ponytail | 5.3.2 | 2026-10-16T23:06:18.715Z | MIT |
| PyPI caveman-sdk | 1.2.0 | wheel 2026-09-30T22:27:38.070979Z; sdist 2026-09-30T22:27:39.368038Z | Apache-2.0 |
| PyPI caveman-middleware | 1.0.0 | wheel 2026-09-30T22:34:39.874597Z; sdist 2026-09-30T22:34:41.213044Z | Apache-2.0 |

These are registry labels, not legal clearance.

The immutable [Caveman CLI 4.0.1 npm archive](https://registry.npmjs.org/@caveman-ai/cli/-/cli-4.0.1.tgz) has registry SHA-1 <code>5572d3985c34efd34b0c0a4c9a2c4170e5019b28</code>; the immutable [Ponytail 5.3.2 npm archive](https://registry.npmjs.org/@dietrichgebert/ponytail/-/ponytail-5.3.2.tgz) has registry SHA-1 <code>95842ed4d5dadf4191a8d1d26eba9b9cff56d2fa</code>. The retained <code>package-correspondence.json</code> records downloaded-archive SHA-256 values: the 445,537-byte Caveman CLI archive is <code>78cda01d208eb67747bcc7c063869252a2d316c7786fe504e3f695940e59366f</code>, and the 95,495-byte Ponytail archive is <code>d2e8ef2f98e8f70bf40ffcd3ff157634fe13b2572e10ad70a62fc26db6de6dc5</code>. The 3,496-byte Ponytail skill file is <code>9badd22a32e19cb884e46dfa8d3aae82bb81dfa5484105b8b4f0262df823a509</code> and matches the pinned tag. The npm Ponytail <code>gitHead</code> matches the pinned source commit; the Caveman CLI npm metadata has no <code>gitHead</code>, so its package and GitHub source records remain separate.

The package file checks preserve an important runtime distinction: <code>package/dist/binaries.generated.js</code> references bin-v4.0.1 while <code>package/dist/index.js</code> references bin-v2.0.0. Do not replace these file-specific references with a universal pin. The four retained PyPI archive hashes are: <code>caveman_sdk-1.2.0-py3-none-any.whl</code>, <code>86a930a7f525756d307b741115962e0ef3211369ed3c590ca5d87b88db1d6e74</code>; <code>caveman_sdk-1.2.0.tar.gz</code>, <code>6bb4b61015c860c4161bce68efb5d4fde51d8402e67328883859e1eb72642a17</code>; <code>caveman_middleware-1.0.0-py3-none-any.whl</code>, <code>3fe070982fadce8a8d382d7ac6aafd16d8caa82a204bf5cd48178b8bc2ffb24c</code>; and <code>caveman_middleware-1.0.0.tar.gz</code>, <code>0886e93d9684ab76a3627c13e518aa6b93dfdab602a90ed52d017d7fe3913faf</code>.

The inspected [Linux amd64 bin-v4.0.2 Engine asset](https://github.com/JuliusBrussee/caveman/releases/download/bin-v4.0.2/caveman-engine_linux_amd64) was 30,798,072 bytes with SHA-256 <code>0b450e46732461a931bb86c78e0c8ea1b08d525fa2cf5aa4592d9c218edc46a4</code>; build metadata reports Go 1.26.10, CGO enabled, the pinned source commit, and <code>vcs.modified=false</code>. This is source-attributed build metadata and a digest observation, not an independent release-signature verification, byte-for-byte reproducible build, or acceptance of every platform artifact. Some Go dependency checksum strings were omitted from the retained public build-info capture because the repository secret scanner matched a public checksum; source revision, build flags, binary size, and digest evidence remain. The retained package correspondence provides file-level checks and verification limits.

## Interpretation and outstanding experiments

Current Caveman code is Apache-2.0; pre-v3 BSL conclusions do not apply to this source. Cloud remains a separately operated service. Ponytail's current policy has stronger completeness/security safeguards than its older versions; its map and metrics are independent executable additions. Same-project mode sharing and conditional shared Codex plugin-data state were reproduced. The Band ledger's potential concurrent update race was inspected, not reproduced.

Historical blog/paper and vendor benchmark results were reopened and read, not recomputed or paid-rerun. Exact upstream source links, study versions, denominators and limitations appear in the product chapters and evidence ledger. No percentage from those studies is a measured outcome of these local probes.

Next decisive work is a fixed-model/fixed-effort paired accepted-task experiment with exact model-facing bytes, cache categories, recovery calls and failures retained. Ponytail needs separate exact-policy, full-plugin and Jackin-adaptation arms; Caveman needs one content family and complete native read/edit/permission semantics before a full proxy experiment. Actual host activation, cache effects, complete task quality, subscription capacity, sustained resource use and multitenant security remain unmeasured here.

Captured text logs normalize trailing whitespace. The binary build-info log omits Go dependency checksum strings because the repository secret scanner matched one public checksum; source revision, build flags and binary SHA-256 evidence remain. This does not alter fixture input/output bytes.
