# T02 Claude Code with Anthropic: blocked

State: blocked-vendor.
Reason: vendor clarification pending.

Anthropic permits a person to sign in to the unmodified Claude CLI. Anthropic restricts a third party that collects and routes Claude.ai credentials. It is not clear that the jackin gateway flow is permitted (rewrite/GOAL-MULTI-AGENT.md, Claude block rule).

No written Anthropic clarification is on file. No clarification arrived on 2026-11-08. What arrived on 2026-11-08 is the operator order recorded below, and no operator order was on file before that date.

To unblock: Anthropic replies in writing that the gateway flow is permitted, or the operator orders a live test in writing. Then do the class O steps of task A6, with the host and sandbox refresh race test. If that test logs out the host, T02 stays unavailable.

Update 2026-11-08: the operator ordered a live subscription test in the session goal. The durable record is rewrite/evidence/operator-order-2026-11-08.md. T02 stays blocked-vendor until the live test passes with the host-login check.
Update 2026-11-09: the final live proof (`rewrite/evidence/A14-subscription-final.md`) imported the host login on both gateways. One-shot Claude answered on both gateways with the subscription credential. Supervised Claude exited because `t02-claude.yaml` permits only `api.anthropic.com` and refuses `platform.claude.com`. Host login stayed healthy. The profile has no gateway refresh configuration, so the refresh race remains unproven. T02 stays blocked-vendor pending the policy fix and proof of a running supervised agent, a successful shell-pane agent check, and a gateway-refresh race check.

Update 2026-11-09: commit `8f4a4bfd31db1f2fd98d5dfc8c23d73ab3d3743c` corrected the supervised policy by adding `platform.claude.com`. The independent policy review passed (`rewrite/evidence/A14-t02-policy-review.md`). The follow-up live proof (`rewrite/evidence/A14-t02-live-fixed.md`) shows that supervised Claude stayed running under `jackin-start` and answered one fixed prompt on `jackin-docker`. After host Claude answered, the next sandbox request failed with HTTP 401 and `OAuth token revoked`. Host Claude still answered afterward and after sandbox cleanup, and the `Claude Code-credentials` Keychain item remained. The evidence does not establish the revocation cause.

The provider reported no refresh configurations. Credential continuity and refresh-race safety remain unproven. The stop prevented the `jackin-local` recheck, a passing reattach replay marker, a direct post-shell Claude process check, and export. T02 stays blocked-vendor pending gateway-owned refresh metadata, a fresh host-login re-import, and a successful complete live proof. This evidence does not establish a working tuple.

Update 2026-11-10: the final proof at `94d880efefe94cc9915c9369d15a8cb6a3507394` used the new generic `OAuth2RefreshToken` path and native placeholder file (`rewrite/evidence/A14-t02-live-final.md`). The ordinary host-then-sandbox sequence passed. Explicit gateway refresh passed and a post-refresh sandbox response passed, but the next host request failed with `Failed to authenticate: OAuth session expired and could not be refreshed`. Host Claude then reported `loggedIn=false` and stayed unusable after docker cleanup. The later gateway response still passed. No rotations ran concurrently.

The docker run passed cached rebuild, uid/no sudo, direct-child supervision, exact model replies, a real Bash tool call, placeholder and no-disclosure checks, provider-only egress with non-agent denial, observed-screen replay, shell persistence, exact export with unchanged host state, guarded purge, and subscription-only response attribution. These passes do not close the host-safety failure. The local import then exited 2 with `no claude host login is available`, so no local sandbox ran and no local check is PASS. T02 stays blocked-vendor pending a fresh host login and a complete two-gateway proof in which gateway refresh does not log out the host.
