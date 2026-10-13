# Task 002 qualification decision

Read-only check of `/Users/donbeave/Projects/jackin-project/jackin`. No clone edit, commit, build, or `cargo` run. Symbols and tests below were re-read with `git grep` / `git show` on the named tips.

**Decision: PORT**

Port the predicate `green_claim_qualified` and its fail-closed exit `require_qualified` onto the policy push-head ledger. `validate_qualification_binding` and `validate_live_api_binding` move with that predicate. Leave the stash tree `d80503236a7b61a8aa2f21dae164693cbf1ed9d1` in place. This file does not approve a branch. This file does not approve a source edit.

`RUSTC_WRAPPER` and `SCCACHE_GHA_ENABLED` are absent from stash tip `c7b8a95856cce6f5e8442f0e6fafcf025a089132` (`git grep` returned no hits). This PORT does not restore either setting.

## Tips

| Tip | SHA | Subject |
| --- | --- | --- |
| Stash | `c7b8a95856cce6f5e8442f0e6fafcf025a089132` | `untracked files on codex/ci-evidence-repair-current-main-20260923: a65f5d75c fix(usage): route Claude API keys away from OAuth adapter (#1092)` |
| M main | `1c3dd1be4a3b1ed518133ec3899c084c792370ff` | current main |
| P policy | `fba69d60c2373156dc0dcecee8e085c83776966f` | policy |

The stash commit has zero parents. Its tree is `d80503236a7b61a8aa2f21dae164693cbf1ed9d1`. Qualification code on that tree lives under `crates/jackin-xtask/src/ci_evidence/`. Policy code lives under `crates/tools/jackin-xtask/src/ci_evidence/`.

`git grep` for `require_qualified`, `green_claim_qualified`, `validate_qualification_binding`, and `validate_live_api_binding` on M and on P returned no matches (exit 1). `green_claim_qualified` on the stash is a `bool` field, computed in `build_rollup`. The other three names are functions.

`14bd1471616056e07c81c59d59eafc0963b16865` has the same `rollup.rs` as the stash tip (`git diff` of that file is empty), so `green_claim_qualified` (`rollup.rs:119`) and `require_qualified` (`rollup.rs:151`) match. `validate_qualification_binding` is at `validation.rs:605` and returns after the push-head observation checks. `validate_live_api_binding` is absent on `14bd147` (`git grep` returned no matches). The stash function at `validation.rs:1041` is the binding to port: line 1063 calls `validate_live_api_binding`. The four qualification tests below are also on `14bd147`; the stash copies are the ones that sit on this stricter binding.

## Predicate to port

`require_qualified` in stash `crates/jackin-xtask/src/ci_evidence/rollup.rs:151`.

`rollup` writes the JSON and markdown, then returns that function:

```535:542:crates/jackin-xtask/src/ci_evidence.rs
fn rollup(args: RollupArgs) -> Result<()> {
    let evidence: EvidenceFile = read_json(&args.input)?;
    validate_evidence(&evidence)?;
    let summary = build_rollup(&evidence);
    write_json(&args.json, &summary)?;
    write_markdown(&args.markdown, &summary, &evidence)?;
    print_rollup_summary(&summary)?;
    require_qualified(&summary)
}
```

The boolean it enforces is `green_claim_qualified`. Missing obligations and cancelled first attempts increment `total_first_attempt_failures`. An empty expected set, a non-ledger denominator, a failed fetch, unclassified runs, or a failed binding also clear the boolean. `qualification_bound` is `validate_qualification_binding(...).is_ok()`, and that function calls `validate_live_api_binding`.

