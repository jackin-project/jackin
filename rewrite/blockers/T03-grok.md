# T03 Grok Build (grok) with xAI: blocked

Tuple: T03.
State: unavailable.
Reason: host Grok answers fixed prompts, but sandbox Grok 1.0.77 reports `You are using XAI_API_KEY`, default model `grok-4.6`, and available models `grok-4.6` and `grok-4.5`. Fixed one-shot prompts hang without a response on both gateways, and supervised Grok stops at the directory-trust prompt. Subscription billing attribution is unproven.

Current evidence, 2026-11-09:
- `rewrite/evidence/A14-oauth-final.md` records host responses, successful host-login imports on both gateways, the sandbox model-list mismatch, both fixed-prompt hangs, and the supervised directory-trust stop. It records no sandbox fixed-prompt response and no proof of subscription billing attribution.
- `rewrite/evidence/A14-oauth-verifier.md` independently reproduces the host response, sandbox model-list mismatch, bounded fixed-prompt timeout, directory-trust stop, and unproven billing attribution on `jackin-local`.

The host-login import succeeds, but the rendered `auth.json` contains a placeholder and `XAI_API_KEY` contains an `openshell:resolve:` broker reference. The import and placeholder/session shape defect needs a separately reviewed code unit. The fixed-prompt hang and directory-trust stop also need product-code review before another support disposition.

The earlier HTTP 402 results are historical. Current host responses do not establish persistent HTTP 402, and neither evidence file records a live sandbox fixed-prompt response.

Re-prove condition: complete the separately reviewed code work, then obtain fixed-prompt and supervised launch-task responses on both gateways. Prove subscription billing attribution, refresh and host concurrency, and second-sandbox isolation before changing T03 to supported.
