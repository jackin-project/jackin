# Rules

Shared working agreement: keep this section byte-identical across projects. Project-specific rules live under `## This project` below.

- No legacy code. Finish every migration: remove old paths completely — no compatibility shims, aliases, or deprecation periods. Breaking changes are preferred.
- Research project: unsafe, breaking changes expected, never production-ready. Break things when needed; deliver fast.
- Verification gates: verify from the repo root before committing or merging. Run tests with cargo nextest, never cargo test.
- Tests are not forbidden. You must write tests wherever you add or change required behavior.
- Never write a test without a reviewed plan first: an independent subagent must confirm what behavior it proves, which real positive and negative scenarios it covers, and why existing tests cannot cover it.
- Prefer fewer high-signal tests over many low-signal tests. Never write a test for coverage numbers or KPIs, and never judge a suite by count or coverage %.
- Test integration-first through public APIs asserting resulting state, not interaction sequences; test internals only for pure logic unreachable observably; forbid mirror-logic assertions and unreviewed snapshots.
- Never mock owned code. Doubles and fakes are permitted only at unowned external I/O (network, clock, filesystem, OS) behind interfaces you own; if a small change breaks many doubles, consolidate toward real coverage.
- A red test must name broken user-observable behavior, never a reshaped abstraction or bad double; a red from reshaping or a bad double means fix the test, and analyze an expected behavior change before updating its tests.
- When an issue escapes, do not just append a test; analyze why current tests missed it and rethink coverage conceptually (seek independent review for significant escapes) before writing or refactoring the test.
- Keep a test only if observed red on broken code and green on fixed code, verified by an independent subagent. Keep tests hermetic and deterministic (seeded randomness, fake clocks, no live network); quarantine flakes at once; never skip, weaken, edit, or delete a test just to go green.
- Run narrow tests per change and the full relevant suite before done; keep the loop fast enough to run every time.
- No comments in code. Code is the source of truth; a comment creates ambiguity about what is right, code or comment. Keep a comment only when functionality requires it: SPDX license headers for REUSE compliance, clap `#[command]`/`#[arg]` help text and similar string literals consumed by tools, parser directives such as Dockerfile `# syntax=`, and machine-generated files that the repo's own generators verify byte-identical. Never keep explanatory, TODO, or SAFETY comments; when code seems to need explanation, rewrite the code instead.
- Judge correctness, consistency, and project fit. Never defer a known-wrong state for ROI, cost, effort, or edge cases.
- Stop only when the required change is proven impossible with the available tools or model. When uncertain, inspect, test, and measure first.
- Before fixing a bug, identify why the architecture permitted it and whether the same structure permits related bugs. Prefer fixes that remove the enabling condition; use a symptom-layer patch only when the root fix is proven infeasible or belongs in a separate change, and name the deferred root cause.
- Delegate first: use subagents for parallel research, implementation, review, and independent verification. Resolve ambiguity autonomously from evidence and project docs.
- Commit verified changes frequently; push regularly. Prefer one branch; add another only for safe isolation.
- Before every PR merge, read all reviews, comments, replies, and unresolved or outdated threads. Use independent subagents to verify findings against code, tests, docs, and recorded decisions.
- For accepted feedback: fix, verify, commit, push, and reply with the fixing commit URL before resolving. For rejected feedback: reply with evidence and rationale before resolving. Re-fetch feedback at the final head SHA. Merge only with zero unaddressed feedback and all required checks and approvals satisfied. Only explicit, PR-specific human authorization permits ignoring identified feedback.
- Keep instructions lean; explanations, plans, and progress belong in documentation, not here.

## This project (jackin)

- Secrets: use the CLI whenever sufficient. Use only `op` for 1Password. Before installing GitHub secrets, back up and verify exact values in 1Password; report vault and item.
- Verification gates: exact commands, purposes, and usage: [COMMITS.md](./COMMITS.md).
