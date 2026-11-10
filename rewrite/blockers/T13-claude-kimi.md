# T13 Claude Code with Kimi: supervised interactive launch unavailable

State: unavailable.
Exact catalog reason: `supervised interactive Claude Code exited 1 after provider-only policy refused api.anthropic.com`.
Evidence: `rewrite/evidence/A14-claude-final-live.md`.

On 2026-11-10, T13 passed print-mode replies and real tool use on jackin-docker and jackin-local with the exact `kimi-for-coding` model. The required supervised interactive check failed on both gateways. Interactive Claude attempted `api.anthropic.com`, the provider-only policy refused that connection, and Claude exited 1. No running Claude direct child remained under `jackin-start`. Successful print-mode evidence does not promote this supervised-runtime failure.
