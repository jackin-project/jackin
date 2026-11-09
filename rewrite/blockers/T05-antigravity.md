# T05 Antigravity with Google: unavailable

State: unavailable.
Reason: subscription requires a desktop keyring in the sandbox.

Google terms for Antigravity restrict third-party tools that use Antigravity OAuth. It is not clear that an unmodified `agy` in a private OpenShell sandbox is permitted (rewrite/GOAL-MULTI-AGENT.md, Google block rule).

No written Google clarification is on file. No clarification arrived on 2026-11-08. What arrived on 2026-11-08 is the operator order recorded below, and no operator order was on file before that date.

To unblock: Google replies in writing that the flow is permitted, or the operator orders a live test in writing. Then prove that `agy` works with no desktop keyring in the sandbox. If it needs a desktop keyring, T05 stays unavailable with that reason.

Update 2026-11-08: the operator ordered a live test in the session goal. The durable record is rewrite/evidence/operator-order-2026-11-08.md. T05 stays blocked-vendor until the live test proves agy works with no desktop keyring, or records why not.
Update 2026-11-09: the final live proof (`rewrite/evidence/A14-subscription-final.md`) started `agy` under `jackin-start` on both gateways with no D-Bus or desktop keyring. Fixed prompts returned `authentication required` on both gateways. The vendor's [authentication documentation](https://antigravity.google/docs/cli/install/) and [troubleshooting documentation](https://antigravity.google/docs/cli/troubleshooting/) state that personal subscription tokens use the OS keyring and that `GOOGLE_API_KEY` has no effect. `GEMINI_API_KEY` with `modelProvider=gemini` selects direct Gemini API authentication, not this tuple's subscription class, and must not be used as a fallback. T05 is unavailable because the subscription requires a desktop keyring in the sandbox. `AuthClass::BlockedVendor` remains unchanged.
