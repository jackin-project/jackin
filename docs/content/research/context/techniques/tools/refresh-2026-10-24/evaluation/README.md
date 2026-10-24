# October 24 evaluation and accounting evidence

## Status and scope

Frozen source cutoff: **2026-10-24T01:15:00Z**. Repository baseline: **6e266406b49da2ab3c55355f3b7d33d1643ee6cd**. Later local operations use the pinned sources. Mutable web pages have separate retrieval timestamps; they are not represented as immutable release artifacts.

This pack records public-source/harness inspection, numeric reaggregation, one upstream pure-grader counterexample, and local accounting regressions. **No paid model calls, subscription sessions, live six-product benchmark, or historical agent reruns were performed.** Reported historical dollars retain their original scope and are not repriced as observed present bills.

The canonical interpretation and proposed acceptance gates are in [the evidence chapter](../../../../tools/07-evidence-and-claims.mdx). The current provider contracts are in [economics](../../../01-economics-and-measurement.mdx). Repository navigation uses the site links on those pages; files here remain directly reproducible without building the site.

## Files

| File | What it contains |
|---|---|
| `source-register.json` | 213 pinned repository files with commit, URL, size, SHA-256 and retrieval/read time |
| `article-source-register.json` | Original articles and official governance docs; successful snapshot hashes and explicit download failures |
| `benchmark-audit.json` | Study-level relationship, treatment, host/model/effort/auth, denominator, exclusions and transfer limits |
| `benchmark-rows.json` | Extracted numeric/public identifiers needed for offline aggregation; no full agent trajectories |
| `aggregation-results.json` | Results of the executed extraction/aggregation |
| `extract_public_rows.py` | Extraction from explicitly downloaded pinned source; also executes only two extracted pure BFCL grading functions |
| `reaggregate.py` | Offline arithmetic over the curated extracts; checks the recorded results |
| `fetch_public_sources.py` | Explicit opt-in public download plan; checks every recorded SHA-256 |
| `provider-accounting-register.json` | Dated official rates and disjoint usage paths; not automatic price discovery |
| `accounting-fixtures.json` | Synthetic API receipts, exact-rate profiles and expected normalized values |
| `accounting-fixture-results.json` | Executed normalization and price results for all four synthetic examples |
| `execution-manifest.json` | Commands, working directories, exit statuses and boundaries for local validation |
| `provider-representation-review.md` / `.json` | Independent review of the sibling provider/representation chapters, accepted corrections, reviewed file hashes and source scope |
| `runtime-independent-review.md` / `.json` | Independent Headroom/LemonCrow runtime review, exact reviewed page hashes, source corroboration and bounded findings |
| `review_runtime_boundaries.py`, `runtime-independent-review-*.json` | Direct Python verifier/history probes, 14 immutable source-file checks, commands and results; no model or native-host run |
| `review_lemon_package.py`, `lemon-package-independent-*.json` | Independent comparison of all 87 release-wheel client files with immutable source objects; no installation |
| `frontier-source-register.json` | Reopened primary paper versions, current provider contracts, capture hashes and mutable-cutoff limits for K1–K16/V1–V8 |
| `frontier-coverage-dispositions.json` | All 66 old headings plus all 24 frontier mechanisms mapped to current treatment |
| `frontier-numerical-dispositions.json` | Conservative inventory of 294 old numerical-line candidates and their scoped withdrawals, corrections or retained conditions |
| `literature-independent-review.md` / `.json` | Independent chapter24 source/method review, AgentDiet/Table4 and TurnCost scope checks, citation/version corrections and exact reviewed hashes |
| `coverage-reconciliation-review.md` | Defects found in the original inventory generator, author-supplied exact mappings, strict remediation and limits of the coverage count |
| `verify_coverage_reconciliation.py`, `coverage-integrity-*.json` | Fifteen mapping guard checks plus actual site-parser/slugger comparison for 663 headings; initial harness failures separately retained |
| `coverage-dispositions.json` | Every old heading in the owned evaluation/economics pages, mapped to a current disposition |
| `removed-numerical-claims.json` | Retired, corrected or conditional historical numerical claims |
| `logs/` | Exact offline extraction, reaggregation and accounting-test output |
| `artifact-sha256.json` | Integrity inventory for this curated pack |

