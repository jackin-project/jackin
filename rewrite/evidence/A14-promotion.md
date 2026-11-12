# A14 D29 catalog promotion

Date: 2026-11-10. Base commit: `d9411a2d72a3710a28675072adef1bcc64c07093`. Worktree: `/tmp/jackin-a14-promotion`. Branch: `wt/a14-promotion`.

## Independent verdicts

The independent test-plan reviewer agent-96 approved the promotion plan. The live-evidence reviewers returned these verdicts before the catalog changed.

| Tuples | Evidence | Independent verdict |
|---|---|---|
| T01 | `A14-t01-final-live.md` | VERIFIED in `/tmp/a14-t01-final-verification.md`, including the final local-rotation addendum |
| T10, T11, T12, T21 | `A14-omp-final-live.md` | VERIFIED by agent-91 |
| T15, T16, and the T13 supervised failure | `A14-claude-final-live.md` | VERIFIED by agent-93 |
| T17, T19, T20 | `A14-codex-final-live.md` | VERIFIED by agent-92 |
| T09, T14, T18 | `A14-zai-recheck.md` | VERIFIED by agent-90 |

No hermetic test claims live model behavior. This unit did not rerun a live proof or use the network in nextest.

## Exact catalog changes

T01, T10, T11, T12, T15, T16, T17, T19, T20, and T21 are now `Supported` with `reason == None`. T09, T14, and T18 remain supported controls with `glm-5.3`.

T13 remains `Unavailable` with this exact reason:

```text
supervised interactive Claude Code exited 1 after provider-only policy refused api.anthropic.com
```

T02 remains `BlockedVendor` with `vendor clarification pending`. T03, T04, T07, and T08 remain unproven. T05 retains its sandbox-keyring reason. T06 retains its missing-artifact reason. T22 remains `Unavailable` with `no API key on this host`.

The change removes the ten promoted blocker records. T13 retains `rewrite/blockers/T13-claude-kimi.md`, which names the exact reason and links `A14-claude-final-live.md`. Other blocker records are unchanged. The decision trail includes the newer rows through the independent account-integration review.

`crates/jackin-session/src/plan.rs` now uses the selected tuple's default model instead of the agent's base-spec default. `crates/jackin-tui/src/launch.rs` now propagates the selected lead tuple before account selection and uses the tuple default. These changes prevent provider-specific launches from falling back to `glm-5.3` or to the default Codex tuple.

## Behavioral coverage

The all-22 catalog matrix checks every ID, agent, provider, auth class, state, exact reason, allowed model, and default model through `TUPLES` and `lookup_tuple`. It also checks model case sensitivity, removed models, unknown tuples, provider membership, and spec agreement.

Planner tests cover all 13 supported tuples as leads and secondaries through `plan_launch` and `plan_multi`. They assert accounts, models, providers, labels, lead-first order, and layout. All nine remaining tuples retain exact lead and secondary refusals.

CLI tests launch every supported tuple with explicit and default models after proving an invalid model has zero effects. A three-agent launch combines T01, T10, and T15 in one sandbox and checks exact `agents.json`, labels, accounts, models, lead order, and provider resources. Every remaining tuple retains its early usage refusal.

Console tests check supported offers and the T02, T13, and T22 controls. T01 and T20 launch successfully. T13 and T22 fail with exact reasons and no effects. The Codex picker uses only the local ChatGPT login for T01 and never calls `op`. The usage projection keeps MiMo quota `Unsupported` separate from catalog availability.

The repository-record test checks all five evidence files, all ten removed blockers, all retained blockers, both-gateway PASS rows, both-gateway FAIL rows for T13, and the exact T13 blocker text.

## Old-catalog red

The tests ran before the catalog flip. The old table produced these behavioral failures with no live network.

