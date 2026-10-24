# Independent review of product, provider and benchmark findings

Reviewer: real delegated workstream `/root/headroom_lemoncrow`. Product author: `/root/rtk_leanctx`. Date: 2026-10-24. Source pins: RTK `4ef8730cf3a0dea11c8d64a9b90bc0616fe52100`; LeanCTX `3afe2bbffe0834ec58517ffd493e5a51c5c1ca57`. This review reads immutable source and recorded outputs; it does not claim to have rerun the author's entire test suite.

## Challenges and dispositions

| Question challenged | Independent evidence | Disposition |
|---|---|---|
| Does RTK still have no reusable library? | `engine/src/lib.rs`, engine manifest and root `src/lib.rs` | Old conclusion is false. Typed engine API exists; root application command modules remain private. Do not confuse them. |
| Can the typed engine be treated as Apache CLI extraction? | Exact engine LICENSE and root feature manifest | No. Engine is BUSL-1.1 with a specific additional grant; Apache OSS executable is a distinct distribution. Forking does not remove its terms. |
| Does `pipe --filter git-diff` use the corrected structural parser? | `pipe_cmd.rs` resolves that family through Git compaction; `git_cmd.rs` differs from `diff_cmd.rs` | No. The product page correctly separates the two paths. |
| Do diff logs support the reported omissions? | Captured producer patch, per-surface stdout/hashes and malformed inputs in `rtk-diff-boundary-results.json` | Yes: Git filter omits no-EOL marker, emits selected partial hunk without a completeness warning, and returns empty for non-diff input. Structural fallback can add a newline, so it is not byte-identical either. |
| Does stdin filtering provide durable recovery? | `pipe_cmd.rs::run` reads bounded UTF-8, calls filter and prints; no archive write in that path | No. Caller capture/recovery is required. Recall elsewhere is conditional and bounded, not raw-rerun-only or universal. |
| Is Lean facade a minimal no-ML/caller-runtime library? | `lean-ctx-embed/Cargo.toml` explicitly enables embeddings/http-server/secure-update; engine builder owns a Tokio runtime | No. `default-features=false` alone does not mean no ML infrastructure or caller-owned runtime. |
| Does Lean facade truly deny unexpected tools? | `READ_TOOLS`, `WRITE_TOOLS`, `EXEC_TOOLS`, `Engine::call` in pinned `engine.rs` | Yes, the facade has an explicit allowlist and write/exec opt-ins. The broader MCP server's hidden/steering settings are a different surface. Do not falsely claim the facade has no authorization. |
| Does `.data_dir(...)` create isolated multiple engines? | Static `Once` in `configure_process_env`; all four path variables set once; default fixed temp subdirectory | No. First engine wins process globals. Instance caches/root checks do not remove those globals. Source fact, not executed two-engine race. |
| Are unchanged stubs always invalid after restart or always fixed for Claude? | Cold same-A, other-B, simulated marker and Claude-scope outputs | Neither blanket is supported. Same custom identity/no-marker can reuse stub; different identity gets content; Claude simulation withheld stubs; no real host context was run. |
| Is returned auto content after compaction a raw full file? | `leanctx-cold-results.json` and MCP wire records show cognitive selection/annotations | No. Product wording correctly distinguishes current content from exact/full content. Explicit full/raw is a separate path. |
| Did the incomplete facade check prove an upstream failure? | Recorded interrupt/shared disk pressure and separate offline missing-rmcp consumer attempt | No. They are incomplete/environment-limited attempts, not passing builds or product defects. |

## Additional finding accepted by product author

LeanCTX's safe `EngineBuilder::build` invokes `configure_process_env`, which calls `unsafe std::env::set_var`. Its safety comment establishes only that the engine has not spawned **its own** runtime threads yet. A general embedding host can already have other threads reading process environment through C/native code or DNS. [Official Rust 1.99.1 documentation](https://doc.rust-lang.org/std/env/fn.set_var.html) identifies the broader non-Windows threading requirement.

