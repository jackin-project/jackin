# A14 OAuth implementation red/green record

## Approved plans

The T03 review and final approval are preserved verbatim in `.audit/a14-oauth-t03-review.log` and `.audit/a14-oauth-t03-final-approval.log`. The T02 approval is preserved verbatim in `.audit/a14-oauth-t02-approval.log`.

## Local controls and fixed greens

The baseline command was:

```sh
rtk mise exec -- mbx +1.99.1 nextest run -p jackin-core --test agent_table --test catalog -p jackin-session --test accounts -p jackin-openshell --test gateway --test tuple_policy
```

The exact output is in `.audit/a14-oauth-baseline-nextest.log`. It ran 193 tests and passed 193 tests.

The same command after implementation ran 199 tests and passed 199 tests. The exact output is in `.audit/a14-oauth-targeted-nextest.log`.

The G-FAST command was:

```sh
rtk mise exec -- mbx +1.99.1 xtask ci --fast
```

Its exact output is in `.audit/a14-oauth-g-fast.log`. Rustfmt and clippy passed. Nextest stopped after 952 tests with 950 passes and two failures in `launch/omp_models.rs`. Both failures expect `https://api.z.ai/api/coding/paas/v4`, while `HEAD` and the current code render `https://api.z.ai/api/anthropic`. The test file and the T09 renderer are unchanged by this unit.

The remaining CI steps passed with this command:

```sh
rtk mise exec -- mbx +1.99.1 xtask ci --only deny,shear,comments,layers,pins,containers,errors,baseline,screen,demo-gate,reuse
```

The exact output is in `.audit/a14-oauth-remaining-gates.log`.

The separate planner control exposed three more assertions from the same pre-existing ZAI URL mismatch. The final run excluded the `omp_models` group and `omp_config_sets_the_exact_plan_base_url_per_tuple`, then passed 94 tests with six filtered out. Its exact output is in `.audit/a14-oauth-planner-nextest.log`. The complete `jackin-start` control passed 16 tests. Its exact output is in `.audit/a14-oauth-image-nextest.log`.

## Local development failures

These failures were implementation observations, not independent review results.

- The first build rejected an unnecessary `std::collections::BTreeMap` qualification in `crates/jackin-openshell/src/contract.rs`. The fixture now uses the existing import.
- The OAuth-to-static test failed with `left: 2` and `right: 1` because the provider retained the old account credential after refresh deletion. `update_provider` now reads the current credential keys and sends empty values for keys absent from the desired state, which is OpenShell's deletion contract. The fixed test passes.
- The first stale-key implementation failed with Rust error `E0502`. Collecting the removed keys before extending the desired map fixed the borrow conflict.
- Two build attempts found import and formatting lints in migrated fixtures. Those fixtures now compile and pass rustfmt.

## Independent review status

Independent old-code and wrong-direction reds and fixed greens are pending. This record does not claim independent verification or approval to retain the new tests under the repository test protocol.