| Suite | Nextest run ID | Result |
|---|---|---|
| Catalog | `e1aecc7f-4945-4482-be71-cf0d5b5904e9` | 2 passed, 1 failed, 2 not run after fail-fast |
| Planner | `4ec46196-1670-46ac-a6be-569a24b115e5` | 94 passed, 12 failed, 0 skipped |
| CLI | `9dad5e28-1ab4-49da-88d2-211ef9484d55` | 179 passed, 2 failed, 0 skipped |
| Console launch | `b0633d19-561d-4218-8d07-fbd61ada48ce` | 48 passed, 6 failed, 0 skipped |
| Console projection | `4af19172-14b2-442b-aed9-ea3c8f865150` | 219 passed, 1 failed, 0 skipped |
| T01 credentials | `f208bbb1-0cd2-474e-adad-32383d2815e4` | 0 passed, 3 failed, 109 filtered out |

The repository-record test passed before the flip, so its evidence package was complete before the behavior changed.

## Fixed green

The focused sequence completed with exit 0 after the catalog and request-routing fixes. It ran 571 tests with zero skipped tests.

| Suite | Result | Representative nextest run ID |
|---|---|---|
| Catalog | 5 passed | `a58e6f7d-bcd5-49a5-bd87-f1307b9eb5c2` |
| Planner | 107 passed | `67ddf3b0-efc3-4b2c-ae21-f5d72dba8ff1`; `9877f1c3-82f0-427a-94f0-69d0a0a50a53` |
| T01 credentials | 3 passed | `866cd087-5593-43cc-8ef7-70ec8c1107ee` |
| CLI | 181 passed | `e639a5ec-6dab-4a89-bbce-a310f06ba278` |
| Console launch | 54 passed | `2b442dff-68de-4576-9ae0-6bc28574d229` |
| Console projection | 220 passed | `18120933-30d5-4761-b23b-2d621b6c4467` |
| Repository records | 1 passed | `4bd25d7c-90f8-4df3-9874-800b37ad5f02` |

## Discriminating mutations

All 55 mutation groups produced behavioral test failures and were restored. No group relied on a compile failure. Every source, evidence, and blocker file matched its pre-mutation bytes after restoration.

