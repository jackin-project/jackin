# A14 T03 promotion

Date: 2026-11-10. Base commit: `97c7a8ce644fa84b962c9f22cf724c181efa9450`. Worktree: `/tmp/jackin-a14-t03-promotion`. Branch: `wt/a14-t03-promotion`.

## Independent verdict and scope

Reviewer agent-111 returned **VERIFIED** for `rewrite/evidence/A14-t03-live-final.md` after the evidence corrections. The final review closed the chronological contradiction at line 51 and found no remaining issue in the reviewed evidence. Reviewer agent-96 approved the T03-only promotion amendment before that verdict.

The verified live record proves Grok Build 1.0.77 with an xAI subscription on `jackin-docker` and `jackin-local`. It records exact `PONG` responses, real terminal calls, refresh and post-refresh responses, trusted supervised launch, replay, shell persistence, export, and purge on both gateways. It also proves second-sandbox isolation on `jackin-docker`. Both gateways used the requested model `grok-4.7` and returned effective model `grok-4.7-build`. The effective reply is not a selectable catalog model.

The report preserves the attempt1 HTTP 402 qualification. The retained detector was a false-positive observation on launch metadata, not reliable HTTP status or quota evidence. The original substring is unknown, so the report does not claim that no HTTP 402 occurred. No retained Grok error response reports HTTP 402.

Only T03 changes to `Supported` with reason `None`. T02 remains `BlockedVendor` with `vendor clarification pending`. Every other tuple keeps its prior state and reason. The stale `rewrite/blockers/T03-grok.md` file is deleted.

## Product and coverage changes

- `crates/jackin-core/src/agent.rs` changes only the T03 state and reason. The catalog model and default remain exactly `grok-4.7`.
- `crates/jackin-core/tests/catalog.rs` pins all 22 states and reasons. It also proves that `grok-4.7-build` and `grok-4.6` are unknown models.
- `crates/jackin-core/tests/agent_table.rs` retains trusted `grok --trust`, `GROK_SESSION_TOKEN`, the provider hosts, and the absence of `XAI_API_KEY` while expecting `Supported` and `None`.
- `crates/jackin-core/tests/manifest.rs` accepts default and explicit `grok-4.7` role mappings and rejects `grok-4.7-build`. The unchanged unsupported-agent control now uses T04.
- `crates/jackin-session/tests/plan/availability.rs` proves lead and secondary planning, default and explicit model acceptance, secondary `plan_multi`, trusted launch, and external-session configuration. It rejects `grok-4.7-build` with `unknown model grok-4.7-build`.
- `crates/jackin-session/tests/plan/grok.rs` retains the helper, malformed-placeholder, and lead and secondary file coverage. The config has mode 0600, disables API-key authentication, uses only `GROK_SESSION_TOKEN`, and creates no generic `auth.json`.
- `crates/jackin/tests/cli/grok.rs` proves real CLI default and explicit launches. Generic `OAuth2RefreshToken` publication retains the access token, refresh token, and client ID. The tests reject `grok-4.7-build` with `ExitClass::Usage` and no builds, providers, or sandboxes. A host `XAI_API_KEY` canary and raw OAuth tokens do not reach sandbox files, environment, output, or errors.
- `crates/jackin-tui/tests/launch/grok.rs` proves a supported console offer with only `grok-4.7`, a direct `LaunchEvent::Ready`, exact T03 identity, trusted launch, and host session authentication. An env-only Grok account is not selected.
- `crates/jackin-testkit/src/gateway/handle.rs` exposes the created main process so the CLI and console tests can inspect the actual `grok --trust` arguments.
- `crates/xtask/tests/repo_gates.rs` requires both gateway names, requested and effective model identities, generic OAuth material, trusted launch, and the HTTP 402 qualification in the live record. It also requires the T03 blocker to be absent and the T02 blocker to remain unchanged.
- `rewrite/evidence/A14-t03-live-final.md` is the independently verified live record.

The existing account publication and invalid-grant tests in `crates/jackin-session/tests/accounts/host_login.rs` remain unchanged. No production change was needed in `crates/jackin-session/src/accounts.rs` or `crates/jackin-session/src/files.rs` because the approved behavior already existed at the base commit.

## Red and green controls

The old-state focused run was nextest run `3d56d45b-4a7b-43a3-935a-d82d5f178ae9`. It ran 19 tests with 9 expected failures and 10 passes. Failures covered the catalog state, role-manifest availability boundary, planner, CLI, console, and stale blocker. Existing OAuth publication, helper, model declaration, and lead and secondary file tests passed.

