# Usage reporting research todo

## Autonomous run playbook steps

1. State the exit condition as a checkable predicate before the first iteration (tests green, repro fixed, all N PRs merged, pixel-diff zero).
2. Pick the wake mechanism using Claude Code's `loop` skill (a built-in, not a pstack skill). An event to watch (CI, a merge, a ref advancing) gets a watcher subagent that wakes you on the event, with a long time-based heartbeat as fallback. No event gets a fixed-interval heartbeat sized to when the result is worth re-checking. Pick each subagent's reasoning level for the job, per the Subagents section.
   - skip: subagents wake this session reactively on completion without polling; no external CI or PR event is watched.
3. Each iteration makes the smallest change the evidence justifies, verifies it against the predicate, commits if it advanced, discards changes that didn't help. Belt-and-suspenders that "might help" gets reverted, not left to ride. Sequence the work via the **sequence-verifiable-units** principle skill, verifying each unit before the next instead of batching checks at the end.
   - note: per operator instructions, do not commit, push, merge, or publish; verify and update `rewrite/research/usage-reporting.md` in the working tree.
4. Mid-run discoveries that support the goal are yours. Address broken skills, related bugs, flaky verifiers, review noise, tooling failures, orphaned follow-ups, and fixable drift yourself via poteto-mode. Put out-of-band fixes in their own PR. Do not park reversible work for the human or use `AskUserQuestion`. Surface only irreversible actions, genuine product or preference calls no experiment can settle, or a real dead end. Keep the predicate as the main drive, and return to it after each side fix.
5. Checkpoint every iteration via the **show-me-your-work** skill, a row for what changed and whether the predicate moved.
6. Stop when the predicate is met. A plateau is not a stop, so keep going and pivot your approach to push past it. Surface a genuine dead end rather than spinning, and never relax the predicate to declare victory.

## Figure-it-out phases and task units

- [x] Read the Principles section of the `poteto-mode` skill and relevant leaf principles.
- [x] Phase A: Frame the done predicate, scope, and rigor level.
- [x] Phase B: Design the workflow and subagent research slices.
- [x] Phase C1 (xAI / Grok Build): Check current local Grok CLI auth store, running Grok processes/panes, latest upstream `xai-org/grok-build`, `openusage`, and `CodexBar` sources, and if an unexpired matching token exists, run a safe read-only verification comparing `creditUsagePercent` vs `productUsage` (`GrokBuild`) and reset timestamps against any contemporaneous display or source contract.
- [x] Phase C2 (Google Antigravity / agy): Inspect the installed `agy` binary and any running `agy` processes, local listeners, Unix sockets, or status-line/state files (without launching `agy`, bypassing 403/CSRF/TLS, or refreshing tokens), plus latest `openusage` and `CodexBar` sources and official docs.
- [x] Phase C3 (Meta / Muse Code): Inspect the installed `muse` binary/bundle, local auth/config/session stores, any running `muse` processes or browser captures (without launching `muse` or calling `POST /muse-code/key`), plus latest `meta-models/muse-code-sdk`, `openusage`, and `CodexBar` sources.
- [x] Phase C4 (Remaining provider gaps): Audit the latest `openusage` and `CodexBar` commits and local state for Claude, Codex, Z.AI, Kimi Code, MiniMax Code, Xiaomi MiMo, and OpenCode Go (leaving OpenRouter and standalone Gemini deferred), checking if any fresh unexpired tokens or new upstream commits resolve open questions.
- [x] Phase C5 (Update spec): Update `rewrite/research/usage-reporting.md` with all newly verified evidence, exact recipes, comparison tables, and test plans, applying `technical-writing` and `unslop`.
- [x] Phase D & E (Audit trail and independent review): Audit `.audit/usage-research.tsv` against the session transcript, run an independent review subagent on the spec diff and privacy rules, fix any findings, and verify git status and diff cleanliness.
