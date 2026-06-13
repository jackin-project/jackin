# 51 - Code-intelligence tools: codedb, fff, and CodeGraff

Research conducted: **2026-06-13**. This is a targeted addendum requested after the
main dossier: compare `codedb`, `fff`, the CodeGraff codedb article, and the
CodeGraff product site; judge whether they improve AI-agent work and whether they
save tokens.

## TL;DR

- **Yes, this tool class can save tokens, but only when it replaces blind
  grep/read loops with precise path, symbol, caller, dependency, or function-scope
  retrieval.** It does not magically shrink model reasoning, output, or cache rent.
- **codedb is the strongest candidate for token savings** because it returns
  structured code context: symbols, outlines, callers, dependency graph, compact
  reads, and `codedb_context` that combines several lookup steps. Its published
  token numbers are vendor-side and response-size based, not yet independently
  reproduced here.
- **fff is primarily a resident file/content search accelerator.** It likely saves
  tokens by reducing dead-end searches and wrong-file reads, but the upstream text
  publishes latency and qualitative token claims, not a numeric token percentage.
- **CodeGraff is a larger agent/toolchain bet, not just a retrieval tool.** The
  useful token-saving ideas are scope reads, symbol-safe patching, batching, and
  codedb-backed retrieval. The commercial/agent stack should be evaluated as a
  role-level opt-in, not silently installed on the host.
- **For jackin': pilot inside a role container, measure locally, and keep host
  effects explicit.** The existing the-architect roadmap already proposes `fff`;
  codedb deserves an adjacent A/B pilot if MCP schema overhead is deferred or
  bounded.

## What is being compared

| URL | Entity | Category | Honest scope |
|---|---|---|---|
| <https://github.com/justrach/codedb> | `codedb` | Open source code-intelligence server and MCP toolset | Local structural index, MCP/HTTP/CLI, remote public-repo queries |
| <https://github.com/dmtrKovalenko/fff> | `fff` / `fff-mcp` | Open source resident file and content search toolkit | Fast path search and grep with frecency/git metadata |
| <https://codegraff.com/blog/codedb-code-intelligence> | CodeGraff article on codedb | Evidence and design explainer | Vendor write-up with latency, byte/token, and workflow examples |
| <https://codegraff.com/> | CodeGraff / Graff / Pro tools | Terminal coding agent plus optional local file-tool suite and model gateway | Bigger workflow replacement; not a drop-in search primitive |

CodeGraff and codedb are related: CodeGraff says Graff is powered by `codedb`,
while codedb's repository links back to the CodeGraff article. Treat codedb as the
retrieval engine and CodeGraff as the broader agent/product surface around it.

## Comparison matrix

| Axis | codedb | fff | CodeGraff |
|---|---|---|---|
| Primary job | Code intelligence for agents: tree, outline, symbols, callers, deps, search, compact reads, snapshots, remote repo queries | Fast resident file-name and content search | Full terminal coding agent plus optional Pro local file primitives (`muonry`, `zigrep`, `zigread`, `zigpatch`) |
| Agent interface | 21 MCP tools, HTTP server, CLI, `npx` launcher | MCP tools (`ffgrep`, `fffind`, `fff-multi-grep`), SDKs, Neovim plugin | `graff` CLI/TUI/SDK/gateway, MCP management, Pro MCP tools |
| Data model | In-memory structural indexes: outlines, word index, trigram search, dependency graph, content cache, change log | Warm file tree/content index, frecency DB, git status annotations, typo/fuzzy matching | Agent loop plus codedb retrieval and local daemon file tools |
| Best use | "Where is this symbol?", "what calls this?", "what depends on this?", "give context for this task", "read only this range/compact view" | "Find likely files/matches fast; avoid repeated `rg` process startup and bad rankings" | Replace whole-file reads and line-fragile edits with function/symbol-scope reads and patches |
| Token-savings evidence | Explicit vendor numbers: e.g. README claims structured search results around tens of tokens vs large raw grep dumps; benchmarks page claims 4x fewer bytes in a full edit workflow | Text says fewer grep roundtrips and token-efficient; no numeric token percentage in README text. Latency and memory claims are specific | Site claims "40x leaner", aggregate "tokens saved", structural read 47 vs 2,103 tokens, and 9x byte reduction in article workflow |
| Evidence tier for tokens | **T3/T4**: specific public numbers, vendor-interested, not locally replicated here | **T4**: plausible, but numeric token effect unpublished in text | **T4**: product counters and examples, vendor-interested, broad workflow confounders |
| Quality risk | Stale/wrong project root, unbounded tree/snapshot dumps, over-trusting fuzzy or structural approximation, telemetry/cache host writes | Fuzzy false positives, overusing it for semantic questions better answered by LSP/ast-grep/codedb | Larger adoption surface, paid/proprietary pieces, possible agent displacement, gateway/provider changes |
| jackin' fit | Good candidate for a role-scoped MCP/CLI pilot, with host-write guardrails | Already mapped in the-architect roadmap as resident file-search MCP | Evaluate only as an explicit alternative agent role/toolchain, not as default core behavior |

