# T21 omp with Anthropic: final live proof pending

State: unavailable.
Reason: final live proof pending with Anthropic API key.
Current evidence, 2026-11-10 (Asia/Saigon): authorized 1Password access supplied the Anthropic API key. It was exported to a private host file with mode 0600, and the omp/Anthropic account is registered on jackin-docker and jackin-local. Only the credential source is recorded; no value was exposed.
The hermetic provider, policy, account, configuration, console, and usage surfaces remain implemented. To unblock: run the complete final-head D29 live gate with the exact `claude-haiku-5-5` model on both gateways, including API-key-funded usage and no other credential or paid fallback.