```106:124:crates/jackin-xtask/src/ci_evidence/rollup.rs
    let total_first_attempt_failures = cohorts
        .iter()
        .map(|cohort| {
            cohort.product
                + cohort.infrastructure
                + cohort.cancellation
                + cohort.missing
                + cohort.inapplicable
                + cohort.data_quality
        })
        .sum();
    let qualification_bound = docs::repo_root()
        .is_ok_and(|root| validate_qualification_binding(&root, evidence).is_ok());
    let green_claim_qualified = total_first_attempt_failures == 0
        && !evidence.expected.is_empty()
        && evidence.denominator.source == DenominatorSource::PushHeadLedger
        && evidence.denominator.fetch_succeeded
        && evidence.unclassified_runs.is_empty()
        && qualification_bound;
```

`green_claim_qualified` is stored on `RollupFile` at line 136. `require_qualified` is the exit:

```151:172:crates/jackin-xtask/src/ci_evidence/rollup.rs
fn require_qualified(rollup: &RollupFile) -> Result<()> {
    let mut reasons = Vec::new();
    if rollup.denominator.source != DenominatorSource::PushHeadLedger {
        reasons.push("denominator is not the durable push-head ledger");
    }
    if !rollup.denominator.fetch_succeeded {
        reasons.push("denominator fetch proof is incomplete");
    }
    if rollup.unclassified_runs != 0 {
        reasons.push("unclassified workflow runs are present");
    }
    if rollup.total_first_attempt_failures != 0 {
        reasons.push("first-attempt failures or missing obligations are present");
    }
    if !rollup.green_claim_qualified {
        reasons.push("green claim is not qualified (workflow/Git/artifact binding is absent)");
    }
    if reasons.is_empty() {
        Ok(())
    } else {
        bail!("CI evidence rollup is unqualified: {}", reasons.join(", "))
    }
}
```

Fail-closed cases:

| Case | Stash behavior |
| --- | --- |
| Missing | `OutcomeClass::Missing` increments `cohort.missing`, which is inside `total_first_attempt_failures`. A nonzero sum makes `require_qualified` return `Err` with `first-attempt failures or missing obligations are present`. An empty `evidence.expected` clears `green_claim_qualified` through `!evidence.expected.is_empty()`. |
| Cancelled | `classify_outcome("completed", Some("cancelled"), ...)` returns `OutcomeClass::Cancellation` (`tests.rs`, `outcome_classes_keep_platform_failures_separate`). `cohort.cancellation` is inside the same failure sum, so `require_qualified` returns `Err`. `OutcomeClass::is_failure` is true for every class other than `Success`. |
| Unqualified | Fixture source, incomplete fetch, local collection, or scheduled provenance whose workflow contract / live API binding fails makes `green_claim_qualified` false. `require_qualified` then returns `Err` with `green claim is not qualified (workflow/Git/artifact binding is absent)`. |

`validate_qualification_binding` (`validation.rs:1041`) requires scheduled `main` provenance on `DEFAULT_CI_EVIDENCE_WORKFLOW`, a collector `run_id`, the workflow contract, runtime identity, git state, and each push-head observation. It then calls `validate_live_api_binding` (`validation.rs:917`). That binding bails when `GITHUB_ACTIONS` is not `true`, when the collector run id is missing, or when a live re-fetch of the collector run, attempt runs, or push-head artifact disagrees with the stored evidence.

## Policy ledger gate

P `expected_from_push_head_ledger` (`crates/tools/jackin-xtask/src/ci_evidence/ledger.rs:42`) builds the denominator. It bails when the branch is not `main`, when the ledger workflow id set is empty, when the authoritative tip moves during fetch, or when no in-window ledger runs exist. A missing predecessor becomes `DenominatorBoundary::Bootstrap` when the first in-window observation exists (`ledger.rs:132-145`). On success it returns `Ok(ExpectedDenominator)` with `fetch_succeeded: true`.

That function does not inspect cohort cancellation, missing obligations, or a green-claim binding. After evidence exists, P `rollup` returns success:

```619:634:crates/tools/jackin-xtask/src/ci_evidence.rs
fn rollup(args: RollupArgs) -> Result<()> {
    write_rollup_files(args).map(|_| ())
}

fn write_rollup_files(args: RollupArgs) -> Result<RollupFile> {
    let root = docs::repo_root()?;
    let input = repository_output_path(&root, &args.input)?;
    let json = repository_output_path(&root, &args.json)?;
    let markdown = repository_output_path(&root, &args.markdown)?;
    let evidence: EvidenceFile = read_json(&input)?;
    validate_evidence(&evidence)?;
    let summary = build_rollup(&evidence);
    write_json(&json, &summary)?;
    write_markdown(&markdown, &summary, &evidence)?;
    print_rollup_summary(&summary)?;
    Ok(summary)
}
```

P `build_rollup` stores the same failure sum, including `cohort.cancellation` and `cohort.missing`, then sets the status string to advisory (`rollup.rs:106-128`). Markdown states that the observer makes no semantic CI or merge-readiness claim (`rollup.rs:158`). `print_rollup_summary` prints the failure count and `rollup.status` (`rollup.rs:225-234`) and returns `Ok(())`. `total_first_attempt_failures` is stored, printed, and asserted equal to `2` in `skipped_cohort_is_reported_by_the_advisory_observer`. No policy function returns `Err` from that count.

P tests that lock the advisory exit on the same situations:

| Policy test | Path | Assertion |
| --- | --- | --- |
| `skipped_cohort_is_reported_by_the_advisory_observer` | `tests/case_02.rs:248` | `total_first_attempt_failures == 2` and `status == "advisory"` |
| `fixture_denominator_remains_advisory` | `tests/case_04.rs:135` | `status == "advisory"` |
| `local_collection_remains_advisory` | `tests/case_04.rs:148` | `status == "advisory"` |
| `scheduled_provenance_is_reported_without_a_semantic_claim` | `tests/case_04.rs:165` | `status == "advisory"` |

`rollup_counts_missing_and_does_not_recode_cancelled_rerun` (`tests/case_03.rs:187`) already asserts `cancellation == 1` and `missing == 1` and does not call a qualification gate. The rollup command still returns `Ok`.

The policy push-head ledger therefore records missing and cancelled evidence and keeps the rollup advisory. It does not fail the rollup command for missing, cancelled, or unqualified evidence. REJECT does not apply.

## Tests that must move with the predicate

These four tests are on the stash at `crates/jackin-xtask/src/ci_evidence/tests.rs` and absent on M and P (`git grep` exit 1). Each calls `require_qualified(&rollup).is_err()`.

| Stash test | Line | What it locks |
| --- | --- | --- |
| `required_skipped_cohort_is_non_green` | 573 | Inapplicable first attempt, `total_first_attempt_failures == 2`, `green_claim_qualified == false`, `require_qualified` is `Err` |
| `fixture_denominator_cannot_qualify_green` | 1182 | `DenominatorSource::Fixture` and `fetch_succeeded == false` |
| `local_collection_cannot_qualify_green` | 1196 | Push-head denominator fields with no live qualification binding |
| `scheduled_provenance_without_current_workflow_contract_cannot_qualify_green` | 1214 | Scheduled provenance and successful attempt rows whose workflow contract is not current |

They land with `require_qualified` on the policy ledger modules. The four policy `*_remains_advisory` / `*_without_a_semantic_claim` / `skipped_cohort_is_reported_by_the_advisory_observer` tests cover those same fixtures and currently assert `status == "advisory"`. Those assertions cannot remain the qualification proof once `require_qualified` is the exit.

`rollup_counts_missing_and_does_not_recode_cancelled_rerun` and `outcome_classes_keep_platform_failures_separate` already exist on P. They stay. The missing/cancelled fixture must also assert `require_qualified(&rollup).is_err()`, because `cancellation` and `missing` are inputs to the predicate and that test is the one that builds both counts.

Policy records those outcomes and still returns `Ok` with `status: "advisory"`. It does not fail closed on missing, cancelled, or unqualified evidence, so the decision is PORT. This record does not approve a branch and does not approve a source edit. Do not restore global `RUSTC_WRAPPER` or `SCCACHE_GHA_ENABLED`.