The source snapshots, upstream clones, toolchains and binaries are deliberately not included. Numeric rows are factual extracts, not a redistribution of full upstream repositories. JSON fields preserve source-reported metrics; for example, an ungraded exploration row's `correct=False` is not interpreted as an independently failed acceptance test.

The frontier additions preserve all sixteen K proposals and eight V extensions while retiring unsupported daily-cost floors and combined multipliers. Their primary-paper scope is abstract/version inspection; no training, learned compression or self-hosted serving experiment was run. Mutable provider pages retrieved after the cutoff do not prove identical bytes at the cutoff. The 66 frontier rows are separate from the original 110 evaluation/economics heading dispositions; product RTK/LeanCTX headings have their own [23-row map](../rtk-leanctx/heading-dispositions.json).

## Reproduce without network or provider access

From this directory:

~~~sh
PYTHONDONTWRITEBYTECODE=1 python3 reaggregate.py
PYTHONDONTWRITEBYTECODE=1 python3 ../../test_measurement_tools.py
~~~

The second command uses the instruments directory's test file and imports its sibling modules. The recorded suite has **17 passing tests**. It verifies duplicate/scoped receipt handling, monotone updates, missing write TTLs, invalid accounting, exact CRLF text, image dimensions, paid OpenAI write partitions, Claude compaction iterations, and Gemini disjoint output/thought fields.

Reaggregation checks all numeric sections against `aggregation-results.json`. It carries forward the **recorded** upstream checker result; it does not pretend to re-execute that checker without its pinned source. Likewise, Tokbench's zero source-summary mismatches were established during extraction, not by looking at the derived rows alone.

To reconstruct from public source, first inspect the download plan, then explicitly opt in:

~~~sh
python3 fetch_public_sources.py --destination /tmp/jackin-benchmark-sources
python3 fetch_public_sources.py --destination /tmp/jackin-benchmark-sources --fetch
python3 extract_public_rows.py --source-dir /tmp/jackin-benchmark-sources --output-dir /tmp/jackin-benchmark-extract
python3 reaggregate.py --input /tmp/jackin-benchmark-extract/benchmark-rows.json --expected /tmp/jackin-benchmark-extract/aggregation-results.json
~~~

The fetch step makes ordinary read-only requests to pinned public GitHub content. It does not execute upstream harnesses, install dependencies or contact model endpoints. The original extraction was executed over the inspected snapshots; this generalized re-download helper was not itself rerun against all 213 URLs.

## Repository identities

| Study/source | Pinned commit | Relationship |
|---|---|---|
| daseinlabs/code-compression-bench | `466077f002e6ef71fe5e8f24fc1c930edd4e3691` | Competitor-owned Parsec benchmark |
| rossoctl/context-guru | `17314835e339674d1ae8e6ff6bfd43a6c534e7ca` | Competitor-owned |
| Entelligentsia/tokbench | `6b7098f8b4df20715963d0895ae9a63fd5348dc5` | Forge harness/compressor author |
| shreyassks/headroom-benchmarks | `1eb5a7ad1101697fbf82caaf8c3717e5e3604e1e` | External repository; affiliation not established |
| lemoncrow-lab/benchmarks | `8aaee7ffa5b6f5f7e4a4a9a520af89ed527c7ec1` | First-party historical fuller runtime |
| zuwasi/rtk-token-benchmark | `9803a553838ae59e66c31a55295dd57e56a94fe0` | External RTK 1.0.1 study |
| hystax/optscale-kompressor | `91156bb29c05b2fa6b10f4ca2db5aa9e538f228c` | Competitor-owned |
| JuliusBrussee/caveman | `d262ab3882952a59312ff3537cd86d894523c5ab` | First-party current tree, historical experiments |
| headroomlabs-ai/headroom | `76133cff59b71284399bf760c5578dc373c97231` | First-party current tree, earlier benchmark fixtures |
| lemoncrow-lab/lemoncrow | `d96bad0e59dbc799d54ac5ca1a6a4fe9b8a8ab8d` | Runtime Lite 0.8 source and historical benchmark narrative |

