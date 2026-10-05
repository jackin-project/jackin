# PR #1109 independent application/API/authentication review

## Reviewed revisions

- Base: `main` `310e644832193232344ca99838ccf599f28faf53`
- Head: `a6c85ba8a154ccce224dce80d1ed5ef8fa03bc6e`
- Reviewed PR range: `310e644832193232344ca99838ccf599f28faf53..a6c85ba8a154ccce224dce80d1ed5ef8fa03bc6e`
- Head commits in the range: `37c18b0a7a546afa801880801ad8f615b30295d4`, `b349e1c47c013a7f74848d0b68151b09d3688a97`, `f09c716be1516f0b49021fe0e68dc4ea87cea992`, `793663f919ed82213b11dc70ee74f9f74642a175`, `7f5ed14b5581d28c7db621d12c189435a9b771f3`, `0a98671e90eb1151aecf7d5a48ff9075ea8ceaad`, `a6c85ba8a154ccce224dce80d1ed5ef8fa03bc6e`.

## Review conditions

Read-only source/diff review. No build, tests, network calls, comments, or Git mutations. Existing unrelated worktree edits were left untouched.

## Result

**No PR-introduced high, medium, or low credential-confusion finding was confirmed.** The changed API-key/OAuth route path is account- and material-fenced through the host broker.

## Auth-boundary trace

1. `AccountConfig::resolved_credential_descriptor` rejects disabled/incompatible routes and returns the exact launch environment key (`crates/jackin-config/src/accounts.rs:395-429`). `credential_descriptors_for_account` enumerates compatible launch routes for discovery (`accounts.rs:1295-1361`).
2. Codex/OpenCode private provider configuration is fetched from the exact `instance.account_id`, uses that account's provider and exact descriptor key, and writes the instance endpoint (`crates/jackin-runtime/src/runtime/launch/account_config.rs:700-749`, `900-922`). No account fallback is present.
3. Launch staging records account ID, provider surface, source declaration identity, and resolved material fingerprint (`crates/jackin-runtime/src/usage_relay.rs:301-349`). `forwarded_sources_from_launch_config` adds the selected account IDs/surfaces (`usage_relay.rs:355-373`).
4. Discovery resolves each registered account through an isolated alias, keeps canonical owner and exact launch aliases, and records the resolver handle/source material (`crates/jackin-usage/src/host/discovery.rs:772-955`, `1630-1685`). Resolver cache hits require canonical key, dispatch key, and declaration equality; refresh requires the dispatch key and opaque handle (`crates/jackin-usage/src/host/credential_resolver.rs:151-230`, `311-340`).
5. Scoped relay requests discard any Capsule-supplied scope and send the host-held immutable scope. Capsule capabilities are exact per-instance capabilities and peer-authorized (`crates/jackin-runtime/src/usage_relay.rs:675-755`, `crates/jackin-capsule/src/usage_relay_proxy.rs:103-174`, `260-350`).
6. Host broker grouped authorization matches account/surface/launch alias/source/material to a binding, rejects profile+environment mixtures and conflicting authorities, then refreshes the selected opaque handle (`crates/jackin-usage/src/host/broker.rs:564-742`, `1319-1409`). Scoped refresh carries the same scope into the worker job (`crates/jackin-usage/src/coordinator.rs` PR diff; `broker.rs:1340-1376`).

## Requested challenge points

### Endpoint/tenant binding

Account `base_url` is applied to the exact selected instance's private Codex/OpenCode config. The usage broker receives no account endpoint and provider usage adapters use host-process/provider-surface routing. Therefore a custom account endpoint is not an authorization selector for usage refresh. This is a correctness limitation for providers whose quota authority is tenant/endpoint-specific, but I found no PR path that lets a Capsule select another account's endpoint or causes a selected account's secret to be routed by another configured account.

### Empty-scope ambient authorization

`ForwardingRequirement::Env::is_forwarded` retains an empty-`selected_account_ids` fallback based on matching launch env names (`broker.rs:789-818`). Production relay construction calls `forwarded_sources_from_launch_config`, which fills selected account IDs before capability derivation (`usage_relay.rs:355-373`); the host relay also owns the immutable scope. The ambient branch remains a legacy/helper path and was not reachable from the production launch path reviewed. No PR-introduced bypass confirmed.

### Spoofing/confused deputy

Capsule request scopes are ignored by the host relay; exact opaque capabilities and peer identity are checked before tunneling. Host broker source proofs require exact account ID, surface, source identity, and material fingerprint. Mixed profile/environment groups fail closed. No spoofable user-controlled provider/account label participates in capability identity.

### Fallbacks and stale references

Explicit launch account failures do not silently fall back (`accounts.rs` launch resolution path). Resolver cache reuse is declaration-scoped and refresh dispatch is route-scoped. Scope material fingerprints reject rotated/repointed values; catalog revisions and broker reconciliation fence stale capabilities. Scoped refresh carries the scope into the actual provider worker. No new fallback or stale-reference credential use was found.

### Baseline regressions / inherited risks

Profile forwarding remains surface-only and profile groups may choose the first profile without a source proof. The same behavior exists at base (`broker.rs` baseline `ForwardingRequirement::Profile` and single-binding profile authorization); PR grouping preserves it for pure-profile groups and only changes mixed groups to fail closed. I therefore do not attribute this inherited issue to PR #1109.

## Non-findings

- No account-ID/provider-surface mismatch in the changed API-key/OAuth route map.
- No route alias that crosses provider surfaces without an exact account/material proof.
- No Capsule-controlled endpoint or scope injection into host provider refresh.
- No confirmed cross-account outbound API call introduced by the PR.