After the T03 state change and blocker deletion, the first fixed run passed 17 tests and failed 2. Those failures asserted execution-time provider credentials in the fake `SandboxCreate.environment()`, which is the wrong boundary. The tests now assert the provider's `GROK_SESSION_TOKEN` key and rendered helper instead. No production code changed for this correction.

The restored fixed control was nextest run `e0c0e498-6bcc-49a1-be18-7e447a4ee403`. All 19 tests passed. The full core suite then passed 189 tests, including the exact all-22 matrix and updated role-manifest controls.

## Wrong-direction mutants

Each mutation was independent and was restored before the next run.

1. T03 returned to `Unavailable` with its old reason. Run `96134179-56ef-45f3-b081-af5d3e97a4ac` failed 8 of 19 tests across the catalog, planner, CLI, and console.
2. Supported T03 gained `Some("incorrect mutation reason")`. Run `5af63b51-dc8e-44af-85f0-7478a0da83fe` failed all 4 catalog and console checks.
3. The catalog and default model became `grok-4.7-build`. Run `9e699f22-9e18-43fe-b102-c68a902d65a3` failed all 4 catalog and CLI checks.
4. `grok-4.7-build` was added beside `grok-4.7`. Run `9c418419-0652-4c00-a1ca-34c452d9d4b3` failed all 3 exact-model checks, including CLI acceptance of the invalid model.
5. `--trust` was removed. Run `781432fd-7489-4e64-8094-21a72a94baef` failed all 5 core, planner, CLI, and console launch checks.
6. Generic `OAuth2RefreshToken` became `StaticKey`. Run `1b47bd2c-0f63-413a-98d6-be5a268a0c2c` failed the retained account publication test because the refresh grant disappeared.
7. `disable_api_key_auth` became false. Run `04e0971e-2d63-444c-88d7-98a32d42d117` failed all 5 planner, CLI, and console configuration checks.
8. The evidence file was deleted, its `grok-4.7-build` distinction was erased, and the stale blocker was restored in three separate runs. Runs `3b86f312-8fd1-458f-aed7-61a9a18b095e`, `d531fb4f-2aab-4aa1-94ff-0c7fa0fe30d5`, and `ca66e140-08cf-4c57-aa01-47b52713a292` each failed the repository-record test.
9. T02 changed to `Supported`. Run `68d4f5d7-b4a1-4fb4-bae5-2a7d2e7812c8` failed both unchanged blocked-vendor controls.

A supplementary old-state run covered the updated role-manifest test after the broader core suite found its stale expectation. Run `e585c211-6d4b-4bcb-8dbc-3a00bb4d269d` failed with `UnsupportedAgentSection { agent: GrokBuild }`. After restoration, run `832d2203-feb2-4d51-8d40-4c6b7ef7a818` passed.

`git diff --exit-code` confirmed that `accounts.rs` and `files.rs` had no residual mutation. `cmp` confirmed that the restored live evidence matched the independently verified source. `git diff --check` passed.

## Focused suites and gate

The focused suites used `NEXTEST_TEST_THREADS=2` and `mise exec -- mbx +1.99.1` with `--all-features --locked`.

- `jackin-core`: 189 passed, 0 skipped, run `84ca7e7f-31f4-40dd-a124-c7cd153c2987`.
- `jackin-session --test accounts --test plan`: 236 passed, 0 skipped, run `f29fca1b-4c80-4247-818a-eb1683bb10d3`.
- `jackin --test cli`: 182 passed, 0 skipped, run `ec1bada6-6fae-429c-9d15-d15086d0933b`.
- `jackin-tui --test launch --test console`: 281 passed, 0 skipped, run `f63f6f70-5fde-4bfd-a19a-b61e0e946fa1`.
- `jackin-xtask --test repo_gates`: 61 passed, 0 skipped, run `97d2aba6-4c96-498c-9dae-6baceb620bb4`.

The first root G-FAST run stopped at clippy because the console test cloned a provider name to create a one-item slice. The test now uses `std::slice::from_ref`. Nextest run `76cf751e-8d96-442c-82fb-20768d7b5e96` passed the console test, and package clippy passed before the full gate rerun.

The final root gate is pending:

```sh
rtk env NEXTEST_TEST_THREADS=2 mise exec -- mbx +1.99.1 xtask ci --fast
```

No live gateway, Docker command, real credential, or live agent ran during this promotion unit. No `.audit` file is included in the commit. No push occurred. A different independent reviewer still needs to verify the final change before integration.