| Mutant | Failing tests |
|---|---|
| `T01-unavailable` | `jackin-core::catalog all_22_catalog_rows_match_the_approved_matrix`<br>`jackin-session::plan availability::every_supported_tuple_plans_as_a_secondary_in_canonical_lead_first_layouts`<br>`jackin::cli load::every_supported_tuple_launches_with_explicit_and_default_models`<br>`jackin-session::plan availability::every_supported_tuple_plans_as_lead_with_its_exact_identity_and_default_model`<br>`jackin-tui::launch picker::tuple_offers_match_the_complete_supported_matrix_and_remaining_controls` |
| `T01-unavailable` restored | `.audit/a14-mutations/T01-unavailable-*.log` |
| `T01-wrong-reason` | `jackin-core::catalog all_22_catalog_rows_match_the_approved_matrix` |
| `T01-wrong-reason` restored | `.audit/a14-mutations/T01-wrong-reason-*.log` |
| `T10-unavailable` | `jackin-core::catalog all_22_catalog_rows_match_the_approved_matrix`<br>`jackin-session::plan availability::every_supported_tuple_plans_as_a_secondary_in_canonical_lead_first_layouts`<br>`jackin-session::plan availability::every_supported_tuple_plans_as_lead_with_its_exact_identity_and_default_model`<br>`jackin-tui::launch picker::tuple_offers_match_the_complete_supported_matrix_and_remaining_controls`<br>`jackin::cli load::every_supported_tuple_launches_with_explicit_and_default_models` |
| `T10-unavailable` restored | `.audit/a14-mutations/T10-unavailable-*.log` |
| `T10-wrong-reason` | `jackin-core::catalog all_22_catalog_rows_match_the_approved_matrix` |
| `T10-wrong-reason` restored | `.audit/a14-mutations/T10-wrong-reason-*.log` |
| `T11-unavailable` | `jackin-core::catalog all_22_catalog_rows_match_the_approved_matrix`<br>`jackin-session::plan availability::every_supported_tuple_plans_as_a_secondary_in_canonical_lead_first_layouts`<br>`jackin-session::plan availability::every_supported_tuple_plans_as_lead_with_its_exact_identity_and_default_model`<br>`jackin-tui::launch picker::tuple_offers_match_the_complete_supported_matrix_and_remaining_controls`<br>`jackin::cli load::every_supported_tuple_launches_with_explicit_and_default_models` |
| `T11-unavailable` restored | `.audit/a14-mutations/T11-unavailable-*.log` |
| `T11-wrong-reason` | `jackin-core::catalog all_22_catalog_rows_match_the_approved_matrix` |
| `T11-wrong-reason` restored | `.audit/a14-mutations/T11-wrong-reason-*.log` |
| `T12-unavailable` | `jackin-core::catalog all_22_catalog_rows_match_the_approved_matrix`<br>`jackin-session::plan availability::every_supported_tuple_plans_as_a_secondary_in_canonical_lead_first_layouts`<br>`jackin-session::plan availability::every_supported_tuple_plans_as_lead_with_its_exact_identity_and_default_model`<br>`jackin-tui::launch picker::tuple_offers_match_the_complete_supported_matrix_and_remaining_controls`<br>`jackin::cli load::every_supported_tuple_launches_with_explicit_and_default_models` |
| `T12-unavailable` restored | `.audit/a14-mutations/T12-unavailable-*.log` |
| `T12-wrong-reason` | `jackin-core::catalog all_22_catalog_rows_match_the_approved_matrix` |
| `T12-wrong-reason` restored | `.audit/a14-mutations/T12-wrong-reason-*.log` |
| `T15-unavailable` | `jackin-core::catalog all_22_catalog_rows_match_the_approved_matrix`<br>`jackin-session::plan availability::every_supported_tuple_plans_as_a_secondary_in_canonical_lead_first_layouts`<br>`jackin-session::plan availability::every_supported_tuple_plans_as_lead_with_its_exact_identity_and_default_model`<br>`jackin-tui::launch picker::tuple_offers_match_the_complete_supported_matrix_and_remaining_controls`<br>`jackin::cli load::every_supported_tuple_launches_with_explicit_and_default_models` |
| `T15-unavailable` restored | `.audit/a14-mutations/T15-unavailable-*.log` |
| `T15-wrong-reason` | `jackin-core::catalog all_22_catalog_rows_match_the_approved_matrix` |
| `T15-wrong-reason` restored | `.audit/a14-mutations/T15-wrong-reason-*.log` |
| `T16-unavailable` | `jackin-core::catalog all_22_catalog_rows_match_the_approved_matrix`<br>`jackin-session::plan availability::every_supported_tuple_plans_as_a_secondary_in_canonical_lead_first_layouts`<br>`jackin-session::plan availability::every_supported_tuple_plans_as_lead_with_its_exact_identity_and_default_model`<br>`jackin-tui::launch picker::tuple_offers_match_the_complete_supported_matrix_and_remaining_controls`<br>`jackin::cli load::every_supported_tuple_launches_with_explicit_and_default_models` |
| `T16-unavailable` restored | `.audit/a14-mutations/T16-unavailable-*.log` |
| `T16-wrong-reason` | `jackin-core::catalog all_22_catalog_rows_match_the_approved_matrix` |
| `T16-wrong-reason` restored | `.audit/a14-mutations/T16-wrong-reason-*.log` |
| `T17-unavailable` | `jackin-core::catalog all_22_catalog_rows_match_the_approved_matrix`<br>`jackin-session::plan availability::every_supported_tuple_plans_as_a_secondary_in_canonical_lead_first_layouts`<br>`jackin-session::plan availability::every_supported_tuple_plans_as_lead_with_its_exact_identity_and_default_model`<br>`jackin-tui::launch picker::tuple_offers_match_the_complete_supported_matrix_and_remaining_controls`<br>`jackin::cli load::every_supported_tuple_launches_with_explicit_and_default_models` |
| `T17-unavailable` restored | `.audit/a14-mutations/T17-unavailable-*.log` |
| `T17-wrong-reason` | `jackin-core::catalog all_22_catalog_rows_match_the_approved_matrix` |
| `T17-wrong-reason` restored | `.audit/a14-mutations/T17-wrong-reason-*.log` |
| `T19-unavailable` | `jackin-core::catalog all_22_catalog_rows_match_the_approved_matrix`<br>`jackin-session::plan availability::every_supported_tuple_plans_as_a_secondary_in_canonical_lead_first_layouts`<br>`jackin-session::plan availability::every_supported_tuple_plans_as_lead_with_its_exact_identity_and_default_model`<br>`jackin-tui::launch picker::tuple_offers_match_the_complete_supported_matrix_and_remaining_controls`<br>`jackin::cli load::every_supported_tuple_launches_with_explicit_and_default_models` |
| `T19-unavailable` restored | `.audit/a14-mutations/T19-unavailable-*.log` |
| `T19-wrong-reason` | `jackin-core::catalog all_22_catalog_rows_match_the_approved_matrix` |
| `T19-wrong-reason` restored | `.audit/a14-mutations/T19-wrong-reason-*.log` |
| `T20-unavailable` | `jackin-core::catalog all_22_catalog_rows_match_the_approved_matrix`<br>`jackin-session::plan availability::every_supported_tuple_plans_as_a_secondary_in_canonical_lead_first_layouts`<br>`jackin-session::plan availability::every_supported_tuple_plans_as_lead_with_its_exact_identity_and_default_model`<br>`jackin-tui::launch picker::tuple_offers_match_the_complete_supported_matrix_and_remaining_controls`<br>`jackin::cli load::every_supported_tuple_launches_with_explicit_and_default_models` |
| `T20-unavailable` restored | `.audit/a14-mutations/T20-unavailable-*.log` |
| `T20-wrong-reason` | `jackin-core::catalog all_22_catalog_rows_match_the_approved_matrix` |
| `T20-wrong-reason` restored | `.audit/a14-mutations/T20-wrong-reason-*.log` |
| `T21-unavailable` | `jackin-core::catalog all_22_catalog_rows_match_the_approved_matrix`<br>`jackin-session::plan availability::every_supported_tuple_plans_as_a_secondary_in_canonical_lead_first_layouts`<br>`jackin-session::plan availability::every_supported_tuple_plans_as_lead_with_its_exact_identity_and_default_model`<br>`jackin-tui::launch picker::tuple_offers_match_the_complete_supported_matrix_and_remaining_controls`<br>`jackin::cli load::every_supported_tuple_launches_with_explicit_and_default_models` |
| `T21-unavailable` restored | `.audit/a14-mutations/T21-unavailable-*.log` |
| `T21-wrong-reason` | `jackin-core::catalog all_22_catalog_rows_match_the_approved_matrix` |
| `T21-wrong-reason` restored | `.audit/a14-mutations/T21-wrong-reason-*.log` |
| `T02-wrong-reason` | `jackin-core::catalog all_22_catalog_rows_match_the_approved_matrix`<br>`jackin-session::plan availability::every_remaining_tuple_refuses_lead_launch_with_its_exact_typed_reason`<br>`jackin-session::plan availability::every_remaining_tuple_refuses_secondary_launch_and_multi_with_the_same_reason`<br>`jackin-tui::launch picker::blocked_vendor_agent_is_refused_before_any_stage` |
| `T02-wrong-reason` restored | `.audit/a14-mutations/T02-wrong-reason-*.log` |
| `T03-wrong-reason` | `jackin-core::catalog all_22_catalog_rows_match_the_approved_matrix`<br>`jackin-session::plan availability::every_remaining_tuple_refuses_lead_launch_with_its_exact_typed_reason`<br>`jackin-session::plan availability::every_remaining_tuple_refuses_secondary_launch_and_multi_with_the_same_reason` |
| `T03-wrong-reason` restored | `.audit/a14-mutations/T03-wrong-reason-*.log` |
| `T04-wrong-reason` | `jackin-core::catalog all_22_catalog_rows_match_the_approved_matrix`<br>`jackin-session::plan availability::every_remaining_tuple_refuses_lead_launch_with_its_exact_typed_reason`<br>`jackin-session::plan availability::every_remaining_tuple_refuses_secondary_launch_and_multi_with_the_same_reason` |
| `T04-wrong-reason` restored | `.audit/a14-mutations/T04-wrong-reason-*.log` |
| `T05-wrong-reason` | `jackin-core::catalog all_22_catalog_rows_match_the_approved_matrix`<br>`jackin-session::plan availability::every_remaining_tuple_refuses_lead_launch_with_its_exact_typed_reason`<br>`jackin-session::plan availability::every_remaining_tuple_refuses_secondary_launch_and_multi_with_the_same_reason` |
| `T05-wrong-reason` restored | `.audit/a14-mutations/T05-wrong-reason-*.log` |
| `T06-wrong-reason` | `jackin-core::catalog all_22_catalog_rows_match_the_approved_matrix`<br>`jackin-session::plan availability::every_remaining_tuple_refuses_lead_launch_with_its_exact_typed_reason`<br>`jackin-session::plan availability::every_remaining_tuple_refuses_secondary_launch_and_multi_with_the_same_reason` |
| `T06-wrong-reason` restored | `.audit/a14-mutations/T06-wrong-reason-*.log` |
| `T07-wrong-reason` | `jackin-core::catalog all_22_catalog_rows_match_the_approved_matrix`<br>`jackin-session::plan availability::every_remaining_tuple_refuses_lead_launch_with_its_exact_typed_reason`<br>`jackin-session::plan availability::every_remaining_tuple_refuses_secondary_launch_and_multi_with_the_same_reason` |
| `T07-wrong-reason` restored | `.audit/a14-mutations/T07-wrong-reason-*.log` |
| `T08-wrong-reason` | `jackin-core::catalog all_22_catalog_rows_match_the_approved_matrix`<br>`jackin-session::plan availability::every_remaining_tuple_refuses_lead_launch_with_its_exact_typed_reason`<br>`jackin-session::plan availability::every_remaining_tuple_refuses_secondary_launch_and_multi_with_the_same_reason` |
| `T08-wrong-reason` restored | `.audit/a14-mutations/T08-wrong-reason-*.log` |
| `T13-wrong-reason` | `jackin-core::catalog all_22_catalog_rows_match_the_approved_matrix`<br>`jackin-session::plan availability::every_remaining_tuple_refuses_lead_before_account_selection`<br>`jackin-session::plan availability::every_remaining_tuple_refuses_lead_launch_with_its_exact_typed_reason`<br>`jackin-session::plan availability::every_remaining_tuple_refuses_secondary_launch_and_multi_with_the_same_reason`<br>`jackin-tui::launch picker::t13_launch_refusal_has_the_exact_supervised_reason_and_no_effects` |
| `T13-wrong-reason` restored | `.audit/a14-mutations/T13-wrong-reason-*.log` |
| `T22-wrong-reason` | `jackin-core::catalog all_22_catalog_rows_match_the_approved_matrix`<br>`jackin-session::plan availability::every_remaining_tuple_refuses_lead_launch_with_its_exact_typed_reason`<br>`jackin-session::plan availability::every_remaining_tuple_refuses_secondary_launch_and_multi_with_the_same_reason`<br>`jackin-tui::launch picker::t22_launch_refusal_has_the_exact_missing_key_reason_and_no_effects` |
| `T22-wrong-reason` restored | `.audit/a14-mutations/T22-wrong-reason-*.log` |
| `T13-supported` | `jackin-core::catalog all_22_catalog_rows_match_the_approved_matrix`<br>`jackin-session::plan availability::every_remaining_tuple_refuses_lead_launch_with_its_exact_typed_reason`<br>`jackin::cli load::t13_load_refusal`<br>`jackin-tui::launch picker::t13_launch_refusal_has_the_exact_supervised_reason_and_no_effects` |
| `T13-supported` restored | `.audit/a14-mutations/T13-supported-*.log` |
| `T09-control-unavailable` | `jackin-core::catalog all_22_catalog_rows_match_the_approved_matrix`<br>`jackin-session::plan availability::every_supported_tuple_plans_as_a_secondary_in_canonical_lead_first_layouts`<br>`jackin-session::plan availability::every_supported_tuple_plans_as_lead_with_its_exact_identity_and_default_model`<br>`jackin-tui::launch picker::tuple_offers_match_the_complete_supported_matrix_and_remaining_controls`<br>`jackin::cli load::every_supported_tuple_launches_with_explicit_and_default_models` |
| `T09-control-unavailable` restored | `.audit/a14-mutations/T09-control-unavailable-*.log` |
| `T14-control-unavailable` | `jackin-core::catalog all_22_catalog_rows_match_the_approved_matrix`<br>`jackin-session::plan availability::every_supported_tuple_plans_as_a_secondary_in_canonical_lead_first_layouts`<br>`jackin-session::plan availability::every_supported_tuple_plans_as_lead_with_its_exact_identity_and_default_model`<br>`jackin-tui::launch picker::tuple_offers_match_the_complete_supported_matrix_and_remaining_controls`<br>`jackin::cli load::every_supported_tuple_launches_with_explicit_and_default_models` |
| `T14-control-unavailable` restored | `.audit/a14-mutations/T14-control-unavailable-*.log` |
| `T18-control-unavailable` | `jackin-core::catalog all_22_catalog_rows_match_the_approved_matrix`<br>`jackin-session::plan availability::every_supported_tuple_plans_as_a_secondary_in_canonical_lead_first_layouts`<br>`jackin-session::plan availability::every_supported_tuple_plans_as_lead_with_its_exact_identity_and_default_model`<br>`jackin-tui::launch picker::t18_console_launch_is_the_positive_control`<br>`jackin-tui::launch picker::tuple_offers_match_the_complete_supported_matrix_and_remaining_controls`<br>`jackin::cli load::every_supported_tuple_launches_with_explicit_and_default_models` |
| `T18-control-unavailable` restored | `.audit/a14-mutations/T18-control-unavailable-*.log` |
| `lead-support-guard-removed` | `jackin-session::plan availability::every_remaining_tuple_refuses_lead_before_account_selection` |
| `lead-support-guard-removed` restored | `.audit/a14-mutations/lead-support-guard-removed-*.log` |
| `secondary-support-guard-removed` | `jackin-session::plan availability::every_remaining_tuple_refuses_secondary_launch_and_multi_with_the_same_reason` |
| `secondary-support-guard-removed` restored | `.audit/a14-mutations/secondary-support-guard-removed-*.log` |
| `cli-support-guard-removed` | `jackin::cli load::t03_load_refusal`<br>`jackin::cli load::t02_load_refusal`<br>`jackin::cli load::t04_load_refusal`<br>`jackin::cli load::t05_load_refusal`<br>`jackin::cli load::t06_load_refusal`<br>`jackin::cli load::t07_load_refusal`<br>`jackin::cli load::t08_load_refusal`<br>`jackin::cli load::t13_load_refusal`<br>`jackin::cli load::t22_load_refusal` |
| `cli-support-guard-removed` restored | `.audit/a14-mutations/cli-support-guard-removed-*.log` |
| `T11-model-mutated` | `jackin-core::catalog model_matching_is_case_sensitive_and_rejects_removed_models`<br>`jackin-session::plan availability::every_supported_tuple_plans_as_lead_with_its_exact_identity_and_default_model` |
| `T11-model-mutated` restored | `.audit/a14-mutations/T11-model-mutated-*.log` |
| `T15-model-mutated` | `jackin-core::catalog model_matching_is_case_sensitive_and_rejects_removed_models`<br>`jackin-session::plan availability::every_supported_tuple_plans_as_lead_with_its_exact_identity_and_default_model` |
| `T15-model-mutated` restored | `.audit/a14-mutations/T15-model-mutated-*.log` |
| `T19-model-mutated` | `jackin-core::catalog model_matching_is_case_sensitive_and_rejects_removed_models`<br>`jackin-session::plan availability::every_supported_tuple_plans_as_lead_with_its_exact_identity_and_default_model` |
| `T19-model-mutated` restored | `.audit/a14-mutations/T19-model-mutated-*.log` |
| `T21-model-mutated` | `jackin-core::catalog model_matching_is_case_sensitive_and_rejects_removed_models`<br>`jackin-session::plan availability::every_supported_tuple_plans_as_lead_with_its_exact_identity_and_default_model` |
| `T21-model-mutated` restored | `.audit/a14-mutations/T21-model-mutated-*.log` |
| `evidence-A14-t01-final-live.md-truncated` | `jackin-xtask::repo_gates promotion_records::a14_evidence_and_blocker_records_are_consistent` |
| `evidence-A14-t01-final-live.md-truncated` restored | `.audit/a14-mutations/evidence-A14-t01-final-live.md-truncated-*.log` |
| `evidence-A14-omp-final-live.md-truncated` | `jackin-xtask::repo_gates promotion_records::a14_evidence_and_blocker_records_are_consistent` |
| `evidence-A14-omp-final-live.md-truncated` restored | `.audit/a14-mutations/evidence-A14-omp-final-live.md-truncated-*.log` |
| `evidence-A14-claude-final-live.md-truncated` | `jackin-xtask::repo_gates promotion_records::a14_evidence_and_blocker_records_are_consistent` |
| `evidence-A14-claude-final-live.md-truncated` restored | `.audit/a14-mutations/evidence-A14-claude-final-live.md-truncated-*.log` |
| `evidence-A14-codex-final-live.md-truncated` | `jackin-xtask::repo_gates promotion_records::a14_evidence_and_blocker_records_are_consistent` |
| `evidence-A14-codex-final-live.md-truncated` restored | `.audit/a14-mutations/evidence-A14-codex-final-live.md-truncated-*.log` |
| `evidence-A14-zai-recheck.md-truncated` | `jackin-xtask::repo_gates promotion_records::a14_evidence_and_blocker_records_are_consistent` |
| `evidence-A14-zai-recheck.md-truncated` restored | `.audit/a14-mutations/evidence-A14-zai-recheck.md-truncated-*.log` |
| `blocker-T01-codex.md-reintroduced` | `jackin-xtask::repo_gates promotion_records::a14_evidence_and_blocker_records_are_consistent` |
| `blocker-T01-codex.md-reintroduced` restored | `.audit/a14-mutations/blocker-T01-codex.md-reintroduced-*.log` |
| `blocker-T10-omp-kimi.md-reintroduced` | `jackin-xtask::repo_gates promotion_records::a14_evidence_and_blocker_records_are_consistent` |
| `blocker-T10-omp-kimi.md-reintroduced` restored | `.audit/a14-mutations/blocker-T10-omp-kimi.md-reintroduced-*.log` |
| `blocker-T11-omp-minimax.md-reintroduced` | `jackin-xtask::repo_gates promotion_records::a14_evidence_and_blocker_records_are_consistent` |
| `blocker-T11-omp-minimax.md-reintroduced` restored | `.audit/a14-mutations/blocker-T11-omp-minimax.md-reintroduced-*.log` |
| `blocker-T12-omp-mimo.md-reintroduced` | `jackin-xtask::repo_gates promotion_records::a14_evidence_and_blocker_records_are_consistent` |
| `blocker-T12-omp-mimo.md-reintroduced` restored | `.audit/a14-mutations/blocker-T12-omp-mimo.md-reintroduced-*.log` |
| `blocker-T15-claude-minimax.md-reintroduced` | `jackin-xtask::repo_gates promotion_records::a14_evidence_and_blocker_records_are_consistent` |
| `blocker-T15-claude-minimax.md-reintroduced` restored | `.audit/a14-mutations/blocker-T15-claude-minimax.md-reintroduced-*.log` |
| `blocker-T16-claude-mimo.md-reintroduced` | `jackin-xtask::repo_gates promotion_records::a14_evidence_and_blocker_records_are_consistent` |
| `blocker-T16-claude-mimo.md-reintroduced` restored | `.audit/a14-mutations/blocker-T16-claude-mimo.md-reintroduced-*.log` |
| `blocker-T17-codex-kimi.md-reintroduced` | `jackin-xtask::repo_gates promotion_records::a14_evidence_and_blocker_records_are_consistent` |
| `blocker-T17-codex-kimi.md-reintroduced` restored | `.audit/a14-mutations/blocker-T17-codex-kimi.md-reintroduced-*.log` |
| `blocker-T19-codex-minimax.md-reintroduced` | `jackin-xtask::repo_gates promotion_records::a14_evidence_and_blocker_records_are_consistent` |
| `blocker-T19-codex-minimax.md-reintroduced` restored | `.audit/a14-mutations/blocker-T19-codex-minimax.md-reintroduced-*.log` |
| `blocker-T20-codex-mimo.md-reintroduced` | `jackin-xtask::repo_gates promotion_records::a14_evidence_and_blocker_records_are_consistent` |
| `blocker-T20-codex-mimo.md-reintroduced` restored | `.audit/a14-mutations/blocker-T20-codex-mimo.md-reintroduced-*.log` |
| `blocker-T21-omp-anthropic.md-reintroduced` | `jackin-xtask::repo_gates promotion_records::a14_evidence_and_blocker_records_are_consistent` |
| `blocker-T21-omp-anthropic.md-reintroduced` restored | `.audit/a14-mutations/blocker-T21-omp-anthropic.md-reintroduced-*.log` |


