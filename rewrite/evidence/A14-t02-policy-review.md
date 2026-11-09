# A14 T02 policy review

**PASS**. Independent review of commit `8f4a4bfd31db1f2fd98d5dfc8c23d73ab3d3743c` found no blocking correctness, secret-boundary, policy, test, or scope issue. The commit implements the approved minimal correction. This verdict covers hermetic policy permission and closure. It does not claim live supervised-agent success.

## Root cause and scope

The live failure record in `rewrite/evidence/A14-subscription-final.md:97` identifies `platform.claude.com` as the host that supervised Claude Code 2.1.311 requires. Two independent product lists omitted that host.

- `crates/jackin-core/src/agent.rs:690` now pins `Agent::ClaudeCode.spec().provider_hosts` to exactly `api.anthropic.com` and `platform.claude.com`.
- `crates/jackin-core/src/assets/t02-claude.yaml:20-29` now permits the same two hosts on port 443 with REST, read-write, and enforced access.
- `AuthoredPolicy::close_for_tuple` parses the tuple YAML asset at `crates/jackin-openshell/src/policy.rs:236-246`. The YAML correction therefore also makes an authored rule for `platform.claude.com` fail provider-host closure.

The commit changes only `agent.rs`, `t02-claude.yaml`, and `crates/jackin-openshell/tests/tuple_policy.rs`. The exact assertions at `tuple_policy.rs:49-54`, `tuple_policy.rs:97-104`, and `tuple_policy.rs:315-326` cover the spec, profile, and both T02 closure cases. Exact host comparisons reject additional hosts. The existing binary, `CLAUDE_CODE_OAUTH_TOKEN`, and no-refresh assertions remain intact.

The credential name, bearer authentication style, gateway-held credential model, binary, and blocked-vendor state are unchanged. The profile still has no refresh block. The new endpoint extends the existing gateway-managed bearer credential policy to `platform.claude.com`. It does not expose a credential value to the sandbox, diff, or tests. The added test code contains no comments.

## Independent red and green reproduction

Every run used the committed test file and this command from the worktree root.

```sh
mise exec -- mbx +1.99.1 nextest run -p jackin-openshell --test tuple_policy
```

1. **Old YAML and old AgentSpec.** The run exited 100 with 3 failures and 3 passes. `t02_spec_permits_supervised_claude_hosts` showed left `["api.anthropic.com"]` and right `["api.anthropic.com", "platform.claude.com"]`. The T02 assertion in `every_tuple_profile_parses_with_exact_asset_hosts_env_binary_and_refresh` showed the same mismatch. `t02_role_rules_matching_provider_hosts_are_rejected` reached the second loop case and failed with `platform.claude.com: Ok(ClosedPolicy(...))`. The `api.anthropic.com` case was rejected before the loop reached the platform case.
2. **YAML-only fix.** The run exited 100 with 1 failure and 5 passes. Only `t02_spec_permits_supervised_claude_hosts` stayed red, with the same spec-host mismatch. The exact T02 profile assertion and both T02 closure cases passed.
3. **AgentSpec-only fix.** The run exited 100 with 2 failures and 4 passes. The spec assertion passed. The T02 profile assertion stayed red with left `["api.anthropic.com"]` and right `["api.anthropic.com", "platform.claude.com"]`. The closure test also stayed red with `platform.claude.com: Ok(ClosedPolicy(...))`.
4. **Complete committed product change.** `git diff --exit-code -- crates/jackin-core/src/agent.rs crates/jackin-core/src/assets/t02-claude.yaml crates/jackin-openshell/tests/tuple_policy.rs` exited 0 before the final targeted run. The run exited 0 with 6 tests run, 6 passed, and 0 skipped. No temporary product or test mutation remained.

## G-FAST

The required command ran unchanged from the worktree root.

```sh
mise exec -- mbx +1.99.1 xtask ci --fast
```

It exited 0. Nextest reported 2,158 tests run, 2,158 passed, and 0 skipped. The `fmt`, `clippy`, `nextest`, `deny`, `shear`, `comments`, `layers`, `pins`, `containers`, `errors`, `baseline`, `screen`, `demo-gate`, and `reuse` steps passed. The fast gate skipped only the docs build as designed.

## Remaining live requirements

This commit does not flip T02 to supported. A running supervised agent, a successful shell-pane agent check, and a gateway-refresh race check remain separate live requirements. No live network or gateway run was part of this review.
