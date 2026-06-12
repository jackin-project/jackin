# 03 — Prior art and market scan

Research conducted: 2026-06-12

**TL;DR**

- All four operator-named tools exist and were verified from primary pages on 2026-06-12: **caveman** (71,891 stars) compresses visible prose; **cavemem** (521) stores pre-compressed memory; **cavekit** (1,014) is a spec-driven build loop; **fff** (8,351) is a resident file-search MCP with latency numbers (3–9 s -> sub-10 ms) but **no published token percentage**.
- The market crowds onto the two smallest cost buckets of a real heavy session — visible prose (17% of dollars) and memory files (slice of the 2% uncached input) — while **nothing ships** for the big three: cache reads (32%), cache writes (29%), thinking (20%) (local measurement, 2026-06-12; see 01-economics-and-measurement.md).
- The best-evidenced technique is negative-cost and already a Claude Code default: MCP tool deferral / Tool Search Tool — **85% tool-token cut with accuracy up 49% -> 74%** (Anthropic, accessed 2026-06-12); locally 1,420 -> ~60 tok (96%) on an 11-tool fleet.
- Caching is the floor, not free money: on the modeled heavy day, caching already turns ~$84 into ~$22 (**-74%, ESTIMATE** from local profile + live multipliers) — and the remaining bill is 61% cache reads+writes. Any technique that breaks prefix stability must beat ~5.5x compression just to break even (ESTIMATE, arithmetic below).
- Local kills (fresh measurements, /tmp/ct.py, Fable 5 tokenizer, 2026-06-12): TOON saves **41.2% vs minified JSON**, not "60%" (34.3% of the headline is just minification); caveman's "~75%" replicates locally as **58.5%** on tokens; the Fable-vs-Sonnet tokenizer gap measured **+39.7% (code) / +44.7% (prose)** today — *above* the official "up to 35%" note, making cross-tier price math wrong by ~1.4x.

## Scope and method

Scanned: the four operator-named tools, the Claude Code official cost levers, the plugin/MCP ecosystem, gateways, and research-grade prompt compression. Every external claim cites URL + access date 2026-06-12. Local numbers use `/tmp/ct.py` (free `count_tokens` endpoint; single-user-message wrapper adds a constant 7 tokens, measured 2026-06-12 — percentages below are therefore conservative by <1 point). Dollar arithmetic uses the modeled heavy-day profile from 01-economics-and-measurement.md: ~6 sessions/day, ~19 calls/session, 5.5k uncached input, 85k cache-write, 1.17M cache-read, 27k output (54.8% thinking), ~$22/day split 32% cache-read / 29% cache-write / 20% thinking / 17% visible output / 2% uncached input. Validation protocols below are designed to run on the harness in 31-validation-harness.md. Baseline overhead for this repo is in 02-baseline-audit.md.

## Market map (2026-06-12)

