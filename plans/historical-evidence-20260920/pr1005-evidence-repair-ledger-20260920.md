# Requirement-to-evidence ledger

Proof levels: `implemented` < `fixture_verified` < `container_verified` <
`live_verified`. Also: `failed`, `unavailable` (credentials), `unsupported`
(genuinely, with evidence), `not_run`.

## Evidence audit and live-head recheck — 2026-09-20

PR #1005 is `audit/pr1002-evidence`; the prior audit started at
`cefa882f6c3f5a3b4f0ce52a2052a05cb4706756` (`cefa882f`) and was pushed as the
signed repair commit `128aaa4d53a861b0ce5175d2f4d775c4b7cc088f` (`128aaa4d`).
This follow-up repair started from PR #1005 head
`ebf74abe5ed89cebfc604b21e613d32120969e3c` (`ebf74abe`). The
original PR #1005 evidence baseline used #1002 source
`ca128f8a80907319ea6d5648cf172ed81e33b1d4` (`ca128f8`). The pre-sync #1002
coordinator snapshot used by the prior recheck was
`72feee236162468aeeeb416e8139a97db06538eb` (`72feee23`). Current #1002 head at
this audit is `3218706cf2b992ceeb20ca2d0ea280d4c6eae3d9` (`3218706c`), 27
commits beyond that snapshot. A normal merge of current #1002 into this branch
was performed in merge commit
`798ba7de1d86144c8a474a252ec0d7c28f08b886` (`798ba7de`); no rebase or force-push
was used. The `ca128f8` and `72feee23` runs below remain historical pre-sync
snapshots and must be refreshed again if #1002 advances. The earlier audit starting #1005 head was
`33810907904ccc4b0858e9479d71da54c8df0719` (`3381090`), whose tree differed from
the `ca128f8` source only in these two ledger files. The audit worktree is
`/private/tmp/jackin-pr1002-evidence` on `audit/pr1002-evidence`.

The prior GitHub review query for #1002 at source head `a73743ab` is historical;
its empty review state and mixed hosted checks are not substituted for current
evidence at `3218706c`. This repair separately queried PR #1005 at starting head
`ebf74abe`: the four REST inline findings each had a matching unresolved,
outdated GraphQL thread, and the formal review was `COMMENTED`; no additional
review finding was found. No live provider call, container run, GUI/keychain
run, or native-client comparison was performed here. Those surfaces remain
`not_run`.

The live #1002 PR description still contains older synchronized-gate claims of
`563 passed` and `1055 passed, 1 skipped`; those are historical PR-body values,
not current evidence for #1002 head `3218706c` and do not supersede the
pre-sync labels below.

The historical `/tmp` artifacts named by this document are absent in the audit
filesystem, including `/tmp/review-providers.md`, `/tmp/lane-*.md`,
`/tmp/tracer/evidence.md`, `/tmp/split.log`, and
`/tmp/provider-catalog-ledger.md`. Historical claims that cite them are kept as
historical claims only; this audit does not re-issue their `container_verified`
or `live_verified` labels.

Evidence runs and their source heads, all observed on 2026-09-20 unless stated:

- PR #1005 baseline `AUDIT-F`: `cargo nextest run -p jackin-config -p jackin-env -p jackin-protocol -p jackin-usage --all-features` at `ca128f8a80907319ea6d5648cf172ed81e33b1d4` — **1059 passed, 1 skipped**, exit 0 (5 binaries, 16.084 seconds). This is the source baseline carried by the unsynchronized PR.
- PR #1005 baseline config/instance fixture gate: `cargo nextest run -p jackin-instance -p jackin-config --all-features` at `ca128f8a80907319ea6d5648cf172ed81e33b1d4` — **567 passed**, exit 0 (2 binaries, 21.135 seconds). Counts are per command and overlap on `jackin-config`; they are not additive.
- PR #1005 baseline broad fixture/console gate: `cargo nextest run -p jackin-runtime -p jackin-console -p jackin-capsule -p jackin-instance -p jackin-core -p jackin --all-features` at `ca128f8a80907319ea6d5648cf172ed81e33b1d4` — **3836 passed**, exit 0 (41 binaries, 200.269 seconds). This is separate from historical `AUDIT-R` below.
- Pre-sync #1002 coordinator `COORD-F`: the same five-package fixture command at `72feee236162468aeeeb416e8139a97db06538eb` — **1069 passed, 1 skipped**, exit 0 (5 binaries, 20.460 seconds). Not a current `3218706c` result.
- Pre-sync #1002 coordinator config/instance recheck: the two-package fixture command at `72feee236162468aeeeb416e8139a97db06538eb` — **577 passed**, exit 0 (2 binaries, 23.414 seconds). Not a current `3218706c` result.
- Pre-sync #1002 coordinator `COORD-R`: the broad fixture/console command at `72feee236162468aeeeb416e8139a97db06538eb` — **3839 passed, 1 skipped**, exit 0 (41 binaries, 140.370 seconds). Not a current `3218706c` result.
- Current post-merge audit-tree MDX build: `cd docs && bun run build` — MDX/Vite compilation completed, but static prerender failed with `ECONNRESET` while requesting `/og/research/context/techniques/09-output-discipline.webp`; exit 1. A prior same-tree attempt failed with `ConnectionRefused` on the same generated route. No live or provider evidence is implied.
- Current post-merge audit-tree type gate: `cd docs && bun run types:check` — **pass** (`fumadocs-mdx` generated files; `tsc --noEmit` reported no errors).
- Current post-merge audit-tree docs tests: `cd docs && bun test` — **18 passed, 0 failed** across 2 files, exit 0.
- Current post-merge roadmap gate: `cargo xtask roadmap audit` — **pass**, 18 `meta.json` files resolved.
- Current post-merge research gate: `cargo xtask research check` — **pass**, 63 `meta.json` files resolved.
- Current repository-link gate before this repair: `cargo xtask docs repo-links` — **failed** on two pre-existing references to `.github/workflows/preview.yml` in `docs/content/roadmap/(isolation-security)/security-threat-model-and-signed-releases.mdx:22,64`; the repair wraps both existing paths in `<RepoFile>` without changing the workflow.
- Current post-merge repository-link gate: `cargo xtask docs repo-links` — **pass**, after both existing `preview.yml` paths were wrapped in `<RepoFile>`.
- Historical pre-sync workflow policy gate: `velnor-workflow policy --workflow-root . --head-sha cefa882f6c3f5a3b4f0ce52a2052a05cb4706756 --base-sha ca128f8a80907319ea6d5648cf172ed81e33b1d4 --ruleset-contexts DCO,Policy,ci-required` with validator pin `0dc79895ff1c5e88be7c3822c437e1c5b5282e12` — **11 rules, 0 failed**. This is not a current `3218706c` or final-PR-head gate.

The previous synchronized audit at `176dcc0632f977a78d58443bde1b3ceb40304606`
reported **1057 passed, 1 skipped** for its fixture command. That is stale
176-era evidence, not a current #1002 result; the pre-sync `72feee23` count above
supersedes it only for that historical snapshot. No current `3218706c` count is
claimed here. The earlier aggregate at `1c8b99077a5fe82cbfd19e6b5803a0a2db67d8ba` remains
historical `AUDIT-R`: **3826 passed, 1 skipped, 1 timed out**, exit 100, with
the focused PNG rerun **1 passed**. Its run date was not retained.

Review disposition for the current #1005 review surfaces: the four active review
findings on this PR are reflected as open evidence gaps below. D13 is not
marked implemented without Docker argv/inspect/image-history evidence; F22 is
not marked implemented without OpenRouter history coverage; F28 is not marked
implemented without usage-event fixtures; and Antigravity live GUI/keyring
support remains `not_run`, not `unsupported`. No code behavior is claimed from
these documentation-only updates.

## PR #1005 review gate — four P2 evidence threads

The REST review-comment ids and GraphQL thread ids below were read from live PR
#1005 before this repair. All four GraphQL threads were `isResolved: false` and
`isOutdated: true`, and all targeted the earlier reviewed commit
`77e2473a8abc0b892ce5cbf63326ff8ff9ffb567`. The comments were checked against
the current ledger rows and current fixture evidence; none authorizes a stronger
proof label.

| Review comment / thread | Current evidence check | Concrete disposition |
|---|---|---|
| REST `4055299925` / GraphQL `PRRT_kwDOR2N26c6kE6Ic` — D13 | D13's cited runtime/capsule tests cover serialized config, host-only env-file cleanup, and PTY output; no Docker argv, labels, image history, `Config.Env`, staged-mount inventory, or target-container artifact exists. | Keep `fixture_verified` / `not_run`; Docker leakage evidence is explicitly unverified. No container-security claim is closed. |
| REST `4055299929` / GraphQL `PRRT_kwDOR2N26c6kE6Ih` — F22 | OpenRouter fixtures cover key, credits denial, BYOK, and model semantics; delayed history has no collector implementation or fixture, and host credential dispatch remains absent. | Keep `fixture_verified` / `not_run`; history is explicitly unimplemented/unverified. The key-scoped collector is not promoted to full F22 support. |
| REST `4055299937` / GraphQL `PRRT_kwDOR2N26c6kE6Io` — F28 | Broker-generation and repeated-root tests are not usage-event tests; no shared/fork/subagent/release event stream fixture exists. | Keep `fixture_verified` / `not_run`; event-level deduplication remains unverified. Cache/root deduplication is not counted as event proof. |
| REST `4055299941` / GraphQL `PRRT_kwDOR2N26c6kE6Ir` — D08/F14/provider matrix | The Antigravity fixture only records the gated headless/old-command state; no GUI/keyring-backed live call, container, or multiple-account artifact exists. | Keep Antigravity live GUI/keyring support `not_run`, not `unsupported`; only the narrow headless fixture state is described. |

Table convention: `Proof` is the highest evidence layer actually available in
this audit. `Status` is the requirement disposition: `implemented` means the
code path and cited deterministic evidence are present; `failed` means a cited
gate failed; `unavailable` means the required credential/access was absent;
`unsupported` means the fixture proves that the capability is not available;
`not_run` means no sufficient evidence was found or executed. `fixture_verified`
does not imply service, container, or live-provider proof.

## Current exact-head evidence

The branch now contains current #1002 source `3218706c`. The three `ca128f8`
nextest runs and the `72feee23` coordinator recheck above predate this sync and
are not current `3218706c` counts. No new aggregate nextest result is claimed
by this documentation correction. The required container,
live-provider, GUI/keychain, native-client, and independent-review surfaces
remain `not_run`; no status below upgrades them.

## Historical lane spine (retained, not current verification)

The following lane rows preserve earlier reports for provenance. Their `/tmp`
artifacts are absent, their counts and SHAs are not current unless explicitly
labelled `AUDIT-F`, and their historical `implemented`/`fixture_verified`
dispositions do not re-issue current proof.

