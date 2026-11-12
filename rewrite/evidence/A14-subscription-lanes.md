# A14 subscription lanes — T02, T05, T01 live results

Date: 2026-11-09. Worktree `/tmp/jackin-live-sub` (branch `wt/live-sub`, HEAD `74bf8ac22`, same head as `feature/openshell-native`). Authority: operator order of 2026-11-08 (`rewrite/evidence/operator-order-2026-11-08.md`), session brief of 2026-11-09. No tuple flipped, no code edited, no secret read or printed. Keychain checks are existence-only; in-sandbox scans print only modes, sizes, counts, and the resolve-marker shape.

Build and environment: `mise exec -- mbx +1.99.1 build -p jackin` at HEAD, clean. Gateway `jackin-local` connected; `sandbox list` empty before and after. Config backed up before the launch and restored after. Catalog at HEAD: T02 and T05 `TupleState::BlockedVendor` ("vendor clarification pending"); `account add --from-host-login` accepts the grok and muse providers only (`crates/jackin/src/commands/account.rs`, provider match before any credential access).

## T02 claude × Anthropic subscription — launch refused, host login intact

- Host before, 2026-11-09T07:23:44Z: `claude -p "Reply with exactly: PONG"` -> `PONG`, exit 0. Keychain item `Claude Code-credentials` EXISTS (existence check only; no `~/.claude/.credentials.json`, as in A14 prep).
- Import, 07:23–07:24Z, run twice: `./target/debug/jackin --gateway jackin-local account add claude --name claude-host --from-host-login` -> `error: --from-host-login needs the grok or muse provider`, exit 2, both runs. The provider check runs before any credential access, so no Keychain read was attempted and the "Keychain read may need operator" case cannot arise at HEAD. The claude host-login import does not exist.
- Launch, before 07:24:41Z: `./target/debug/jackin load sentinel /Users/donbeave/Projects/jackin-project/jackin-sentinel --agent claude --provider claude --account claude-host --gateway jackin-local --task "Reply with exactly: PONG"` -> `error: tuple claude with provider claude is not supported`, exit 2 (`crates/jackin/src/commands/load.rs` tuple-state check; `crates/jackin-session/src/plan.rs` refuses non-Supported tuples the same way). No sandbox existed immediately after (`sandbox list` -> none). A `workspace add sentinel <role path>` was needed first; removed by the config restore.
- Host after, 07:24:41Z: `claude -p "Reply with exactly: PONG"` -> `PONG`, exit 0; keychain item EXISTS.
- Outcome: the sandbox launch is not permitted for T02 at HEAD; the planner/CLI refusal of the `BlockedVendor` tuple is the recorded outcome. Host login works before and after. T02 stays blocked-vendor; the class O steps of A14 prep remain unrunnable until the import exists and the tuple is flippable.

## T05 agy × Google Antigravity subscription — launch refused, no-keyring gate unproven

- Host before, 07:24:52Z: `agy -p "Reply with exactly: PONG"` -> `PONG`, exit 0.
- Import, run twice: `./target/debug/jackin --gateway jackin-local account add antigravity --name agy-host --from-host-login` -> `error: --from-host-login needs the grok or muse provider`, exit 2, both runs. The antigravity host-login import does not exist.
- Launch: `./target/debug/jackin load sentinel <role> --agent agy --provider antigravity --account agy-host --gateway jackin-local --task "Reply with exactly: PONG"` -> `error: tuple agy with provider antigravity is not supported`, exit 2. No sandbox created.
- Host after, 07:25:13Z: `agy -p "Reply with exactly: PONG"` -> `PONG`, exit 0.
- No-desktop-keyring behavior: NOT PROVEN. No sandbox launch is permitted while T05 is `BlockedVendor`, so the no-keyring gate (A14 prep T05 step 4) cannot run at HEAD. T05 stays blocked-vendor; its deciding proof is still pending behind the vendor rule and the missing import.

## T01 codex × ChatGPT subscription — launch works, stored login is dead at the vendor

- Host auth metadata before, 07:25:36Z: `~/.codex/auth.json` mode `-rw-------`, 4250 bytes, mtime `2026-10-29 22:50:55 +0700` (unchanged since A9).
- Launch, 07:25:43Z: `./target/debug/jackin load sentinel <role> --agent codex --account codex-local --gateway jackin-local --task "…a14-t01-proof.txt…"` -> image cached, `provider: skipped, the provider exists`, sandbox `jackin-sentine-vua7` Ready, exit 0.
- Fixed prompt, 07:26:43Z and retry 07:27:56Z: `env HOME=/sandbox/.jackin/homes/codex codex exec --sandbox danger-full-access -c check_for_update_on_startup=false "Answer exactly: PONG"` -> NO PONG, both runs. Repeated `401 Unauthorized … Provided authentication token is expired. Please try signing in again.` (`token_expired`) from `chatgpt.com/backend-api/codex/models` and workspace routing, reconnects exhausted, exit 1.
- Credential delivery verified working: `/sandbox/.jackin/homes/codex/.codex/auth.json` mode 600, 468 bytes, written at launch (07:25:47Z), `last_refresh` stamped `2026-11-09T07:25:47Z`, one `openshell:resolve` reference, `"OPENAI_API_KEY":null` — the A9 shape. Delivery and resolve are not the failure.
- Root cause, gateway evidence: `openshell -g jackin-local provider refresh status jackin-codex-local-codex` -> credential `CODEX_AUTH_ACCESS_TOKEN`, strategy `oauth2_refresh_token`, status `investigation_required`, recovery `investigate`, failure `oauth_unrecognized_error_response`: "token endpoint returned HTTP 401 Unauthorized without a recognized OAuth…", `NEXT_REFRESH 2026-11-09 07:30:24`. The gateway attempted the refresh during the run and the stored refresh token is rejected by OpenAI. The stored `codex-local` login (access and refresh tokens) is expired/revoked at the vendor; the host copy and the stored copy have diverged since the account was added.
- Host unchanged after, 07:29:46Z: identical mode, size 4250, mtime `2026-10-29 22:50:55` — the host login file is untouched by the sandbox runs.
- Remediation surface, untested here: `jackin account add codex --name … --chatgpt-from <CODEX_HOME>` re-import, then re-run the fixed prompt. That needs its own proof that the host login is still live and that a fresh import restores the lane; it is not part of this re-check.
- Regression vs A9 (2026-11-06 pass): the lane degrades silently when the stored subscription login goes stale — launch, delivery, and resolve all succeed and the failure only appears as `token_expired` inside the sandbox. The gateway refresh machinery did fire and did record the failure.

## Cleanup defects and end state

- `jackin purge jackin-sentine-vua7 --force` printed `sandbox jackin-sentine-vua7 is already gone`, exit 0, while `openshell -g jackin-local sandbox list` still showed the sandbox Ready; `openshell -g jackin-local sandbox delete jackin-sentine-vua7` removed it. Same family as the A4 cleanup defects; recording only.
- End state, 2026-11-09T07:31Z: sandboxes none; workspaces the original 5 (config restored from backup); no `jackin-claude-host-claude` or `jackin-agy-host-agy` provider exists (both imports died before writing); host logins claude PONG, agy PONG, codex auth.json untouched.
