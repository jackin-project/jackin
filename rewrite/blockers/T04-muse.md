# T04 Muse Code with Meta: blocked

State: unavailable.
Reason: the existing `jackin-local` provider reaches a supervised launch, but Muse rejects its gateway-held credential. `jackin-docker` has no Muse provider, and the required fresh host-login import times out at the interactive 30-second Keychain read.

Current evidence, 2026-11-10:

- `rewrite/evidence/A14-muse-final-live.md` re-checks final head `d9411a2d72a3710a28675072adef1bcc64c07093` through the sanctioned provider-present path. On local, the launch reuses `jackin-muse-live-muse` and creates a sandbox, but the exact-model request fails with `your API key from META_API_KEY was rejected`. The provider has no refresh configuration. No reply or tool call occurs.
- On docker, the provider step fails after 31.08 seconds with `no credential is available for account muse-live`. No provider or sandbox is created. The run attempts no Keychain bypass and no API-key substitution.
- Host Muse metadata still reports an active Keychain-backed subscription. `muse usage --json` completes afterward without a model turn, but returns `server_sent_no_usage`. Subscription consumption and paid-fallback absence remain unproven.

The 2026-11-09 fresh-import failures remain in `rewrite/evidence/A14-oauth-final.md` and `rewrite/evidence/A14-oauth-verifier.md`. The prior Landlock read-only `/` diagnosis remains in `rewrite/evidence/A14-muse-strace.md`. The current local run passes image build and reaches the provider, but it certifies none of the remaining sandbox lifecycle requirements.

Re-prove condition: register a valid Muse subscription credential on both gateways through the sanctioned account path, then pass the complete two-gateway T04 live gate. A stale provider, a timed-out import, or a skipped check must not change T04 to supported.