## Decisive reproduced results

### Dasein

The published 700 rows cover 100 tasks in each of seven arms. Recomputed reported cost and solved totals are:

| Arm | Solved /100 | Reported dollars | Dollars / solved |
|---|---:|---:|---:|
| A0 | 57 | 147.3010 | 2.5842 |
| Parsec | 62 | 89.6527 | 1.4460 |
| Woz | 55 | 128.2811 | 2.3324 |
| Headroom | 58 | 212.1427 | 3.6576 |
| RTK | 54 | 165.7721 | 3.0699 |
| Caveman | 58 | 118.9869 | 2.0515 |
| Fermat | 55 | 115.0893 | 2.0925 |

These are the selected published attempts. Earlier aborted or bug-affected runs remain outside that dataset. The July CLI version is not recorded, and the Fermat addition belongs to a later harness/date. The inspected Headroom adapter uses token mode. The official SWE acceptance grader is useful, but these facts do not establish a current same-day seven-arm quality result.

### Current RTK study

The 12 command fixtures produce aggregate reductions of 87.3–88.0% across the supplied local/proxy tokenizers. Those are payload totals, not bills. The constrained agent study has one task with seven factual answers and three threads per arm.

Claude's supplied records improve from 17/21 correct answers to 21/21 while reported cost rises from $1.41271225 to $2.7771755. Amp and Droid differ. Native read tools are forbidden, shell calls are constrained, effort is not fixed, hosts use different model defaults, and an aborted row is not retained in the runner's successful result set. The result is useful release-matched evidence; it does not establish 21 independent task successes or a universal efficiency ranking.

### Tokbench

All 28 available run-summary input sums match the published analysis data. The designated 14-run matrix contains five native runs and three runs for each optimizer, all on **one bug**. Median input counts are 2,183,153 native; 3,015,055 LeanCTX; 2,571,467 Headroom; and 2,839,459 RTK.

Four void attempts contribute 4,998,311 input tokens outside that matrix. Passing build/lint/test checks does not make an incomplete plan or quota failure a clean accepted attempt. Historical GLM/Ollama and tool versions cannot be treated as current selected-model billing.

### Structured Headroom checker counterexample

The pinned BFCL checker accepts a prediction naming `delete_everything` with an unrelated argument value 42 when the expected function is `add(x=42, y=7)`. The extraction parses the module AST and executes only the two pure grading functions; it does not import the API runner or call a model.

The LangGraph headline is also an estimated counterfactual from one compressed trajectory, not two independently executed task arms. Tier-1 calls always place baseline first, and the claimed input reduction excludes cache reads. This combination blocks a broad quality/cost conclusion even though the repository is public.

### LemonCrow historical outcomes and the Terminal input-class discrepancy

Verified rows reproduce 202/250 to 232/250 correct and $234.84328645 to $165.45074645 reported cost. Lite reproduces 49/50 to 48/50 and $19.8300085 to $17.5053555. The Verified manifest conflicts with observed models and narrative settings. September debug source changed during the run; the exploration rows have no independent correctness grade.

The July 28 Terminal job reports 351/445 successes and 30 errors; its historical baseline also reports 351/445. Of the baseline's 445 rows, 34 lack reported cost. Summing the known entries gives $286.92; **that is not the complete bill**.

