# T03 Grok Build (grok) with xAI: blocked

Tuple: T03.
State: unavailable.
Reason: Host Grok subscription usage balance exhausted (HTTP 402 on live run, 2026-11-06).

The host live proof on 2026-11-06 returned HTTP 402 "Grok Build usage balance exhausted" from the `grok-subscription-proof` run, so no sandbox prompt can succeed and billing attribution cannot be verified. Full record: rewrite/evidence/A6-grok.md.

Closed defect: `jackin account add grok --name grok-host --from-host-login` failed with "provider credentials are not declared by profile 'grok': access_token". The cause was a jackin bug, not a gateway bug. OpenShell v0.1.7 accepts only the profile credential env var names as stored keys. Jackin stored the key under the logical name `access_token`. The fix stores the Grok key under `XAI_API_KEY` (commit 7c30d78c4). Account creation is no longer blocked by this defect.

To unblock: the Grok subscription quota recovers, `jackin account add grok --name grok-host --from-host-login` succeeds against the gateway, and the G-E2E live gate passes in a sandbox. Then flip T03 to supported.
