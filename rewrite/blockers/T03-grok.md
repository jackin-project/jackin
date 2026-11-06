# T03 Grok Build (grok) with xAI: blocked

Tuple: T03.
State: unavailable.
Reason: Host Grok subscription usage balance exhausted (HTTP 402 on live run, 2026-11-06).

The host live proof on 2026-11-06 returned HTTP 402 "Grok Build usage balance exhausted" from the `grok-subscription-proof` run, so no sandbox prompt can succeed and billing attribution cannot be verified. Full record: rewrite/evidence/A6-grok.md.

Open gateway defect: `jackin account add grok --name grok-host --from-host-login` failed with "provider credentials are not declared by profile 'grok': access_token", although `openshell provider profile describe grok -g jackin-local` declares `access_token`. The gateway-registered profile and the declared profile in crates/jackin-core/src/assets/t03-grok.yaml disagree, or profile ensure does not replace the registered profile. This must be fixed or explained before account creation can succeed.

To unblock: the Grok subscription quota recovers, `jackin account add grok --name grok-host --from-host-login` succeeds against the gateway, and the G-E2E live gate passes in a sandbox. Then flip T03 to supported.