## Token economics

The relevant equation is:

```text
net_saved =
  avoided failed searches
+ avoided wrong-file reads
+ avoided whole-file reads
+ avoided follow-up calls
- MCP schema/tool-description rent
- oversized indexed-tool outputs
- index/setup prompts and stale-index recovery
```

These tools are valuable when the first four terms dominate. They lose when the
agent simply adds their output on top of normal `rg`, `cat`, and file reads.

### codedb verdict

**Likely token-positive when used correctly; not yet proven locally.**

codedb's upstream claims are stronger than fff's on token economics. The README
describes a context engine for agents and lists MCP tools for tree, outline,
symbol, search, word lookup, callers, dependency graph, compact reads, changes,
status, snapshots, local projects, remote public repos, and a `codedb_context`
composer. It also publishes token-efficiency examples where structured results are
orders of magnitude smaller than raw grep output, plus a benchmark page claiming a
full edit workflow drops from roughly 50 KB to roughly 12 KB.

The practical mechanism is sound:

- `codedb_word` or `codedb_symbol` can replace broad grep for exact identifiers.
- `codedb_callers` can replace "grep symbol, read candidates, infer scope".
- `codedb_deps` can replace ad hoc import greps.
- `codedb_outline` can orient on a file before reading the whole file.
- `codedb_read` with line ranges or compact mode can avoid full-file dumps.
- `codedb_context` can collapse 3-5 serial location calls into one task-shaped
  response when the query is broad enough.

The caveat is output discipline. `codedb_tree`, `codedb_snapshot`, and remote tree
queries can be large. The CodeGraff hooks lab explicitly shows a guard for
unbounded `codedb_remote action=tree` calls. That is the right instinct: code
intelligence saves tokens only if calls return bounded, task-shaped context.

**Local adoption verdict:** Add codedb to the validation harness as an A/B arm
against native `rg`/Read and against fff. Do not count vendor token multipliers as
banked savings until measured on jackin' tasks.

### fff verdict

**High-confidence latency win; token savings are plausible but unquantified.**

fff is narrower than codedb. It is a resident Rust search core exposed through MCP,
SDKs, and Neovim. It keeps a file/content index warm, adds git/frecency metadata,
supports typo/fuzzy fallback, and exposes agent-facing tools such as `ffgrep` and
`fffind`. The README's strongest concrete data is latency and memory: on very large
repos, repeated warm queries are positioned as sub-10 ms instead of repeated
multi-second ripgrep process spawns, with about 26 MB resident memory on a 14k-file
repo and roughly 360 bytes per indexed file for the content index.

Token savings can happen in three ways:

- fewer zero-result grep calls because fuzzy fallback finds likely variants;
- fewer wrong-file reads because frecency/git status rank active files higher;
- smaller result sets because weak-match detection prevents fuzzy noise from
  flooding the context.

But the upstream text does not provide a numeric token percentage in normal text.
The prior dossier therefore correctly kept fff at **T1 for latency** and **T4 for
tokens**. That verdict still stands after the 2026-06-13 re-check.

**Local adoption verdict:** Keep the existing the-architect fff pilot, but require
equal target-file hit rate and measured tool-result-token reduction before treating
fff as a token-saving lever.

### CodeGraff verdict

**Potentially token-positive as a workflow replacement; too broad to treat as a
single retrieval optimization.**

CodeGraff's product page says Graff is a terminal coding agent powered by codedb.
Its Pro surface adds local file tools that return enclosing functions, structural
outlines, symbol reads, symbol-safe patches, and batch operations through a
persistent daemon. Those ideas attack a real waste pattern in coding agents:
reading whole files, grepping without scope, editing by fragile line numbers, then
re-reading entire files to verify.

The promising primitives are:

- "scope mode": return the enclosing function/block, not a naked match line;
- structural read: outline first, then symbol/function body instead of whole file;
- patch by symbol: reduce line drift and verification reads;
- batch operations: amortize per-call overhead and keep intermediate plumbing out
  of the chat transcript;
- codedb as a retrieval layer before action.