| Requirement | Owner | Test / scenario | Result / artifact | Proof | Status |
|---|---|---|---|---|---|
| S1 catalog (12 agents, 12 providers, adapters, matches) | s1-catalog-expand | nextest 909 (core/config/instance/image/telemetry/agent-status) + usage 412 + console/runtime 1933 + workspace check clean | /tmp/lane-s1-catalog.md | fixture_verified | implemented |
| provD OpenRouter/OpenCode-Go/Grok | prov-d | openrouter 9 + opencode/grok ext (in-tree green) | /tmp/lane-prov-d.md | fixture_verified | implemented |
| T10 metric groups | t10 | protocol 56 + projection 36 + console 20 (in-tree) | /tmp/lane-t10.md | fixture_verified | implemented |
| C04 committed-agent defaults | c04 | prompts 22 + list 38 (in-tree) | /tmp/lane-c04.md | fixture_verified | implemented |
| broker cadence (coordinator) | broker-coord | 27 isolated + 28 in-tree | /tmp/lane-broker-coord.md | fixture_verified | implemented |
| provider review (13 fixes) | audit reviewer | `AUDIT-F` plus provider source/test inspection; `/tmp/review-providers.md` absent and PR review API returned no reviews | fixture tests pass; independent/live review artifact unavailable | fixture_verified (code/tests only) | not_run |
| zshrc static parser | zshrc lane | standalone rustc --test 17/17 + probe | /tmp/lane-zshrc.md | fixture_verified | implemented |
| stores enumerators | stores lane | config 317/317 (stores 29) | /tmp/lane-stores.md | fixture_verified | implemented |
| T20 harness | t20 lane | standalone 26/26 | /tmp/lane-t20-harness.md | fixture_verified | implemented |
| provA Claude/Codex/Amp | prov-a | isolated worktree 314/314 + clippy clean | /tmp/lane-prov-a.md | fixture_verified | implemented |
| provB Kimi/ZAI/MiniMax | prov-b | kimi 14 + zai 9 + minimax 15 | /tmp/lane-prov-b.md | fixture_verified | implemented |
| provC Antigravity/Gemini/Cursor | prov-c | 26/26 new-collector tests | /tmp/lane-prov-c.md | fixture_verified | implemented |
| provE Muse/omp/Hermes | prov-e | 26/26 (13+5+8) | /tmp/lane-prov-e.md | fixture_verified | implemented |
| console-usage phase-1 | console-usage-1 | isolated worktree 20/20 + lib 1270 + adapter 106 | /tmp/lane-console-usage-1.md | fixture_verified | implemented |
| S2 schema + resolver + bootstrap | orchestrator | config/resolver unit + migration tests (landed in S4 integration commit) | git log feat(multi-account): S4 | fixture_verified | implemented |
| S3 instance-keyed credential transport | orchestrator + 4 lanes | protocol 117 + instance 143 + env 56 + capsule 883 + runtime 646 + console/usage/jackin/xtask green; clippy -D warnings; xtask lint --strict | 6f0280c4 | fixture_verified | implemented |
| Tracer bullet: 2×Claude + Codex live container | orchestrator | historical claim only; `/tmp/tracer/evidence.md` and `/tmp/split.log` are absent in this audit filesystem | historical `live_verified` label not revalidated | not_run | not_run |
| S4 discovery/Settings/launch/Capsule lanes + integration | A/B/C/D/E + orchestrator | config/protocol/console/jackin 2457 + core/instance/env/runtime/capsule/usage 2326 + console re-run 2337; clippy -D warnings; xtask lint --strict; scan bridge (input→Manager→StartAccountScan→worker→AccountScanCompleted), usage offscreen heartbeat via UsageRouteState, boxed dispatch Action | S4 commit (see git log) | fixture_verified | implemented |

## Checklist A–H (from jackin-implementation-and-verification.md)

Every requirement is listed once. Test names are repository paths in the current
source tree. `AUDIT-F` means the PR #1005 baseline exact-source fixture run at
`ca128f8`; `CURRENT-R` means the PR #1005 baseline broad fixture/console run at
`ca128f8`; `COORD-F` and `COORD-R` name the separate pre-sync coordinator
rechecks at `72feee23`; `AUDIT-R` means the historical aggregate at `1c8b9907`,
not a current run. A row citing `AUDIT-R` retains historical evidence only; its
gap/disposition text controls current status.

### A. Initialization, registration, discovery

| ID | Requirement | Test path/name + exact result | Proof | Status | Gap or disposition |
|---|---|---|---|---|---|
| A01 | First start imports supported defaults | `crates/jackin-config/src/accounts/tests.rs::first_start_discovers_once_and_does_not_grant_workspace_access`; AUDIT-F pass | fixture_verified | implemented | No live home import. |
| A02 | Empty pre-created config runs explicit initialization | `crates/jackin-config/src/editor/tests.rs::open_detailed_fresh_install_scans_and_stamps_sentinel`, `open_detailed_consumes_installer_marker_exactly_once`, `failed_fresh_install_bootstrap_keeps_marker_for_retry`; AUDIT-F pass | fixture_verified | implemented | Deterministic fresh-install and retry paths covered. |
| A03 | Interrupted discovery/config write retries atomically | `crates/jackin-config/src/editor/tests.rs::startup_rolls_back_partial_publication_and_removes_staged_files`, `editor_save_atomic_staging_failure_preserves_every_original_file`, `config_lock_competing_editors_serialize`; AUDIT-F pass | fixture_verified | implemented | No crash-injected live filesystem run. |
| A04 | Ordinary start does not re-add a removed account | `crates/jackin-config/src/editor/tests.rs::removed_account_stays_excluded_from_scan_after_reload`; AUDIT-F pass | fixture_verified | implemented | Exclusion survives reload in fixture. |
| A05 | Scan finds later accounts without changing labels/defaults | `crates/jackin-config/src/editor/tests.rs::scan_for_accounts_reads_live_home_and_environment`, `scan_for_accounts_never_overwrites_operator_id_registrations`; AUDIT-F pass | fixture_verified | implemented | “Live home” here is a temporary fixture, not host evidence. |
| A06 | Empty scan and partial source failure remain visible | `crates/jackin-console/src/tui/screens/settings/update/tests.rs::reduce_account_scan_completion_joins_candidates_and_committed`, `scan_merge_failure_surfaces_panel_error`; AUDIT-R pass | fixture_verified | implemented | UI reduction and error visibility are deterministic; no multi-source OS failure injection. |
| A07 | Empty, metadata-only, expired, missing, and valid sources differ | `crates/jackin-config/src/accounts/discovery/tests.rs::environment_discovery_returns_names_without_secret_values`, `recognizes_each_agents_credentials_and_rejects_metadata`, `oauth_discovery_keeps_only_nonempty_subscription_reference`; AUDIT-F pass | fixture_verified | implemented | Provider-specific host login states remain unverified. |
| A08 | Size, malformed data, unreadable/escaping paths, and spaces are deterministic | `crates/jackin-config/src/accounts/discovery/tests.rs::malformed_credentials_return_sanitized_error`, `oversized_credentials_are_rejected_before_parsing`; `crates/jackin-config/src/accounts/stores/sqlite/tests.rs::rejects_malformed_images`; AUDIT-F pass | fixture_verified | implemented | No host symlink-loop sweep was run. |
| A09 | Native credential files and Amp XDG roots import without shell execution | `crates/jackin-config/src/accounts/discovery/tests.rs::recognizes_each_agents_credentials_and_rejects_metadata`, `amp_alias_root_retains_root_and_reports_nested_evidence`; `crates/jackin-config/src/editor/tests.rs::apply_zshrc_plan_persists_amp_xdg_roots_with_discovered_credentials`; AUDIT-F pass | fixture_verified | implemented | Fixture-only path import. |
| A10 | Static `.zshrc` literal/profile/model mapping | `crates/jackin-config/src/accounts/zshrc/tests.rs::plan_extracts_custom_config_dirs_and_skips_relative`, `plan_collects_complete_xdg_triple_only`, `plan_groups_model_profiles_by_stem`; AUDIT-F pass | fixture_verified | implemented | Dynamic expressions are reported, not evaluated. |
| A11 | `.zshrc` and stores never execute helpers, functions, or prompts | `crates/jackin-config/src/accounts/zshrc/tests.rs::op_read_is_typed`, `defined_function_calls_are_typed`, `unknown_commands_are_command_substitutions`; `crates/jackin-runtime/src/exec_host/tests.rs::validate_op_source_rejects_flag_segments`; AUDIT-F/AUDIT-R pass | fixture_verified | implemented | No shell-startup canary process was captured. |
| A12 | Normal and YOLO wrappers deduplicate one source | `crates/jackin-config/src/editor/tests.rs::apply_zshrc_custom_profile_does_not_collide_with_default_profile`, `scan_for_accounts_imports_profiles_with_bootstrap_naming_and_dedupes`; AUDIT-F pass | fixture_verified | implemented | Execution-preference split is not a live wrapper run. |
| A13 | Old/new Kimi layouts are schema-aware | `crates/jackin-config/src/accounts/discovery/tests.rs::kimi_default_discovery_accepts_cli_home_without_duplicate_accounts`, `kimi_discovery_prefers_live_env_grant_over_drained_base_file`, `kimi_discovery_ignores_newer_env_grant_directories`; AUDIT-F pass | fixture_verified | implemented | No installed Kimi account was used. |
| A14 | OpenCode, omp, and Hermes stores enumerate entries separately | `crates/jackin-config/src/accounts/stores/opencode/tests.rs::selects_usable_entries_sorted_by_provider`, `selects_credential_rows_skipping_inactive`; omp `selects_rows_in_rowid_order`; Hermes `merges_inline_and_file_profiles_with_auth`; AUDIT-F pass | fixture_verified | implemented | Store fixtures cover selected provider rows. |
| A15 | Repeated source is idempotent; distinct scoped keys stay distinct | `crates/jackin-config/src/editor/tests.rs::scan_for_accounts_imports_profiles_with_bootstrap_naming_and_dedupes`; `crates/jackin-protocol/src/usage_broker/tests.rs::independent_key_caps_never_merge`; AUDIT-F pass | fixture_verified | implemented | No host rescan after credential rotation. |
| A16 | Account IDs survive rename/order/rescan/restart/rotation/model changes | `crates/jackin-usage/src/host/tests.rs::canon_sel_valid_historical_choice_survives_reopen`, `canonical_identity_domain_separates_evidence_and_normalizes_stable_handles`; AUDIT-F pass | fixture_verified (partial) | not_run | No single deterministic fixture covers the complete rename + rotation + restart sequence. |
| A17 | Subject change invalidates old identity/cache association | `crates/jackin-usage/src/host/tests.rs::canonical_identity_domain_separates_evidence_and_normalizes_stable_handles`; AUDIT-F pass | fixture_verified (partial) | not_run | Subject replacement and cache invalidation are not directly exercised together. |
| A18 | Keychain sources classify valid, absent, and locked states | `crates/jackin-config/src/accounts/discovery/tests.rs::custom_claude_keychain_scope_never_falls_back_to_default`, `antigravity_discovery_is_keychain_only`; `crates/jackin-usage/src/usage/tests.rs::classify_claude_keychain_status_maps_denial_and_absence`; AUDIT-F pass | fixture_verified | implemented | No real macOS keychain lock/permission run. |
| A19 | Environment references stay references when unresolved | `crates/jackin-config/src/editor/tests.rs::scan_for_accounts_imports_environment_references_without_values`; `crates/jackin-env/src/accounts/tests.rs::empty_credential_rejected`; AUDIT-F pass | fixture_verified | implemented | Secrets were not persisted in fixture output. |
| A20 | Removing registration preserves host paths and reports affected bindings | `crates/jackin-config/src/editor/tests.rs::removing_account_prunes_all_assignments_and_bindings`, `removed_account_stays_excluded_from_scan_after_reload`; AUDIT-F pass | fixture_verified (config/binding portion) | implemented | No destructive host-directory operation is performed by the fixture. |
| A21 | Pre-sentinel upgrade never resurrects removed accounts | `crates/jackin-config/src/editor/tests.rs::open_detailed_upgrade_never_resurrects_or_rescans`; `crates/jackin-config/src/migrations/tests.rs::v1alpha10_to_current_stamps_initialized_sentinel_without_touching_accounts`; AUDIT-F pass | fixture_verified | implemented | Migration fixture covers the sentinel boundary. |
| A22 | Concurrent bootstrap/manual scans merge under lock | `crates/jackin-config/src/editor/tests.rs::config_lock_fresh_editor_bootstraps_without_recursive_acquisition`, `config_lock_competing_editors_serialize`, `editor_save_keeps_exclusive_lock_during_publication`; AUDIT-F pass | fixture_verified | implemented | No multi-process host stress run. |
| A23 | Discovery has no indirect shell/helper writes | `crates/jackin-config/src/accounts/zshrc/tests.rs::plan_carries_no_secret_values`, `unknown_commands_are_command_substitutions`; `crates/jackin-runtime/src/exec_host/tests.rs::credential_process_exports_typed_spawn_failure_without_program_or_arguments`; AUDIT-F/AUDIT-R pass | fixture_verified (partial) | not_run | Required source-byte scan and shell-startup canary artifact are absent. |

