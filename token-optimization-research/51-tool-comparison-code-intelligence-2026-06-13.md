# 51 — Tool Deep-Dive & Comparison: codedb · Codegraff · fff

**Research conducted: 2026-06-13.** Operator-requested deep dive on three code-search / code-intelligence
tools and whether they help AI coding agents and save tokens. Sources fetched live today (GitHub repos,
codegraff.com + its blog, crates.io/npm, one independent benchmark, one HN thread); every external claim
carries a URL + access date in the per-tool notes and the source ledger. The token mechanism is
**measured locally** with `tools/count_tokens.py` against this repo, not taken on faith. This file extends
the dossier's market scan (`03-prior-art-and-market-scan.md`) and its context-architecture findings
(`12-context-architecture.md`).

## TL;DR

- **All three productize the same dossier lever (file 12): don't dump whole files — serve structure,
  symbols, or targeted lines.** Measured locally on three real repo files (18,613 tokens if read whole):
  a signature **outline costs 91% fewer tokens**, a **targeted symbol-search result 98% fewer**. The lever
  is real and large. **T1 (locally reproduced).**
- **codedb (justrach, BSD-3, free) is the substantive one for agents:** a persistent Zig MCP server with
  21 tools and five in-memory indexes. Its genuine value over Claude Code's built-ins is **call-graph and
  dependency queries** (`codedb_callers`/`codedb_deps`) and a **one-shot context composer** — capabilities
  grep+read cannot express — plus a persistent O(1) index. Reproducible vendor benchmark: **~20% fewer
  tokens + ~70% less wall-time vs a compressor competitor**. **T1-vendor (author-run, not independent).**
- **Codegraff is the same author's paid product** (graff agent + four Pro tools + a model gateway,
  $99–349/yr) wrapped around codedb. Its token lever is identical to free codedb; you pay for
  symbol-safe patching, batch reads, and a key-less gateway — not for a bigger token saving.
- **fff (dmtrKovalenko, MIT, free) is a speed tool, not a token tool.** Its 20–50× ripgrep speedup is
  **independently corroborated (T3)**; its only token figure (**−17%**) is a single author tweet with no
  published method — **T4, do not cite as fact.** Worth wiring for fast grep-first navigation; any token
  win is indirect (fewer failed-search retries, paginated/windowed output) and must be measured locally.
- **The headline multipliers are marketing.** "1,628× fewer tokens" / "40× leaner" / "−17%" compare a
  structured result to *dumping a whole file or raw grep output* — a baseline a disciplined Claude Code
  agent (Read offset/limit, Grep, outline-on-read) **already avoids**. Net realistic saving over a
  careful agent is **moderate, not order-of-magnitude**; over a *naive* file-dumping agent it is large.

---

## 1. What each tool is

**codedb** — `https://github.com/justrach/codedb` (accessed 2026-06-13). "Code intelligence server for AI
agents. Zig core. MCP native. Zero dependencies." A single zero-dependency Zig binary that, on startup,
walks the repo and builds **five in-memory indexes — Outlines, inverted Word index, Trigram, Dependency
graph, Content cache** — then serves **21 MCP tools** over JSON-RPC/stdio. A 2-second polling watcher keeps
it fresh (single-file re-index <2 ms). Positioned as "a context engine, not an editor"; `codedb_edit` is an
explicit fallback. BSD-3-Clause, latest release v0.2.5825 (2026-06-12), ~1.3k stars, **alpha**, sole author
Rach Pradhan (`justrach`).

**Codegraff** — `https://codegraff.com/` (accessed 2026-06-13). The **same author's commercial product**:
the open-source `graff` terminal agent "powered by codedb," plus paid **Codegraff Pro** (four tools —
`muonry`, `zigrep`, `zigread`, `zigpatch`) and a model **Gateway** ("one login, six models, no keys").
Local-first (code never leaves the machine); optional cloud (`codedb_remote`, gateway). Pricing: Individual
$99/yr, Team $160/yr (2 seats), Team Plus $349/yr (5). So: **codedb is the free OSS engine; Codegraff is the
paid agent + tooling layer around it — one author, two products**, not a third party.

