# A14 T13 auth-token correction

Implementation base: `6d22dc1015340facc54fb9c13d4cb9a531996f37`.

Worktree: `/tmp/jackin-a14-t13-auth` on branch `wt/a14-t13-auth`.

## Scope

T13 now delivers `ANTHROPIC_AUTH_TOKEN` through both `AgentTuple::credential_env` and the `t13-claude-kimi` gateway profile. The two existing expectations now require the same variable.

The new T13–T16 invariant checks the tuple catalog, parsed profile, and rendered settings. The new account-publish test checks the T13 provider record through `Session::sync_account` and the fake gateway. Both tests use synthetic fixtures. No settings renderer, shared placeholder, endpoint, other tuple, or availability state changed.

## Red and green proof

Every command ran from the worktree root. The focused new-test command was:

```sh
rtk env NEXTEST_TEST_THREADS=2 mise exec -- mbx +1.99.1 nextest run -p jackin-session --test plan --test accounts -E 'test(claude_compatible_tuples_agree_on_auth_token_profile_and_settings) | test(a_kimi_file_key_sync_for_claude_code_creates_the_t13_auth_token_provider)' --no-fail-fast
```

The unfixed production code exited 100. Nextest run `b950481c-1f9e-4763-a476-ab73707dedcf` ran 2 tests with 0 passed and 2 failed. The invariant reported `Some("ANTHROPIC_API_KEY")` instead of `Some("ANTHROPIC_AUTH_TOKEN")`. Account publishing reported `"ANTHROPIC_API_KEY"` instead of `"ANTHROPIC_AUTH_TOKEN"`. The log is `/tmp/a14-t13-red-unfixed.log`.

After both production edits, the same command exited 0. Nextest run `069efc73-6b07-4fd5-8d22-877a881ff381` ran 2 tests with 2 passed. The log is `/tmp/a14-t13-green-fixed.log`.

Restoring `ANTHROPIC_API_KEY` only in `agent.rs` exited 100. Nextest run `67220ad8-fae8-4a30-bc5b-39d62845a0b3` ran 2 tests with 0 passed and 2 failed. Both assertions reported the API-key value instead of the auth-token value. The log is `/tmp/a14-t13-red-agent-only-reverted.log`.

With `agent.rs` fixed, restoring `ANTHROPIC_API_KEY` only in `t13-claude-kimi.yaml` exited 100. Nextest run `e6d11f7c-7038-46b2-b8f6-cee970181d26` ran 2 tests with 0 passed and 2 failed. The invariant failed its profile declaration assertion at `claude.rs:94`. Account publishing failed its profile declaration assertion at `sync.rs:407`. The log is `/tmp/a14-t13-red-yaml-only-reverted.log`.

With both production halves fixed, replacing tuple credential selection with only `agent.spec().credential_env_names.first().copied()` exited 100. Nextest run `1157af86-370b-4291-82e9-d4cb7bb33fb9` ran 2 tests with 1 passed and 1 failed. The catalog invariant passed. Account publishing reported `"ANTHROPIC_API_KEY"` instead of `"ANTHROPIC_AUTH_TOKEN"`. The log is `/tmp/a14-t13-red-account-ignore-tuple.log`.

I restored the account publisher and ran `rtk mise exec -- mbx +1.99.1 fmt`. `git diff --check` passed. The final focused command was:

```sh
rtk env NEXTEST_TEST_THREADS=2 mise exec -- mbx +1.99.1 nextest run -p jackin-session -p jackin-openshell -E 'test(claude_compatible_tuples_agree_on_auth_token_profile_and_settings) | test(a_kimi_file_key_sync_for_claude_code_creates_the_t13_auth_token_provider) | test(claude_config_pins_only_the_base_url_and_no_auth_env) | test(every_tuple_profile_parses_with_exact_asset_hosts_env_binary_and_refresh)' --no-fail-fast
```

It exited 0. Nextest run `bc7737d3-15ce-41bc-bbe1-be53e83f5c27` ran 4 tests with 4 passed and 762 skipped. The log is `/tmp/a14-t13-green-final-focused.log`.

## G-FAST

The full gate ran with this command:

```sh
rtk env NEXTEST_TEST_THREADS=2 mise exec -- mbx +1.99.1 xtask ci --fast
```

It exited 0 after all 14 steps passed. Nextest run `ca67b780-08aa-45fb-8261-df2b6f1e3b98` ran 2,236 tests with 2,236 passed and 0 skipped in 1,176.894 seconds. The `fmt`, `clippy`, `nextest`, `deny`, `shear`, `comments`, `layers`, `pins`, `containers`, `errors`, `baseline`, `screen`, `demo-gate`, and `reuse` steps passed. The log is `/tmp/a14-t13-gfast.log`.

## Remaining proof

A different independent reviewer must verify the change and its red/green evidence before the live re-proof.

The supervised T13 live proof remains. It must use `kimi-for-coding` on both `jackin-docker` and `jackin-local`. Each run must show `ANTHROPIC_AUTH_TOKEN` without `ANTHROPIC_API_KEY`, retain only the Kimi base URL and provider endpoint, and succeed through the actual interactive PTY with the exact model. Claude must remain a direct child of `jackin-start` with supervisor state `running 1`. Two detach and reattach cycles, replay, and shell entry and exit must preserve that child. The existing secret, provider-only egress, tool-use, export, and guarded-purge checks remain required. Print-mode success does not satisfy this proof.