| Bucket | Members (record #) |
|---|---|
| Industry standard (automatic or near-universal) | prompt caching (05), /clear+/compact hygiene (06), CLAUDE.md slimming (07), model tiering (08), effort control (09), MCP tool deferral (10) |
| Significant industry use | caveman (01), fff (04), TOON serialization (14), claude-mem (15), aider repo map (16), LiteLLM gateway (17), ccusage + /usage (18) |
| Proven tactics (vendor or peer-reviewed numbers, partial availability) | programmatic tool calling (11), context editing + memory tool (12), batch API + team discipline (13), LLMLingua family (19) |
| Engineer-verified facts and patterns | preprocessing/hooks filtering (20), fixed-overhead accounting (21) |
| Named-tool early adopters (low adoption, weak numbers) | cavemem (02), cavekit (03) |

## Local measurements run for this file (2026-06-12, /tmp/ct.py, claude-fable-5)

Serialization formats — identical 10-row x 4-field uniform dataset; nested config control:

| Sample | Tokens | vs pretty JSON | vs minified JSON |
|---|---|---|---|
| Pretty JSON (2-space) | 484 | — | — |
| Minified JSON | 318 | -34.3% | — |
| TOON | 187 | -61.4% | -41.2% |
| Nested config, pretty | 272 | — | — |
| Nested config, minified | 194 | -28.7% | — |

An independent same-day run (sweep agent, different dataset, same shape) got -60.6%/-40.8%/-33.6% — agreement within ~1 point.

Tool-result filtering — synthetic 630-line cargo-test log (577 pass, 3 failure blocks), filtered with `grep -B1 -A6 -E "FAILED|panicked|error\[" | head -100`:

| Sample | Lines | Tokens | Cut |
|---|---|---|---|
| Raw log | 630 | 10,108 | — |
| Filtered (all 3 failures fully preserved) | 35 | 590 | **-94.2%** |

Caveat: repetitive pass-lines are the favorable (and typical) case; logs of unique lines compress less — the honest claim stays "80–99%".

Cross-tokenizer, identical fixed text (wrapper constant 7 tok included):

| Sample | Fable 5 | Sonnet 4.6 | Haiku 4.5 | Fable premium |
|---|---|---|---|---|
| English prose, 730 chars | 224 | 157 | 157 | **+42.7%** (+44.7% net of wrapper) |
| Rust code, 1,800 bytes (`crates/jackin-build-meta/src/lib.rs`) | 777 | 558 | 558 | **+39.2%** (+39.7% net) |

Sonnet 4.6 and Haiku 4.5 return byte-identical counts — shared tokenizer confirmed. Phase-0 (same day, other samples) got +15% (code) / +38% (prose); the band is **+15% to +45%, content-dependent**, and today's samples exceed the official "up to 35%" note (flagged in record 08 and the kill list).

Self-cost of the optimizer: the caveman plugin family's always-on skill listing (7 skills, names + descriptions as injected this session) = **940 tokens** of prefix per session — ~0.5% of the modeled day in cache-read rent before it saves anything. Root AGENTS.md re-measured at **2,744 tokens** (phase-0: 2,738; 0.2% drift).

---

## The four operator-named tools

### 01. caveman — terse-register output compression (JuliusBrussee/caveman)

- **Pitch:** "why use many token when few do trick" — skill/plugin forcing fragment-syntax answers; README headline "cuts ~75% of output tokens" (fetched 2026-06-12). 71,891 stars (gh api, 2026-06-12).
- **Layer:** output (visible prose only).
- **Mechanism:** register change via skill prompt: lite/full/ultra strip filler; wenyan variants answer in Classical Chinese. Companions: caveman-compress (memory files, "~46% input tokens"), cavecrew subagents ("~60% fewer tokens than vanilla"), caveman-commit/review; sibling project caveman-code claims "~2x fewer tokens than Codex" for a whole agent.
- **Expected savings:** visible output is 17% of modeled-day dollars ($3.74). Ultra register locally cuts 58.5% of prose tokens; code/diffs pass through verbatim, so realistic whole-bill effect is ~4–6% of the day (ESTIMATE: 17% x 58.5% x prose-share; independent mayhemcode estimate ~4%), hard ceiling ~10% if all visible output were prose.
- **Evidence tier:** T3, partially replicated locally. README benchmark table (fetched 2026-06-12): 10 tasks, per-task mean 65% (range 22–87%), pooled 1214 -> 294 tokens = 75.8% — the "~75%" headline is the pooled ratio, the 65% is the per-task mean; both are real numbers off one table, against an "Answer concisely." baseline ("so the delta is honest" — README), reproduction scripts in `benchmarks/`. Local: ultra = 58.5%, wenyan-full = 56.6% (tokens; 80.9% chars), wenyan-ultra = 74.5% (local measurement, 2026-06-12).
- **Quality risk:** QUALITY-TRADE at ultra/wenyan (lossy register, transcripts hard to read for humans); ~NEUTRAL at lite/full for technical content. README itself concedes the economics: "Caveman only affects output tokens — thinking/reasoning tokens untouched... Biggest win is readability and speed, cost savings a bonus," and cites a March 2026 arXiv paper (2604.00025, repo-cited, not independently verified) claiming brevity *improved* accuracy 26 points on some benchmarks. Degradation would show as dropped caveats/rationale in answers; falsify by blind-grading paired verbose/caveman answers for information loss.
- **Availability:** CLAUDE-CODE-TODAY (plugin marketplace; installed in this very environment).
- **Effort to adopt:** minutes.
- **Composability:** stacks with caching, hygiene, deferral. Anti-synergy: does not touch thinking (54.8% of output tokens locally); its own skill listing costs 940 tok/session of prefix (local measurement above); wenyan modes interact badly with the Fable tokenizer (CJK ~1.47 chars/tok).
- **Validation protocol:** 20 fixed Q&A/coding tasks, A/B caveman-ultra vs "answer concisely" baseline; count visible-output tokens from JSONL usage minus thinking; blind-grade answer completeness on a 5-point rubric; require <=0.5 point quality drop and report token delta per register.

### 02. cavemem — pre-compressed persistent memory over MCP (JuliusBrussee/cavemem)

- **Pitch:** cross-agent memory that compresses observations *before* storing (SQLite+FTS5), so recalled context re-enters already terse; claims "~75% fewer prose tokens". 521 stars (gh api, 2026-06-12).
- **Layer:** input (memory/context injection).
- **Mechanism:** session event -> redact -> caveman-compress -> store; retrieval via MCP search (BM25 + vector blend, alpha 0.5), timeline, get_observations; progressive disclosure (snippets first, bodies on demand); code/URLs/paths preserved verbatim.
- **Expected savings:** unmeasurable from published data. Addressable surface on the modeled day: memory injection lands in the 29% cache-write + 32% cache-read lines as added prefix; the claimed offset (avoided re-exploration) is unquantified by anyone. Given local register measurements, expect ~55–60% on stored prose, not 75% (ESTIMATE).
- **Evidence tier:** T4 verging T3-weak — one worked example in the README (fetched 2026-06-12), no benchmark table, no independent replication found, plus it adds its own MCP schema overhead.
- **Quality risk:** RISKY — lossy memory can drop nuance the original session had; retrieval ranking unmeasured. Degradation shows as confidently-wrong recalled "facts". Falsify by quizzing the store against original transcripts.
- **Availability:** CLAUDE-CODE-TODAY (MCP server; also Cursor, Gemini CLI, OpenCode, Codex).
- **Effort to adopt:** minutes-to-hours (install + its schema rides every session).
- **Composability:** family stack ("cavekit orchestrates, caveman compresses output, cavemem compresses memory" — getcaveman.dev, 2026-06-12); direct competitor to claude-mem (15); deferral (10) mitigates its schema cost.
- **Validation protocol:** one week A/B (memory on/off) on the same project; ccusage day totals + count injected-context tokens per session vs re-exploration tokens saved; separately ct.py-verify the ~75% claim on 50 real observations.

### 03. cavekit — spec-driven autonomous build loop (JuliusBrussee/cavekit)

- **Pitch:** NL -> compressed blueprint kits -> parallel build -> verification; token story is caveman-encoded blueprints ("~75% fewer tokens than prose") and a durable SPEC.md surviving /clear. 1,014 stars (gh api, 2026-06-12).
- **Layer:** turn-structure / orchestration artifacts.
- **Mechanism:** planning artifacts are stored compressed; a repo-root spec means compaction or /clear does not force re-deriving intent. Token relevance is secondary to the workflow change.
- **Expected savings:** no end-to-end number exists anywhere (verified 2026-06-12). On the modeled day the artifact compression touches a slice of the 2% uncached + write lines; the loop itself can *multiply* total calls — net sign unknown.
- **Evidence tier:** T4 — token claims inherited from caveman encoding; no published measurement of cavekit-vs-vanilla cost.
- **Quality risk:** RISKY — autonomous iteration loops can spend far more than they save; degradation shows as ballooning call counts per feature. Falsify with the protocol below; verdict pending measurement.
- **Availability:** CLAUDE-CODE-TODAY (plugin).
- **Effort to adopt:** hours-to-days (whole-workflow change, not a drop-in).
- **Composability:** designed for the caveman family; orthogonal to caching; the durable-spec idea composes with /clear (06) for free.
- **Validation protocol:** build the same 3 small features via cavekit and via a vanilla plan->implement session; compare total day tokens (ccusage) and review both diffs; require equal-or-better diff quality and report net token delta with iteration counts.

### 04. fff — resident file-search MCP (dmtrKovalenko/fff)

- **Pitch:** "A file search toolkit for humans and AI agents. Really fast." — "Fewer grep roundtrips, less wasted context, faster answers" (README, fetched 2026-06-12). 8,351 stars (gh api, 2026-06-12).
- **Layer:** turn-structure (tool-call count) + tool-result tokens.
- **Mechanism:** always-warm Rust index (no per-query spawn/.gitignore re-read); MCP tools ffgrep/fffind with frecency ranking, git-aware annotations, weak-match detection ("flags scattered fuzzy noise before it floods the agent's context"), fuzzy fallback that avoids zero-result dead ends — each avoided dead end is one avoided tool_use/tool_result pair.
- **Expected savings:** token side unquantified by the project — README text publishes latency ("3-9 SECONDS per ripgrep spawn and sub-10 ms per FFF query" on a 500k-file Chromium checkout; ~26 MB resident on 14k files) and a benchmark *chart image* claiming token efficiency, but no percentage appears in text. Tool results flow through the 29%+32% cache lines of the modeled day; direction favorable, magnitude unknown.
- **Evidence tier:** T1 for latency (published, reproducible); T4 for tokens (qualitative claim only: "faster and more token-efficient than the built-in one").
- **Quality risk:** potential NEGATIVE-COST — better-ranked results mean fewer wrong-file reads (quality up, tokens down); unproven on the token axis. Degradation would show as fuzzy false-positives polluting context; falsify via the A/B below.
- **Availability:** CLAUDE-CODE-TODAY (MCP; also Neovim/Rust/C/Node surfaces).
- **Effort to adopt:** minutes (MCP install; deferral (10) absorbs schema cost).
- **Composability:** competes with native Grep/Glob and LSP-style code intelligence (official docs note a "go to definition" call replaces grep + reading candidate files); alternative philosophy to aider's repo map (16): ranked search vs ranked map.
- **Validation protocol:** fixed 20-query search suite on this repo, native Grep/Glob vs fff MCP; sum tool_result tokens and call counts from session JSONL; require equal-or-better target-file hit rate.

---

## Industry standard (automatic or near-universal)

### 05. Prompt caching

- **Pitch:** the single biggest shipped lever; automatic in Claude Code. Reads 0.1x, writes 1.25x (5 min) / 2x (1 h), 512-token minimum on Fable 5 (platform.claude.com/docs/en/build-with-claude/prompt-caching, accessed 2026-06-12).
- **Layer:** cache / price multiplier.
- **Mechanism:** prefix storage with free refresh-on-hit; Claude Code places breakpoints automatically.
- **Expected savings:** already banked in the baseline. Counterfactual on the modeled day: without caching, reads bill 10x (32% -> $70.4) and writes drop to 1.0x — day total ~$84 vs $22 = **-74% (ESTIMATE, arithmetic from local profile + live multipliers)**. Vendor launch table: up to -90% (book chat), -53% (multi-turn) (claude.com/blog/prompt-caching, accessed 2026-06-12). Remaining bill is still 61% cache reads+writes — the optimization target for 13-caching-exploitation.md.
- **Evidence tier:** T1 (live pricing docs + launch table) corroborated locally: heavy-session prompt mix 0.44% uncached / 6.73% write / 92.83% read (local measurement, 2026-06-12).
- **Quality risk:** NEGATIVE-COST (pure price; no model-behavior change). Failure mode is economic only: prefix churn converts 0.1x reads into 1.25x writes. Falsify by diffing usage fields across a deliberately churned prefix.
- **Availability:** CLAUDE-CODE-TODAY (automatic) / SDK (explicit breakpoints, 1h TTL).
- **Effort to adopt:** zero in Claude Code; hours of prefix-stability discipline in SDK.
- **Composability:** stacks with batch (13); undermined by anything mutating the prefix (model switch (08), compaction (06), dynamic system prompts, LLMLingua (19)).
- **Validation protocol:** from session JSONL, compute cache-hit ratio and re-price the same usage at no-cache rates; confirm the 74% modeled figure against your own traffic; verify whether Claude Code ever issues 1h-TTL writes (open question, affects the 29% write line).

### 06. Context hygiene: /clear, steered /compact, auto-compact

- **Pitch:** clear between tasks, steer compaction, auto-compact at ~83% capacity (~167k) reserving ~33k buffer.
- **Layer:** turn-structure / context window.
- **Mechanism:** /clear resets ("Stale context wastes tokens on every subsequent message" — code.claude.com/docs/en/costs, accessed 2026-06-12); /compact takes focus instructions; auto-compact summarizes.
- **Expected savings:** unquantified by Anthropic. Independent JSONL trace: typical sessions = 76% useful work / 7% compaction summaries / 17% reserved headroom (dev.to slima4 trace, accessed 2026-06-12). On the modeled day, earlier /clear directly shrinks the 1.17M cache-read line (32% of dollars) per call.
- **Evidence tier:** T1 (official docs) + T3 (trace with thresholds).
- **Quality risk:** QUALITY-TRADE — summaries lose detail; degradation shows as the agent re-asking answered questions post-compaction. Falsify by quizzing post-compaction state against pre-compaction facts.
- **Availability:** CLAUDE-CODE-TODAY.
- **Effort to adopt:** minutes (habits + compact instructions in CLAUDE.md).
- **Composability:** compaction rewrites the prefix — a hidden cache-write spike (anti-synergy with 05); /clear is cheaper than /compact when continuity is not needed; cavekit's durable spec (03) reduces what compaction must preserve.
- **Validation protocol:** capture usage.cache_creation_input_tokens around 10 compaction events; quantify the write spike; compare task success on sessions using /clear-per-task vs run-long-and-compact.

### 07. CLAUDE.md slimming + path-scoped rules + skills offload

- **Pitch:** always-on instructions are perpetual rent; official guidance "Aim to keep CLAUDE.md under 200 lines" (code.claude.com/docs/en/costs, accessed 2026-06-12); skills load on demand (30–100 tok each at startup).
- **Layer:** input (per-call fixed prefix).
- **Mechanism:** memory file rides every call's prefix; path-scoped rules load per subtree (this repo already does this via per-directory AGENTS.md); skills carry name+description only until invoked.
- **Expected savings:** honest arithmetic is small in dollars: this repo's root AGENTS.md = 2,744 tok (local, 2026-06-12) -> 2,744 x 19 calls x 6 sessions x $1/MTok cache-read = **$0.31/day rent, ~1.4% of the modeled day**; an 80% trim saves ~$0.25/day (~1.1%) plus window headroom. Firecrawl benchmark: 3,847 -> 312 tok = 91.9% on the file itself, "no quality regression"; 41% overhead cut from path-scoping (firecrawl.dev blog, accessed 2026-06-12).
- **Evidence tier:** T1 (official guidance) + T3 (Firecrawl, vendor-interested) + local file measurement.
- **Quality risk:** RISKY if overdone — instructions exist to prevent expensive wrong behavior; one bad PR from a dropped rule erases months of rent savings. Degradation shows as rule violations; falsify by replaying a rule-sensitive task set against the slimmed file.
- **Availability:** CLAUDE-CODE-TODAY.
- **Effort to adopt:** hours (editorial); caveman-compress automates lossily at claimed ~46%.
- **Composability:** multiplies with caching (smaller prefix = smaller perpetual rent); synergy with skills/path-scoping; feeds record 21's denominator.
- **Validation protocol:** ct.py the file before/after; run the 10 most rule-sensitive recent tasks against both versions; zero new rule violations allowed; log $-rent delta.

### 08. Model tiering — with the tokenizer trap

- **Pitch:** Sonnet for most work, Haiku subagents, Fable/Opus for hard reasoning; prices verified live 2026-06-12: Fable 5 $10/$50, Opus 4.8 $5/$25, Sonnet 4.6 $3/$15, Haiku 4.5 $1/$5 per MTok. Sticker ratios LIE across the 4.7+ tokenizer boundary.
- **Layer:** infra / price-per-token and token count.
- **Mechanism:** /model switching, plan-on-big/build-on-small, subagent model pinning. Pricing page note: "Opus 4.7 and later use a new tokenizer... may use up to 35% more tokens for the same fixed text" (platform.claude.com/docs/en/about-claude/pricing, accessed 2026-06-12).
- **Expected savings:** independent trace: same session ~$5.50 on Sonnet vs ~$10+ on Opus (dev.to slima4, accessed 2026-06-12). **Corrected cross-boundary math:** Fable produces +15% to +45% more tokens than Sonnet for identical text (local band, 2026-06-12: +39.7% code / +44.7% prose today; +15%/+38% phase-0), so the effective fixed-text input-cost ratio Fable:Sonnet is 3.33 x 1.15–1.45 = **~3.8x–4.8x, not the 3.33x sticker** (ESTIMATE). Note: the sweep-agent draft of this file stated "effectively ~2.4–2.9x" — that is the same arithmetic *inverted* (division instead of multiplication) and is corrected here: downgrades save MORE than sticker, upgrades cost more.
- **Evidence tier:** T1 (prices + official tokenizer note) + local measurement; both fresh local samples *exceed* the official 35% ceiling — flagged.
- **Quality risk:** QUALITY-TRADE — capability cliff on hard tasks; degradation shows as failed/looping attempts on the small model (which then cost more in retries). Falsify with $-per-solved-task, not $/MTok.
- **Availability:** CLAUDE-CODE-TODAY.
- **Effort to adopt:** minutes.
- **Composability:** model switch invalidates the prompt cache (anti-synergy with 05) and changes the tokenizer basis mid-ledger; routers (17) that rank by $/MTok mis-price cross-boundary moves by up to ~1.45x.
- **Validation protocol:** fixed 10-task set run on Fable 5 / Opus 4.8 / Sonnet 4.6; record solved-or-not + total $ from JSONL; normalize by re-counting one canonical text on each tokenizer (ct.py); report $/solved-task.

### 09. Thinking/effort control (/effort, MAX_THINKING_TOKENS)

- **Pitch:** thinking bills as output ($50/MTok on Fable 5) and is the *largest* output component; effort is the only shipped lever attacking it.
- **Layer:** output (thinking).
- **Mechanism:** effort low/medium/high shapes reasoning depth; MAX_THINKING_TOKENS=8000 for fixed-budget models; adaptive-reasoning models ignore nonzero budgets; **Fable 5 cannot disable thinking** (code.claude.com/docs/en/costs, accessed 2026-06-12); default budgets "can be tens of thousands of tokens per request".
- **Expected savings:** vendor: "Set to a medium effort level, Opus 4.5 matches Sonnet 4.5's best score on SWE-bench Verified, but uses 76% fewer output tokens"; highest effort: +4.3 points at 48% fewer (anthropic.com/news/claude-opus-4-5, accessed 2026-06-12; model-version-specific, no Fable 5 curve published). Local ceiling: thinking = 54.8% of output tokens and **20% of modeled-day dollars ($4.40)**; halving thinking ~= $2.20/day (ESTIMATE).
- **Evidence tier:** T1 (vendor numbers) + local decomposition (n=1, 2026-06-12).
- **Quality risk:** QUALITY-TRADE — extended thinking "significantly improves performance on complex planning" (docs); degradation shows as shallow plans/missed edge cases on hard tasks. Falsify per-task-type: easy tasks at low effort should show zero quality delta.
- **Availability:** CLAUDE-CODE-TODAY (/effort, env var) / SDK (effort parameter).
- **Effort to adopt:** minutes.
- **Composability:** orthogonal to all input-side levers; the ONLY shipped tool touching the bucket caveman (01) explicitly skips. White-space: per-task-type effort routing (see Gaps).
- **Validation protocol:** 20 tasks stratified easy/hard at high vs medium effort; output tokens from JSONL (thinking = output minus count_tokens of visible blocks); require no hard-task regression; report $/day delta.

### 10. MCP tool deferral / Tool Search Tool

- **Pitch:** the proven negative-cost optimization, now a Claude Code DEFAULT: schemas stay out of context (names only) until needed.
- **Layer:** input (tool schemas).
- **Mechanism:** API Tool Search Tool lets the model query a tool registry; Claude Code defers MCP definitions by default ("only tool names enter context until Claude uses a specific tool" — code.claude.com/docs/en/costs, accessed 2026-06-12). This research session itself runs on ToolSearch.
- **Expected savings:** vendor: "an 85% reduction in token usage" (context preserved 191,300 vs 122,800 of 200k), MCP-eval accuracy **49% -> 74%** (Opus 4) and 79.5% -> 88.1% (Opus 4.5); five-server example ~55K schema tokens; internal pre-optimization fleets hit 134K (anthropic.com/engineering/advanced-tool-use, accessed 2026-06-12). Local: 11 schemas = 1,420 tok loaded vs ~60 deferred = **96%** (local measurement, 2026-06-12). Modeled day: an undeferred 20k fleet would cost 20k x 19 x 6 x $1/MTok = **$2.28/day (~10%) in read rent alone** (ESTIMATE).
- **Evidence tier:** T1 + local reproduction (direction and magnitude consistent).
- **Quality risk:** NEGATIVE-COST — vendor evals show accuracy *rises* (less irrelevant-tool confusion). Residual failure mode: the model fails to find a deferred tool it needed; falsify by task suites requiring rare tools.
- **Availability:** CLAUDE-CODE-TODAY (default) / SDK.
- **Effort to adopt:** zero; audit with /context and /mcp.
- **Composability:** makes big MCP fleets viable; complements CLI-over-MCP (20); absorbs the schema cost of cavemem (02) and fff (04).
- **Validation protocol:** /context audit with fleets loaded vs deferred; fixed task set exercising 3 rarely-used tools; require 100% tool-discovery success and report prefix-token delta.

---

## Proven tactics (vendor or peer-reviewed numbers)

### 11. Programmatic tool calling / code-execution orchestration

- **Pitch:** model writes code that calls tools in a sandbox; intermediate results never enter context — 37% average cut with accuracy gains.
- **Layer:** turn-structure / tool-result tokens.
- **Mechanism:** only final results return to the model; community "Code Mode" collapses 12-turn MCP workflows into 4 (thenewstack.io, accessed 2026-06-12).
- **Expected savings:** "Average usage dropped from 43,588 to 27,297 tokens, a 37% reduction on complex research tasks"; accuracy up (25.6 -> 28.5 internal retrieval; 46.5 -> 51.2 GIA) (anthropic.com/engineering/advanced-tool-use, accessed 2026-06-12). On the modeled day, tool results ride the 29%+32% cache lines; a 37% cut on tool-heavy turns plausibly reaches low-double-digit % of day (ESTIMATE, workload-dependent).
- **Evidence tier:** T1 (vendor evals).
- **Quality risk:** NEGATIVE-COST per vendor numbers; failure mode is sandbox bugs silently eating results. Falsify by checksumming sandbox outputs against direct calls.
- **Availability:** SDK (API feature); Claude Code analog = Bash piping, hooks, subagent offload — not the API feature itself (not in the costs-page levers as of 2026-06-12).
- **Effort to adopt:** days (sandbox + tool re-plumbing).
- **Composability:** stacks with Tool Search (10); same philosophy as preprocessing (20).
- **Validation protocol:** port 5 multi-tool workflows to code-execution; compare total tokens and end-result equality; require bit-identical final artifacts.

### 12. Context editing + memory tool (API beta)

- **Pitch:** server-side pruning of stale tool results + external memory: 84% token cut on a 100-turn eval while *improving* performance 39%.
- **Layer:** cache / context window (API-level).
- **Mechanism:** auto-clears stale tool calls near the limit; memory tool persists state outside the window.
- **Expected savings:** "reducing token consumption by 84%" on a 100-turn web-search eval; +39% (memory+editing) / +29% (editing alone) performance (claude.com/blog/context-management, accessed 2026-06-12). Maps onto the 32% read line of the modeled day for SDK agents.
- **Evidence tier:** T1 (vendor evals; search-task domain, NOT code-editing).
- **Quality risk:** NEGATIVE-COST on agentic-search per vendor; unproven on code workloads where an evicted tool result may be needed 40 turns later. Falsify on SWE-bench-style tasks before trusting.
- **Availability:** SDK (beta header); NOT-USER-ACCESSIBLE as a tunable inside Claude Code (which ships compaction instead).
- **Effort to adopt:** hours (SDK flag + config).
- **Composability:** edits invalidate cached prefix sections — same cache-vs-pruning tension as compaction (06); the user-side analog gap is in Gaps item 4.
- **Validation protocol:** replicate the vendor eval shape on 10 long code sessions; measure tokens + task success vs no-editing control.

### 13. Batch API (50%) + agent-team cost discipline

- **Pitch:** two price-structure facts: batch halves everything non-interactive; agent teams multiply everything ~7x.
- **Layer:** infra / price multiplier and session multiplicity.
- **Mechanism:** async batch = "50% discount on both input and output tokens", stacks with caching (platform.claude.com/docs/en/about-claude/pricing, accessed 2026-06-12). Teams: "approximately 7x more tokens than standard sessions when teammates run in plan mode" (code.claude.com/docs/en/costs, accessed 2026-06-12).
- **Expected savings:** moving offline-able work (CI review, nightly triage, evals) to batch: $22 -> $11 on the modeled day if all sessions were batchable (upper bound; realistic = the offline fraction). Batch+Haiku stack ~= 5% of Fable-interactive unit cost (ESTIMATE: 0.5 x 0.1 input ratio). Team discipline is loss-avoidance: one env flag (CLAUDE_CODE_EXPERIMENTAL_AGENT_TEAMS=1) swings cost more than every compression plugin in this scan combined.
- **Evidence tier:** T1 (both verbatim from live Anthropic pages).
- **Quality risk:** NEUTRAL (batch trades latency only); teams: degradation is economic. Falsify batch-quality by diffing batch vs interactive outputs on identical jobs.
- **Availability:** SDK (batch) / CLAUDE-CODE-TODAY (teams flag).
- **Effort to adopt:** days (re-architect offline jobs); zero (don't over-spawn teams).
- **Composability:** batch x caching x Haiku is the deepest legitimate price stack; teams anti-compose with everything.
- **Validation protocol:** run the nightly PR-review job both ways for a week; require equal review-finding recall; report $ delta.

### 14. Serialization-format engineering: TOON and JSON minification [LOCALLY VERIFIED]

- **Pitch:** TOON claims 30–60% fewer tokens than JSON for tabular prompt data; locally a third of the headline is just minification.
- **Layer:** input (structured data in prompts / tool results).
- **Mechanism:** schema header + CSV-like rows replace repeated keys/braces.
- **Expected savings:** LOCAL TABLE ABOVE (2026-06-12, Fable 5): TOON -61.4% vs pretty, **-41.2% vs minified**; minification alone -34.3% (tabular) / -28.7% (nested). Official: 39.9% fewer tokens at equal-or-better retrieval accuracy (76.4% vs 75.0%) (toonformat.dev/guide/benchmarks, accessed 2026-06-12). Reality check on nested/irregular payloads: 1.8–20% (dev.to tejas_page, accessed 2026-06-12). Affects only the fraction of your prompt that is structured data.
- **Evidence tier:** T3 + local reproduction (two independent same-day local runs agree within ~1 point).
- **Quality risk:** NEUTRAL on uniform data (accuracy equal-or-better in official benchmark); RISKY for deep nesting (format breaks down, less training exposure). Falsify with retrieval-QA over your own TOON-ized payloads.
- **Availability:** GATEWAY-OR-SELF-HOST for your own MCP servers/hooks; NOT adoptable for Claude Code's native tool results.
- **Effort to adopt:** hours (own tools only). Minification is minutes and risk-free.
- **Composability:** natural for MCP servers returning tabular data; orthogonal to caching.
- **Validation protocol:** ct.py your three largest real tool-result payloads in pretty/minified/TOON; then a 20-question retrieval quiz over each encoding; adopt only where accuracy is flat.

---

## Significant industry use, weaker or absent numbers

### 15. claude-mem — compressed cross-session memory (thedotmack/claude-mem)

- **Pitch:** adoption king of the memory niche: **81,976 stars** (gh api, 2026-06-12) — bigger than caveman; "~10x token savings by filtering before fetching details".
- **Layer:** input (memory injection).
- **Mechanism:** 5 lifecycle hooks + Bun worker (:37777) over SQLite + Chroma; tiered retrieval: search ~50–100 tok/result, get_observations ~500–1,000 (README, fetched 2026-06-12).
- **Expected savings:** the ~10x is a retrieval-path claim, not whole-session; no independent net accounting (injection + compression-call cost vs re-exploration saved) exists — same hole as cavemem (02).
- **Evidence tier:** T3 (massive adoption, self-reported number).
- **Quality risk:** NEUTRAL-to-RISKY — stale/wrong memories mislead sessions; degradation shows as acting on outdated project facts. Falsify by auditing 50 injected memories for currency.
- **Availability:** CLAUDE-CODE-TODAY (also Gemini CLI, OpenCode).
- **Effort to adopt:** minutes-to-hours (persistent local service).
- **Composability:** competitor to cavemem; SessionStart injection enlarges the cached prefix (interacts with the 29% write line).
- **Validation protocol:** identical to cavemem's week A/B; additionally meter the worker's own SessionEnd compression calls.

### 16. aider repo map — graph-ranked code context

- **Pitch:** the original "right context, not more context": whole-repo awareness for ~1k tokens.
- **Layer:** input (codebase context selection).
- **Mechanism:** "a concise map of your whole git repository" with key signatures, ranked on a dependency graph; `--map-tokens` "defaults to 1k" (aider.chat/docs/repomap.html, accessed 2026-06-12).
- **Expected savings:** never published as a single number; influence is the evidence — Claude Code's Explore agent and code-intelligence plugins are descendants.
- **Evidence tier:** T1 for mechanism/budget; T4 for savings magnitude.
- **Quality risk:** NEGATIVE-COST by design intent (fewer wrong-file reads); degradation = map points at stale symbols. Falsify via tokens-per-solved-task with map on/off.
- **Availability:** GATEWAY-OR-SELF-HOST (aider itself); pattern portable as a Claude Code skill.
- **Effort to adopt:** zero in aider; days to port well.
- **Composability:** alternative to fff (04); pairs with grep retrieval.
- **Validation protocol:** aider benchmark harness, `--map-tokens 1024` vs `0`, fixed task set; report tokens/solved.

### 17. LiteLLM gateway — spend tracking, caching, routing

- **Pitch:** de facto self-host gateway, name-checked in Anthropic's own costs docs for enterprise spend metrics.
- **Layer:** infra / gateway.
- **Mechanism:** per-key spend tracking; exact + semantic response caching in 7 backends (similarity_threshold 0.8); routing/fallbacks (docs.litellm.ai/docs/proxy/caching, accessed 2026-06-12).
- **Expected savings:** **none quantified in its own docs**; semantic-cache hits on coding-agent traffic (unique diffs, file contents) are near-worst-case — expect ~0 (ESTIMATE; see kill list).
- **Evidence tier:** T1 product, T4 for any coding-agent savings number.
- **Quality risk:** RISKY (semantic cache can return stale code for similar-but-different questions); NEUTRAL for spend tracking. Pass-through danger: a misconfigured gateway that drops cache_control destroys the 74% caching floor (05).
- **Availability:** GATEWAY-OR-SELF-HOST.
- **Effort to adopt:** days.
- **Composability:** must preserve cache_control headers; tokenizer-blind routing mis-prices cross-boundary moves (08).
- **Validation protocol:** replay a week of real traffic through the proxy; measure semantic-cache hit rate (expect ~0; kill the pattern for this domain with data) and verify cache-read ratios unchanged end-to-end.

### 18. Usage observability: ccusage, /usage, JSONL tracing

- **Pitch:** saves nothing itself; underwrites every other claim. ccusage = 16,080 stars (gh api, 2026-06-12).
- **Layer:** infra / observability.
- **Mechanism:** ccusage parses local transcript JSONL into daily/session/model costs; built-in /usage attributes usage to skills, subagents, plugins, individual MCP servers (code.claude.com/docs/en/costs, accessed 2026-06-12).
- **Expected savings:** none direct. Reveals the official denominators: "~$13/dev/active day" average, $150–250/month, 90% of users under $30/day, background burn <$0.04/session (same page).
- **Evidence tier:** T1 (shipped tools on real local data).
- **Quality risk:** NEUTRAL. Failure mode: misattribution; falsify by reconciling ccusage totals against API console billing.
- **Availability:** CLAUDE-CODE-TODAY.
- **Effort to adopt:** minutes.
- **Composability:** foundation for every validation protocol in this file; see 31-validation-harness.md.
- **Validation protocol:** reconcile one week of ccusage vs console invoice; require <5% divergence before trusting any A/B below.

### 19. LLMLingua / LongLLMLingua / LLMLingua-2 — research-grade prompt compression

- **Pitch:** the peer-reviewed ceiling — "up to 20x compression with minimal performance loss" — absent from every shipped coding agent.
- **Layer:** input (prompt body).
- **Mechanism:** small-LM perplexity pruning (EMNLP'23); question-aware coarse-to-fine for RAG (ACL'24: +21.4% RAG performance at 1/4 tokens); distilled token classifier 3–6x faster (ACL'24 Findings) (github.com/microsoft/LLMLingua README, accessed 2026-06-12; 6,284 stars, last push 2026-04-08 — maintenance slowing).
- **Expected savings:** headline 20x is NL-domain. **Killer arithmetic on the modeled day:** compression that breaks prefix caching must clear (70.4+5.1+0.44)/x + 8.14 = 22 -> **x ≈ 5.5 (82% compression) just to break even** against plain Claude Code caching; 4x compression *loses money* ($27.1 > $22) (ESTIMATE, local profile + live multipliers).
- **Evidence tier:** T2 (peer-reviewed) for NL benchmarks; T4 for code-agent use (no code-domain validation found, searched 2026-06-12).
- **Quality risk:** RISKY for code — identifiers and syntax are exactly what perplexity pruning drops; degradation shows as hallucinated symbol names. Falsify on SWE-bench-style tasks before any use.
- **Availability:** GATEWAY-OR-SELF-HOST (Python lib; LangChain/LlamaIndex integrations).
- **Effort to adopt:** project (compression model in the hot path; GPU + latency).
- **Composability:** CONFLICTS with prompt caching (05) — the defining anti-synergy of the input-compression category.
- **Validation protocol:** the break-even formula above on your own JSONL profile first; only if x > 5.5 is plausible, run 20 code tasks compressed-vs-not and diff symbol-level accuracy.

### 20. Preprocessing pipeline: hooks filtering, markdown-not-HTML, CLI-over-MCP

- **Pitch:** shrink content *before* the model sees it — the highest-leverage user-controlled knob for tool-result bloat.
- **Layer:** input (tool results).
- **Mechanism:** PreToolUse hooks rewrite commands (official example: pipe test output through grep — "tens of thousands of tokens to hundreds", code.claude.com/docs/en/costs, accessed 2026-06-12); fetch web as markdown not raw HTML; `max_content_tokens` caps runaway pages (10 kB page ~2,500 tok, 500 kB PDF ~125,000 tok — pricing docs); prefer gh/aws/gcloud/sentry-cli over equivalent MCP servers ("still more context-efficient" — official, verbatim).
- **Expected savings:** LOCAL: 630-line synthetic test log 10,108 -> 590 tok = **-94.2% with all 3 failures fully preserved** (local measurement, 2026-06-12, table above). Cited: 94% per web page (38,381 -> 2,788), "80-99% compression on build and test logs" (firecrawl.dev blog, accessed 2026-06-12). Tool results ride the 29%+32% cache lines of the modeled day.
- **Evidence tier:** T1 (official pattern) + local reproduction + T3 (Firecrawl).
- **Quality risk:** RISKY if filters hide the error the model needed (over-aggressive grep); NEGATIVE-COST when failure signal is preserved — the local run shows both are achievable simultaneously. Falsify by injecting known novel error shapes and checking the filter passes them.
- **Availability:** CLAUDE-CODE-TODAY (hooks, settings.json). This repo's filtered test runners (TESTING.md) are the same idea already shipped.
- **Effort to adopt:** hours.
- **Composability:** stacks with everything; the Claude Code-native sibling of programmatic tool calling (11).
- **Validation protocol:** for each filter, a red-team set of 10 unusual failure modes; require 10/10 surfaced post-filter; track tokens saved per tool call from JSONL.

### 21. Fixed-overhead accounting: the ~14.3k baseline + MCP fleet bloat

- **Pitch:** diagnosis, not cure: before you type, every call carries ~14,328 tokens of system prompt + tools + memory; careless MCP fleets added 15–20k more (pre-deferral).
- **Layer:** input (fixed per-call).
- **Mechanism:** measured by independent JSONL traces ("consistently using approximately 14,328 tokens" — dev.to slima4, accessed 2026-06-12); per-version prompt catalog exists (Piebald-AI/claude-code-system-prompts: Explore 575 tok, Plan mode 715, statusline 2,433); GitHub issue #52979 reports 20–30k on trivial prompts; community 15–20k multi-server MCP overhead, one deployment 143k/200k; Anthropic internal 134K pre-optimization; Anthropic cut its own tool-use system prompt 675 -> 290 tok (-57%) between Opus 4.7 and 4.8 (pricing page, accessed 2026-06-12).
- **Expected savings:** N/A directly. Rent arithmetic: 14,328 x 19 x 6 x $1/MTok = **$1.63/day (~7.4% of the modeled day) in cache-read rent alone** (ESTIMATE). Cures are records 07 and 10.
- **Evidence tier:** T3 (converging independent traces) + T1 (Anthropic's own figures). All cached after first write — overhead seeds the 32% read line rather than billing at full price.
- **Quality risk:** NEUTRAL (a fact, not an action). Mis-reading it as "uncached cost" overstates the problem 10x; the kill list guards this.
- **Availability:** CLAUDE-CODE-TODAY (audit via /context).
- **Effort to adopt:** minutes to audit.
- **Composability:** the denominator for every percentage in this file; see 02-baseline-audit.md for this machine's own numbers.
- **Validation protocol:** run /context on this setup; decompose into system prompt / tools / memory / skills; reconcile with first-call cache_creation_input_tokens in JSONL; re-audit after applying 07 + 10.

---

## Claims to kill (folklore ledger)

| # | Claim in the wild | Verdict and corrected number |
|---|---|---|
| K1 | "caveman cuts ~75% of your tokens" | Three-layer overstatement. The 75% headline is the *pooled* benchmark ratio (1214 -> 294 = 75.8%); the same table's per-task mean is 65% (range 22–87%) (README, fetched 2026-06-12); local replication: 58.5% (ultra) on tokens. And it targets visible prose only = 17% of heavy-session dollars; whole-bill effect ~4–6% (ESTIMATE; mayhemcode ~4%). The README itself calls cost savings "a bonus". |
| K2 | "wenyan/Classical-Chinese mode saves ~80%" | Character-token confusion: 80.9% char cut = 56.6% token cut (CJK ~1.47 chars/tok vs 3.35 English; local measurement, 2026-06-12). Billing is tokens. wenyan-ultra reaches 74.5% tokens at maximum lossiness. |
| K3 | "TOON cuts 60% of data tokens" | Locally 61.4% only vs *pretty* JSON on uniform tabular; vs minified it is 41.2%, and minification alone is 34.3% (free, riskless). On nested/irregular real payloads independent measurement found 1.8–20%. |
| K4 | "Prompt caching saves 90%" | 90% is the best single launch-table row (book chat); multi-turn row is -53%; writes bill 1.25–2x. In the local heavy session caching was already maximal (92.83% reads) and cache reads+writes still cost **61% of dollars**. Modeled-day actual: -74% vs no caching (ESTIMATE). Not an available saving for an already-cached session. |
| K5 | "Model price ratios = cost ratios" | Official: Opus 4.7+ tokenizer "may use up to 35% more tokens for the same fixed text". Local band +15% to +45% — today's fresh samples (+39.7% code, +44.7% prose) *exceed* the official ceiling. Effective Fable:Sonnet fixed-text input ratio ~3.8–4.8x, not 3.33x. Also kills the sweep-draft's inverted "~2.4–2.9x" (arithmetic error, corrected in record 08). Routers do not tokenizer-normalize. |
| K6 | "Perplexity dropped MCP citing 72% context waste" | Single secondary source (nevo.systems); no primary statement found 2026-06-12. Do not repeat. |
| K7 | "fff saves X% tokens" | No token percentage exists in the README text (fetched 2026-06-12) — latency numbers and a chart image only. Cite the mechanism, not a number. |
| K8 | "Semantic caching will cut your coding-agent bill" | LiteLLM's own docs claim no percentage; coding prompts are near-worst-case for similarity caches and hits risk stale code. No published coding-agent hit rate exists (searched 2026-06-12). |
| K9 | "The 14.3k overhead costs 14.3k input tokens per call" | It is cached: marginal cost is 0.1x after the first write (~$1.63/day rent on the modeled profile, not ~$16). Directionally real, economically 10x smaller than naive reading. |
| K10 | Phase-0 internal note: "caveman's 75% is character-level folklore" | Partially corrected: the repo's benchmark does claim Claude-API *token* methodology with reproduction scripts, and the 75% = pooled-vs-65% = per-task-mean distinction explains the headline. The honest kill is K1's "75 ≠ 65 ≠ 58.5, smallest bucket", not "they counted characters". |

## White space — what nobody ships (as of 2026-06-12)

1. **Thinking-token optimization** — 54.8% of output tokens, 20% of dollars, not disableable on Fable 5; only blunt levers exist (/effort, MAX_THINKING_TOKENS). No per-task-type effort routing, no thinking-waste detection. Caveman's README concedes the bucket explicitly.
2. **Cache-economics tooling** — reads+writes = 61% of heavy-session dollars; no shipped tool optimizes breakpoint placement, predicts 5-min-vs-1-h TTL value, schedules compaction to minimize write spikes, or even reports write amplification (ccusage shows totals, not causes). See 13-caching-exploitation.md.
3. **Tokenizer-aware routing** — official "up to 35%" (locally up to +45%) means every $/MTok router mis-prices cross-boundary switches; nobody normalizes.
4. **Mid-session selective context pruning in Claude Code** — the API has context editing (84% cut, vendor-proven); Claude Code end users get only whole-conversation compaction.
5. **Net-accounted memory** — claude-mem (82k stars) and cavemem inject context and run compression calls; neither publishes injection-cost-vs-exploration-saved accounting.
6. **Format-aware tool-result serialization** — the locally-verified 41.2%/34.3% wins are not integrated into any agent's tool-result path or popular MCP server.
7. **Independent replication infrastructure** — every vendor self-reports; a public ct.py-style harness re-measuring plugin claims against live tokenizers does not exist (this dossier's method is itself novel prior art; see 31-validation-harness.md).
8. **Output brevity with quality gates** — caveman trades readability blindly; nothing compresses output only when a verifier confirms zero information loss.

## Surprising findings

- Anthropic's own pricing page corroborates — and the fresh local samples *exceed* — the tokenizer premium: official "up to 35%", measured +39.7%/+44.7% today (2026-06-12).
- Token-cutting can be quality-improving with vendor proof: Tool Search -85% tokens, accuracy 49 -> 74; programmatic calling -37% with accuracy up. Negative-cost optimization is shipped, not theoretical.
- Anthropic admitted internal tool definitions hit 134K tokens pre-optimization, and cut its own tool-use system prompt 57% (675 -> 290) between model versions.
- The memory niche out-adopted the compression niche: claude-mem 81,976 stars vs caveman 71,891 (caveman 5x'd from ~14k in ~10 weeks since early April 2026 — fastest-growing plugin category of 2026).
- The optimizer taxes itself: caveman's always-on skill listing costs 940 tokens of prefix per session (local measurement, 2026-06-12), ~0.5% of the modeled day — and just *minifying JSON* (-34.3%, free) goes unmarketed while TOON gets the headlines.
- One experimental flag (agent teams, ~7x) moves cost more than every compression plugin in this scan combined.

## Verification ledger

| Number | Source / method (access date 2026-06-12) |
|---|---|
| Stars: caveman 71,891; cavemem 521; cavekit 1,014; fff 8,351; claude-mem 81,976; ccusage 16,080; LLMLingua 6,284 | `gh api repos/<repo> --jq .stargazers_count`, run locally 2026-06-12 |
| caveman "~75% of output tokens" headline; benchmark mean 65% (range 22–87%); pooled 1214 -> 294 (=75.8%); "Answer concisely." baseline; "thinking/reasoning tokens untouched"; "cost savings a bonus"; compress ~46%; cavecrew ~60%; caveman-code "~2x fewer than Codex"; arXiv 2604.00025 "+26 points" | curl of raw.githubusercontent.com/JuliusBrussee/caveman/main/README.md, lines 27/136/151/154/168/126/128/88/170, fetched 2026-06-12 (arXiv paper itself not independently verified) |
| caveman local: ultra 58.5%; wenyan-full 56.6% tok / 80.9% chars; wenyan-ultra 74.5%; CJK ~1.47 chars/tok | local measurement, 2026-06-12 (phase-0) |
| caveman ~4% of session total (independent) | mayhemcode.com/2026/04/caveman-claude-code-how-to-save-tokens.html, accessed 2026-06-12 |
| cavemem "~75% fewer prose tokens"; architecture | raw cavemem README + getcaveman.dev, accessed 2026-06-12 |
| fff "3-9 SECONDS per ripgrep spawn and sub-10 ms"; ~26 MB/14k files; "fewer grep roundtrips"; no token % in text | curl of raw fff README lines 22/72/673/714, fetched 2026-06-12 |
| Cache multipliers 0.1x/1.25x/2x; 512-tok min; free refresh; launch table -90/-86/-53; break-even note | platform.claude.com/docs/en/build-with-claude/prompt-caching + claude.com/blog/prompt-caching + pricing page, accessed 2026-06-12 |
| Local prompt mix 0.44/6.73/92.83%; dollar split 32/29/20/17/2; thinking 54.8% of output; modeled day ~$22 | local measurement, 2026-06-12 (phase-0); profile detailed in 01-economics-and-measurement.md |
| Modeled no-cache day ~$84, -74%; LLMLingua break-even x≈5.5; CLAUDE.md rent $0.31/day; 14.3k rent $1.63/day; 20k fleet rent $2.28/day; caveman ceiling ~10%/realistic 4–6%; thinking ceiling $4.40/day; caveman listing rent ~0.5%/day | ESTIMATE — arithmetic shown in records 05/19/07/21/10/01/09/01 on the modeled profile |
| Auto-compact ~83% (~167k), ~33k buffer; 76/7/17 split; 14,328 fixed overhead; $5.50 vs $10+ session | dev.to/slima4/where-do-your-claude-code-tokens-actually-go-we-traced-every-single-one-423e, accessed 2026-06-12 |
| "under 200 lines"; skills 30–100 tok; deferral default; CLI-over-MCP; /effort + MAX_THINKING_TOKENS=8000; Fable 5 thinking not disableable; ~7x teams; $13/day; $150–250/mo; <$30/day for 90%; <$0.04 background; LiteLLM name-check; grep-hook example | code.claude.com/docs/en/costs, accessed 2026-06-12 |
| Firecrawl 3,847 -> 312 (91.9%); 41% path-scoping; 38,381 -> 2,788 (94%); 80–99% logs | firecrawl.dev/blog/claude-code-token-efficiency, accessed 2026-06-12 (vendor-interested) |
| Prices Fable 5 $10/$50, Opus 4.8 $5/$25, Sonnet 4.6 $3/$15, Haiku 4.5 $1/$5; "up to 35% more tokens" note; 675 -> 290 tool-use prompt; batch 50%; max_content_tokens scale | platform.claude.com/docs/en/about-claude/pricing, accessed 2026-06-12 |
| Local tokenizer premium +42.7%/+39.2% raw (+44.7%/+39.7% net of 7-tok wrapper); Sonnet=Haiku counts; phase-0 +15%/+38% | /tmp/ct.py vs claude-fable-5 / claude-sonnet-4-6 / claude-haiku-4-5 on fixed prose (730 chars) + `crates/jackin-build-meta/src/lib.rs` head (1,800 B), run 2026-06-12 |
| Effort: 76% fewer output tokens at medium; +4.3 pts at -48% | anthropic.com/news/claude-opus-4-5, accessed 2026-06-12 (Opus 4.5-era, no Fable 5 curve) |
| Tool Search: 85% cut; 191,300 vs 122,800; 49 -> 74; 79.5 -> 88.1; ~55K five-server; 134K internal; programmatic 43,588 -> 27,297 (37%); examples 72 -> 90 | anthropic.com/engineering/advanced-tool-use, accessed 2026-06-12 |
| Local tool deferral 1,420 -> ~60 tok (96%) | local measurement, 2026-06-12 (phase-0) |
| Context editing 84% / +39% / +29% | claude.com/blog/context-management, accessed 2026-06-12 |
| Code Mode 12 -> 4 turns; 15–20k MCP overhead; 143k/200k deployment | thenewstack.io/how-to-reduce-mcp-token-bloat/, accessed 2026-06-12 |
| 20–30k trivial-prompt reports; prompt catalog (575/715/2,433 tok) | github.com/anthropics/claude-code/issues/52979; github.com/Piebald-AI/claude-code-system-prompts, accessed 2026-06-12 |
| aider map: 1k default, graph ranking | aider.chat/docs/repomap.html, accessed 2026-06-12 |
| LiteLLM: 7 cache backends, similarity_threshold 0.8, no savings % | docs.litellm.ai/docs/proxy/caching, accessed 2026-06-12 |
| LLMLingua: 20x headline; +21.4% RAG at 1/4 tokens; 3–6x speedup; venues EMNLP'23/ACL'24; last push 2026-04-08 | github.com/microsoft/LLMLingua README, accessed 2026-06-12 |
| TOON official: 39.9% fewer at 76.4% vs 75.0%; 60.7%/36.8% uniform | toonformat.dev/guide/benchmarks, accessed 2026-06-12 |
| TOON reality check 1.8–20% nested | dev.to/tejas_page/toon-vs-json-when-60-token-savings-becomes-18-a-reality-check-3e60, accessed 2026-06-12 |
| Local format table: 484/318/187 tabular (-34.3%/-61.4%/-41.2%); 272/194 nested (-28.7%); sweep same-day run -60.6/-40.8/-33.6 | /tmp/ct.py claude-fable-5 on generated fixtures (/tmp/tok/gen.py), run 2026-06-12 |
| Local log filter: 630 lines/10,108 tok -> 35 lines/590 tok (-94.2%), 3/3 failures preserved | synthetic cargo-test log + `grep -B1 -A6 -E "FAILED|panicked|error\[" \| head -100`, ct.py, run 2026-06-12 |
| Local: AGENTS.md 2,744 tok (phase-0 2,738); caveman skill listing 940 tok; wrapper constant 7 tok | ct.py claude-fable-5 on repo AGENTS.md, reconstructed skill listing, 1-char probe; run 2026-06-12 |