**fff** — `https://github.com/dmtrKovalenko/fff` (accessed 2026-06-13). "The fastest and the most accurate
file search toolkit for AI agents, Neovim, Rust, C, and NodeJS." Crucially **"a file search library, not a
CLI"**: a long-lived process keeps a resident index + file cache, so each query hits warm memory instead of
forking a new scan. Frecency-ranked, typo-resistant fuzzy filename search + SIMD/regex/fuzzy content grep
(aho-corasick multi-grep). Ships an **MCP server** (`ffgrep`, `fffind`, `fff-multi-grep`) plus Rust/C/Node/
Python/Go bindings and the original `fff.nvim`. MIT, ~8.4k stars, very active (v0.9.5-nightly, 2026-06-12).
Author Dmitriy Kovalenko (odiff, Cypress, Material-UI contributor). *(Disambiguation: not `dylanaraps/fff`,
the bash file manager.)* A sibling engine `scry` adds tree-sitter symbol indexing/callers-callees.

## 2. The shared mechanism — measured locally (the honest core)

Every one of these tools wins the same way: replace "read the whole file into context" with "ask for just
the structure or the matching lines." That lever is the dossier's `12-context-architecture.md` finding. To
size it on this codebase, `tools/count_tokens.py` measured three real Rust files against the real Anthropic
tokenizer:

| File | Lines | Read whole (tok) | Outline (tok) | One symbol-search (tok) | Outline cut | Search cut |
|---|---:|---:|---:|---:|---:|---:|
| `mount_info.rs` | 289 | 4,404 | 323 | 82 | 93% | 98% |
| `dialog_widgets.rs` | 415 | 5,598 | 189 | 89 | 97% | 98% |
| `update.rs` | 647 | 8,611 | 1,141 | 209 | 87% | 98% |
| **Total** | | **18,613** | **1,653** | **380** | **91%** | **98%** |

*Outline = signature lines (fn/struct/impl/…) with line numbers, i.e. what `codedb_outline` returns. Search
= up to six matching lines with line numbers for one identifier, i.e. what `codedb_search` / `ffgrep` return.
**T1 — locally reproduced.***

Two conclusions fall out of this table:

- **The lever is genuine and big — ~90%+ — when the alternative is reading the whole file.** This matches
  codedb's directional claim and the dossier's own repo-map/outline measurement (−85 to −92%).
- **It also explains why the vendor multipliers are best-case, not session savings.** "1,628× fewer tokens"
  (codedb README: ~20 tokens vs a ~32,564-token raw-`grep` dump for `allocator`) and "40× leaner"
  (codegraff homepage) and fff's "−17%" all measure *one query against a file-dump baseline*. They are real
  per-query ratios, not the end-to-end saving across a coding session — and the baseline they beat (dumping
  whole files / raw grep) is one a disciplined agent already avoids. The author's own multi-task benchmark
  is the honest number: **~20% fewer tokens** vs a competitor across real tasks, with codedb at
  *parity-to-slightly-more* tokens than SQLite FTS5 on one 8-task eval (it wins on quality and latency
  there, not tokens).

## 3. Side-by-side

| | **codedb** | **Codegraff (Pro)** | **fff** |
|---|---|---|---|
| Type | OSS MCP code-intelligence server | Paid agent + tools + gateway over codedb | OSS fast file-search library + MCP |
| Core mechanism | 5 in-memory indexes; 21 MCP tools; call-graph + deps | codedb + `zigread`/`zigpatch`/`zigrep`/`muonry` | resident frecency/typo index + SIMD grep |
| Agent interface | MCP (Claude Code, Codex, Cursor, Gemini CLI, Windsurf, Devin) | MCP + `graff` agent + model gateway | MCP (`ffgrep`/`fffind`/`fff-multi-grep`) + SDKs |
| Install | `curl -fsSL https://codedb.codegraff.com/install.sh \| bash` or `npx -y codedeebee mcp` | `curl -fsSL https://codegraff.com/install-graff.sh \| sh` | `curl -L https://dmtrkovalenko.dev/install-fff-mcp.sh \| bash` or `brew install dmtrKovalenko/fff/fff-mcp` |
| Token claim | ~20% vs lean-ctx **(T1-vendor)**; 1,628× **(T4 marketing)** | `zigread` "47 vs 2,103 tok" **(T4 example)** | −17% **(T4, one tweet)** |
| Speed claim | 5–200× FTS5; 538× vs ripgrep pre-indexed **(T1-vendor)** | inherits codedb | 20–50× ripgrep **(T3, independently 127–467×)** |
| Unique capability | **call-graph (`callers`/`deps`), one-shot `context`, persistent index** | symbol-safe patching, batch ops, key-less gateway | **fastest + typo-resistant first hit**, paginated output |
| License / maturity | BSD-3, alpha, ~1.3k★, daily releases | commercial, young | MIT, ~8.4k★, pre-1.0, daily releases |
| Cost | free | $99–349/yr | free |

