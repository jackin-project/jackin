# A6 live proof — T03 Grok Build (grok) / xAI — FAIL

Date: 2026-11-06. Repo HEAD 2d732fc8f (feature/openshell-native). Gateway jackin-local, OpenShell 0.1.7, healthy (mTLS).
Host grok 1.0.70 (f50904eaee6c). Sandbox grok spec version 1.0.77, model grok-4.7 (host default confirmed: grok-4.7).
Host auth: ~/.grok/auth.json present, mode 0600, one session key, email alexey@chainargos.com, auth_mode oidc, expires_at 2026-11-06T08:40:57Z. No key printed anywhere.

## Verdict: FAIL (two independent blockers)

### Blocker 1 — host subscription quota exhausted (billing/concurrency FAIL)
Pre-run host check (2026-11-06T08:47Z, after token expiry time; grok auto-refresh path):
- `grok -p "ping" --no-auto-update` -> empty stdout
- `grok -p "ping" --no-auto-update --output-format json` ->
  {"type":"error","message":"Internal error: API error (status 402 Payment Required): Grok Build usage balance exhausted"}
- `grok models` -> "You are logged in with grok.com. Default model: grok-4.7" (login intact, quota exhausted)
Billing attribution cannot be verified and no prompt can run: subscription usage balance exhausted. Per protocol step 6, FAIL.
Host remains logged in (login not invalidated), but not working (402).

### Blocker 2 — account add fails on gateway profile credential declaration
Temp worktree /tmp/jackin-a6-grok at HEAD, T03 flipped Unavailable->Supported there only, built `mise exec -- cargo build -p jackin` OK (28s).
- `./target/debug/jackin --gateway jackin-local account add grok --name grok-host --from-host-login`
  -> `error: gateway jackin-local failed: invalid argument : provider credentials are not declared by profile 'grok': access_token`
Provider jackin-grok-host-grok was NOT created (openshell provider list shows only omp/codex providers).
Root cause candidates: gateway-registered `grok` profile (user/workspace source) does not declare credential `access_token` as crates/jackin-core/src/assets/t03-grok.yaml declares it, and ensure_profile did not replace it; or profile parse/ensure mismatch. Defect in crates/jackin-session/src/accounts.rs:745-775 (sync_login Grok path) / t03-grok.yaml credentials block.

## Steps not reached
Sandbox launch, sandbox prompt+tool, post-run host concurrency, subscription billing verification — not run because account creation failed and host quota is exhausted (402). No sandbox created, nothing to delete.

## Additional defects noted (from spec read, not live-proven)
- crates/jackin-core/src/agent.rs GROK_BUILD_SPEC: provider hosts only ["x.ai"]; inference host cli-chat-proxy.grok.com and auth.x.ai absent (research/agents.md sec.5). Profile t03-grok.yaml endpoints likewise only x.ai:443.
- GROK_BUILD_SPEC install targets list is empty (&[]) despite version 1.0.77 pin.

## Cleanup
Temp worktree removed after writing this file. No secret in any file.
