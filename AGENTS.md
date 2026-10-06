# Rules

- No legacy code. Finish every migration: remove old paths completely—no compatibility shims, aliases, or deprecation periods. Breaking changes are preferred.
- This is a research project. It is unsafe and expected to contain breaking changes; never treat it as production-ready. Break things when needed and deliver new implementations fast.
- Always apply these principles:
  - Judge work by correctness, consistency, and project fit. Never defer a known-wrong state because of ROI, cost, effort, or claims that it is low-value, marginal, or an edge case.
  - Stop only when the required change is proven impossible with the available tools or model. When uncertain, inspect, test, and measure first.
  - Before fixing a bug, identify why the architecture permitted it and whether the same structure permits related bugs.
  - Prefer fixes that remove the enabling condition. Use a symptom-layer patch only when the root fix is proven infeasible or belongs in a separate change, and name the deferred root cause.
- Delegate first: use subagents for parallel research, implementation, review, and independent verification. Resolve ambiguity autonomously using evidence and project documentation.
- Commit meaningful, verified changes frequently and push regularly. Prefer one working branch; create another only when safe work requires it. Merge small PRs promptly after all gates pass.
- Before every PR merge, read all reviews, comments, replies, and unresolved or outdated threads. Use independent subagents to critically verify findings against code, tests, project documentation, and recorded decisions; research uncertainty.
- For accepted feedback, fix, verify, commit, push, and reply on GitHub with the fixing commit URL before resolving. For rejected feedback, reply with evidence and rationale before resolving. Address general comments in linked PR replies. Never delete feedback or resolve it without a justified disposition.
- Re-fetch feedback at the final head SHA. Merge only with no unaddressed feedback or unresolved review threads and all required checks and approvals satisfied. Only explicit, PR-specific human authorization permits ignoring identified feedback; general merge approval is not a waiver.
- Keep agent instructions lean. Put explanations, plans, and progress in documentation, not here.