The claims are vendor-side and confounded by the full agent loop. The product site
mentions aggregate tokens saved, per-op token counts, and last-30-day counters, but
those are not independently auditable from the page. The codedb article's 9x byte
workflow example is more concrete but still a selected vendor example.

**Local adoption verdict:** Do not fold CodeGraff into jackin' core. If useful,
model it as an explicit role/toolchain experiment. Extract the technique pattern
first: function-scope read, bounded search, diff-returning edits, and batch tools.

## Recommended setup for AI agents

### General rules for all three

1. **Teach the agent the retrieval contract.** The instruction should be explicit:
   use indexed tools to locate paths/symbols first, then read the smallest exact
   span needed. Do not dump full trees, full snapshots, or whole files unless the
   task requires them.
2. **Bound every broad query.** Cap result counts, prefer prefixes, and ask for
   path:line plus symbol names over raw line dumps.
3. **Use the right tool class.** Plain text and literal strings can stay with `rg`.
   File discovery can use fff. Symbol/caller/dependency questions should use
   codedb, rust-analyzer, or ast-grep where available.
4. **Keep MCP schema overhead under control.** If the client supports tool search
   or schema deferral, use it. If not, consider CLI/HTTP wrappers for rarely-used
   tools and expose only the hot retrieval calls over MCP.
5. **Smoke-test freshness at session start.** Run a status/index-ready check before
   relying on results, especially after large code generation or checkout changes.
6. **Make host effects explicit.** The upstream installers may write `~/.codedb`,
   `~/.claude.json`, `~/.codex/config.toml`, or other client config. In jackin',
   install and register inside the role container unless the operator explicitly
   opts into host changes.

### codedb setup

Recommended agent instruction:

```markdown
Use codedb for code navigation before broad text search:
- Start unfamiliar tasks with `codedb_context` when the question spans multiple files.
- Use `codedb_symbol` or `codedb_word` for exact identifiers.
- Use `codedb_callers` before refactoring or changing public behavior.
- Use `codedb_deps` for import/dependency impact.
- Use `codedb_outline` before reading a large file.
- Prefer bounded `codedb_read` ranges or compact reads; avoid full snapshots and
  unbounded trees unless explicitly needed.
- Prefer the client's native edit tool; `codedb_edit` is fallback only.
```

Setup shape:

- Install inside the agent environment, not the host, for jackin' roles.
- Disable telemetry if the environment requires it: `CODEDB_NO_TELEMETRY=1`.
- Register MCP at user scope inside the container, for example:
  `claude mcp add codedb -s user -- /usr/local/bin/codedb mcp` or
  `codex mcp add codedb -- /usr/local/bin/codedb mcp`.
- Verify with `codedb --version` and `codedb status` or `codedb_status`.
- Ensure root resolution points at the mounted workspace, not `~` or a system
  directory. Pass the `project` argument when a client does not supply MCP roots.
- Add a guard hook for remote tree calls: require `expand=false`, a `prefix`, or a
  `limit` before allowing large `codedb_remote action=tree` responses.

### fff setup

Recommended agent instruction:

```markdown
For file-name search and grep in the current git-indexed project, use fff before
falling back to repeated shell `rg` calls. Use `fffind` for paths and `ffgrep` for
content. Keep result sets small, refine weak/fuzzy matches, then read exact file
spans with the normal file-read tool.
```

Setup shape:

- Install `fff-mcp` inside the role image or setup hook.
- Register at user scope in the container:
  `claude mcp add -s user fff -- fff-mcp`.
- Keep the existing the-architect pilot requirement: fff must measurably beat
  native ripgrep on this repo or be dropped.
- Do not use fff as a semantic engine. It finds likely files and lines; it does
  not replace rust-analyzer, ast-grep, or codedb's caller/dependency graph.

### CodeGraff setup

Recommended agent instruction, if evaluating the full toolchain:

```markdown
Use CodeGraff/Graff local file tools to avoid whole-file reads:
- Search in scope mode when changing a function or method.
- Outline before reading a large file.
- Read by symbol/function when possible.
- Patch by symbol when line drift is likely.
- Batch related reads/searches/diffs when the daemon supports it.
```

Setup shape:

- Treat CodeGraff as an explicit agent/toolchain role, not as a transparent
  dependency of jackin' core.
- Install only in the container or on an operator-approved host path.
- Separate the free/open `graff`/codedb path from paid Pro tooling and the
  CodeGraff model gateway. Measure each independently.
- If using the gateway, keep model-routing/cache effects separate from local
  file-tool savings; otherwise the token analysis becomes impossible to attribute.

## jackin' adoption recommendation

