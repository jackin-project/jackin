# Caveman and Ponytail: 2026-10-24 evidence

This directory contains fresh source registers, selected execution logs and small offline reproductions. The source snapshot is frozen at **2026-10-24T01:15:00Z**. Reads and tests occurred on October 24 after that cutoff; only releases published before the cutoff are incorporated. Registry/release rechecks at 01:28 UTC, followed by a GitHub-connector fallback for Ponytail, found the same selected versions. `release-recheck.json` retains the HTTP 403 and the successful fallback rather than treating the HTTP error as missing software.

## Source and package identity

| Treatment | Exact source | What was fetched or executed |
|---|---|---|
| Caveman product v4.0.1 | `d262ab3882952a59312ff3537cd86d894523c5ab` | Tagged skills/hooks/source; fresh CLI 4.0.1 npm archive; selected Node tests |
| Caveman runtime bin-v4.0.2 | `b4531dfc05a3ef0ba93b57e8bfa5f48564a4b4d6` | Separate runtime checkout; selected Go source tests and generated fixture |
| Ponytail v5.3.2 | `16352ac229eb08a9598dcae48fe9f2b342567d86` | Tagged policy/hooks/source; fresh npm archive; selected Node and offline Python tests |

`caveman-sources.json` and `ponytail-sources.json` record source-file hashes, pinned URLs, release publication times, peeled commits, annotated tag identities where applicable, release assets, component registry queries and observed licenses. `package-correspondence.json` records downloaded npm archive SHA-256 values, registry SHA-1 matches and selected delivered-file comparisons. No archive was installed. Registry signatures and attestations were observed as metadata, not verified cryptographically.

The Caveman product and runtime commits differ by 103 files (2,611 insertions and 317 deletions). Runtime behavior must be attributed to the runtime commit. The published CLI's generated runtime pin is still `bin-v4.0.1`. Its `bin-v2.0.0` comment describes the signature-format threshold, not a second runtime pin. No fresh runtime-binary download, signature check or byte-identical rebuild was performed; the separate October 23 artifact inspection remains historical evidence.

The runtime checkout was clean throughout the selected Go tests and after execution. Tracked files in the other checkouts match their tags; unrelated untracked later-branch files were observed after filesystem synchronization. Their paths are recorded in the registers. Selected Node/Python test paths do not consume those files. This distinguishes actual source identity from a bare `HEAD` label.

## Execution boundaries

All model calls, subscription sessions and host-global installations were excluded. Test commands used public toolchain/dependency downloads and local HTTP mocks. No provider credentials were passed into the test environment. The inherited network-proxy variable **names** are recorded without values; the environment otherwise limits settings to the selected toolchain, temporary homes where specified, and telemetry/warming off.

Tools were Node **24.19.0**, Python **3.12.14**, Go **1.26.10 linux/amd64**, and Ubuntu GCC **13.3.0**. The Go archive SHA-256 was `ccfe7ae822d2c2f41f4fe7c956199967a4dac22d0ee0d2b2bfdb440aab3b20c5`, checked against the official download metadata. Go cache/module directories and the toolchain were isolated scratch intermediates and removed after execution to release shared disk. No host toolchain was replaced.

`execution-manifest.json` records the exact command argv, cwd, selected source commit, start UTC, relevant environment, duration, child exit status and full original log hashes. `logs/` contains full successful/test-failure output and compact diagnostic excerpts for two setup/build failures. The Python runner's own exit code is not the result of its child: use the recorded `exit_code` field and test log.

| Check | Actual outcome |
|---|---|
| Caveman selected hook suites | 84 tests passed, exit 0 |
| Ponytail selected hook/map/command/uninstall/import suites | 22 tests passed, exit 0 |
| Caveman dependency setup without required network-proxy inheritance | Failed DNS/dependency setup; no tests ran |
| Caveman initial Go compilation | Failed with ENOSPC on the shared filesystem; no test result |
| Caveman six packages, `CGO_ENABLED=0` | Engine TS/JS/Python listing compression cases failed; five other packages passed; exit 1 |
| Same six packages, `CGO_ENABLED=1` | All six passed, exit 0 |
| Cache-warmer switch tests | Passed, exit 0; no provider call |
| Gateway warm/prefix/recovery-exemption selection | Passed, exit 0; local mocks |
| Ponytail benchmark scorer self-test | 18 good/bad scorer pairs and isolation/accounting/cleanup checks passed, exit 0 |
| Ponytail completeness gate offline self-test | Correct stub accepted, reversed stub rejected as intended, exit 0 |

The no-CGO failure is a narrower source-build profile lacking tree-sitter behavior, not a reproduced failure of the release binary. Neither a passing package nor a passing benchmark self-test establishes accepted agent-task quality.

## Reproduction layout and commands

Place the four scripts here in a fresh scratch directory. Create sibling checkouts named `caveman`, `caveman-runtime` and `ponytail` at the three commits above. Supply the stated Go toolchain at `toolchains/go`; create `logs/`. Review public dependency download policies before rerunning. The scripts assume Node/Python on PATH. They do not require a model account.

