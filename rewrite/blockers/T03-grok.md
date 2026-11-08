# T03 Grok Build (grok) with xAI: blocked

Tuple: T03.
State: unavailable.
Reason: host Grok subscription usage balance exhausted (HTTP 402).

Dated evidence:
- 2026-11-06: the host live run `grok -p "ping" --no-auto-update --output-format json` returned HTTP 402 "Grok Build usage balance exhausted", so no sandbox prompt can succeed and billing attribution cannot be verified. Login stayed intact. Full record: rewrite/evidence/A6-grok.md.
- 2026-11-08T13:05:49Z: re-check `grok -p "Reply with exactly: PONG" --no-auto-update --output-format json` returned the same HTTP 402, exit 1. `grok models` at 13:05:58Z still answers "You are logged in with grok.com. Default model: grok-4.7". Quota still exhausted, login intact. Record: rewrite/evidence/A14-live-prep.md.

Closed defect: `jackin account add grok --name grok-host --from-host-login` failed with "provider credentials are not declared by profile 'grok': access_token". The cause was a jackin bug, not a gateway bug. OpenShell v0.1.7 accepts only the profile credential env var names as stored keys. Jackin stored the key under the logical name `access_token`. The fix stores the Grok key under `XAI_API_KEY` (commit 7c30d78c4). Account creation is no longer blocked by this defect.

Re-prove condition: the cheap quota re-check (`grok -p "Reply with exactly: PONG" --no-auto-update --output-format json`) returns a reply instead of HTTP 402. Then run the class O steps of rewrite/evidence/A14-live-prep.md: `jackin account add grok --name grok-live --from-host-login`, flip T03 in a temp worktree, add `[agents.grok]` to the sentinel role, launch, sandbox prompt, `/sandbox/.grok/auth.json` placeholder scan, refresh and concurrency (host and sandbox logged in in the same window), subscription billing basis (Auto Top-up off, prepaid balance zero, the reply billed to the subscription), and second-sandbox isolation. When that gate passes, flip T03 to supported.
