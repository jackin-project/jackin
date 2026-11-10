# T01 Codex with ChatGPT: final live proof pending

Verified implementation head: `e275950ca3f5d82d17dfa49789f9582d3487088b`.
State: unavailable.
Exact catalog reason: `final live proof pending after host re-login`.
Required agent version: Codex CLI 0.167.0.

The 2026-11-09 host check returned `token_expired` and repeated HTTP 401 refresh responses. The operator subsequently completed interactive re-login. On 2026-11-10, `codex --version` reported `codex-cli 0.167.0` and `codex login status` reported `Logged in using ChatGPT`. No credential value was read or recorded.

Re-proof condition: import the fresh login through the supported account path and complete the final-head A1/G-E2E proof on jackin-docker and jackin-local. It must show a live model response and real tool use, cached rebuild, supervised process identity, placeholders and no key disclosure, provider-only egress, replay, shell persistence, export and guarded purge, subscription usage with no paid fallback, and a usable host login after the sandbox runs. Only then may T01 return to `Supported`.

Evidence: `rewrite/evidence/A14-final-head-availability.md`.
