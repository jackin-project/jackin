# T05 Antigravity with Google: blocked

State: blocked-vendor.
Reason: vendor clarification pending.

Google terms for Antigravity restrict third-party tools that use Antigravity OAuth. It is not clear that an unmodified `agy` in a private OpenShell sandbox is permitted (rewrite/GOAL-MULTI-AGENT.md, Google block rule).

No written Google clarification is on file. No clarification arrived on 2026-11-08. What arrived on 2026-11-08 is the operator order recorded below, and no operator order was on file before that date.

To unblock: Google replies in writing that the flow is permitted, or the operator orders a live test in writing. Then prove that `agy` works with no desktop keyring in the sandbox. If it needs a desktop keyring, T05 stays unavailable with that reason.

Update 2026-11-08: the operator ordered a live test in the session goal. The durable record is rewrite/evidence/operator-order-2026-11-08.md. T05 stays blocked-vendor until the live test proves agy works with no desktop keyring, or records why not.
Update 2026-11-09: the ordered live test ran within the blocked state (`rewrite/evidence/A14-subscription-lanes.md`). Result: `account add antigravity --from-host-login` does not exist at HEAD, and `jackin load --agent agy` refuses the `BlockedVendor` tuple, so no sandbox was created and the no-desktop-keyring proof remains unrun. Host login answered PONG before and after. T05 stays blocked-vendor.
