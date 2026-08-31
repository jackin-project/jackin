# Rules

- No legacy code. Finish every migration: remove old paths completely—no compatibility shims, aliases, or deprecation periods. Breaking changes are preferred.
- This is a research project. It is unsafe and expected to contain breaking changes; never treat it as production-ready. Break things when needed and deliver new implementations fast.
- Always apply these principles:
  - Judge work by correctness, consistency, and project fit. Never defer a known-wrong state because of ROI, cost, effort, or claims that it is low-value, marginal, or an edge case.
  - Stop only when the required change is proven impossible with the available tools or model. When uncertain, inspect, test, and measure first.
  - Before fixing a bug, identify why the architecture permitted it and whether the same structure permits related bugs.
  - Prefer fixes that remove the enabling condition. Use a symptom-layer patch only when the root fix is proven infeasible or belongs in a separate change, and name the deferred root cause.