This is an unmet caller-thread safety premise visible in source, **not a reproduced race or observed undefined behavior**. The product author added that distinction to the LeanCTX page. Request injected instance configuration or initialize child process environment before startup; the existing static `Once` is not a general thread-safety proof.

## Additional executed constraint proof

Three minimal offline resolver graphs reproduce `links = "sqlite3"` conflicts for Jackin rusqlite 0.40.2 paired with LeanCTX's 0.39, RTK application's 0.34, or sem's optional disk-cache 0.32 constraint. Exact diagnostics are in `linkage-constraint-results.json`. This is stronger than guessing from version numbers, but narrower than building a whole facade/application in Jackin. Sem default disk-cache-off and RTK engine-only do not inherit that conclusion automatically.

## Result

The current RTK and LeanCTX product recommendations are supported within their stated component scope. No whole-task winner or automatic activation follows. The additional threading caveat was incorporated; source/global-state, byte fidelity, missing recovery and incomplete-build limitations remain explicit. Current external outcome studies belong in the independent benchmark ledger and must be assessed for exact treatment identity rather than ignored or projected into every backend mode.

## Provider and representation follow-up review

Reviewer: `/root/headroom_lemoncrow`; chapter author: `/root/caveman_ponytail`. Reviewed October 24 after the architecture handoff. Scope: complete current techniques 04, 09, 20 and 23, the author's provider/representation registers, and the relevant captured primary documentation under `research/providers/sources/`. This was an independent source-and-artifact review. No paid model request, native host task, schema execution, image processing or OCR evaluation ran.

