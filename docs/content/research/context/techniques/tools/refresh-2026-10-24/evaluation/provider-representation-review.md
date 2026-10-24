# Independent provider and representation review

Reviewer: `/root/rtk_leanctx`. Author reviewed: `/root/caveman_ponytail`. Source cutoff: **2026-10-24T01:15:00Z**; review and correction verification occurred later on October 24. The reviewer did not author the seven reviewed chapters. Shared provider captures and the reviewer's separate accounting work informed the check; this is an independent analysis of the documentation, not an independent provider experiment.

## Scope and result

Fully read chapters **05, 06, 07, 12, 19, 22 and 28**, plus the relevant CAVEWOMAN discussion in chapter 04. Checked the representation probe's codec/counting implementation, exact fixture/result records, primary paper versions, selected official cache/pricing/compaction/subscription captures and their source registers. The paired JSON file records the reviewed files and capture hashes.

**No blocking issue remains in this scope after two accepted corrections.** This does not qualify task quality, actual cache hits, bills, quotas, a production fleet or every native-host adapter. No paid call, account session, installer, build or model execution ran for this review. The representation author's 14 passing checks are inspected results, not claimed as a second execution by this reviewer.

## Findings and dispositions

| Finding | Evidence and consequence | Disposition |
|---|---|---|
| Argon cache eligibility omitted from the continuation discussion | [Official Gemini caching](https://ai.google.dev/gemini-api/docs/caching) specifies a **4,096-token minimum input** and, separately, the **2,050-token aggregate increment** needed to reuse the full previous request. The latter is not the former. | Author added the minimum to chapter 07's table and chapter 12's paragraph; reviewer confirmed the changed bytes. |
| Claude compaction wording was less precise than the accounting contract | Top-level input/output can omit compaction work. The documented `iterations` array should be priced once, without also adding top-level totals. See [Claude compaction](https://platform.claude.com/docs/en/build-with-claude/compaction). | Author aligned chapter 07 with chapter 12. The accounting implementation and four explicit fixtures were already consistent. |

## Checks that supported the conclusions

**Size, codec fidelity and model use remain separate.** The 18-by-seven fixture includes null, empty strings, booleans, numeric values, leading-zero strings and escaped/multilingual text. Plain delimited rows lose types; JSON-valued cells and fields-plus-rows preserve the recorded typed fixture. Canonical comparison is not byte identity or arbitrary numeric-lexeme preservation, and the chapter says so. Headers are counted while grammar instructions, schemas, host wrappers and recovery are explicitly excluded. The named `cl100k_base` and `o200k_base` encodings are correctly labeled proxies. No model comprehension or current-provider bill is inferred.

**Paper treatments and denominators are retained.** [Notation Matters v2](https://arxiv.org/html/2605.29676v2) studies five open configurations and four tool-use benchmarks. Its schema-plus-response input treatment and additional emitted-call full treatment are not presented as a pure result-codec intervention. The maxima and accuracy losses are study-specific. [CAVEWOMAN v1](https://arxiv.org/html/2606.24083v1) uses eight models, five datasets and 11,465 items per model, with output budgets changing by level and 17 of 20 API cells cheaper. The later [October repository](https://github.com/danielle34/cavewoman/tree/28875b39a6c957a0ae96950538cdacfcd0eff0b3) has a different nine-model panel and 18-of-20 statement. The chapters do not merge these denominators or transfer them to the current Caveman product. Neither complete paper experiment was rerun.

**Cache arithmetic is conditional and internally consistent.** The Argon example's 10,000-token prior satisfies its eligibility minimum. At $2 ordinary / $0.10 cached per million, 2,100 new tokens plus 10,000 reads cost $0.0052; 1,000 new tokens plus a reprocessed 2,050-token suffix and 7,950 reads cost $0.006895. The 32.6% increase concerns input only and is labeled synthetic. The fleet example's ten requests cost 1.70 normalized units after one useful 1.25-unit write, versus 1.75 with an extra warm-up and ten 0.05-unit reads. Neither becomes a total-task or quota percentage.

**Current billing surfaces are distinct.** [OpenAI caching](https://developers.openai.com/api/docs/guides/prompt-caching) treats paid writes as total class prices and divides 30-minute and 12-hour writes; [Codex pricing](https://learn.chatgpt.com/docs/pricing) has no separate credit-billing write class. Claude reasoning is included in output, while Gemini Interactions thoughts are disjoint from visible output. Prewarming is paid and constrained by compatible settings. Workspace isolation is not relaxed to obtain fleet reuse.

**Subscription statements have direct support.** [Monthly API credits](https://support.claude.com/en/articles/17154008-monthly-api-credits-for-max-and-team-plans) documents the Max/Team balances, seven-day eligibility, opt-in/linking, $500 Team pool cap and the distinct self-run API-key versus plan-signed-in headless routes. Its FAQ supports the GitHub Action/IDE/Desktop classification caveat. Codex Pro's five-hour-limit distinction and speed multipliers are separate from opaque included-plan utilization. No actual account capacity is claimed.

**Temporal and causal boundaries remain visible.** Chapter 28 distinguishes access time from frozen source time, immutable revision from mutable documentation, current from announced future prices, and API availability from host exposure. Chapter 06 treats CompactionBench/APEC as author-reported controlled results, separate from the 13.5-million-session observational population; its complete experiment was not independently reproduced.

## Other independent reviews of this workstream

The sibling [accounting review](../representation/independent-review/accounting-review-results.json) independently checks this workstream's four normalization fixtures and exact prices. The architecture review independently recomputes the LemonCrow join and token classes. Those reviews have separate authors, operations and artifacts; they are not counted as executions performed in this review.
