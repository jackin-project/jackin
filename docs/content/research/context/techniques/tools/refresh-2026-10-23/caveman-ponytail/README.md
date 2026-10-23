# Caveman and Ponytail execution record — 2026-10-23

Evidence cutoff: **2026-10-23T15:31:31Z**. Sources and experiments were accessed/executed on October 23; registry records retain exact access timestamps. This directory contains the small probes, sanitized result data, upstream test logs and release/package metadata used by product chapters 01 and 09. It contains no source clone, downloaded runtime, build cache, credential or paid-model result.

## Source identities and environments

| Local source directory used by probes | Identity | Toolchain |
|---|---|---|
| `caveman` | Product v4.0.1, `d262ab3882952a59312ff3537cd86d894523c5ab` | Node v24.19.0; Go 1.26.9 |
| `caveman-runtime` | Runtime bin-v4.0.2, `b4531dfc05a3ef0ba93b57e8bfa5f48564a4b4d6` | Go 1.26.10 |
| `ponytail` | v5.3.2, `16352ac229eb08a9598dcae48fe9f2b342567d86` | Node v24.19.0 |

Canonical repositories: <https://github.com/JuliusBrussee/caveman> and <https://github.com/DietrichGebert/ponytail>. Python 3.12.14 drove the policy probe. Go toolchains, module cache and build cache were downloaded into the task scratch directory. No global optimizer was installed. The probes do not call model providers. Source dependencies needed network access for the initial Go module download.

## Exact test selections and recorded outcomes

From the product `caveman` checkout:

```sh
node --test --test-reporter=tap tests/hooks/codex-sessionstart.test.mjs tests/hooks/host-session-start.test.mjs tests/hooks/subagent-start.test.mjs tests/hooks/caveman-config.test.mjs tests/hooks/headless-entrypoint.test.mjs
```

The ordinary execution sandbox could not support the child-process tests. `caveman-hooks-tests.log` and the two `caveman-headless-*` logs preserve that unsuccessful attempt/diagnosis. With permitted isolated subprocess execution, the same selection passed **84 Node tests**, exit zero, in `caveman-hooks-tests-escalated.log`. Do not count the initial environment failure as a product regression or omit it from execution provenance.

From `ponytail`:

```sh
node --test --test-reporter=tap tests/hooks.test.js tests/behavior.test.js tests/map.test.js tests/uninstall.test.js tests/activation-map.test.js tests/commands.test.js tests/band-import.test.js
```

The initial sandbox attempts are `ponytail-selected-tests.log` and `ponytail-map-*`. The permitted rerun is `ponytail-selected-tests-escalated.log`: **22 reported Node tests passed**, exit zero. Some files contain more than one internal assertion; 22 is the runner's reported count.

Separately, from **each** Caveman checkout, using its corresponding toolchain and task-specific scratch caches:

```sh
GOTOOLCHAIN=local GOCACHE="$TASK_GO_CACHE" GOMODCACHE="$TASK_GO_MODULE_CACHE" "$TASK_GO_BIN" test -count=1 -short ./engine ./engine/ccr ./engine/tokens ./engine/pixel ./proxy/internal/config
```

All five packages passed for product 4.0.1 (`caveman-go-tests.log`) and independently for runtime bin-v4.0.2 (`caveman-runtime-go-tests.log`). `-short` and this package selection are intentional limits; this is not the full upstream suite or a cross-platform verification.

## Reproducing the custom probes

Use a disposable directory with these exact source checkouts in sibling directories named `caveman`, `caveman-runtime` and `ponytail`. Copy `probe_policy.py` and `probe_caveman.go` into their parent. Verify `git rev-parse HEAD` against the table before running. The scripts preserve the layout and command inputs actually executed; no downloaded source is vendored here.

```sh
python probe_policy.py
```

The Python script creates temporary `CLAUDE_CONFIG_DIR`, `XDG_CONFIG_HOME` and `PLUGIN_DATA` paths. It invokes delivered JavaScript, disables the map for instruction/startup measurement, and writes generated instruction text plus `policy-probe-results.json` beside itself. It tests state behavior under explicit environment conditions, not a live host session. It does not claim all real Codex hosts share plugin-data paths.