The published baseline's `tasks.csv.cache_tokens` equals `turns.csv.cache_read_tokens` in **all 445 rows**. Despite its filename, this `turns.csv` contains one aggregate row per trial, not individual provider receipts. The extractor rejects duplicate trial keys and unequal key sets before joining. An [independent reviewer](../jackin-architecture/lemon-join-review-results.json) confirmed both files have 445 unique matching keys, consistent task/input/output fields and the same totals. The cache column does not equal read-plus-write except in the one row with no writes. Exact totals are:

| Class | Tokens |
|---|---:|
| Prompt total | 174,804,998 |
| Cache reads | 161,931,526 |
| Cache writes | 12,803,162 |
| Ordinary input after both subtractions | **70,310** |
| Prompt minus the table's cache column | **12,873,472** |

The headline 12.87 million “fresh” baseline therefore includes writes, contradicting the README's combined-cache label. The advertised 98.6% fresh-input reduction is not supported as named. The candidate's 182K figure was not independently reconstructed here, so no replacement reduction is invented.

The separate normalized-cost script does subtract both reads and writes. Its 16% historical comparison uses one-hour cache-write normalization and 86/89 priceable task averages; it is not the same claim and is not an actual all-attempt bill. The current public Runtime Lite package remains a separate treatment from those historical fuller-engine/persona runs.

### First-party small tables

The six published Caveman wrap cases sum to 885,793 direct input versus 591,673 Caveman input: 33.204% less summed input. The HTML case increases input and remains included. This only reproduces report arithmetic; the private 54-run harness/receipts are absent, cache classes are unweighted, and the October engine snapshot was not rerun through the agent experiment.

The Headroom synthetic proof table sums four scenarios to 178,024 before versus 104,261 after, a 41.434% reduction with its stated tokenizer/seed. The separate tool-search study is one pricing-fix fixture with fake MCP servers, diagnostic variants and three post-fix repeats; 73 runs do not supply 73 diverse tasks.

### Other inspected evidence

The original JetBrains RTK, Ponytail and Caveman articles, Quesma study, CodePointer replay and OptScale Kompressor report were reopened. Their method and exclusion dispositions are recorded in `benchmark-audit.json` and the canonical chapter. Complete private/raw trajectories were not recovered.

Context Guru's July CSV contains nine baseline tasks and ten in each other configuration. Its September correction changes estimated removed-content value while the provider spend stays unchanged. Kompressor's live comparison excludes seven failures from its own streaming integration; its 22.7% claim therefore does not include all attempted deployment work.

## Accounting evidence and policy boundaries

Synthetic fixtures exercise OpenAI paid 30m/12h write classes, Claude compaction iteration replacement, and Gemini output-plus-thought accounting. OpenAI/Claude reasoning is already part of generation totals; Gemini Interactions thought is a separate reported class. Missing historical reasoning, price or write lifetime is not backfilled.

The proposed default-promotion gates for a new complex optimizer are not observed study results: zero known new critical failures; a representative paired/clustered one-sided 95% acceptance bound above −1 percentage point; a one-sided 95% upper all-attempt cost/accepted-task ratio at most 0.80; and p95 completion no more than 10% worse unless an explicit tradeoff profile is accepted. The acceptance margin expresses uncertainty tolerance, not permission to sacrifice known correctness. Insufficient cost precision leaves the candidate experimental or provisional. Native and simple improvements retain proportionate conformance checks.

Current native SDK/workspace budget capabilities supersede old alert-only claims. The online-quality and latency chapters separate documentation from tested enforcement, count asynchronous guard cost, preserve selected model/effort, and retain in-flight overshoot.

## Limits

No artifact here proves a six-way winner, current Runtime Lite quality, subscriber quota savings, broad zero regression risk or confirmatory latency. Earlier private-session decompositions remain unverified because the raw sessions were not recovered. Reproduction of arithmetic does not certify that a study's sampling, grader, receipts or historical activation were correct.