## 4. Do they save tokens? Per-tool verdict

**codedb — yes, directionally; NEGATIVE-COST candidate.** It saves tokens *and* improves retrieval
(structured results pick the right file). Strongest defensible figure: **~20% fewer tokens + ~70% less
wall-time vs lean-ctx** on a reproducible agentic benchmark (`code-search-shootout`, facebook/react, Sonnet
4.6 sub-agents, 3×500 iters) — **T1 but author-run, not independently replicated.** Locally, its
outline/search outputs are 91–98% smaller than whole-file reads (T1). The catch: a competent Claude Code
agent already greps and reads partial files, so the *incremental* token saving over disciplined built-ins is
**moderate**, not the 1,628× headline. The bigger draw is the **capability** built-ins lack — `codedb_callers`/
`codedb_deps` (call/dependency graph) and `codedb_context` (keywords + symbol defs + ranked files + snippets
in one round-trip, "replaces 3–5 sequential calls"). Verdict: **worth piloting on large repos and
exploration/impact-analysis-heavy work; validate the token delta on your own codebase before believing any
multiplier.**

**Codegraff Pro — the token lever is the same free one.** `zigread`'s "47 tokens vs 2,103" is the outline
lever my §2 table already measures at ~90%+. You are paying for **symbol-safe patching (`zigpatch`, "no line
drift"), batch reads, and a key-less six-model gateway**, not a larger token saving than free codedb already
gives. Verdict: **evaluate on patch-safety + gateway convenience, not on tokens** — the token win is
available for free below it.

**fff — speed yes (verified), tokens unproven.** Its 20–50× ripgrep speedup is **T3** (author claim plus an
independent benchmark that measured an even larger 127–467× on content search). But the **−17% token** figure
is a single author tweet with **no published dataset or harness (T4)**; an independent agent-focused reviewer
called its token savings "theoretical… unverified by testing." The plausible token mechanism is *indirect* —
frecency/typo ranking returns the right file in one shot (fewer failed-search retries that each cost context),
and typed/paginated/`context`-windowed output avoids dumping raw grep blobs. Verdict: **wire it in for speed
and first-hit accuracy; treat token savings as unproven and measure locally.**

**Cross-cutting caveat — MCP schema overhead (a real setup cost).** Each server adds its tool schemas to the
agent. The dossier measured MCP schema cost at roughly 60–140 tokens *per tool* (`02-baseline-audit.md`:
tirith 7 tools ≈ 1,000 tok, shellfirm 4 ≈ 420 tok). codedb's **21 tools therefore imply ~2–3k tokens of
always-on schema** if the client loads them eagerly — which would eat into per-task savings on short
sessions. **Claude Code mitigates this**: it defers MCP tool schemas via ToolSearch (the dossier measured
~60 tokens of names vs ~1,420 tokens of full schemas), so codedb/fff schemas load on demand. **Setup
implication: prefer a client that defers MCP schemas, or the retrieval saving must clear the standing schema
cost.**

## 5. How to set them up for the most agent value

**codedb (the recommended pilot):**
1. Install + auto-register: `curl -fsSL https://codedb.codegraff.com/install.sh | bash` (or add
   `{ "codedb": { "command": ["npx","-y","codedeebee"], "args": ["mcp"] } }` to the client's MCP config).
2. Let it index once; queries then hit the warm in-memory index.
3. **Steer the agent in `AGENTS.md`/system prompt:** *prefer `codedb_context` for task-shaped retrieval;
   `codedb_outline` before reading a file; `codedb_callers`/`codedb_deps` for impact analysis; use the
   client's native Edit tool for changes (`codedb_edit` is a fallback).* The point is to replace the
   search→read→search loop with one composed query and to read outlines before whole files.
4. Keep editing native — codedb is for *finding and understanding*, not writing.

**fff (speed-first navigation):**
1. Install the MCP server: `curl -L https://dmtrkovalenko.dev/install-fff-mcp.sh | bash` (or
   `brew install dmtrKovalenko/fff/fff-mcp`).
2. **Steering line (author's own):** *"For any file search or grep in the current git-indexed directory, use
   fff tools."*
3. Query patterns that exploit the design: `fffind` with **partial/typo'd path fragments** (frecency + typo
   tolerance surface the right file in one call); `ffgrep` with `path`/`exclude` scoping and a `context`
   window + cursor **pagination** so the agent gets tight line-windows, not whole files.
4. If embedding the SDK, create the finder once per session and reuse it (the speed only holds for the
   resident process).

**Codegraff Pro:** only after free codedb proves out — add it if symbol-safe patching, batch reads, or the
key-less multi-model gateway are worth $99–349/yr for your workflow.

**General rule (from the dossier):** the token saving is realized **only against a file-dumping baseline**.
Pair any of these with the dossier's existing output-side and effort levers; better retrieval shrinks the
*input/cache-read* class, which `02-baseline-audit.md` shows is the high-volume / lower-dollar class — so the
dollar impact is real but secondary to the output + cache-write levers that dominate the bill.

## 6. Verdict and where they land on the tier list

- **codedb → Tier A** for an exploration-heavy or large-repo agent: NEGATIVE-COST (saves tokens *and*
  improves which file the agent opens), free, MCP-native, with call-graph queries Claude Code lacks. Caveat:
  alpha, vendor-measured; validate locally. Its real edge is **capability**, with token saving a moderate
  bonus over a disciplined agent.
- **fff → Tier A for speed, Tier B for tokens.** A genuinely fast, typo-resistant grep-first navigator with
  independently verified speed; token savings plausible but unproven. Low risk to add, measure the token
  effect yourself.
- **Codegraff Pro → conditional.** The token lever is free below it in codedb; pay only for patch-safety +
  gateway. Not a token purchase.

**Answering the operator's questions directly.** *Do they improve how we work with AI agents?* Yes —
codedb adds call-graph/dependency retrieval and one-shot context that genuinely reduce the search loop; fff
makes navigation fast and accurate. *Do they save tokens?* codedb yes (moderate, ~20% measured vs a
competitor; ~90%+ vs whole-file reads), fff indirectly and unproven, Codegraff no more than free codedb. The
order-of-magnitude marketing numbers do **not** survive an honest baseline. *How to set up for the most
results?* codedb as an MCP context engine with native editing retained, fff as the steered grep-first
navigator, both on a client that defers MCP schemas — and validate the token delta on your own repo before
trusting any vendor multiplier.

---

## Verification & source ledger (all accessed 2026-06-13)

| Claim | Basis |
|---|---|
| Outline −91% / search −98% vs whole-file read | `tools/count_tokens.py` on 3 real repo files, real Anthropic tokenizer (T1-local) |
| codedb: 21 MCP tools, 5 indexes, BSD-3, v0.2.5825, ~1.3k★, alpha | https://github.com/justrach/codedb + GitHub REST API |
| codedb ~20% fewer tokens / ~70% less wall-time vs lean-ctx | https://github.com/justrach/code-search-shootout (react corpus, Sonnet 4.6, 3×500) — T1 author-run |
| codedb "1,628× fewer" / "40× leaner" | https://codegraff.com/blog/codedb-code-intelligence + https://codegraff.com/ — T4 marketing (raw-grep baseline) |
| Codegraff product, Pro tools, pricing, gateway, same author | https://codegraff.com/ (Rach Pradhan / justrach) |
| fff: library-not-CLI, MCP (`ffgrep`/`fffind`/`fff-multi-grep`), MIT, ~8.4k★ | https://github.com/dmtrKovalenko/fff + crates.io |
| fff 20–50× ripgrep (independent 127–467×) | https://curtis-arch.github.io/ai-search-benchmarks/ — T3 |
| fff −17% tokens | https://x.com/neogoose_btw (single tweet, no methodology) — T4 |
| MCP schema overhead ~60–140 tok/tool; Claude Code defers via ToolSearch | dossier `02-baseline-audit.md`, `12-context-architecture.md` (T1-local) |

*Reproduce §2 with `python3 tools/count_tokens.py file <path> <path>` and a signature grep; the per-tool
facts are reproducible by fetching the cited URLs.*