```sh
python3 run_verification.py caveman-node
python3 run_verification.py ponytail-node
python3 run_verification.py caveman-go-nocgo
python3 run_verification.py caveman-go-cgo
python3 run_verification.py caveman-warm
python3 run_verification.py caveman-gateway
python3 run_verification.py ponytail-scorers
python3 run_verification.py ponytail-completeness-gate
python3 probe_policy.py
python3 probe_policy_extended.py
```

The six-package command expanded by the runner is:

```sh
GOTOOLCHAIN=local CGO_ENABLED=1 GOMAXPROCS=2 go test \
  -p=1 -mod=readonly -count=1 -short \
  ./engine ./engine/ccr ./engine/tokens ./engine/pixel \
  ./proxy/internal/config ./mcp
```

Run the Go fixture from `caveman-runtime`, after `probe_policy.py` has emitted the two activation files. `RESEARCH_ROOT` below denotes the absolute scratch directory containing the three checkouts and the scripts; it is an illustrative substitution for the executed path in the manifest, not a global setting.

```sh
GOTOOLCHAIN=local CGO_ENABLED=1 GOMAXPROCS=2 \
GOCACHE="$RESEARCH_ROOT/go-cache" \
GOMODCACHE="$RESEARCH_ROOT/go-mod-cache" \
../toolchains/go/bin/go run -mod=readonly ../probe_caveman.go \
  ../caveman-injected.txt ../ponytail-injected-full.txt \
  ../caveman/skills/caveman/SKILL.md \
  ../ponytail/skills/ponytail/SKILL.md
```

The command exited 0 on October 24. `caveman-probe-results.json` is its result with a generated recovery handle omitted and instruction-file labels made descriptive. The standalone probe is unchanged. This fixture supplements the package tests; its known-compressible repeated rows are not a representative workload sample.

## Observed properties

- The 45,007-byte JSON fixture includes Unicode, CRLF, no final newline and a rare selected record. Engine compression estimated 14,002 to 139 tokens. Original bytes were recoverable exactly. Default record mode, no recovery store, and malformed JSON passed original bytes through. Missing recovery returned an error.
- A deliberately closed SQLite store returned original content plus an error and no handle. A 1 MiB store accepted a 600 KiB original, rejected a second 600 KiB object, and still returned the first. This exercises storage failure/capacity semantics without pretending to simulate a machine crash or real disk-full race.
- Eleven in-memory compression calls took 13.479–22.041 ms after tokenizer initialization. No peak-RAM, cold-tokenizer, sustained-concurrency or process-start benchmark was performed.
- Caveman activation emitted 4,961 bytes / 1,246 o200k_base tokens in this path/context; its source skill was 4,678 bytes / 1,158 tokens. Ponytail emitted 2,861 bytes / 639 tokens; its source file was 3,496 bytes / 802 tokens. Dynamic paths/context may change exact tokenization. These are local estimates, not host-wire or provider-billing measurements.
- Five locally reserialized Engine MCP tool definitions totalled 3,097 bytes / 694 o200k_base tokens. Host framing, schemas from other tools and repeated instructions are additional costs.
- Ponytail mode state separated the two tested project paths but was shared between sessions in one project. With a shared `PLUGIN_DATA` and no project root, another session saw the changed mode. This last result is conditional on host-provided scope.
- Ponytail preserved off after compact but injected full policy after resume and clear. Caveman preserved off for compact/resume/fork and restored default policy on clear. These are direct script calls with explicit event payloads; no installed host was exercised.
- Ponytail's map fixture found the ordinary ASCII symbol and omitted the Unicode path, space-containing path, test file and late-file symbol. This confirms the tested selection rules, not aggregate retrieval quality. Map-disabled activation took 36.54 ms initially and 32.38–41.68 ms in ten further Node processes.

## Claim limits and disposition

The source registers distinguish policy, proxy, warmer, Engine, memory, SDK, map and metrics surfaces. The reports separate the exact upstream policies from proposed Jackin adaptations. Caveman's optional model-override hook can change installed role frontmatter; no override was set in these probes. The effective default agent files are not a fixed-model compression treatment. Warming was explicitly disabled in the tests; its default-on behavior is source evidence plus local mock/unit tests, not measured provider economics.

The vendor Honest Numbers, wrap report, Ponytail October 7 experiment and benchmark harnesses were reopened. Their dates, versions, denominators and limitations remain historical. They do not become release-matched studies because a new package includes the old report. The coordinator independently reassesses the external studies; these fixtures do not rerun them.

`coverage-dispositions.json` maps every heading of the two previous dossiers at Jackin base `6e266406b49da2ab3c55355f3b7d33d1643ee6cd` and the replaced numerical/material claims. No existing section was silently dropped. No production optimizer, policy activation, host configuration, API spend or subscription consumption was introduced.
