# Jackin architecture, reuse and security evidence

Release/source snapshot: **2026-10-24T01:15:00Z**. Jackin base: **`6e266406b49da2ab3c55355f3b7d33d1643ee6cd`**. Research executed on October 24. This directory contains source audits, small isolated probes, independent review and reconciliation; it does not implement or activate a production optimizer.

## What was actually executed

| Operation | Result | Scope and limit |
|---|---|---|
| Pinned-source audit | Read 26 Jackin source files; registered their hashes | Current source ownership, not a full runtime or security test |
| Extracted entrypoint case selection | Six branches produced launch vectors; six registered names returned exit 2 | Only the `case` statement ran, with no runtime setup, role hook, final exec, native agent or container |
| Three minimal Cargo metadata probes | All exited 101 on incompatible `libsqlite3-sys` `links = "sqlite3"` | Extracted rusqlite pairs 0.40.2 plus 0.39/0.34/0.32; no compilation/download and no full product integration graph |
| Full-facade consumer metadata attempt, by `/root/rtk_leanctx` | Exited 101 on uncached `rmcp` before completing resolution | Actual LeanCTX facade path plus Jackin SQLite; not a demonstrated facade build failure or successful link |
| Independent RTK/LeanCTX source and artifact review | Confirmed main source/fixture boundaries; found additional environment-mutation threading prerequisite | Reviewer did not author those product pages and did not rerun their full build/test suites |
| Independent provider/style/host review | No blocking source-versus-execution issue in techniques 04/09/20/23; two completeness suggestions sent | Primary captures and current chapter artifacts only; no native host, schema or image API execution |
| Independent LemonCrow CSV join | Unique/equal 445-trial key sets; every compared aggregate matches; ordinary input 70,310 | Published trial aggregates only; no candidate denominator, billed-trial rerun or invoice validation |
| Local documentation audit | See `documentation-checks.json` for final result | Body/description limits, state markers, local links, RepoFile targets, old-heading coverage and whitespace; repository-wide checks belong to coordinator |

Neither `claude` nor `codex` was found on this workstream's current PATH. No paid inference, subscription session, provider proxy, model download, global installer, production configuration mutation or external issue/comment was performed by this workstream.

## Decision-changing source findings

1. `Agent::ALL` and CLI accept twelve names; the embedded launch script dispatches only Claude, Codex, Amp, Kimi, OpenCode and Grok. The remaining six selections fail before launch. Do not turn twelve adapters into a twelve-host end-to-end support claim.
2. Current six-host launch settings deliberately bypass native prompts/sandbox modes. Jackin has its own configured process/credential boundary; an optimizer must preserve that actual authority instead of claiming native approval prompts are enabled.
3. Existing Claude/Codex/OpenCode integrations report status. Claude PostToolUse is asynchronous; Codex installs seven events and its verifier checks only the script-path string. Installed status configuration is not trusted/accepted synchronous result replacement.
4. `SessionIdentity` contains UID/GID only. Logical session roots/capabilities are separate, and no context-epoch or model-exposure identity follows automatically.
5. The token monitor has five active pollers and aggregate usage fields. It does not yet supply the proposed request/task/transform provenance, lifetime-specific cache writes, reasoning attribution or universal instance-home accounting.
6. `jackin-exec` is a secure execution MCP surface, not a general context optimizer. Its dispatcher ignores cancellation notifications; a new worker needs an explicit cancellation contract.
7. Current official Claude/Codex hooks have different replacement, trust, code-mode and spill semantics. These were researched, not executed against an installed host.

These facts are mapped to proposed ownership, recovery and qualification in the rewritten engine chapters and `tools/11-jackin-integration.mdx`. No proposed manifest field or worker is described as currently shipping.

## Inventory and provenance