### B. Settings, models, and compatibility

| ID | Requirement | Test path/name + exact result | Proof | Status | Gap or disposition |
|---|---|---|---|---|---|
| B01 | Settings supports profile, folder, API key, env, and 1Password references | `crates/jackin-console/src/tui/components/auth_panel/tests.rs::save_enabled_for_api_key_with_literal`, `commit_emits_required_env_var`, `source_folder_row_requires_form_source_state_and_supported_mode`; `crates/jackin-config/src/editor/tests.rs::set_env_var_persists_op_ref_account`; AUDIT-F/AUDIT-R pass | fixture_verified | implemented | No interactive Mac Settings session in this audit. |
| B02 | Inference and usage/billing permission validation are independent | `crates/jackin-config/src/accounts/tests.rs::agent_configuration_validation_rejects_bad_references_and_overrides`; `crates/jackin-console/src/tui/components/auth_panel/tests.rs::unsupported_account_modes_cannot_be_committed`; AUDIT-F/AUDIT-R pass | fixture_verified (partial) | not_run | No dedicated independent billing-permission validation assertion was found. |
| B03 | Secrets are absent from lists, errors, snapshots, help, and logs | `crates/jackin-config/src/accounts/tests.rs::secrets_are_redacted_and_provider_routing_is_explicit`; `crates/jackin-console/src/tui/components/auth_panel/tests.rs::credential_input_redacts_debug_and_paint`; `crates/jackin-protocol/src/account_credentials/tests.rs::debug_redacts_everything`; AUDIT-F/AUDIT-R pass | fixture_verified | implemented | No Docker inspect/history artifact in current audit. |
| B04 | Supported pairs are positive; unsupported pairs fail before transmission | `crates/jackin-config/src/accounts/tests.rs::single_provider_newcomers_accept_only_native_keys`, `multi_provider_clients_accept_every_provider_but_amp`, `invalid_ids_and_on_demand_credentials_rejected`; AUDIT-F pass | fixture_verified | implemented | Pair matrix is fixture-verified, not live-client verified. |
| B05 | Kimi Code routes native Kimi, supported Claude, and Codex Responses | `crates/jackin-config/src/accounts/tests.rs::new_agents_route_native_key_variables`, `native_provider_mapping_covers_new_agents`; `crates/jackin-env/src/accounts/tests.rs::different_accounts_resolve_into_separate_instance_environments`; AUDIT-F pass | fixture_verified | implemented | No Kimi live grant. |
| B06 | Z.AI and other routes obey endpoint/protocol restrictions | `crates/jackin-config/src/accounts/tests.rs::claude_and_codex_routing_is_unchanged_by_new_providers`, `endpoint_overrides_fail_closed_for_new_single_agents`, `omp_and_hermes_endpoint_overrides_fail_closed_until_provider_config_exists`; AUDIT-F pass | fixture_verified | implemented | Vendor endpoint claims still need live proof. |
| B07 | MiniMax Token Plan and PAYG remain distinct | `crates/jackin-config/src/accounts/tests.rs::minimax_codex_account_routes_its_key_and_requires_a_model`; `crates/jackin-usage/src/usage/minimax/tests.rs::minimax_key_product_selects_balance_route`, `minimax_fetch_plan_pins_region_and_product`; AUDIT-F pass | fixture_verified | implemented | No authenticated MiniMax success; historical canary failure is not available as an artifact. |
| B08 | OpenRouter model ID survives save/reload/override/launch/restore | `crates/jackin-config/src/accounts/tests.rs::omp_routing_requires_model_and_selects_provider_variable`; `crates/jackin-runtime/src/runtime/launch/account_config/tests.rs::selected_opencode_account_pairs_endpoint_key_and_model`; AUDIT-F/AUDIT-R pass | fixture_verified (partial) | not_run | No end-to-end OpenRouter save → restore fixture; host dispatch gap is recorded in support ledger. |
| B09 | Removed OpenRouter model fails specifically, without substitution | `crates/jackin-usage/src/usage/openrouter/tests.rs::openrouter_model_catalog_omission_is_unverified_not_rejection`; AUDIT-F pass | fixture_verified (semantic rule only) | not_run | No launch-validator test for authoritative model rejection. |
| B10 | Multiple model presets share one account/quota identity | `crates/jackin-config/src/accounts/tests.rs::resolve_launch_model_chain_prefers_configuration_override`; `crates/jackin-protocol/src/usage_broker/tests.rs::independent_key_caps_never_merge`; AUDIT-F pass | fixture_verified | implemented | Preset sharing is fixture-verified. |
| B11 | Disabled accounts remain visible but are not launched/probed | `crates/jackin-config/src/accounts/tests.rs::disabled_accounts_keep_configuration_but_cannot_authenticate`, `validate_accounts_rejects_disabled_bindings_at_all_scopes`; `crates/jackin-usage/src/host/tests.rs::disabled_probe_policy_skips_dispatch_and_is_never_due`; AUDIT-F pass | fixture_verified | implemented | No live disabled-account broker run. |
| B12 | Optional billing/admin credential cannot replace execution credential | `crates/jackin-usage/src/usage/openrouter/tests.rs::openrouter_credits_403_is_typed_scope_denial`, `openrouter_credits_separates_spend_from_remaining_balance`; AUDIT-F pass | fixture_verified (provider semantics) | not_run | Account-level admin-vs-execution admission test is absent. |
| B13 | Settings keyboard, validation focus, cancellation, persistence, layout | `crates/jackin-console/src/tui/screens/settings/update/tests.rs::settings_focus_chain_walks_tab_bar_then_content_and_wraps`, `settings_confirm_commit_plan_routes_confirmed_actions`; `crates/jackin-console/src/tui/screens/settings/view/tests.rs::settings_frame_areas_match_header_tabs_body_footer_contract`; AUDIT-R pass | fixture_verified | implemented | PNG baseline aggregate timed out; see H04. |
| B14 | Scan preserves dirty edits and cancellation does not apply | `crates/jackin-console/src/tui/screens/settings/update/tests.rs::scan_merge_never_touches_present_or_deleted_ids`, `scan_merge_failure_surfaces_panel_error`, `discard_orphans_in_flight_scan`; `crates/jackin-config/src/editor/tests.rs::config_lock_competing_editors_serialize`; AUDIT-F/AUDIT-R pass | fixture_verified | implemented | No concurrent external editor process. |
| B15 | Explicit model absent from stale cache remains unverified, not rejected | `crates/jackin-usage/src/usage/openrouter/tests.rs::openrouter_model_catalog_omission_is_unverified_not_rejection`; AUDIT-F pass | fixture_verified | implemented | This semantic rule is proven; route integration remains unverified. |
| B16 | All registered providers appear in catalog, Settings, and support ledger | `crates/jackin-config/src/accounts/tests.rs::new_provider_slugs_round_trip`, `provider_wire_spelling_matches_canonical_slug`; store enumerator tests in `crates/jackin-config/src/accounts/stores`; support catalog inspected at this SHA | implemented | implemented | Exhaustive catalog-to-Settings machine check is not present; the ledger now records the explicit OpenRouter host-dispatch gap. |

### C. Workspace/default resolution and container admission

| ID | Requirement | Test path/name + exact result | Proof | Status | Gap or disposition |
|---|---|---|---|---|---|
| C01 | Global defaults provide fast start | `crates/jackin-config/src/accounts/tests.rs::authorized_global_binding_is_inherited_by_workspace`, `resolve_launch_fallback_needs_a_single_eligible_instance`; AUDIT-F pass | fixture_verified | implemented | No live launch. |
| C02 | Workspace and workspace-role defaults override global | `crates/jackin-config/src/accounts/tests.rs::resolve_launch_scope_precedence_replaces_without_union`, `resolve_launch_role_binding_beats_global_binding`; AUDIT-F pass | fixture_verified | implemented | Precedence is deterministic. |
| C03 | One-launch choice does not persist unintended defaults | `crates/jackin-config/src/accounts/tests.rs::resolve_launch_one_launch_wins_and_validates_atomically`; `crates/jackin-runtime/src/runtime/launch/programmatic/tests.rs::launch_selection_without_defaults_synthesizes_ephemeral_default`; AUDIT-F/AUDIT-R pass | fixture_verified | implemented | No interactive one-shot launch. |
| C04 | Valid selected default avoids unnecessary picker | `crates/jackin-config/src/accounts/tests.rs::resolve_launch_default_launch_beats_binding`; `crates/jackin-console/src/services/launch/tests.rs::admitted_account_choices_resolve_role_default_for_agent`; AUDIT-F/AUDIT-R pass | fixture_verified | implemented | Picker UI itself not live. |
| C05 | Ambiguity triggers deterministic picker | `crates/jackin-config/src/accounts/tests.rs::resolve_launch_fallback_needs_a_single_eligible_instance`; `crates/jackin-console/src/services/launch/tests.rs::admitted_account_choices_defer_to_bindings_without_defaults`; AUDIT-F/AUDIT-R pass | fixture_verified | implemented | No random fallback observed in fixture. |
| C06 | Workspace authorization restricts global/default candidates | `crates/jackin-config/src/accounts/tests.rs::resolve_launch_global_candidates_filter_by_authorization`, `unauthorized_global_binding_does_not_fall_back_to_workspace_account`; AUDIT-F pass | fixture_verified | implemented | Authorization is fixture-verified. |
| C07 | Missing/disabled/deleted explicit selection fails without ambient fallback | `crates/jackin-config/src/accounts/tests.rs::resolve_launch_invalid_binding_fails_without_silent_fallback`, `disabled_accounts_keep_configuration_but_cannot_authenticate`; AUDIT-F pass | fixture_verified | implemented | No host ambient environment. |
| C08 | Missing and explicit empty selection differ | `crates/jackin-config/src/accounts/tests.rs::resolve_launch_multi_instance_admission_follows_folder_var_kind`, `resolve_launch_scope_precedence_replaces_without_union`; `crates/jackin-config/src/schema/tests.rs::validate_default_launch_list_accepts_valid_lists`; AUDIT-F pass | fixture_verified | implemented | Shell-only empty launch remains distinct. |
| C09 | Manifest admits exactly two Claude and one Codex accounts | `crates/jackin-runtime/src/runtime/launch/capsule_setup/tests.rs::capsule_config_carries_instance_accounts_and_labels`, `instance_bindings_keep_launch_order_and_config_id_keys`; `crates/jackin-capsule/src/config/tests.rs::several_instances_may_share_one_agent_with_isolated_env`; AUDIT-R pass | fixture_verified | implemented | Manifest fixture proves shape, not real TUI. |
| C10 | Allowed but unselected accounts are absent | `crates/jackin-runtime/src/runtime/launch/capsule_setup/tests.rs::instance_bindings_carry_roots_only_for_selected_instances`; `crates/jackin-runtime/src/runtime/launch/tests.rs::load_agent_skips_unselected_account_credential_refs`; AUDIT-R pass | fixture_verified | implemented | Canary is synthetic and deterministic. |
| C11 | CLI/Console/programmatic launches agree and cannot expand manifest | `crates/jackin-console/src/services/launch/tests.rs::resolve_committed_agent_launch_carries_admitted_accounts`; `crates/jackin-runtime/src/runtime/launch/programmatic/tests.rs::launch_selection_rejects_accounts_outside_workspace_allowlist`; AUDIT-R pass | fixture_verified | implemented | No three-entry live container comparison. |
| C12 | Editing defaults does not alter running admission | `crates/jackin-runtime/src/runtime/launch/account_identity/tests.rs::fingerprint_covers_the_admitted_instance_set`; AUDIT-R pass | fixture_verified (identity portion) | not_run | No running-container edit-after-start integration fixture. |
| C13 | New tab selects admitted account and rejects outside manifest | `crates/jackin-capsule/src/config/tests.rs::unknown_instances_are_rejected`, `protected_credentials_must_match_admitted_agent_and_account`; `crates/jackin-runtime/src/usage_relay/tests.rs::usage_relay_authorizes_only_exact_forwarded_account`; AUDIT-R pass | fixture_verified | implemented | No live tab. |
| C14 | Account-set change uses explicit update/restart/new-container path | `crates/jackin-runtime/src/runtime/attach/tests.rs::revoked_account_blocks_focused_attach_agent_and_shell_before_exec`; `crates/jackin-runtime/src/runtime/launch/tests.rs::metadata_file_mount_instances_require_recreation_after_layout_change`; AUDIT-R pass | fixture_verified (failure/recreation portions) | not_run | No full requested-account-set transition scenario. |
| C15 | Restore detects manifest/credential revision changes | `crates/jackin-runtime/src/runtime/launch/account_identity/tests.rs::configuration_match_roundtrip`, `fingerprint_covers_the_admitted_instance_set`, `credentials_writer_revokes_stale_instance_files`; AUDIT-R pass | fixture_verified | implemented | No real restored container. |
| C16 | Unauthorized inherited defaults and invalid explicit choices fail atomically | `crates/jackin-config/src/accounts/tests.rs::resolve_launch_global_candidates_filter_by_authorization`, `resolve_launch_invalid_binding_fails_without_silent_fallback`, `validate_launch_lists_rejects_unknown_duplicate_and_unauthorized`; AUDIT-F pass | fixture_verified | implemented | Empty-result fixture is deterministic. |
| C17 | Unrelated account D changes do not rotate A/B/C capabilities | `crates/jackin-runtime/src/runtime/launch/account_identity/tests.rs::fingerprint_covers_the_admitted_instance_set`; AUDIT-R pass | fixture_verified (fingerprint portion) | not_run | No explicit add/rename/disable-D reuse scenario was found. |
| C18 | Disable/removal denies grants and reports active state | `crates/jackin-config/src/editor/tests.rs::disabling_and_removing_accounts_prune_all_launch_scopes_atomically`; `crates/jackin-runtime/src/runtime/attach/tests.rs::revoked_account_blocks_focused_attach_agent_and_shell_before_exec`; AUDIT-F/AUDIT-R pass | fixture_verified | implemented | Already-materialized upstream credential revocation is not claimed. |

