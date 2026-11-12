# A14 OAuth integration

Date: 2026-11-10. Base: `6d22dc1015340facc54fb9c13d4cb9a531996f37`. Worktree: `/tmp/jackin-a14-oauth-integrated`. Branch: `wt/a14-oauth-integrated`.

## Inputs and review

The integration applies `17aacafcfa08300200bea25a8d5562268efaaf04` without its `.audit` files, then applies the complete 30-file snapshot from `/tmp/jackin-a14-oauth-files`. The snapshot at `/tmp/a14-oauth-files-snapshot.diff` has SHA256 `d72c284d2cc04effe7404367a456c9fc48cdcc133c8955cd5a26571433f6b95d`. A fresh capture from the source worktree matched it byte for byte.

The supplied addendum in `/tmp/a14-oauth-independent-review.md` records `REVIEW_PASS` for that snapshot. This integration does not claim a new independent review.

## Conflict resolution

Ten files had textual conflicts. `catalog.rs` retains the supported T09 control and adds the T03 state assertions. The account lock and E2E gateway fixtures retain supported T18 static launches because T01 remains unavailable. `lib.rs` removes `grok_auth_file` but retains the public `render_codex_config` API. The launch credential test adds the bare-prefix case without reviving a T01 launch. The multi-instance imports combine both file planners. The OMP models fixture retains T09's exact `https://api.z.ai/api/anthropic` URL and `anthropic-messages` API. The Codex plan tests retain pure T01 and T20 rendering rather than plan an unavailable T01 launch. The file plan test uses generic `static_files` with the current T18 model. The CLI load tests combine per-instance coverage with exact availability refusals.

The approved credential-collision rule also required semantic fixture reconciliation. Pure identity fixtures now use noncolliding providers. Launch fixtures use supported T14 plus T18 or T14 plus T09 pairs instead of sharing `ZAI_API_KEY` between T09 and T18. The wrong-provider test creates an explicit non-ZAI account. The TUI tuple test sets its new ZAI account as the default. No production behavior was weakened to preserve an invalid fixture.

T02 remains `BlockedVendor` with `vendor clarification pending`. T03 remains `Unavailable` with `not yet proven on a real gateway`. The integration retains generic `OAuth2RefreshToken`, exact T02 and T03 profiles, native placeholders, `GROK_SESSION_TOKEN` without `XAI_API_KEY`, generic per-instance files, and fail-closed placeholder writes. It includes no role-binding or A11b lane changes.

## Verification

Rustfmt passed. The focused hermetic suites passed 770 of 770 tests with zero skipped. They cover core, session, OpenShell, TUI, CLI, and testkit behavior.

The required root command is `rtk env NEXTEST_TEST_THREADS=2 mise exec -- mbx +1.99.1 xtask ci --fast`. Its result is pending.

No live gateway, Docker, real credential, or live agent was used. No live support is claimed.

## Remaining live gates

T02 still needs vendor clarification, refresh and rotation, native placeholder compatibility, host safety, two-gateway isolation, replay, export, process-state, and billing evidence. T03 still needs real synthetic and subscription `xtask e2e` proof of helper login, headless rerun, gateway refresh with a stable placeholder, effective `grok-4.7`, trusted supervised launch, bounded prompts, subscription attribution, host usability, and second-sandbox isolation. A later integration with the account lane may require another G-FAST run.
