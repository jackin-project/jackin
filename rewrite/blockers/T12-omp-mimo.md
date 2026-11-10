# T12 omp with MiMo: blocked on this host

Final base SHA: `ebfb26c9c3961ceba28e0cab84396c5530648b20`.
State: unavailable.
Exact catalog reason: `no MiMo Token Plan key on this host`.
Required agent version: omp 18.18.3.

Credential search: `op whoami` fails with `account is not signed in`, so no 1Password MiMo Token Plan key is accessible. The host omp store has no reachable Singapore MiMo Token Plan key. Existing gateway providers are not an approved credential-discovery source and were not used to establish support.

Earlier proof used omp 18.18.1 and a 1Password credential that is now inaccessible. That proof does not establish omp 18.18.3 on this head.

Re-proof condition: put a Singapore MiMo Token Plan key in the host omp store or another approved source, then register it with `jackin account add mimo --name omp-mimo --credential omp-host` or another supported source. Run the complete final-head A4 live gate on omp 18.18.3, including `mimo-v2.6-pro`, the Singapore endpoint, live text and JSON responses, one real tool call, provider and credential attribution, process identity, export, and cleanup. Only then may T12 return to `Supported`.

Evidence: `rewrite/evidence/A14-final-head-availability.md`.