## Final gate

The final command ran from `/tmp/jackin-a14-promotion` with `NEXTEST_TEST_THREADS=2`:

```sh
rtk env NEXTEST_TEST_THREADS=2 mise exec -- mbx +1.99.1 xtask ci --fast
```

Nextest run `bdac0e4e-19d5-4c89-8a64-98d7aad59c8b` passed all 2,234 tests with zero skipped tests in 1,640.928 seconds. The full command exited 0 after 1,774.119 seconds.

All 14 gate steps passed: `fmt`, `clippy`, `nextest`, `deny`, `shear`, `comments`, `layers`, `pins`, `containers`, `errors`, `baseline`, `screen`, `demo-gate`, and `reuse`. Deny reported advisories, bans, licenses, and sources OK. Shear found no issues. REUSE reported 1,323 files with copyright and license information and zero errors.

No mutation remains applied. No live proof ran for this unit.


## Independent review follow-up

The independent reviewer returned CHANGES_REQUIRED in `/tmp/a14-promotion-review.md`. `tuple_offers` discarded `tuple.reason` whenever the tuple was `Supported`, so an invalid supported row could not fail the existing console assertion.

The follow-up changes only `crates/jackin-tui/src/project.rs` and `crates/jackin-tui/tests/launch/picker.rs`. Offer projection now propagates `tuple.reason` independently of state. Valid supported rows still produce `None`, and invalid supported rows expose their reason.

