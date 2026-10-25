# Rules

Shared working agreement: keep this section byte-identical across projects. Project-specific rules live under `## This project` below.

- No legacy code. Finish every migration: remove old paths completely — no compatibility shims, aliases, or deprecation periods. Breaking changes are preferred.
- Research project: unsafe, breaking changes expected, never production-ready. Break things when needed; deliver fast.
- Verification gates: verify from the repo root before committing or merging. Run tests with cargo nextest, never cargo test.
- Tests are not forbidden.
- Never write a test without a reviewed plan first. The plan must state which required behavior it proves, which real positive and negative scenarios it covers, and why existing tests cannot cover them.
- Write tests only to prove required functionality works and keeps working. Forbid tests written for coverage numbers or KPIs.
- Test functional/integration-first through public APIs. Exercise observable behavior with real cases and real scenarios, never internals. Assert resulting state, not interaction sequences; no mirror-logic assertions and no unreviewed snapshots.
- Never mock owned code. Mocks and fakes are permitted only at unowned I/O boundaries.
- A red test must mean something is clearly broken. An expected change under analysis may stay red only within its plan; beyond that, quarantine it. Never let a test go red over a reshaped abstraction or a bad mock; design every test so its report is trusted without questioning the doubles.
- Prefer fewer high-signal tests with real fault-catching power over many low-signal tests. Forbid judging a suite by count, coverage %, or speed.
- When an issue escapes, forbid appending one more test. Analyze why current tests missed it, spawn an architect subagent to rethink coverage conceptually, then write or refactor the test the analysis designs.
- Every kept test must be observed to fail on broken code and pass on fixed code, with the break recorded for independent subagent verification; reject tests never seen red. Every kept test must be hermetic and deterministic — no live network, seeded randomness, fake clocks — must pass independent subagent verification before it is kept, and flakes must be quarantined via #[ignore] with a tracking reference, never tolerated; never edit a test to make it green.
- Run narrow per change and the full relevant suite before done, per the project's verification doc.
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