### D. Actual multi-account runtime and credentials

| ID | Requirement | Test path/name + exact result | Proof | Status | Gap or disposition |
|---|---|---|---|---|---|
| D01 | Two Claude processes have separate homes and identities | `crates/jackin-runtime/src/runtime/launch/tests.rs::agent_mounts_for_two_claude_slots_isolates_homes_and_handoffs`; `crates/jackin-capsule/src/session/tests.rs::account_credentials_are_scoped_to_selected_instance_and_mode`; AUDIT-R pass | fixture_verified | implemented | No real Claude processes or authenticated identities. |
| D02 | Two Codex processes have separate homes/config/session state | `crates/jackin-runtime/src/runtime/launch/account_config/tests.rs::codex_instances_keep_slot_config_and_credential_identity`, `codex_slots_keep_routed_models_catalogs_and_requested_effort_separate`; `crates/jackin-capsule/src/session/tests.rs::build_agent_command_overrides_stale_agent_env`; AUDIT-R pass | fixture_verified | implemented | No real Codex processes. |
| D03 | Amp XDG profile stages required roots without treating refresh login as API key | `crates/jackin-config/src/accounts/tests.rs::resolved_amp_profile_carries_explicit_xdg_roots`; `crates/jackin-config/src/editor/tests.rs::apply_zshrc_plan_persists_amp_xdg_roots_with_discovered_credentials`; `crates/jackin-runtime/src/runtime/launch/tests.rs::agent_mounts_for_amp_synced_includes_secrets_json`; AUDIT-F/AUDIT-R pass | fixture_verified | implemented | No refreshable Amp login. |
| D04 | OpenCode isolates XDG data/auth, not only config | `crates/jackin-runtime/src/runtime/launch/tests.rs::agent_mounts_derive_opencode_data_and_config_roots`; `crates/jackin-runtime/src/runtime/launch/account_config/tests.rs::selected_opencode_account_pairs_endpoint_key_and_model`; AUDIT-R pass | fixture_verified | implemented | OpenRouter host-dispatch gap remains; see support ledger. |
| D05 | Multi-provider JSON/SQLite staging includes selected entries only | `crates/jackin-runtime/src/runtime/launch/tests.rs::load_agent_skips_unselected_account_credential_refs`; `crates/jackin-runtime/src/runtime/launch/capsule_setup/tests.rs::instance_bindings_carry_roots_only_for_selected_instances`; `crates/jackin-capsule/src/session/tests.rs::google_alias_is_scrubbed_from_siblings_while_selected_credential_is_injected`; AUDIT-R pass | fixture_verified | implemented | Synthetic canaries only. |
| D06 | omp broker pools are not authorization; staged token scope is restricted | `crates/jackin-config/src/accounts/tests.rs::omp_routing_requires_model_and_selects_provider_variable`; `crates/jackin-usage/src/usage/omp/tests.rs::broker_pool_is_not_authorization`; `crates/jackin-capsule/src/config/tests.rs::protected_credentials_must_match_admitted_agent_and_account`; AUDIT-F/AUDIT-R pass | fixture_verified | implemented | No installed omp TUI. |
| D07 | Hermes profiles do not share mutable state or rotating tokens | `crates/jackin-config/src/accounts/stores/hermes/tests.rs::merges_inline_and_file_profiles_with_auth`, `skips_profiles_without_provider_or_secret`; `crates/jackin-usage/src/usage/hermes/tests.rs::profile_is_exclusively_owned`; AUDIT-F pass | fixture_verified (store ownership) | not_run | No concurrent Hermes profile/refresh integration fixture. |
| D08 | Antigravity multiple-account mode works in target Linux container | `crates/jackin-config/src/accounts/discovery/tests.rs::antigravity_discovery_is_keychain_only`; `crates/jackin-usage/src/usage/antigravity/tests.rs::gated_snapshot_reports_unsupported_without_running_usage`; AUDIT-F pass | fixture_verified | not_run | Container/keyring and multiple-account proof absent; headless GUI state is not declared unsupported solely from missing credentials. |
| D09 | Cursor config isolates auth and relevant state | `crates/jackin-usage/src/usage/cursor/tests.rs::scope_urls_never_cross`, `summary_preserves_every_pool_without_invention`; `crates/jackin-usage/src/host/discovery/tests.rs::disc_source_valid_profiles_resolve_without_network_or_fake_presence`; AUDIT-F pass | fixture_verified (usage/source) | not_run | No Cursor launch isolation or real stored-token container run. |
| D10 | Muse HOME/handshake binds account; key exchange never leaks key | `crates/jackin-usage/src/usage/muse/tests.rs::key_exchange_is_never_a_poller`, `identity_from_auth_json`, `view_without_observation_is_unavailable`; AUDIT-F pass | fixture_verified (semantic) | not_run | No authenticated Muse handshake or telemetry inspection. |
| D11 | Grok subscription auth cannot override explicit API billing account | `crates/jackin-usage/src/usage/grok/tests.rs::grok_subscription_auth_outranks_ambient_keys`; `crates/jackin-usage/src/usage/tests.rs::grok_account_label_prefers_auth_identity_over_env_presence`; AUDIT-F pass | fixture_verified | implemented | No live Grok account selection. |
| D12 | Ambient credentials/config cannot override chosen account | `crates/jackin-env/src/accounts/tests.rs::generic_env_cannot_bypass_account_admission`, `same_agent_instances_keep_only_their_own_vars`; `crates/jackin-runtime/src/runtime/launch/tests.rs::unassigned_accounts_never_forward_host_auth`; AUDIT-F/AUDIT-R pass | fixture_verified | implemented | No container inspect. |
| D13 | Secrets absent from argv, labels, history, inspect, diagnostics, and unselected mounts | `crates/jackin-runtime/src/runtime/launch/tests.rs::capsule_config_redacts_literal_exec_binding_source_only`, `env_file_is_private_host_only_and_removed_on_success_and_error_drop`, `load_agent_keeps_zai_secret_out_of_capsule_config`; `crates/jackin-capsule/src/session/tests.rs::conformance_wire_real_pty_spawn_stream_and_exit_exclude_private_content`; AUDIT-R pass | fixture_verified | not_run | Fixture checks cover serialized config, host-only env-file cleanup, and PTY output only. Docker argv/labels/image-history/inspect/staged-mount checks and target-container evidence remain unrun. |
| D14 | Only minimum selected auth/config is staged | `crates/jackin-runtime/src/runtime/launch/tests.rs::agent_mounts_for_two_claude_slots_isolates_homes_and_handoffs`, `load_agent_skips_unselected_account_credential_refs`, `role_container_never_mounts_host_docker_socket`; AUDIT-R pass | fixture_verified | implemented | No Docker mount inventory artifact. |
| D15 | One owner for shared OAuth refresh; no stale overwrite | `crates/jackin-runtime/src/runtime/launch/account_identity/tests.rs::credentials_writer_stages_same_account_oauth_routes_per_instance`, `credentials_writer_revokes_stale_instance_files`; `crates/jackin-usage/src/coordinator/tests.rs::coordinator_timeout_wait_keeps_owner_until_worker_terminates`; AUDIT-R pass | fixture_verified (writer/coordinator) | not_run | No two-process rotating-token race run. |
| D16 | Revocation, expiry, logout, swap, and source revision invalidate the right capability | `crates/jackin-runtime/src/runtime/launch/account_identity/tests.rs::credentials_writer_revokes_stale_instance_files`, `fingerprint_covers_the_admitted_instance_set`; `crates/jackin-runtime/src/runtime/attach/tests.rs::revoked_account_blocks_focused_attach_agent_and_shell_before_exec`; AUDIT-R pass | fixture_verified | implemented | Host logout and provider revocation are not live-tested. |
| D17 | Late refresh cannot resurrect removed account or overwrite another observation | `crates/jackin-usage/src/coordinator/tests.rs::coordinator_stale_and_error_success_results_schedule_retry`, `coordinator_failure_shares_retry_deadline_and_last_good`; `crates/jackin-usage/src/usage/tests.rs::failed_refresh_preserves_last_fresh_quota_rows_as_stale_cache`; AUDIT-F pass | fixture_verified | implemented | No external provider race. |
| D18 | Linux amd64 and arm64 support/version/digest are verified | No current exact-head container architecture artifact; historical `/tmp` lane evidence absent | — | not_run | Requires target container runs on both architectures plus image/version/digest records. |
| D19 | Actual requested TUIs are interactive, resize/input/paste/exit cleanly | `crates/jackin-capsule/src/session/tests.rs::conformance_wire_real_pty_spawn_stream_and_exit_exclude_private_content` is a synthetic PTY test; AUDIT-R pass | fixture_verified (PTY harness only) | not_run | No actual provider TUI in a current container. |
| D20 | No provider secret/token appears in fixtures/docs/screenshots/reports | `crates/jackin-protocol/src/account_credentials/tests.rs::debug_redacts_everything`; `crates/jackin-runtime/src/runtime/launch/tests.rs::capsule_config_redacts_literal_exec_binding_source_only`; AUDIT-F/AUDIT-R pass | fixture_verified | implemented | Current audit added no secret-bearing artifact. |
| D21 | Copied paths with one grant coordinate by lineage; distinct grants remain safe | `crates/jackin-runtime/src/runtime/launch/account_identity/tests.rs::credentials_writer_stages_same_account_oauth_routes_per_instance`, `fingerprint_covers_the_admitted_instance_set`; AUDIT-R pass | fixture_verified (staging) | not_run | No simultaneous copied-grant refresh scenario. |
| D22 | Missing/expired explicit profiles fail without ambient fallback | `crates/jackin-config/src/accounts/tests.rs::resolve_launch_invalid_binding_fails_without_silent_fallback`, `disabled_accounts_keep_configuration_but_cannot_authenticate`; `crates/jackin-env/src/accounts/tests.rs::assigned_key_resolves_host_reference_without_reading_other_accounts`; AUDIT-F pass | fixture_verified | implemented | No live expired custom profile. |