From `caveman-runtime`, with Go 1.26.10 and scratch caches:

```sh
GOTOOLCHAIN=local GOCACHE="$TASK_GO_CACHE" GOMODCACHE="$TASK_GO_MODULE_CACHE" "$TASK_GO_BIN" run ../probe_caveman.go ../caveman-injected.txt ../ponytail-injected-full.txt ../caveman/skills/caveman/SKILL.md ../ponytail/skills/ponytail/SKILL.md > ../caveman-probe-results.json
```

The Go probe generates a 45,007-byte JSON array containing Unicode, CRLF and no final newline. It checks record passthrough, no-store passthrough, exact recovery after compression, malformed-JSON passthrough and a missing-handle error. The synthetic input is highly repetitive: its 14,002-to-139 o200k_base token reduction is not an accepted-task saving. Eleven in-memory compression timings exclude cold tokenizer initialization because instruction counting runs first. No disk-I/O, peak-memory or whole-agent timing claim follows.

Instruction counts are estimated with Caveman's embedded o200k_base tokenizer: generated Caveman 1,248 tokens versus source-file 1,158; generated map-disabled Ponytail 639 versus source-file 802. Generated text is recreated from the pinned upstream sources rather than copied into this repository. These counts exclude other host-added instructions and tool schemas and are not provider-billed usage.

## Release and package verification

- `release-metadata.json`: selected live GitHub release, tag, commit and canonical-repository metadata. It includes the runtime originally associated with the product, bin-v4.0.1.
- `bin402-metadata.json`: separately discovered latest runtime, **published before the cutoff**, tag object, resolved commit and asset metadata. Its inclusion does not silently change the v4.0.1 hook tests into bin-v4.0.2 tests.
- `npm-metadata.json` and `pypi-metadata.json`: live version/publication/license/asset metadata and exact access timestamps where recorded.
- `package-correspondence.json`: downloaded npm archive checksums and file checks; Ponytail skill bytes match the tagged source. The published Caveman CLI's generated binary map still pins bin-v4.0.1.
- `caveman-release-binary-buildinfo.log`: `go version -m` against downloaded Linux amd64 bin-v4.0.2 Engine. SHA-256: `0b450e46732461a931bb86c78e0c8ea1b08d525fa2cf5aa4592d9c218edc46a4`; 30,798,072 bytes. It reports Go 1.26.10, CGO enabled, the pinned source commit and `vcs.modified=false`.

Source availability, archive checksum correspondence and inspected build metadata are different checks. We did **not** reproduce the released Engine byte-for-byte, independently verify a release signature, install every platform artifact, or equate the source version with an unpublished package.

## Interpretation and outstanding experiments

Current Caveman code is Apache-2.0; pre-v3 BSL conclusions do not apply to this source. Cloud remains a separately operated service. Ponytail's current policy has stronger completeness/security safeguards than its older versions; its map and metrics are independent executable additions. Same-project mode sharing and conditional shared Codex plugin-data state were reproduced. The Band ledger's potential concurrent update race was inspected, not reproduced.

Historical blog/paper and vendor benchmark results were reopened and read, not recomputed or paid-rerun. Exact upstream source links, study versions, denominators and limitations appear in the product chapters and evidence ledger. No percentage from those studies is a measured outcome of these local probes.

Next decisive work is a fixed-model/fixed-effort paired accepted-task experiment with exact model-facing bytes, cache categories, recovery calls and failures retained. Ponytail needs separate exact-policy, full-plugin and Jackin-adaptation arms; Caveman needs one content family and complete native read/edit/permission semantics before a full proxy experiment. Actual host activation, cache effects, complete task quality, subscription capacity, sustained resource use and multitenant security remain unmeasured here.

Captured text logs normalize trailing whitespace. The binary build-info log omits Go dependency checksum strings because the repository secret scanner matched one public checksum; source revision, build flags and binary SHA-256 evidence remain. This does not alter fixture input/output bytes.
