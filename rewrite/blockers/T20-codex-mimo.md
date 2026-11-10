# T20 Codex with MiMo: final live proof pending

Verified implementation head: `e275950ca3f5d82d17dfa49789f9582d3487088b`.
State: unavailable.
Exact catalog reason: `final live proof pending with MiMo Token Plan key`.
Required agent version: Codex CLI 0.167.0.

Authorized 1Password access supplied the Singapore MiMo Token Plan key. It was exported to a private host file with mode 0600, and the Codex/MiMo account is registered on jackin-docker and jackin-local. Only the credential source is recorded; no value was exposed. Historical responses on an older head do not prove this final head. The final renderer writes `web_search = "disabled"`, but that implementation detail is not live support evidence.

Re-proof condition: run the complete final-head D29 live gate on both gateways with `mimo-v2.6-pro`, the OpenAI-compatible Singapore endpoint, `web_search = "disabled"`, exact live replies and one real tool call, plan-funded usage with no paid fallback, cached rebuild, supervised process identity, placeholders, provider-only egress, replay, shell persistence, export, and guarded purge. Only then may T20 return to `Supported`.

Evidence: `rewrite/evidence/A14-final-head-availability.md`.