### E. Tabs, sessions, and usage authorization

| ID | Requirement | Test path/name + exact result | Proof | Status | Gap or disposition |
|---|---|---|---|---|---|
| E01 | Tab labels distinguish two Claude accounts and Codex | `crates/jackin-runtime/src/runtime/launch/capsule_setup/tests.rs::capsule_config_carries_instance_accounts_and_labels`; `crates/jackin-capsule/src/daemon/tests.rs::session_launch_renders_instance_labels_for_same_agent_instances`; AUDIT-R pass | fixture_verified | implemented | No visible real TUI. |
| E02 | Metadata follows new session, split, move, rename, exit, resume, restore | `crates/jackin-capsule/src/session/tests.rs::account_credentials_are_scoped_to_selected_instance_and_mode`; `crates/jackin-console/src/tui/screens/usage/tests.rs::apply_refresh_preserves_selection_across_rename_and_reorder`; AUDIT-R pass | fixture_verified (partial) | not_run | No one integrated lifecycle scenario covers every listed event. |
| E03 | Custom title does not hide selected-account identity | `crates/jackin-runtime/src/runtime/launch/capsule_setup/tests.rs::capsule_config_carries_instance_accounts_and_labels`; `crates/jackin-usage/src/usage/tests.rs::usage_identity_presentation_owns_account_and_activity_copy`; AUDIT-F/AUDIT-R pass | fixture_verified | implemented | No terminal interaction. |
| E04 | Usage detail opened from pane selects actual account/provider | `crates/jackin-usage/src/usage/tests.rs::focused_usage_cache_selects_the_exact_account_capability`, `usage_cache_isolates_provider_targets_that_share_one_agent_slug`; `crates/jackin-console/src/tui/screens/usage/tests.rs::projection_keeps_canonical_ids_and_freshness`; AUDIT-F/AUDIT-R pass | fixture_verified | implemented | Non-native live route remains unverified. |
| E05 | Capsule sees only admitted capabilities and rejects guessed IDs | `crates/jackin-usage/src/host/discovery/tests.rs::disc_scope_capsule_uses_only_forwarded_capabilities`; `crates/jackin-runtime/src/usage_relay/tests.rs::usage_relay_authorizes_only_exact_forwarded_account`; AUDIT-F/AUDIT-R pass | fixture_verified | implemented | No socketed live capsule. |
| E06 | Usage discovery never changes launch or blocks only for missing quota | `crates/jackin-runtime/src/usage_relay/tests.rs::hermetic_layout_never_starts_host_usage_discovery`, `forwarded_sources_include_only_provisioned_profiles_and_governed_env`; `crates/jackin-usage/src/host/tests.rs::unavailable_and_refreshing_never_invent_percent`; AUDIT-F/AUDIT-R pass | fixture_verified | implemented | No live provider missing-quota launch. |
| E07 | Multiple same-provider accounts remain separately addressable | `crates/jackin-usage/src/host/discovery/tests.rs::disc_same_provider_sources_with_same_labels_keep_source_capabilities_distinct`, `disc_dedup_repeated_roots_read_once_and_same_identity_merges`; `crates/jackin-usage/src/usage/tests.rs::usage_cache_keeps_account_snapshots_isolated_across_one_provider_target`; AUDIT-F pass | fixture_verified | implemented | No live multi-account provider. |
| E08 | Unused selected account can be monitored when provider permits | `crates/jackin-usage/src/host/discovery/tests.rs::disc_source_valid_profiles_resolve_without_network_or_fake_presence`; `crates/jackin-usage/src/host/tests.rs::multi_account_list_select_and_snapshot`; AUDIT-F pass | fixture_verified (host fixture) | not_run | Provider permission and live unused-account read are not verified. |

### F. Quota semantics and provider contract fixtures

