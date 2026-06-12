# Token-Optimization Research Dossier

> **STATUS: IN PROGRESS** — this README is finalized last. Files land incrementally; each is
> committed and pushed as it completes. Research conducted: **2026-06-12**.

Definitive research dossier on extreme token optimization for coding-agent usage — techniques
beyond common practice, at zero quality loss. Specification: [`token-optimization-research.md`](../token-optimization-research.md)
at the repository root.

## How to read

- Start with `00-executive-summary.md` (the stack, the math, the verdict on 10x).
- `01`–`03` are foundations: economics + measurement, the local baseline audit, the market scan.
- `10`–`20` are the research areas, one file each, every technique in a fixed record schema.
- `30`–`32` are synthesis: composed stacks with dollar math, the validation harness, the roadmap.

## Tier list

_(populated at synthesis — Phase 4)_

## Headline numbers

_(populated at synthesis — Phase 4; every number here must survive the Phase-3 adversarial pass)_

## Assumptions (judgment calls made during the run)

1. **Run date:** 2026-06-12. All "current" pricing/feature claims verified against live docs on
   this date unless marked otherwise.
2. The operator's brief mandates use of the `count_tokens` API. No `ANTHROPIC_API_KEY` is present
   in this environment; the run uses the Claude Code OAuth credential already present on this
   machine to call the free `count_tokens` endpoint (no billable usage). If that fails, token
   counts fall back to clearly-labeled offline estimates.
3. Only one local session transcript exists at run start (this run's own). The thinking-vs-visible
   decomposition therefore measures this session and its subagent transcripts as they accumulate,
   supplemented by cited external measurements; the limitation is stated where the number is used.
4. "Deliverables exactly as specified" is interpreted as: the 19 files in §10 and nothing else in
   the folder; measurement scripts are embedded in the reports as reproducible snippets rather
   than shipped as separate files.

## Self-audit against Definition of Done

_(appended at the end of the run)_