- [source-audit.json](source-audit.json): source hashes, twelve selection outcomes, current hook lists, active poller branches, UID/GID type and host PATH lookup.
- [source-register.json](source-register.json): 42 immutable Jackin/upstream source records, including independently inspected RTK and LeanCTX code.
- [host-document-register.json](host-document-register.json): current official host documentation URLs, access records/hashes where retrieved, and the Rust environment-mutation safety reference. Shared host retrieval is explicitly attributed to the provider workstream.
- [command-results.json](command-results.json): exact executable audit command, initial harness correction and references to per-case command records. Parameterized inspection descriptions are not falsely labeled exact executed argv.
- [linkage-constraint-results.json](linkage-constraint-results.json): exact minimal manifests, argv, cwd, environment, elapsed time, exit status and full resolver diagnostics.
- [lean-facade-metadata-attempt.json](lean-facade-metadata-attempt.json): another real workstream's exact consumer attempt and missing-cache limitation.
- [independent-review.md](independent-review.md): cross-review of RTK/LeanCTX, provider/representation chapters and benchmark accounting/gates.
- [lemon-join-review-results.json](lemon-join-review-results.json) and [probes/review_lemon_join.py](probes/review_lemon_join.py): independent CSV hashes, one-to-one join checks, exact command and reproduced totals.
- [security-review.md](security-review.md): current versus proposed authority/state contracts and the smallest missing adversarial tests.
- [coverage-dispositions.json](coverage-dispositions.json): **12 documents, 164 old headings, 317 material prose/list/table/code units and 169 numerical/version-bearing lines**, with source positions, individual topic dispositions and replacement headings. Historical claims are preserved as audit data, not current recommendations.
- [probes/source_audit.py](probes/source_audit.py): executable audit. Downloaded sources, packages, binaries, registries, build outputs and private data are not included here.

Official mutable documentation was observed after the release cutoff on the same research date. Its hashes/access times identify what was read; this is not a claim that a mutable page had identical contents at 01:15. Immutable product/Jackin source pins remain the frozen code baseline. Broader provider details and the remaining host contracts are in the provider workstream's separate record.

## Reproduce the source/dispatch audit

Run from an existing Jackin checkout containing the pinned commit:

```bash
python3 docs/content/research/context/techniques/tools/refresh-2026-10-24/jackin-architecture/probes/source_audit.py \
  --repo /absolute/path/to/jackin \
  --commit 6e266406b49da2ab3c55355f3b7d33d1643ee6cd
```

The script obtains code through `git show`, so unrelated working-tree edits cannot change its source snapshot. It extracts the explicit launch-selection case and adds only a print of the resulting argument vector. It never executes that vector. The probe validates that executable lines contain no substitution/exec/source operation before evaluating the case.

The first audit attempt stopped at our safety assertion because an inert comment contained backticks. The assertion was corrected to inspect executable lines; that attempt is not an upstream failure or a native launch attempt. The passing record is the one retained as `source-audit.json`.

## Reproduce the minimal linkage constraints

Use the exact manifests in `linkage-constraint-results.json` in disposable directories with a single empty `src/lib.rs`. Their essential dependency pair is:

```toml
[dependencies]
jackin_sqlite = { package = "rusqlite", version = "=0.40.2", features = ["bundled"] }
candidate_sqlite = { package = "rusqlite", version = "0.39", features = ["bundled"] }
```

The other two cases replace `0.39` with `0.34` and `0.32`. Run metadata only, with an isolated registry cache already containing the needed index records:

```bash
CARGO_HOME=/absolute/path/to/disposable/cargo-cache \
CARGO_TARGET_DIR=/absolute/path/to/disposable/unused-target \
  /absolute/path/to/rust-1.99.1/bin/cargo metadata --offline --format-version 1 \
  --manifest-path /absolute/path/to/disposable/probe/Cargo.toml
```

Expected on the recorded cache: explicit duplicate-native-link resolver failure, not compilation. An incomplete cache can fail earlier; that is a different result and must not be relabeled a link conflict. These pairs come from current manifest constraints, but they intentionally exclude the full product graphs. Sem's default disk-cache feature is off; RTK engine-only is different from its application. Headroom's reviewed lock uses the same rusqlite 0.40.2 as Jackin and is not one of the conflicting pairs.

## Unexecuted qualification

No current host result-replacement, native edit-receipt, trust interaction, streaming cancellation, full capsule launch, broker fault-injection, concurrent multi-host isolation, complete dependency build or paid accepted-task experiment was run here. The proposed archive/worker does not exist yet. Product tests and source audits are valuable but narrower evidence.

The rollout's task gates and absolute resource budgets are **proposed research acceptance choices**. They are not observed qualifications, performance claims or permission to accept known correctness loss. Full repository docs/render/CI validation is recorded by the coordinator against the actual PR head; this directory never infers that from an older green job.

## Retrieval and adoption integration wave

Techniques08,17,18 are fully reconciled in [integration-coverage-dispositions.json](integration-coverage-dispositions.json): **3 documents, 47 old headings, 273 material units and 164 numerical/version-bearing lines**, including old source tables. The roadmap specifies native baseline, owned configuration/recovery, one real host/filter, canonical task gates and useful automatic packaging, with experimental/off behavior. It does not activate any optimizer.