| ID | Requirement | Test path/name + exact result | Proof | Status | Gap or disposition |
|---|---|---|---|---|---|
| F01 | Show all supplied session/rolling/weekly/monthly windows | `crates/jackin-usage/src/usage/kimi/tests.rs::kimi_code_api_pools_map_rolling_weekly_monthly`; `crates/jackin-usage/src/usage/tests.rs::claude_oauth_limits_array_surfaces_fable_and_all_models`, `status_bar_headline_joins_windows_and_spend`; AUDIT-F pass | fixture_verified | implemented | Fixture values only. |
| F02 | Classify period by explicit duration/type | `crates/jackin-usage/src/usage/cursor/tests.rs::period_parses_with_team_inference`; `crates/jackin-usage/src/usage/minimax/tests.rs::minimax_remains_time_normalizes_milliseconds`; `crates/jackin-usage/src/usage/tests.rs::zai_duration_classifier_handles_sole_and_reordered_limits`; AUDIT-F pass | fixture_verified | implemented | No provider-native timestamp comparison. |
| F03 | Used/remaining text and bar geometry agree | `crates/jackin-usage/src/host/projection/tests.rs::window_projection_preserves_raw_overage_from_money_ratio`, `window_projection_keeps_checked_math_without_wrap_or_fabrication`; `crates/jackin-console/src/tui/screens/usage/tests.rs::usage_meter_scales_to_remaining_percentage`, `usage_window_mirrors_used_percent_to_meter`; AUDIT-F/AUDIT-R pass | fixture_verified | implemented | PNG aggregate timeout is tracked under H04. |
| F04 | Balance without denominator has no fabricated percentage | `crates/jackin-usage/src/usage/openrouter/tests.rs::openrouter_cap_without_remaining_shows_limit_only`, `openrouter_null_cap_means_no_cap_never_infinite`; `crates/jackin-usage/src/usage/tests.rs::usage_bucket_presentation_limit_only_balance`; AUDIT-F pass | fixture_verified | implemented | No live balance response. |
| F05 | Unknown, permission, N/A, not-started, exhausted, unavailable differ | `crates/jackin-usage/src/host/projection/tests.rs::quota_state_keeps_permission_unknown_and_exhausted_distinct`; `crates/jackin-protocol/src/usage_broker/tests.rs::quota_states_serialize_distinctly`; `crates/jackin-usage/src/host/tests.rs::unavailable_and_refreshing_never_invent_percent`; AUDIT-F pass | fixture_verified | implemented | State taxonomy is deterministic. |
| F06 | Edge values, nulls, overage, negatives, and currency precision follow contract | `crates/jackin-usage/src/usage/openrouter/tests.rs::openrouter_null_cap_means_no_cap_never_infinite`; `crates/jackin-usage/src/usage/kimi/tests.rs::kimi_over_cap_keeps_raw_percent_with_clamped_bar`; `crates/jackin-usage/src/usage/minimax/tests.rs::minimax_decimal_minor_parses_without_float_rounding`; `crates/jackin-usage/src/usage/grok/tests.rs::grok_negative_cents_never_mirror_into_bounds`; AUDIT-F pass | fixture_verified | implemented | Sanitized fixtures only. |
| F07 | Reset, credential expiry, renewal, timezone/DST labels stay separate | `crates/jackin-protocol/src/usage_broker/tests.rs::reset_credential_expiry_and_renewal_are_independent_fields`; `crates/jackin-usage/src/host/projection/tests.rs::credential_expiry_stays_unset_without_provider_signal`; `crates/jackin-usage/src/usage/tests.rs::reset_label_uses_relative_and_local_timestamp`; AUDIT-F pass | fixture_verified | implemented | No DST boundary matrix. |
| F08 | Reset timestamp does not synthesize fresh quota | `crates/jackin-usage/src/usage/tests.rs::usage_snapshot_cache_miss_is_refreshing`, `failed_refresh_preserves_last_fresh_quota_rows_as_stale_cache`; `crates/jackin-usage/src/coordinator/tests.rs::coordinator_empty_result_is_failure_and_preserves_last_good`; AUDIT-F pass | fixture_verified | implemented | No provider call. |
| F09 | Token totals/spend/rate limits are not subscription remaining | `crates/jackin-usage/src/usage/openrouter/tests.rs::openrouter_credits_separates_spend_from_remaining_balance`; `crates/jackin-usage/src/usage/hermes/tests.rs::tracker_counters_yield_no_budget`; `crates/jackin-usage/src/usage/tests.rs::amp_paid_only_balances_do_not_infer_daily_or_plan`; AUDIT-F pass | fixture_verified | implemented | Honest no-budget states are retained. |
| F10 | Shared pools are not double-summed; independent caps stay separate | `crates/jackin-protocol/src/usage_broker/tests.rs::independent_key_caps_never_merge`, `quota_scope_dedup_key_is_stable_and_axis_sensitive`; `crates/jackin-usage/src/usage/tests.rs::usage_cache_keeps_account_snapshots_isolated_across_one_provider_target`; AUDIT-F pass | fixture_verified | implemented | No live cross-client billing identity. |
| F11 | Claude named/array/extra/scope fixtures | `crates/jackin-usage/src/usage/tests.rs::claude_oauth_limits_array_surfaces_fable_and_all_models`, `claude_oauth_limits_array_skips_unnamed_scoped_window`, `claude_spend_disabled_is_surfaced_with_reason`, `claude_scope_restriction_error_is_explicit`; AUDIT-F pass | fixture_verified | implemented | No current OAuth grant. |
| F12 | Codex app-server windows, credits, reset inventory, API auth, missing fields | `crates/jackin-usage/src/usage/tests.rs::codex_rpc_response_maps_account_windows_and_credits`, `codex_rpc_tolerates_missing_windows_credits_and_counts`, `codex_rpc_account_api_key_tag_yields_origin_label_and_rate_limits`; `crates/jackin-usage/src/usage/codex/tests.rs::codex_over_cap_keeps_raw_label_with_clamped_bar`; AUDIT-F pass | fixture_verified | implemented | No ChatGPT live login evidence in current audit. |
| F13 | Amp free/daily, dollars, Orb, monthly, workspace, linked subscription | `crates/jackin-usage/src/usage/tests.rs::amp_daily_parser_preserves_workspace_balances_in_order`, `amp_tier_line_maps_agent_dollars_orb_hours_and_renewal`, `amp_tier_without_orb_keeps_agent_and_skips_orb`, `amp_paid_only_balances_do_not_infer_daily_or_plan`; AUDIT-F pass | fixture_verified | implemented | No current Amp account. |
| F14 | Antigravity pools, CLI JSON, identity, old command, 401, availability-only | `crates/jackin-usage/src/usage/antigravity/tests.rs::summary_pools_map_exact_bucket_ids`, `agy_version_gate_rejects_pre_json`, `availability_only_never_becomes_quota`, `gated_snapshot_reports_unsupported_without_running_usage`; AUDIT-F pass | fixture_verified | not_run | The fixture records the unsupported headless/old-command state only. GUI/keyring-backed live support and multiple-account behavior were not run; do not classify the entire surface as unsupported. |
| F15 | Gemini retirement, eligible account, project quotas, key/Vertex semantics | `crates/jackin-usage/src/usage/gemini/tests.rs::retirement_instant_matches_deprecation_notice`, `entitlement_flags_consumer_shutdown`, `project_quotas_never_invent_denominators`; AUDIT-F pass | fixture_verified | implemented | No Gemini CLI or OAuth account. |
| F16 | Kimi old/new layouts, windows, wallet, shared identity | `crates/jackin-usage/src/usage/kimi/tests.rs::kimi_code_api_pools_map_rolling_weekly_monthly`, `kimi_extra_usage_wallet_maps_to_spend_bucket`, `kimi_identity_falls_back_through_name_to_id`; AUDIT-F pass | fixture_verified | implemented | No live Kimi grant. |
| F17 | Z.AI credit/token/time/MCP/team/error envelopes | `crates/jackin-usage/src/usage/zai/tests.rs::zai_credit_limit_carries_rate_note_and_model_breakdown`, `zai_time_limit_splits_mcp_and_web_search`, `zai_team_scope_parses_selectors`; AUDIT-F pass | fixture_verified | implemented | No Z.AI credentials. |
| F18 | MiniMax Token Plan/PAYG, periods, amount, region, failures | `crates/jackin-usage/src/usage/minimax/tests.rs::minimax_fetch_plan_pins_region_and_product`, `minimax_exhausted_and_unlimited_windows_render`, `minimax_balance_maps_amounts_with_currency`, `minimax_operation_path_covers_balance_endpoint`; AUDIT-F pass | fixture_verified | implemented | No live MiniMax success; do not use historical canary claim as current evidence. |
| F19 | Cursor personal/enterprise, pooled/personal, credits, money units | `crates/jackin-usage/src/usage/cursor/tests.rs::enterprise_keeps_actual_and_estimated_apart`, `summary_preserves_every_pool_without_invention`, `grok_bot_pooled_or_zero_yields_no_meter`; AUDIT-F pass | fixture_verified | implemented | No live Cursor token. |
| F20 | Grok periods, billing, prepaid/on-demand bounds, errors, auth precedence | `crates/jackin-usage/src/usage/grok/tests.rs::grok_monthly_period_never_renders_weekly_percent_meter`, `grok_billing_error_taxonomy_covers_rest_and_rpc`, `grok_subscription_auth_outranks_ambient_keys`; AUDIT-F pass | fixture_verified | implemented | No live Grok token. |
| F21 | Muse cached/changed observation, timestamp, overage, key sanitization | `crates/jackin-usage/src/usage/muse/tests.rs::reread_with_same_observation_keeps_freshness`, `reread_with_new_observation_stamps_now`, `over_cap_percent_preserved_raw`, `key_exchange_is_never_a_poller`; AUDIT-F pass | fixture_verified | implemented | No live MSP observation. |
| F22 | OpenRouter key, credits 403, scope mismatch, null cap, BYOK, model, history | `crates/jackin-usage/src/usage/openrouter/tests.rs::openrouter_key_maps_cap_remaining_and_period_spend`, `openrouter_credits_403_is_typed_scope_denial`, `openrouter_byok_stays_a_separate_row`, `openrouter_model_catalog_omission_is_unverified_not_rejection`; AUDIT-F pass | fixture_verified | not_run | Key, credits, BYOK, and model fixtures pass, but delayed history has no collector implementation or fixture. Live key evidence and host credential dispatch are also absent. |
| F23 | OpenCode Go rolling/weekly/monthly without invented per-model quota | `crates/jackin-usage/src/usage/opencode/tests.rs::opencode_percent_spellings_and_used_limit_fallback`, `opencode_per_model_and_zen_fields_render_nothing`; AUDIT-F pass | fixture_verified | implemented | No OpenCode credentials. |
| F24 | omp/Hermes usage attributes to underlying accounts, not local counters | `crates/jackin-usage/src/usage/omp/tests.rs::attribution_preserves_underlying_buckets`, `pool_routing_yields_no_budget`; `crates/jackin-usage/src/usage/hermes/tests.rs::tracker_counters_yield_no_budget`, `view_attributes_underlying_plus_portal`; AUDIT-F pass | fixture_verified | implemented | No installed omp/Hermes runtime. |
| F25 | Optional enrichment failure preserves primary quota | `crates/jackin-usage/src/usage/opencode/tests.rs::opencode_over_cap_keeps_raw_percent_without_failing_siblings`; `crates/jackin-usage/src/usage/tests.rs::split_fetch_partitions_ok_err_and_absent`; `crates/jackin-usage/src/coordinator/tests.rs::coordinator_stale_and_error_success_results_schedule_retry`; AUDIT-F pass | fixture_verified | implemented | Fixture failure taxonomy only. |
| F26 | Fresh quota and old history stay separate; optional timeout does not delay primary | `crates/jackin-usage/src/usage/tests.rs::usage_snapshot_reads_in_memory_cache`, `failed_refresh_preserves_last_fresh_quota_rows_as_stale_cache`; `crates/jackin-usage/src/coordinator/tests.rs::coordinator_timeout_wait_keeps_owner_until_worker_terminates`; AUDIT-F pass | fixture_verified | implemented | No timed real provider enrichment. |
| F27 | Switching A/B never assigns ownerless history to wrong account | `crates/jackin-usage/src/usage/tests.rs::usage_cache_keeps_account_snapshots_isolated_across_one_provider_target`, `usage_cache_key_canonicalizes_provider_aliases`; `crates/jackin-usage/src/host/tests.rs::canon_projection_ignores_removed_legacy_shared_snapshot`; AUDIT-F pass | fixture_verified | implemented | Offline ownerless migration is fixture-only. |
| F28 | Shared/fork/subagent/release events are deduplicated | `crates/jackin-usage/src/usage/tests.rs::usage_cache_adopts_broker_generations_by_account_capability`; `crates/jackin-usage/src/host/discovery/tests.rs::disc_dedup_repeated_roots_read_once_and_same_identity_merges`; AUDIT-F pass | fixture_verified | not_run | Broker-generation and repeated-root deduplication do not test shared/fork/subagent/release usage events. No event stream fixture exists, so event-level deduplication remains unverified. |
| F29 | Scope-specific fixtures cover Claude, Z.AI, Cursor/Grok, OpenRouter completed-day history, and Go | `crates/jackin-usage/src/usage/tests.rs::claude_scope_restriction_error_is_explicit`; `crates/jackin-usage/src/usage/zai/tests.rs::zai_time_limit_splits_mcp_and_web_search`; `crates/jackin-usage/src/usage/cursor/tests.rs::grok_bot_pooled_or_zero_yields_no_meter`; `crates/jackin-usage/src/usage/openrouter/tests.rs::openrouter_key_maps_cap_remaining_and_period_spend`; `crates/jackin-usage/src/usage/opencode/tests.rs::opencode_one_percent_means_one_percent`; AUDIT-F pass | fixture_verified | not_run | The cited OpenRouter fixture covers key/cap/period spend, not completed-day history. No history collector or fixture exists; no live/native dashboard comparison. |

### G. Broker scheduling, UI, and process behavior

| ID | Requirement | Test path/name + exact result | Proof | Status | Gap or disposition |
|---|---|---|---|---|---|
| G01 | Empty Usage requests due data and shows registered rows | `crates/jackin-usage/src/usage/tests.rs::focused_usage_lifecycle_hides_before_start_and_refreshes_on_start`; `crates/jackin-usage/src/host/tests.rs::provider_glance_rows_empty_without_credentials`; AUDIT-F pass | fixture_verified | implemented | No live console. |
| G02 | Fresh cache avoids redundant provider requests | `crates/jackin-usage/src/usage/tests.rs::usage_cache_adopts_broker_generations_by_account_capability`, `usage_snapshot_reads_in_memory_cache`; `crates/jackin-usage/src/coordinator/tests.rs::coordinator_ambient_tick_honors_success_cooldown`; AUDIT-F pass | fixture_verified | implemented | No live broker process. |
| G03 | Open Usage refreshes periodically and shows update context | `crates/jackin-console/src/tui/screens/usage/tests.rs::heartbeat_due_only_after_first_completion`, `poll_refresh_delivers_ready_outcome_and_clears_in_flight`; `crates/jackin-usage/src/coordinator/tests.rs::cadence_poll_due_fires_once_per_interval_and_honors_success_cooldown`; AUDIT-F/AUDIT-R pass | fixture_verified | implemented | No long-running UI session. |
| G04 | Idle/low-power behavior and wake/reconnect are explicit | `crates/jackin-usage/src/coordinator/tests.rs::cadence_wake_recalculates_without_missed_poll_burst`, `cadence_shared_retry_after_wins_over_periodic_due`; AUDIT-F pass | fixture_verified | implemented | No OS sleep/wake run. |
| G05 | Manual refresh joins active work and does not queue duplicates | `crates/jackin-usage/src/coordinator/tests.rs::coordinator_winner_joiner_and_force_join_share_one_generation`, `coordinator_post_terminal_manual_refresh_starts_later_generation`; `crates/jackin-usage/src/host/broker/tests.rs::subscribe_all_dedups_reuses_fresh_and_forces_only_on_demand`; AUDIT-F pass | fixture_verified | implemented | No keypress session. |
| G06 | Two and twenty clients share one same-account generation | `crates/jackin-usage/src/host/broker/tests.rs::usage_broker_twenty_clients_join_one_generation_and_probe`, `broker_client_scoped_operation_requires_relay_and_never_probes`; AUDIT-F pass | fixture_verified | implemented | Container JUnit for this exact head is absent. |
| G07 | Independent accounts progress concurrently; one stall does not serialize | `crates/jackin-usage/src/coordinator/tests.rs::coordinator_distinct_accounts_refresh_within_concurrency_bound`; `crates/jackin-usage/src/host/broker/tests.rs::healthy_accounts_publish_while_one_account_stalls`; AUDIT-F pass | fixture_verified | implemented | No provider network stall. |
| G08 | Closing a screen releases local subscription without cancelling shared work | `crates/jackin-usage/src/host/broker/tests.rs::unsubscribe_releases_local_interest_without_cancelling_shared_work`, `client_clone_forks_subscription_set`; AUDIT-F pass | fixture_verified | implemented | No live screen close. |
| G09 | Timeout/cancel cleans subprocesses and preserves broker ownership | `crates/jackin-usage/src/host/broker/tests.rs::probe_budget_returns_fast_and_expires_without_waiting`, `join_publication_timeout_leaves_broker_ownership_intact`; `crates/jackin-usage/src/coordinator/tests.rs::coordinator_timeout_wait_keeps_owner_until_worker_terminates`; AUDIT-F pass | fixture_verified | implemented | Synthetic worker only. |
| G10 | 401/403/429/5xx/timeout/malformed/offline recover distinctly | `crates/jackin-usage/src/coordinator/tests.rs::coordinator_rate_limit_without_provider_deadline_uses_shared_backoff`, `coordinator_empty_result_is_failure_and_preserves_last_good`, `coordinator_unsupported_result_stays_unsupported_without_quota`; `crates/jackin-usage/src/host/broker/tests.rs::discovery_provider_stale_and_error_views_are_retryable_failures`; AUDIT-F pass | fixture_verified | implemented | Provider-native 401/5xx live calls absent. |
| G11 | Restart/owner-loss/corrupt state recovers without crossing accounts | `crates/jackin-usage/src/coordinator/tests.rs::coordinator_recovers_persisted_owner_loss_once_without_a_herd`, `coordinator_unavailable_or_corrupt_state_makes_zero_provider_calls`; `crates/jackin-usage/src/host/broker/tests.rs::usage_broker_recovers_stale_guard_with_private_permissions`; AUDIT-F pass | fixture_verified | implemented | No process restart artifact. |
| G12 | Selection survives rename/order; removal returns Overview notice | `crates/jackin-console/src/tui/screens/usage/tests.rs::apply_refresh_preserves_selection_across_rename_and_reorder`, `apply_refresh_removed_selection_falls_back_to_overview_with_notice`; AUDIT-R pass | fixture_verified | implemented | Deterministic view state only. |
| G13 | Loading, empty, disabled, partial, failed, stale, recovered states render | `crates/jackin-console/src/tui/screens/usage/tests.rs::render_detail_overview_renders_all_windows_and_scrolling`, `render_detail_account_shows_freshness_and_refreshing_indicator`, `render_unknown_window_shows_value_without_fabricated_bar`; `crates/jackin-usage/src/usage/tests.rs::usage_detail_presentation_stale_keeps_buckets_and_one_detail`; AUDIT-F/AUDIT-R pass | fixture_verified | implemented | PNG baseline aggregate timeout remains H04 failure. |
| G14 | Required terminal sizes, Unicode labels, focus, scroll, resize | `crates/jackin-console/src/tui/screens/usage/tests.rs::render_full_route_narrow_and_wide`; `crates/jackin-console/src/tui/screens/usage/tests.rs::render_detail_overview_renders_all_windows_and_scrolling`; AUDIT-R pass | fixture_verified | implemented | Full PNG baseline test timed out; no live terminal resize. |
| G15 | Text conveys state without color; controls expose keys | `crates/jackin-console/src/tui/keymap/tests.rs::settings_content_shell_keys`; `crates/jackin-console/src/tui/screens/usage/tests.rs::usage_meter_renders_no_bar_for_unknown_percent`, `freshness_age_label_covers_phases_and_ages`; AUDIT-R pass | fixture_verified | implemented | No accessibility UI run in this audit. |
| G16 | 50-account cached-open/input latency meets measured targets | `crates/jackin-protocol/src/usage_broker/tests.rs::canonical_projection_v1_forty_account_fixture_stays_below_transport_margin`; AUDIT-F pass | fixture_verified (transport size only) | not_run | No 50-account latency measurement or target comparison. |
| G17 | UI/FFI/CLI adapters do not call providers directly | `crates/jackin-usage/src/usage/tests.rs::provider_connector_exports_physical_attempts_without_endpoint_material`; `crates/jackin-usage/src/host/broker/tests.rs::broker_client_scoped_operation_requires_relay_and_never_probes`; AUDIT-F pass | fixture_verified | implemented | Source inventory is fixture/test-backed, not a separate static scan artifact. |
| G18 | Console, CLI, Capsule, native views agree on shared fixture fields | `crates/jackin-usage/src/host/tests.rs::fixture_snapshot_matches_capsule_view_fields`; `crates/jackin-usage/src/usage/tests.rs::provider_tabs_include_cached_account_identity`; `crates/jackin-console/src/tui/screens/usage/tests.rs::projection_keeps_canonical_ids_and_freshness`; AUDIT-F/AUDIT-R pass | fixture_verified | implemented | No native live view comparison. |

