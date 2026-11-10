# T01 Codex with ChatGPT: blocked on this host

Final base SHA: `ebfb26c9c3961ceba28e0cab84396c5530648b20`.
State: unavailable.
Exact catalog reason: `final live proof pending after host re-login`.
Required agent version: Codex CLI 0.167.0.

The host command `codex exec --skip-git-repo-check "Reply with exactly: HOST-PONG"` ran on 2026-11-09 at about 13:12Z. Codex CLI 0.167.0 returned `token_expired` and repeated refresh HTTP 401 responses. Interactive re-login is required. The host `~/.codex/auth.json` stayed unchanged at mode 0600, 4250 bytes, mtime `2026-10-29 22:50:55` local time.

Credential search: `op whoami` fails with `account is not signed in`, so no 1Password credential is accessible. The host omp store has no usable replacement for the expired ChatGPT login. Existing gateway providers are not an approved credential-discovery source and were not used to establish support.

Re-proof condition: complete interactive host re-login, import the fresh login through the supported account path, then run the complete final-head A1 proof on Codex 0.167.0. That proof must show a live model response, credential delivery without a raw secret, refresh behavior, process identity, usage, export, cleanup, and an unchanged host login except for the intentional re-login and refresh updates. Only then may T01 return to `Supported`.

Evidence: `rewrite/evidence/A14-final-head-availability.md`.
