# T11 omp with MiniMax: blocked on this host

Final base SHA: `ebfb26c9c3961ceba28e0cab84396c5530648b20`.
State: unavailable.
Exact catalog reason: `no MiniMax coding plan key on this host`.
Required agent version: omp 18.18.3.

The host MiniMax login is OAuth for the native client and holds no coding plan key.

Credential search: `op whoami` fails with `account is not signed in`, so no 1Password MiniMax coding plan key is accessible. The host omp store has no MiniMax coding plan key. Existing gateway providers are not an approved credential-discovery source and were not used to establish support.

Earlier proof used omp 18.18.1 and a 1Password credential that is now inaccessible. That proof does not establish omp 18.18.3 on this head. T11 keeps the exact lowercase `minimax-m3` declaration.

Re-proof condition: put a MiniMax coding plan key in the host omp store or another approved source, then register it with `jackin account add minimax --name omp-minimax --credential omp-host` or another supported source. Run the complete final-head A4 live gate on omp 18.18.3, including lowercase `minimax-m3`, the built-in `minimax-code` provider, live text and JSON responses, one real tool call, provider and credential attribution, process identity, export, and cleanup. Only then may T11 return to `Supported`.

Evidence: `rewrite/evidence/A14-final-head-availability.md`.