Before the projection fix, nextest run `cfc0bae9-68f4-47ab-9891-460b5588df26` reproduced the review finding. The T01 catalog assertion failed on `Some("incorrect mutation reason")`, while the console offer assertion incorrectly passed.

After the projection fix, nextest run `61a678c8-7c78-4cf9-a491-157df9a7868f` failed both required assertions with the same wrong reason. Restoring the valid catalog produced green run `3368a028-f394-42ba-bfd3-43c1594263c3` with four passes. That run included the new permanent T20 blank-model console launch and the T18 positive control. The offer assertion covers T09, T14, and T18.

The blank-model launch returns `LaunchEvent::Ready` and writes exact Codex, Mimo, `mimo`, and `mimo-v2.6-pro` identity into the planned labels. The account and provider are explicit. No baseline visual changed.

Focused suites passed 55 launch tests in run `341cde6c-733a-43a1-910c-55d1dd8f4f3b` and 220 console projection tests in run `0dae17bb-18d0-4f68-b737-3483514e5b06`, with zero skipped tests in both suites.

A 30-minute task timeout interrupted the first follow-up gate. The final complete rerun used no task timeout and this command:

```sh
rtk env NEXTEST_TEST_THREADS=2 mise exec -- mbx +1.99.1 xtask ci --fast
```

Final nextest run `b8e70136-bebe-4c1c-8896-215235f70b58` passed all 2,235 tests with zero skipped tests in 1,565.012 seconds. The full command exited 0 after 1,599.190 seconds. All 14 gate steps passed: `fmt`, `clippy`, `nextest`, `deny`, `shear`, `comments`, `layers`, `pins`, `containers`, `errors`, `baseline`, `screen`, `demo-gate`, and `reuse`. REUSE reported 1,330 files with copyright and license information and zero errors.

No live proof ran for this follow-up.