The existing roadmap page
`docs/content/docs/reference/roadmap/architect-code-intelligence-tooling.mdx`
already has the right shape for fff: role-scoped, opt-in, no jackin-core change,
registered in the container's user scope, and accepted only if it beats ripgrep
net of MCP overhead.

Extend that experiment rather than generalizing immediately:

1. Keep the planned fff A/B.
2. Add a codedb A/B arm if the role can carry another MCP server without always-on
   schema cost.
3. Do **not** add CodeGraff Pro by default. If evaluated, make it a separate
   "agent stack replacement" arm.
4. Use the same task suite and metrics for all arms.
5. Promote only primitives that show equal-or-better target-file hit rate and
   lower total tokens per solved task.

## Validation harness

Run 20-30 fixed repository-navigation tasks in four arms:

| Arm | Tools allowed |
|---|---|
| Native | Shell `rg`, `find`, normal file reads/edits |
| fff | fff MCP plus normal file reads/edits |
| codedb | codedb MCP plus normal file reads/edits |
| CodeGraff | Graff/Pro local file tools, if explicitly installed |

Task categories:

- exact symbol definition lookup;
- fuzzy remembered phrase or path;
- caller/reference discovery;
- reverse dependency/impact analysis;
- large-file function edit;
- wrong-query/zero-result recovery;
- recently modified file discovery;
- public dependency or remote repo lookup, if testing codedb remote.

Metrics:

- target-file hit rate;
- top-1 target hit rate;
- turns to first correct file;
- tool calls;
- tool-result tokens;
- total input/output/cache tokens from the session ledger;
- wall-clock latency;
- edit correctness and test result;
- stale-index or wrong-root incidents;
- MCP schema tokens loaded at turn start, if the client exposes them.

Acceptance rule:

```text
Accept a tool for token optimization only if:
  target-file hit rate >= native
  edit/test success >= native
  total tokens per solved task <= native by at least 20-30%
  no unbounded output path remains
```

Latency alone is not enough. A tool can be much faster and still neutral or
negative on tokens if the agent reads the same files afterward.

## Failure modes and guardrails

- **Wrong project root:** status looks ready, but it indexed `~` or another repo.
  Guard: status check plus explicit project root.
- **Unbounded tree/snapshot:** the index dumps more than native tools would.
  Guard: hooks or instructions requiring `limit`, `prefix`, compact mode, or line
  ranges.
- **Schema rent:** 20+ MCP tools are loaded into every turn without deferral.
  Guard: tool-search/schema-deferral, CLI fallback, or narrower MCP exposure.
- **Fuzzy confidence error:** search returns plausible but wrong files.
  Guard: target-file benchmark, weak-match refinement, require exact verification
  before editing.
- **Stale index after edits:** agent trusts old symbol/caller data.
  Guard: status/changes checks and re-run query after large rewrites.
- **Host mutation:** installers auto-register client configs.
  Guard: container-only install or explicit operator opt-in surfaced in launch
  summary.

## Bottom line

For AI-agent work, these tools are best understood as **retrieval precision and
observation-shaping tools**, not compression tools. They save tokens when they
help the agent look at fewer, better spans of code.

- **codedb:** best token-saving candidate; pilot it against native search with a
  strict bounded-output policy.
- **fff:** keep as the low-risk resident search pilot; expect latency wins first,
  token wins only if wrong-file/dead-end calls drop.
- **CodeGraff:** valuable ideas, larger adoption blast radius; evaluate as an
  explicit role or workflow replacement, not as a hidden dependency.

## Source ledger

Accessed 2026-06-13 unless noted.

- `justrach/codedb` README: <https://github.com/justrach/codedb>
- codedb MCP setup: <https://github.com/justrach/codedb/blob/main/docs/mcp.md>
- codedb skill/context guidance: <https://github.com/justrach/codedb/blob/main/docs/skills.md>
- codedb benchmarks: <https://github.com/justrach/codedb/blob/main/docs/benchmarks.md>
- codedb hooks lab: <https://github.com/justrach/codedb/blob/main/docs/hooks-labs.md>
- `dmtrKovalenko/fff` README: <https://github.com/dmtrKovalenko/fff>
- CodeGraff codedb article: <https://codegraff.com/blog/codedb-code-intelligence>
- CodeGraff product site: <https://codegraff.com/>
- CodeGraff docs overview: <https://codegraff.com/docs>
- `justrach/codegraff` README: <https://github.com/justrach/codegraff>
- CodeGraff changelog: <https://codegraff.com/changelog>
- Existing jackin' fff pilot roadmap:
  `docs/content/docs/reference/roadmap/architect-code-intelligence-tooling.mdx`
