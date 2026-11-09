# A11b account hardening evidence

## Approved test plan

The original requested adjustments are preserved in `.audit/a11b-test-plan-required-adjustments.log`.
Agent 46's final approval is preserved in `.audit/a11b-test-plan-approved.log`.

## Implementation

`crates/jackin-session/src/accounts.rs` enumerates `jackin_core::TUPLES` for the account provider and filters tuples with `Account::has_provider_for`. Account removal deletes every matching provider on every target gateway. Both rollback paths query all providers before deletion and journal one agent-specific restore for every held record.

`Action::Remove` deletes only `ProviderName::for_account(account.name(), agent)`. `Session::sync_provider` passes its exact provider to the launch failure cleanup. Neither cleanup path calls account-wide deletion.

`HostPorts` carries the parsed `HostPath` home. `Locations`, the production constructor, and the test constructors pass that value to OmpHost validation, synchronization, and launch. `World::seed_omp_agent_db` creates the real `auth_credentials` schema under `World.home`.

The OmpHost tests run child test processes with a temporary ambient home. They use fixture keys and boolean comparisons. They do not mutate shared process environment state or write the real home directory.

## Current-code baseline

Command:

```text
rtk mise exec -- mbx +1.99.1 nextest run -p jackin-session --test accounts --test launch -E 'test(zai) | test(exact_undo) | test(omp_host) | test(failed_codex_provider)'
```

The log is `.audit/a11b-current-session.log`. It ran eight tests. Two rollback tests passed. Multi-gateway removal, exact account-edit undo, exact launch cleanup, OmpHost synchronization, and OmpHost launch failed as predicted. The initial missing-provider negative case also failed because it had not yet received its isolated ambient fixture. That result is not used as red proof. The final test uses the isolated fixture and checks the typed errors.

Command:

```text
rtk mise exec -- mbx +1.99.1 nextest run -p jackin --test cli -E 'test(omp_host_account_add)'
```

The log is `.audit/a11b-current-cli.log`. The OmpHost CLI test failed because publication selected the ambient-home key instead of the World-home key.

## Mutation proofs

For the broad-delete/single-journal mutant, both journal loops used `held.into_iter().take(1)` while `forget_login` deleted the full catalog. The mutant was reverted after the run.

```text
rtk mise exec -- mbx +1.99.1 nextest run -p jackin-session --test accounts -E 'test(a_failed_config_write_after_account_removal_restores_every_original_zai_provider) | test(a_failed_config_write_after_gateway_removal_restores_every_original_zai_provider)'
```

The log is `.audit/a11b-mutant-single-journal.log`. Both rollback tests failed because the Codex record was absent after rollback.

For the account-wide cleanup mutant, `Action::Remove` and `Session::forget_locked` both called `forget_login`. The mutant was reverted after the run.

```text
rtk mise exec -- mbx +1.99.1 nextest run -p jackin-session --test accounts --test launch -E 'test(exact_undo_removes_only_the_codex_provider_created_by_the_failed_account_edit) | test(failed_codex_provider_publication_cleans_up_only_codex_and_preserves_sibling_providers)'
```

The log is `.audit/a11b-mutant-account-wide-cleanup.log`. Both exact-cleanup tests failed because the original Omp record was removed.

These runs are implementation evidence, not independent verification. The independent reviewer owns the final red/green verdict.

## Fixed targeted checks

The initial fixed session selection passed all eight tests. The log is `.audit/a11b-fixed-session-initial.log`.
The initial fixed CLI selection passed. The log is `.audit/a11b-fixed-cli-initial.log`.

Command:

```text
rtk mise exec -- mbx +1.99.1 nextest run -p jackin --test cli account
```

The log is `.audit/a11b-targeted-cli.log`. All 25 selected tests passed.

Command:

```text
rtk mise exec -- mbx +1.99.1 nextest run -p jackin-session --test accounts --test launch --test plan --no-fail-fast
```

The log is `.audit/a11b-targeted-session-all.log`. The run completed all 291 tests. It passed 286 and failed five existing Omp golden/config tests:

- `launch omp_models::a_non_lead_omp_instance_gets_the_lead_models_file_and_a_home_copy`
- `launch omp_models::an_omp_lead_launch_gets_the_lead_models_file_and_no_home_copy`
- `plan omp::omp_config_sets_the_exact_plan_base_url_per_tuple`
- `plan omp_models::omp_config_and_auth_goldens_stay_unchanged_for_every_omp_tuple`
- `plan omp_models::omp_models_yaml_pins_one_provider_the_env_named_key_and_one_model_for_custom_tuples`

Their expected Omp endpoint and configuration text differs from the existing generated text. The A11b diff does not change Omp model generation or those fixtures. The first fail-fast run is preserved in `.audit/a11b-targeted-session.log`.


The session examples pass this command after correcting the two example constructors. The log is `.audit/a11b-examples-check.log`.

```text
rtk mise exec -- mbx +1.99.1 check -p jackin-session --examples
```

The changed crates pass Clippy with warnings denied. The log is `.audit/a11b-clippy-targeted.log`.

```text
rtk mise exec -- mbx +1.99.1 clippy -p jackin-session -p jackin-testkit -p jackin --all-targets --all-features --locked -- -D warnings
```


## Fast CI

The required command was run from the worktree root.

```text
rtk mise exec -- mbx +1.99.1 xtask ci --fast
```

The first run stopped on an example constructor type error. The second run stopped on new-test Clippy errors. Both were fixed. A later run reached the workspace tests but exceeded the tool's five-minute timeout after 470 passing tests. That log is `.audit/a11b-ci-fast-timeout.log`.

The final untimed run is `.audit/a11b-ci-fast.log`. Formatting and full workspace Clippy passed. Nextest reported 952 passing tests and the two known Omp launch golden failures before stopping. The A11b tests passed. The gate did not reach the remaining steps after the unrelated failures.


The four OmpHost tests passed again after the process-policy fixture change. The log is `.audit/a11b-omp-host-final.log`.

The remaining non-test CI steps passed with this command. The log is `.audit/a11b-ci-static.log`.

```text
rtk mise exec -- mbx +1.99.1 xtask ci --only deny,shear,comments,layers,pins,containers,errors,baseline,screen,demo-gate,reuse
```
