# T16 Claude Code with MiMo: blocked on this host

Final base SHA: `ebfb26c9c3961ceba28e0cab84396c5530648b20`.
State: unavailable.
Exact catalog reason: `final live proof pending with MiMo Token Plan key`.
Required agent version: Claude Code 2.1.311.

Credential search: `op whoami` fails with `account is not signed in`, so no 1Password MiMo Token Plan key is accessible. The host omp store has no MiMo Token Plan key. Existing gateway providers are not an approved credential-discovery source and were not used to establish support.

Historical MiMo responses used an older head and currently inaccessible credentials. Claude Code's base-URL-only renderer remains covered, but it does not prove this final head.

Re-proof condition: put a Singapore MiMo Token Plan key in an approved credential source and register it through `jackin account add mimo --name claude-mimo --agent claude --credential omp-host` or another supported source. Run the complete final-head A4 live gate on Claude Code 2.1.311, including a live response, provider and endpoint attribution, base-URL-only settings, credential hygiene, process identity, export, and cleanup. Only then may T16 return to `Supported`.

Evidence: `rewrite/evidence/A14-final-head-availability.md`.
