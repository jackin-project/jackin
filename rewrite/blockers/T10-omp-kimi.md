# T10 omp with Kimi: final live proof pending

Verified implementation head: `e275950ca3f5d82d17dfa49789f9582d3487088b`.
State: unavailable.
Exact catalog reason: `final live proof pending with Kimi plan key`.
Required agent version: omp 18.18.3.

Authorized 1Password access supplied the Kimi membership key. It was exported to a private host file with mode 0600, and the omp/Kimi account is registered on jackin-docker and jackin-local. Only the credential source is recorded; no value was exposed. Earlier omp 18.18.1 evidence does not prove omp 18.18.3 on this head.

Re-proof condition: run the complete final-head A4/D29 live gate on both gateways with `kimi-for-coding`, the `https://api.kimi.com/coding/v1` endpoint in `models.yml`, exact live replies and one real tool call, plan-funded usage with no paid fallback, cached rebuild, supervised process identity, placeholders, provider-only egress, replay, shell persistence, export, and guarded purge. Only then may T10 return to `Supported`.

Evidence: `rewrite/evidence/A14-final-head-availability.md` and the historical `rewrite/evidence/A4-omp-kimi.md`.