### H. Migration and final local acceptance

| ID | Requirement | Test path/name + exact result | Proof | Status | Gap or disposition |
|---|---|---|---|---|---|
| H01 | Existing configuration converts or fails actionably without silent loss | `crates/jackin-config/src/migrations/tests.rs::config_migrations_chain_reaches_current`, `account_schema_preserves_existing_registry_and_assignments`, `v1alpha10_to_current_stamps_initialized_sentinel_without_touching_accounts`; AUDIT-F pass | fixture_verified | implemented | No user-home migration. |
| H02 | Schema transition is atomic/idempotent and obsolete path is removed | `crates/jackin-config/src/migrations/tests.rs::prop_config_migration_idempotent`, `prop_workspace_migration_idempotent`, `rejects_when_migration_path_was_removed`; `crates/jackin-config/src/editor/tests.rs::editor_save_atomic_staging_failure_preserves_every_original_file`; AUDIT-F pass | fixture_verified | implemented | No compatibility-shim audit beyond exact-head source inspection. |
| H03 | Protocol/build mismatch gives restart/upgrade error | `crates/jackin-capsule/src/config/tests.rs::invalid_staged_credentials_reject_with_explicit_upgrade_error`, `protected_credentials_reject_profile_mode_and_arbitrary_environment`; AUDIT-R pass | fixture_verified | implemented | Error text is fixture-verified. |
| H04 | Unit, integration, format, lint, docs, and snapshot gates pass | PR #1005 baseline `AUDIT-F` passed **1059/1 skipped** at `ca128f8`; baseline config/instance passed **567**; baseline broad gate passed **3836** across 41 binaries. Pre-sync coordinator rechecks at `72feee23` passed **1069/1 skipped**, **577**, and **3839/1 skipped** respectively; none is a current `3218706c` result. On the audit branch, post-merge types passed, docs tests passed **18/18**, roadmap/research passed, and repository-links passed. The historical pre-sync policy gate passed **11/11**, but is not a current or final-head gate; post-merge MDX/Vite build reached static prerender but exited 1 with `ECONNRESET` for the generated `/og/research/context/techniques/09-output-discipline.webp` route. `cargo xtask lint --strict` also remains red on inherited container-paths, telemetry-registry, ratchet, and test-layout violations. Historical `AUDIT-R` passed 3826/1 skipped but timed out at `crates/jackin-console/src/tui/view/png_baselines/tests.rs::png_baselines_screens_match` after 360s and exited 100; its focused rerun passed once in 153.035s. | fixture_verified (partial) | failed | Current aggregate/docs gate is not green; isolated historical rerun does not authorize an all-gates pass. |
| H05 | Apple Silicon macOS 26 plus OrbStack usage-broker E2E with JUnit | No current exact-head `cargo xtask ci --e2e` run or JUnit artifact; historical `target/nextest/docker-e2e/junit.xml` and `/tmp` evidence are absent | — | not_run | Mandatory container gate blocker: rerun on the target Mac and attach current JUnit. |
| H06 | Live provider/account matrix records identity and real fields | No current live-provider command or artifact; historical provider matrix is not independently reproducible here | — | not_run | Mandatory live matrix blocker; configured credentials and native outputs must be captured without secrets. |
| H07 | Two-Claude-plus-one-Codex real-account container scenario | `/tmp/tracer/evidence.md` and `/tmp/split.log` absent; no current target-head container/TUI run | — | not_run | Mandatory scenario blocker; do not reuse historical `live_verified` label. |
| H08 | Each requested client gets real container TUI smoke where credentials exist | No current live/container smoke artifacts for Claude, Codex, Amp, Kimi, Muse, Cursor, Grok, OpenCode, omp, Hermes, Gemini, or MiniMax | — | not_run | Mandatory per-client matrix is unrun; absent access is a gap, not `unsupported`. |
| H09 | Provider fields compare with native command/dashboard and timestamp | No current native command/dashboard capture or timestamped comparison | — | not_run | Mandatory comparison blocker; fixture parsers do not satisfy it. |
| H10 | Independent reviewer checks implementation and proof | `gh pr view 1002` returned `reviews: []`; `/tmp/review-providers.md` absent; this audit is an evidence pass, not an independent second reviewer | — | not_run | Mandatory reviewer sign-off remains open. |
| H11 | Limitations are precise states, not fabricated numbers | `crates/jackin-usage/src/usage/antigravity/tests.rs::gated_snapshot_reports_unsupported_without_running_usage`; `crates/jackin-usage/src/host/tests.rs::unavailable_and_refreshing_never_invent_percent`; `crates/jackin-usage/src/usage/openrouter/tests.rs::openrouter_credits_403_is_typed_scope_denial`; AUDIT-F pass | fixture_verified | implemented | Current ledger distinguishes unavailable, unsupported, failed, and not_run. |
| H12 | Final handoff names exact SHA, commands, results, fixtures, live coverage, blockers | This audit record names the starting #1002 and #1005 SHAs, worktree, commands, aggregate results, absent artifacts, and remaining blockers; the signed commit SHA is reported with the handoff | implemented | implemented | Do not mark any unrun row passed. |

### Current provider disposition for this audit

The older provider matrix below records historical lane claims. The table here
is the current exact-head disposition. All provider semantic fixtures were
included in `AUDIT-F` and passed; provider-specific authenticated/container
proof was not rerun.

| Provider surface | Fixture / semantic | Container | Live provider | Current exact gap |
|---|---|---|---|---|
| Claude / Anthropic | fixture_verified | not_run | not_run | No current OAuth/API account artifact. |
| Codex / OpenAI | fixture_verified | not_run | not_run | No current ChatGPT/native usage artifact. |
| Amp | fixture_verified | not_run | not_run | No current live-provider artifact; credential availability was not used to invent a result. |
| Antigravity / Google | fixture_verified | not_run | not_run | `gated_snapshot_reports_unsupported_without_running_usage` proves the headless/old-command fixture state; GUI/keyring-backed live support remains unverified. |
| Gemini CLI | fixture_verified | not_run | not_run | No current live-provider artifact; installation/credential state was not treated as proof. |
| Kimi | fixture_verified | not_run | not_run | No current live-grant artifact. |
| Z.AI | fixture_verified | not_run | not_run | No current live-provider artifact. |
| MiniMax | fixture_verified | not_run | not_run | No current authenticated success; do not promote historical canary claims. |
| Muse | fixture_verified | not_run | not_run | No current authenticated MSP observation. |
| Cursor | fixture_verified | not_run | not_run | No current stored-token artifact. |
| Grok / xAI | fixture_verified | not_run | not_run | No current live-token artifact. |
| OpenRouter | fixture_verified | not_run | not_run | No current key evidence; parser exists, host credential dispatch is not wired. |
| OpenCode | fixture_verified | not_run | not_run | No current auth.json/CLI credential artifact. |
| omp | fixture_verified | not_run | not_run | No current client/container artifact. |
| Hermes | fixture_verified | not_run | not_run | No current client/container artifact. |

Optional live-provider gaps are intentionally explicit: rerun authenticated
read-only lanes for any available Claude, Codex, Amp, Kimi, Muse, Cursor, Grok,
OpenCode, OpenRouter, Z.AI, MiniMax, Gemini, omp, and Hermes accounts; capture
identity, fields, timestamps, and failure class. These are not converted to
`unsupported` merely because current credentials/artifacts are absent. The
mandatory H05–H10 gaps above remain separate from these optional provider lanes.

## Historical evidence retained for provenance only

Everything below this boundary is a report from an earlier lane, not current
verification of PR #1002 at `3218706cf2b992ceeb20ca2d0ea280d4c6eae3d9`.
Historical values such as `pass`, `container`, or `live` preserve what the
earlier lane reported; they do not upgrade the current `not_run` gaps in H05–H10
or the current provider table above. The referenced `/tmp` and `target`
artifacts are not present in this audit worktree. A missing source SHA or raw
artifact is recorded as missing instead of being inferred.

### Superseded 176-era synchronized audit

The prior synchronized audit used source head
`176dcc0632f977a78d58443bde1b3ceb40304606`. Its recorded fixture results were
**1057 passed, 1 skipped** and **567 passed** for the config/instance gate;
roadmap reported 18 resolved `meta.json` files, research reported 63, and the
repository-link gate reported the same two pre-existing `preview.yml` errors.
The prior record did not retain a run date or raw artifacts. These values are
historical-only and must not be read as validation of current #1002 head
`3218706c`.