[retrieval-source-register.json](retrieval-source-register.json) records fifteen fresh primary-source captures and five independently read captures from the provider workstream. [probes/fetch_retrieval_sources.py](probes/fetch_retrieval_sources.py) is the original downloader with its hash/exact historical command in the register. To repeat the public retrieval, copy the script into disposable scratch before running: it writes `retrieval-sources/` and its register beside itself. Raw copyrighted pages are not committed. Mutable pages were accessed after the external cutoff and are not claimed identical at the cutoff. No retrieval/memory/provider benchmark ran in this wave.

The Letta filesystem memory comparison used automatic embeddings and mandatory `search_files`; it is not a grep-only comparator. Native Claude memory documentation now specifies the root-file size limit, worktree sharing and child/enablement exceptions; current skill listings rebuild after compaction. Historical token/day forecasts and broad no-evidence-anywhere claims are bounded or withdrawn in the ledger.

[policy-independent-review.md](policy-independent-review.md) and [policy-review-source-register.json](policy-review-source-register.json) independently review Caveman/Ponytail policy, state and recovery conclusions. The Ponytail map budget was clarified by the author as an approximate body budget; no independent Go/host/provider execution is implied. A small additional market-component review confirms the narrow Rust feature/dependency scope and identifies internal-link/fallback wording corrections for its author.

## Main integration overlay

[main-overlay-review.md](main-overlay-review.md), [main-overlay-source-register.json](main-overlay-source-register.json) and [main-overlay-comparison.json](main-overlay-comparison.json) record the post-cutoff source review at `c98f5f5fdd831e6075e38fde93b9713575406140`. All twenty-six original source-audit files are byte-identical; thirteen selected new source files cover checked installation, account-marker transactions and non-activating receipt inspection. The external-product cutoff is unchanged. [probes/main_overlay_audit.py](probes/main_overlay_audit.py) reproduces the read-only byte comparison; its executed argv is retained in the result. New source guards are preserved without inferring a context broker or source/build execution trust.

[integration-documentation-checks.json](integration-documentation-checks.json) records the final owned-file structure/link/coverage/JSON/whitespace checks for this wave. Repository-wide build, link checks, research gate and actual PR-head status remain the coordinator's responsibility.

## Rendered page verification

The final local browser sweep passed **112/112 page/viewport checks** over all 56 routes, **718 internal URL checks including 657 rendered-DOM fragments**, and **two actual sidebar navigations**, with no captured script error or page-wide horizontal overflow. All 58 recorded build-input hashes remained unchanged. Separate visual review covers **22 representative desktop/mobile captures**, not all 112 full pages.

[rendered-qa-review.md](rendered-qa-review.md) explains the tested boundaries, visual review, resolved harness issues and exact reproduction with caller-supplied local tool paths. [rendered-pages-results.json](rendered-pages-results.json), [rendered-qa-command.json](rendered-qa-command.json), [rendered-qa-environment.json](rendered-qa-environment.json), [rendered-qa-run-history.json](rendered-qa-run-history.json) and [rendered-qa-screenshots.json](rendered-qa-screenshots.json) retain results, all four actual runs, browser provenance, image hashes and limits. The browser/images/build remain disposable scratch artifacts; the shipped script performs no automatic download, paid call or public serving. This page sweep supplements the coordinator's repository gates; it does not qualify an optimizer or native-host integration.

The final sweep was repeated after the late `e737f4b5eb7c1a0d15ba6595fc260407ad178452` main merge and complete-site rebuild. [rendered-qa-main-build-comparison.json](rendered-qa-main-build-comparison.json) preserves exact comparisons; the preceding successful snapshot remains under `historical-render-runs/pre-release-merge/`. The final loopback server is stopped and its port was confirmed closed.

## Publication JSON formatting

The two archived build-input manifest copies use a documented whitespace-only JSON normalization for publication. All **58 path/digest pairs** and the existing JSON readers are unchanged. Original execution `inputManifest.sha256` fields, dated environment records and snapshot hashes continue to identify the exact original bytes; the published manifest files have distinct file digests. [Publication formatting provenance](../integration/publication-json-formatting.json) records both representations and the verified inverse: `json.dumps(json.loads(text), indent=2) + "\n"` reconstructs the original serialization. The current artifact inventory hashes the published bytes. This manifest-formatting exception does not change the MDX, HTML or screenshot identity comparisons, and no new browser execution is claimed for it.

The pre-release-merge snapshot now includes its referenced [verification script](historical-render-runs/pre-release-merge/probes/render_pages.mjs). The supplied file is byte-identical to the script digest recorded by that historical run. This repairs archive completeness without changing the historical review, execution records or timestamps.
