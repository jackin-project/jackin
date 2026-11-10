# T13 Claude Code with Kimi: final live proof pending

State: unavailable.
Reason: not yet proven on a real gateway.
Current evidence, 2026-11-10 (Asia/Saigon): authorized 1Password access supplied the Kimi membership key. It was exported to a private host file with mode 0600, and the Claude/Kimi account is registered on jackin-docker and jackin-local. Only the credential source is recorded; no value was exposed.
The earlier plumbing proof reached api.kimi.ai with a dummy credential and received HTTP 401; it is not a live reply or support proof (rewrite/evidence/A14-kimi-lanes.md).
The hermetic provider, policy, account, configuration, console, and usage surfaces remain implemented. To unblock: run the complete final-head D29 live gate with the exact `kimi-for-coding` model on both gateways, including plan-funded usage and no paid fallback.