| Claim challenged | Primary capture and source | Review result |
|---|---|---|
| Native Concise, style switching and child inheritance | `claude-output-styles.txt`, `claude-code-cache.txt`; [output styles](https://code.claude.com/docs/en/output-styles), [prompt caching](https://code.claude.com/docs/en/prompt-caching) | Version thresholds 2.1.237/2.1.251/2.1.269 match the documentation. Next-message application and append-based cache preservation are documented; edited custom style files still require restart. Full-prompt `keep-coding-instructions` does not restore instructions absent from short prompt mode. Forks inherit the style; ordinary subagents use their own prompt. These are documented contracts, not observed quality equivalence. |
| Structured output guarantees and exceptions | `claude-structured.txt`, `claude-code-headless.txt`, `openai-structured.txt`; [Claude schemas](https://platform.claude.com/docs/en/build-with-claude/structured-outputs), [headless mode](https://code.claude.com/docs/en/headless), [OpenAI schemas](https://developers.openai.com/api/docs/guides/structured-outputs) | Unsupported Claude recursion/numeric constraints, SDK transformation versus validation, refusal/incomplete outputs, string enum/const casing, 20/24/16 combined limits and grammar cache behavior are accurately qualified. OpenAI recursion is separately documented. Host JSON envelopes are not conflated with constrained decoding. |
| Current prefill and generation-budget behavior | `claude-prompting-best-practices.txt`, `claude-prefill.txt`; [migration guidance](https://platform.claude.com/docs/en/build-with-claude/prompt-engineering/claude-prompting-best-practices#migrating-away-from-prefilled-responses) | Current unsupported last-assistant prefill is broader than a thinking-only restriction. A generation cap is not accepted-task cost or successful completion. No hidden-reasoning saving is inferred from visible output. |
| Claude image sizing, bounds and coordinate space | `claude-vision.txt`, `claude-vision-coordinates.txt`; [vision](https://platform.claude.com/docs/en/build-with-claude/vision), [coordinates](https://platform.claude.com/docs/en/build-with-claude/vision-coordinates) | Both 28-pixel patch budget and edge limit apply; the A4 and screenshot examples match the reference. Ordinary input downscaling differs from tool-result rejection. Coordinates refer to resized content, not padded dimensions; PDF server rasterization limits reliable source-page mapping. |
| OpenAI detail and image-token estimates | `openai-vision.txt`; [image guide](https://developers.openai.com/api/docs/guides/images-vision) | GPT-6 Astra high/original/auto, 65,535-pixel bound, 2,500 resizing budget, separate 30,000 rejection bound and 1.2 multiplier are supported. The chapter correctly avoids extrapolating to unspecified GPT-6 siblings or treating all low-detail modes as fixed 85 tokens. The local probe reproduces named examples, not a general executed API preprocessor. |
| Gemini media budgets and PDF components | `gemini-media-resolution.txt`, `claude-pdf.txt`; [media resolution](https://ai.google.dev/gemini-api/docs/media-resolution), [PDF guide](https://platform.claude.com/docs/en/build-with-claude/pdf-support) | 280/560/1,120/2,240 image budgets remain approximate/model-specific. Video frame selection is separate from per-frame resolution; PDF native text is additional. Claude's 1,500–3,000 estimate concerns text, and page/context/platform limits are preserved. |
| Actual host boundary and fixed-model scope | Pinned Jackin source audit in this directory; the provider workstream's `host-contracts.json` and official/source captures | Twelve registrations versus six launch branches, current approval-bypass flags, observation versus replacement, Codex schema/trust uncertainty and OpenCode discovery/API drift remain explicit. No installed-host qualification or production optimizer is claimed. Model/effort routing remains outside the main comparison. |

**Disposition:** no blocking factual or source-versus-execution error found within this scope. Two nonblocking completeness suggestions were accepted and applied by the author, then rechecked here: chapter 20 now explicitly retains per-axis coordinate scaling, resized-bound clamping and HiDPI logical-screen conversion; chapter 23 records the Gemini CLI distribution banner for unpaid/Google One users without treating it as a new runtime or replacement capability. Primary captures and access hashes remain in the provider/representation registers rather than being duplicated as downloaded pages here.

The paper and synthetic-payload discussions retain their narrow scope: CAVEWOMAN's versioned model/data grids are distinct, constrained style and decoder budgets are a combined treatment, NLI is agreement with a model reference, proxy token counts are not provider invoices, and a successful synthetic edit does not establish model anchor selection or host acceptance.

## Independent LemonCrow baseline join and acceptance-gate review

Reviewer: `/root/headroom_lemoncrow`; benchmark author: `/root/rtk_leanctx`. The executable [review probe](probes/review_lemon_join.py) was independently written and does not import the author's extractor or reaggregation implementation. It checks both raw CSV hashes against the pinned `8aaee7ffa5b6f5f7e4a4a9a520af89ed527c7ec1` source register, then validates key uniqueness and identical key sets before joining. `turns.csv` here contains **trial-level aggregated turn fields**, not individual provider-request receipts.

The exact argv, Python version, timestamps, input hashes and output are in [lemon-join-review-results.json](lemon-join-review-results.json). Execution completed with exit 0. All 445 trial keys are unique in both files; task names and input/output totals match per trial; ordinary input is nonnegative throughout. All independently recomputed benchmark fields agree with the author's current aggregate:

| Quantity | Independent result |
|---|---:|
| Trials / tasks | 445 / 89 |
| Prompt tokens | 174,804,998 |
| Cache reads | 161,931,526 |
| Cache writes | 12,803,162 |
| Ordinary input: prompt minus reads minus writes | 70,310 |
| Table input minus table cache | 12,873,472 |
| Rows where table cache equals reads | 445 |
| Rows where table cache equals reads plus writes | 1 |
| Missing reported task costs | 34 |
| Sum of available task costs, excluding unknowns | $286.92 |

The baseline label correction is supported. The larger “fresh” figure includes cache writes. This check does not validate a candidate denominator, reconstruct invoices, rerun paid trials, grade tasks or turn the known-cost sum into a complete spend total. A replacement reduction ratio remains unestablished.

The canonical tools 07 and techniques 16 gates retain the proposed status, fixed main model/effort, task-cluster accounting, one-sided acceptance bound, hard critical-failure exclusion, all-attempt economics, latency/resource requirements and pilot-versus-confirmation distinction. No blocking inconsistency was found. Nonblocking extractor hardening was suggested and accepted by its author: assert unique/equal trial keys before dictionary construction so changed inputs cannot silently overwrite or drop rows.