### Historical provider lanes

Common provenance: provider lane `01a0b122-2e86`, observed 2026-09-18. The
surviving lane note did not record its source SHA. The follow-up Kimi discovery
correction is source `06b64e2032fa39e8fd50662331c9ec4b519ffe89`; it does not
revalidate the other historical rows. Referenced artifacts
`/tmp/provider-catalog-ledger.md`, `/tmp/lane-*.md`,
`/tmp/tracer/evidence.md`, and `/tmp/split.log` are absent here. Every value
in this table is therefore historical-only.

| Provider | Historical parser/semantic | Historical service/process | Historical container | Historical Live Mac | Notes |
|---|---|---|---|---|---|
| Claude/Anthropic | pass | pass | pass (tracer A/B) | capped (default session-limited; claude-b OAuth expired) | keychain creds, oauthAccount cache |
| Codex/OpenAI | pass | pass | pass (tracer C) | live (ChatGPT login, claude+codex 3/3 green) | app-server 0.154.0, file backend |
| Amp | pass | pass | broker-only | live (alexey@zhokhov, credits shown) | XDG data secrets.json, auto-update pin |
| Antigravity/Google | pass | pass | broker-only | unsupported-headless (no CLI; GUI state unverifiable) | keyring singleton |
| Kimi | pass | pass | broker-only | live (`kimi -p` exit 0; default-kimi registered) | both families; 06b64e20 per-env grant fix |
| Z.AI | pass | pass | broker-only | unavail (no CLI/creds) | provider only |
| Muse | pass | pass | broker-only | live (native auth resolution green) | `.config/muse`, keychain resolved |
| Cursor | pass | pass | broker-only | live (stored-token auth green) | file+keychain lineages |
| Grok/xAI | pass | pass | broker-only | live (token refresh OK) | 1.0.30, embedded principal |
| OpenRouter | pass | pass | broker-only | unavail (no key configured) | provider only, exact model IDs |
| omp | pass | pass | broker-only | unavail (NOT installed) | broker file is not authz |
| Hermes | pass | pass | broker-only | unavail (NOT installed) | `hermes --tui` |
| OpenCode | pass | pass | broker-only | unavail (CLI present, 0 credentials) | 1.18.30, auth.json absent locally |
| Gemini CLI | pass | pass | broker-only | unavail (NOT installed) | separate Google client |
| MiniMax | pass | pass | broker-only | failed (canary-d in-band 1004 login fail; placeholder key) | shell provider routes verified |

### Historical environment snapshot — T00 (2026-09-17)

Source SHA: `21232c7e218026c2a3ecf34795acc9ca322330f6`. Provenance is the
recorded T00 environment capture below; its raw command output is not retained
in this worktree. This snapshot is not the current audit environment.

- HEAD at session start: `21232c7e`, branch `feat/multi-account-support`, tree clean.
- macOS 26.6.2 arm64; Rust 1.97.1; nextest 0.9.140; node 24.18; bun 1.3.14.
- Installed: claude 2.1.274, codex 0.154.0, amp 0.0.1789639648-g3c529d, agy 1.2.5,
  kimi 0.43.0, muse 1.3.0, cursor-agent 2026.09.10, grok-build 1.0.30,
  opencode 1.18.30. Missing: gemini, omp, hermes, mmx.

### Historical CI watch — PR #1002 (2026-09-17)

Source SHA: `6f0280c4b0f1bc2e7196f2d276cf6262542f37ff`. Provenance is the
GitHub Actions/check observation recorded below; no raw local log artifact is
retained here. These results predate the exact-head audit.

- `Rust · jackin` + `Rust · jackin-runtime`: FAILED at `Set up Mr. Boxington` (cache setup, before any build/test) — same mbx infra-flake signature as head 3c807144 (`Quota exceeded`), not code. Siblings (capsule/config/console/core/usage) PASS on this head. Rerun blocked while the workflow runs; the S4 push supersedes with a fresh full run.
- `Policy` (Velnor workflow policy): FAILED on `generated-tree` drift vs pinned generator 06050c9f. Branch has zero diff vs main under `.github-gen/` + `.github/` — inherited main breakage, out of scope (generated files are never hand-edited). Recorded, not fixed.

### Historical gates and console-live (2026-09-17/18)

Source SHAs: `06b64e2032fa39e8fd50662331c9ec4b519ffe89` for the post-compaction
discovery/gate work and `a219c97e6cee87b01dff0a4f380cf97d2a7e4d68` for the
full E2E/desktop observations. Provenance includes the JUnit path and `/tmp`
paths named in the bullets; those artifacts are absent from this audit
worktree. The results are historical-only and are not current H05–H10 proof.

- Console-live: `jackin console --debug` under PTY, Settings → Accounts renders all
  accounts (canary-d masked, claude-b, 8 defaults, +Add rows); Ctrl-Q confirm exits 0.
  Evidence: /tmp/tracer/evidence.md.
- Focused gates green: fmt, clippy, nextest 839 + 3959 passed.
- `cargo xtask ci` findings fixed: E0063 in reactive_daemon/tests.rs (all-features-only
  module missed the account_id/instance fields); lint container-paths
  (capsule_setup.rs:133 → container_paths::JACKIN_ROOT), telemetry-registry
  regenerate, ratchet test-layout (launch_runtime.rs inline tests → sibling file).
- Docs gates fixed (pre-existing main breakage from velnor regen #982/#992):
  repo-links 17 stale workflow refs repointed (docs/construct/jackin-dev →
  generated ci-pr.yml units; desktop-cadence → desktop-merge.yml; preview.yml →
  planned-workflow prose since SAN pin + xtask preview.rs still reference it);
  brand 42 (`Jackin` → `jackin❯` in 4 root companion docs); map-check 3 crates
  (telemetry t0, otlp-testbed t3, usage-ffi t4) added to codebase-map.
- Lane results (pre-compaction gates agent): docker-e2e usage_broker_e2e 12/12 PASS
  (JUnit target/nextest/docker-e2e/junit.xml); desktop-ci PASS (Rust 454, Swift 78+2);
  desktop-merge FAIL on testOverviewPassesAccessibilityAudit (85 contrast/label
  findings, native/ untouched by branch — pre-existing; dedicated fix running).
- Launch-resolver fix 9cc2d675 (2026-09-18): `resolve_launch` ignored the
  committed agent and `account_bindings`, so valid-default multi-account
  configs failed `multiple accounts are eligible` and parked on the ack
  dialog; dind_e2e chaos tests timed out with no container. Fix adds the
  agent-scoped per-agent binding layer (role → workspace → global) plus
  agent-scoped sole-eligible fallback; 5 new regression tests; config 392,
  env+console 1386, runtime+jackin 1284 all green. Focused
  `chaos_drop_control_socket` e2e: `ci gate OK`. Follow-up: interactive
  picker launches still failed the same way because
  `resolve_provision_inputs` used `opts.agent` (CLI override only, `None`
  for picker commits); it now takes the committed agent. Sentinel dind_e2e
  green in 13.6s (was 3x300s timeout).
- Full `cargo xtask ci --e2e` GREEN at a219c97e (2026-09-18, exit 0,
  `ci gate OK`, 19 steps): docker-e2e JUnit 23/23 PASS — dind 9/9 (chaos
  trio, sentinel, agentsmith, 4 exit-gates), load_options 1/1, session_send
  1/1, usage_broker_e2e 12/12 (2/20-client single-flight host+dind,
  owner loss, timeout ownership, shared deadlines, capability isolation,
  distinct-account concurrency, unavailable-state zero calls).
  JUnit: target/nextest/docker-e2e/junit.xml. (One intermediate full run was
  SIGTERM-murdered externally at 6116/6117 with only the 60s png-baseline
  test in flight; that test passes alone in 60.7s — not a code failure.)
- desktop-merge at a219c97e (2026-09-18): desktop-ci parts GREEN
  (bindings-check, Rust 454, Swift 78+2, 0 failures); desktop-test-ui
  BLOCKED — Mac is at the lock screen ("Touch ID or Enter Password",
  screenshot /tmp/screen-check4.png), so no app can activate:
  testEmptyUsageStateIsDistinct fails `Failed to activate application ...
  (current state: Running Background)` 3x deterministically, incl. under
  `caffeinate -d -u`. Re-run `mise run desktop-test-ui` after unlock.
  (Prior unlocked run failed testOverviewPassesAccessibilityAudit with 85
  pre-existing contrast/label findings; native/ app code untouched by
  branch — separate change if still red after unlock.)
- desktop-merge GREEN end to end on final tree (2026-09-18, exit 0):
  desktop-ci exit 0 (bindings-check, Rust 454, Swift 78+2) + desktop-test-ui
  19/19 incl. scroll + all 3 AX audits (JUnit 19 tests, 0 failures).
  Fixes on the way: (a) test locator buttons["Retry"] → element("usage.retry")
  + label assert; (b) moved usage.global-error identifier from
  ContentUnavailableView container (shadowed all children incl. the Retry
  button) onto its Label — UsageWindowRoot.swift; (c) scroll() helper
  re-activates + retries on focus steal; (d) broker test
  projection_refresh_runs_due_checks_and_join_settles chases superseding
  publications to Idle (publish_due mints a fresh id per intermediate
  snapshot; single join could observe Refreshing under load — branch-new
  race, now deterministic, 8/8 stress + package 454/454).
  Full `cargo xtask ci --e2e` green (exit 0, ci gate OK, 23/23 docker-e2e)
  stands from a219c97e; post-merge Rust delta is the broker-test-only fix.
- Fixture HTTP harness race fixed: accepted sockets inherit the listener's
  nonblocking mode on macOS, so read timeouts never applied and
  `read_request` failed with WouldBlock whenever the server thread outran
  the client write (empty-EOF/RST flakes under parallel load).
  `serve_one` now restores blocking mode first. Package 28/28 x6.
  Production broker unaffected (explicit WouldBlock loops with deadlines).

### Historical provider live matrix (lane 01a0b122-2e86, Mac 2026-09-18)

The surviving lane note does not record the matrix's source SHA. Its named
follow-up source is `06b64e2032fa39e8fd50662331c9ec4b519ffe89` (Kimi discovery
fix), but that does not prove the other rows at that source. The matrix's raw
`/tmp` artifacts are absent. The entries below are historical reports only;
current exact-head provider status is in the table above and H05–H10 remain
open.

- live-verified: codex (ChatGPT login), amp (alexey@zhokhov, credits), muse,
  cursor (stored token), grok (refresh OK), kimi (`kimi -p` exit 0; jackin
  `default-kimi` registered after discovery fix).
- auth-verified/capped: claude default (session-limit msg, quota-capped).
- unavailable/expired: claude-b scentbird (OAuth expired, no refresh).
- unsupported headless: antigravity (no CLI; GUI app state unverifiable).
- unavailable (no CLI/creds): gemini-cli, zai, openrouter, omp, hermes, opencode
  (CLI present, 0 credentials).
- failed: minimax canary-d (HTTP 200 in-band 1004 login fail; placeholder key).
- Parsers: all 15 lanes exist with tests; `nextest -p jackin-usage` 442/442.

### Historical split/resize live (2026-09-18)

Source SHA was not recorded in the surviving tracer note. Provenance is the
same historical tracer container and the absent artifacts
`/tmp/tracer/evidence.md` and `/tmp/split.log`; no current container/TUI proof
is claimed.

- Palette split Right with claude-personal: 2 panes, per-pane account_id/agent
  correct in snapshot; 2x Alt-Shift-Left moved divider col 40 -> 32; both panes
  alive after client detach. Evidence: /tmp/tracer/evidence.md, /tmp/split.log.
