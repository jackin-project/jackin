# T02 Claude Code with Anthropic: blocked

State: blocked-vendor.
Reason: vendor clarification pending.

Anthropic permits a person to sign in to the unmodified Claude CLI. Anthropic restricts a third party that collects and routes Claude.ai credentials. It is not clear that the jackin gateway flow is permitted (rewrite/GOAL-MULTI-AGENT.md, Claude block rule).

No written Anthropic clarification is on file. No clarification arrived on 2026-11-08. What arrived on 2026-11-08 is the operator order recorded below, and no operator order was on file before that date.

To unblock: Anthropic replies in writing that the gateway flow is permitted, or the operator orders a live test in writing. Then do the class O steps of task A6, with the host and sandbox refresh race test. If that test logs out the host, T02 stays unavailable.

Update 2026-11-08: the operator ordered a live subscription test in the session goal. The durable record is rewrite/evidence/operator-order-2026-11-08.md. T02 stays blocked-vendor until the live test passes with the host-login check.
Update 2026-11-09: the final live proof (`rewrite/evidence/A14-subscription-final.md`) imported the host login on both gateways. One-shot Claude answered on both gateways with the subscription credential. Supervised Claude exited because `t02-claude.yaml` permits only `api.anthropic.com` and refuses `platform.claude.com`. Host login stayed healthy. The profile has no gateway refresh configuration, so the refresh race remains unproven. T02 stays blocked-vendor pending the policy fix and proof of a running supervised agent, a successful shell-pane agent check, and a gateway-refresh race check.
