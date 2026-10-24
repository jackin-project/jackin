# Independent Caveman and Ponytail product review

Reviewer: the Headroom/LemonCrow and architecture workstream, independently reviewing product chapters authored by the Caveman/Ponytail workstream. Review date: 2026-10-24. [policy-review-source-register.json](policy-review-source-register.json) records exact source commits/hashes and reviewed chapter/evidence hashes.

## Scope and result

No blocking contradiction was found in the current policy/runtime conclusions. The review reopened the actual tagged policy, child-agent definitions, Node state/injection/map code and Go recovery/storage code, then compared the author's recorded fixture outputs and execution limitations. It did not rerun Go tests, launch an agent, invoke a provider, warm a cache or install either product.

The product tag `d262ab3882952a59312ff3537cd86d894523c5ab` (Caveman4.0.1), runtime tag `b4531dfc05a3ef0ba93b57e8bfa5f48564a4b4d6` (bin4.0.2) and Ponytail tag `16352ac229eb08a9598dcae48fe9f2b342567d86` (5.3.2) remain separate evidence identities. The npm CLI's older runtime pin must not inherit tests of the newer runtime automatically.

## Findings checked against source

| Claim reviewed | Independent conclusion and scope |
|---|---|
| Caveman's delivered policy is more than “be terse” | Confirmed. It preserves technical facts, literals, negation, language and protected persistent artifacts; general brevity rules do not override clarity or required detail. Full-plugin child/model/workflow rules are a separate treatment from the core skill. |
| Caveman child behavior can change the experimental control | Confirmed. Investigator/reviewer definitions select Haiku; builder scope limits and the no-Bash statement are delivered policy. A prose statement about tool availability is not independent proof that a host enforces a tool allowlist. The chapter does not claim such enforcement. |
| Ponytail changes implementation policy | Confirmed. It asks for complete affected callers/tests/fixtures/configuration/exports, shared-cause repair and validation, while also preferring a short diff and reduced structure. The whole policy must be evaluated; deleting completeness requirements to construct a “safer” adaptation creates a separate arm. |
| Project-scoped mode is not session opt-out | Confirmed by source and the author's activation probes. Normalized project keys/selected plugin-data storage can share mode between sessions. Compact-source handling differs from ordinary startup reactivation. |
| Ponytail map is a cheap local retrieval hint | Bounded. It uses a lexical path allowlist and exclusions, caps file enumeration/eligible paths and truncates processing, but reads the full selected file before slicing the considered content. It is neither a complete symbol index nor an immutable original archive. |
| Caveman transforms preserve original on a failed required store | Source agrees with the recorded closed-store result. Compress starts with original content, stages recovery before assigning replacement, and returns the original plus an error on required-store failure. No-store and malformed paths are bounded pass-through cases. |
| Simulation is not a recovery guarantee | Confirmed. Simulate can report a would-be transformation without an active store; it must not be counted as a delivered recoverable replacement. |
| Native store capacity preserves older records | Source and the author's fixture support refusal of a new record at capacity rather than automatic eviction of the tested older records. This does not establish every retention/failure path or cross-session authorization. Content-derived handles do not themselves grant authority. |
| Warmer and proxy behavior are separate from selected engine tests | Confirmed. Enabled/default paths, record mode and mocked switch tests do not establish a real provider warm-up outcome. The six-package CGO test is not a full release-binary build or native host trial. |
| Generated JSON proves component recovery, not accepted-task quality | Confirmed. The 45,007-byte generated fixture, tokenizer proxy, exact CRLF/Unicode/no-final-newline recovery and closed-store/capacity controls retain their specific scope. They are not representative task savings. |

## Correction requested and applied

Ponytail's map `BUDGET` counts the accumulated body lines, excluding the fixed header/footer and with emitted newline accounting outside that counter. The initial “2,000-character output budget” wording could imply a hard bound on the entire emitted response. The author changed product09 to **an approximate 2,000-character body budget**, explicitly naming the excluded envelope/accounting. The reviewed chapter hash in the register is after this correction.

## Limits and remaining qualification

The existing recorded Node probes show concrete cross-session/state behavior and selected activation outputs. The Go tests distinguish no-CGO listing failures from successful CGO-enabled package tests. This review inspected those results without claiming another execution. Package signatures, complete release-binary reproducibility, native discovery/trust, model-bound injection, provider warming and accepted-task quality remain separate gates.

No recommendation lowers the user's chosen main model or reasoning effort. A future owned adaptation must retain required constraints and be compared with both the native control and the exact upstream asset. All extra calls, recovery, configuration payloads and failed attempts belong in the treatment cost.

## Additional bounded market-component review

At the market author's request, the four Rust-component rows in techniques03 were checked against the pinned extracted manifests. Sem-core's default disk cache is off; enabling it introduces the separately demonstrated minimal SQLite linkage constraint. Fff-search defaults to ripgrep, with optional zlob and remaining libgit2/LMDB dependencies. Mq-markdown has no default feature set; mq-lang defaults to HTML conversion with separate file/process/HTTP feature gates. These agree with the architecture shortlist, and the chapter distinguishes source inspection and archive verification from a complete consumer build or task qualification.

The reviewer requested correction of three obsolete internal routes and an explicit already-authorized-observation qualifier for optional failure fallback. Authorization/path-integrity failures must not select raw output. The author applied these internal-route and authorization/fallback clarifications; the final reviewed chapter hashes are recorded in the source register. No market chapter was edited by this reviewer.
