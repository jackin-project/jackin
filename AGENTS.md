# Rules

- Use CLI whenever sufficient; otherwise computer interaction. Use only `op` for 1Password. Before installing GitHub secrets, back up and verify exact values in 1Password; report vault and item.
- No legacy code. Finish every migration: remove old paths completely—no compatibility shims, aliases, or deprecation periods. Breaking changes are preferred.
- Research project: unsafe, breaking changes expected, never production-ready. Break things when needed; deliver fast.
- Verification gates: always verify from the repo root before committing or merging. Always run tests with `cargo nextest`; never use `cargo test`. Read `COMMITS.md` for the exact required commands, purposes, and usage details.
- No comments in code. Code is the source of truth; a comment creates ambiguity about what is right, code or comment. Keep a comment only when functionality requires it: SPDX license headers for REUSE compliance, clap `#[command]`/`#[arg]` help text and similar string literals consumed by tools, parser directives such as Dockerfile `# syntax=`, and machine-generated files that the repo's own generators verify byte-identical. Never keep explanatory, TODO, or SAFETY comments; when code seems to need explanation, rewrite the code instead.
- Principles:
  - Judge correctness, consistency, and project fit. Never defer known-wrong states for ROI, cost, effort, marginal value, or edge cases.
  - Stop only when the required change is proven impossible with the available tools or model. When uncertain, inspect, test, and measure first.
  - Before fixing a bug, identify why the architecture permitted it and whether the same structure permits related bugs.
  - Prefer fixes that remove the enabling condition. Use a symptom-layer patch only when the root fix is proven infeasible or belongs in a separate change, and name the deferred root cause.
- Delegate first: parallel research, implementation, review, and independent verification. Resolve ambiguity autonomously from evidence and project docs.
- Commit verified changes frequently; push regularly. Prefer one branch; add another only for safe isolation. Merge small PRs promptly after all gates pass.
- Before every PR merge, read all reviews, comments, replies, and unresolved or outdated threads. Use independent subagents to critically verify findings against code, tests, project documentation, and recorded decisions; research uncertainty.
- For accepted feedback, fix, verify, commit, push, and reply on GitHub with the fixing commit URL before resolving. For rejected feedback, reply with evidence and rationale before resolving. Address general comments in linked PR replies. Never delete feedback or resolve it without a justified disposition.
- Re-fetch feedback at the final head SHA. Merge only with no unaddressed feedback or unresolved review threads and all required checks and approvals satisfied. Only explicit, PR-specific human authorization permits ignoring identified feedback; general merge approval is not a waiver.
- Keep instructions lean; explanations, plans, and progress belong in documentation.
