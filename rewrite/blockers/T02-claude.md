# T02 Claude Code with Anthropic: blocked

State: blocked-vendor.
Reason: vendor clarification pending.

Anthropic permits a person to sign in to the unmodified Claude CLI. Anthropic restricts a third party that collects and routes Claude.ai credentials. It is not clear that the jackin gateway flow is permitted (rewrite/GOAL-MULTI-AGENT.md, Claude block rule).

No written Anthropic clarification is on file. No clarification arrived on 2026-11-08. What arrived on 2026-11-08 is the operator order recorded below, and no operator order was on file before that date.

To unblock: Anthropic replies in writing that the gateway flow is permitted, or the operator orders a live test in writing. Then do the class O steps of task A6, with the host and sandbox refresh race test. If that test logs out the host, T02 stays unavailable.

Update 2026-11-08: the operator ordered a live subscription test in the session goal. The durable record is rewrite/evidence/operator-order-2026-11-08.md. T02 stays blocked-vendor until the live test passes with the host-login check.
Update 2026-11-09: the ordered live test ran within the blocked state (`rewrite/evidence/A14-subscription-lanes.md`). Result: `account add claude --from-host-login` does not exist at HEAD (rejected before any Keychain access), and `jackin load --agent claude` refuses the `BlockedVendor` tuple, so no sandbox was created. Host login answered PONG before and after. T02 stays blocked-vendor; the class O steps need the import implemented and a flippable tuple.
