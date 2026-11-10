# T08 Kimi Code with Kimi: blocked on guest model resolution

State: unavailable.
Reason: Kimi Code 2.5.2 rejects the rendered model configuration before inference on both gateways.
Update 2026-11-10 (Asia/Saigon): `rewrite/evidence/A14-t08-live.md` records the final-head global-region proof at `bafb12ec0c157df12ca0d497b0b22620a6e3c074`. Native `--from-host-login` import selected `kimi-global` on `jackin-docker` and `jackin-local`, generic `OAuth2RefreshToken` refresh passed, the host login remained usable, and independent containment and lifecycle checks passed. The fixed guest prompt failed on both gateways with `Model "kimi-code/kimi-for-coding" is not configured in config.toml`. Explicit model diagnostics also failed. No guest reply, successful tool call, post-refresh reply, or nonzero membership usage was obtained.
The mainland China lane was not exercised because `/Users/donbeave/.kimi-code/credentials/kimi-code.json` does not exist. No membership API key is a permitted fallback.
To unblock: correct the rendered config and supervised-launch contract for the pinned Kimi Code runtime, then rerun the complete A5/D29 live gate, including exact replies, a real tool, membership usage, post-refresh inference, and live secondary-home coverage. The temporary `Supported` enablement used for the proof was restored and was not committed or pushed.
