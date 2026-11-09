# T20 Codex with MiMo: blocked on this host

Final base SHA: `ebfb26c9c3961ceba28e0cab84396c5530648b20`.
State: unavailable.
Exact catalog reason: `no MiMo Token Plan key on this host`.
Required agent version: Codex CLI 0.167.0.

Credential search: `op whoami` fails with `account is not signed in`, so no 1Password MiMo Token Plan key is accessible. The host omp store has no MiMo Token Plan key. Existing gateway providers are not an approved credential-discovery source and were not used to establish support.

Historical MiMo responses used an older head and currently inaccessible credentials. The final renderer writes `web_search = "disabled"`, but that implementation detail does not prove this final head.

Re-proof condition: put a Singapore MiMo Token Plan key in an approved credential source and register it through `jackin account add mimo --name codex-mimo --agent codex --credential omp-host` or another supported source. Run the complete final-head A4 live gate on Codex CLI 0.167.0, including a live response, provider and endpoint attribution, `web_search = "disabled"`, credential hygiene, process identity, usage, export, and cleanup. Only then may T20 return to `Supported`.

Evidence: `rewrite/evidence/A14-final-head-availability.md`.
