# T10 omp with Kimi: blocked on this host

Final base SHA: `ebfb26c9c3961ceba28e0cab84396c5530648b20`.
State: unavailable.
Exact catalog reason: `no Kimi Code membership key on this host`.
Required agent version: omp 18.18.3.

The host Kimi login is OAuth and has an empty `api_key` field.

Credential search: `op whoami` fails with `account is not signed in`, so no 1Password Kimi Code membership key is accessible. The host omp store holds only a Z.AI record and no Kimi Code membership key. Existing gateway providers are not an approved credential-discovery source and were not used to establish support.

Earlier proof used omp 18.18.1 and a 1Password credential that is now inaccessible. That proof does not establish omp 18.18.3 on this head.

Re-proof condition: put a Kimi Code membership key in the host omp store or another approved source, then register it with `jackin account add kimi --name omp-kimi --credential omp-host` or another supported source. Run the complete final-head A4 live gate on omp 18.18.3, including `kimi-for-coding`, the `https://api.kimi.com/coding/v1` endpoint in `models.yml`, live text and JSON responses, one real tool call, provider and credential attribution, process identity, export, and cleanup. Only then may T10 return to `Supported`.

Evidence: `rewrite/evidence/A14-final-head-availability.md` and the historical `rewrite/evidence/A4-omp-kimi.md`.
