# A14 final live proof for T03

Date: 2026-11-10. Base commit: `94d880efefe94cc9915c9369d15a8cb6a3507394`. Worktree: `/tmp/jackin-a14-t03-final-oauth`. Branch: `wt/a14-t03-final-oauth`.

## Verdict

**PASS** for Grok Build 1.0.77 with an xAI subscription on `jackin-docker` and `jackin-local`.

Both gateways returned exact `PONG` with effective model `grok-4.7-build`, ran a real terminal tool, refreshed the imported OAuth grant, and returned exact `PONG` after refresh. Both passed cache, process, credential, egress, supervised-response, replay, shell-persistence, export, and purge checks. Docker also passed second-sandbox isolation. Host Grok answered after each gateway run and after cleanup.

No run used `XAI_API_KEY`, another API key, a paid-provider fallback, Podman, or a host execution fallback. Docker was used only for BuildKit image builds through `jackin-image`. No final run displayed API-key mode or a directory-trust prompt. The ambiguous attempt1 402 observation is qualified below rather than presented as quota evidence.

The proof changed only the T03 tuple state temporarily to permit launch. That edit was restored. No commit or push occurred. Independent review and a separate catalog-promotion change remain necessary before the product tuple can become `Supported`.

## Proof setup

`openshell --version` reported 0.1.7. `jackin doctor` passed gateway registration, health, version, and engine checks on both gateways. The proof binary was `target/debug/jackin`, built successfully with:

```sh
rtk mise exec -- mbx +1.99.1 build -p jackin
```

The only product-source edit changed T03 from `TupleState::Unavailable` to `TupleState::Supported` and removed its reason during the proof. `git diff --exit-code -- crates/jackin-core/src/agent.rs` exited 0 after restoration.

The isolated configuration used `XDG_CONFIG_HOME=/tmp/a14t03f-xdg`. Its OpenShell gateway directories were symbolic links to the existing mTLS registrations. The main jackin configuration was not changed. The temporary sentinel clone started at `6722f8024623c76726ce0293fc7e93da47b5ee74`. Role `a14t03f-sentinel` added only the Grok instruction mapping and passed `jackin role check`.

Every owned name used the `a14t03f` prefix. The docker account and workspace were `a14t03fd`. The local account and workspace were `a14t03fl`. Only one lead Grok instance ran in each primary sandbox, so no secondary-instance configuration was used. Docker BuildKit built images only through `jackin-image`. OpenShell 0.1.7 ran every sandbox and sandbox command.

Each account import used this host-only flow:

```sh
jackin account add grok --name <owned-account> --agent grok --from-host-login --gateway <gateway>
```

The publication path used generic `OAuth2RefreshToken` material. No credential appeared in argv, the evidence, the decision trail, or the summary JSON files. The capture environment removed `XAI_API_KEY`, `GROK_SESSION_TOKEN`, and alternate paid-key variables before each host-side proof command.

## Refresh sequence and host safety

The host baseline reported `You are logged in with grok.com`, default model `grok-4.7`, and exact response `A14T03F-HOST-BEFORE` with `grok-4.7-build` usage.

The first successful docker rotation completed at `2026-11-10T06:26:18Z`. Its status changed from `configured` to `refreshed`, with expiry `07:26:18` and next refresh `07:21:18`. The attempt4 summary records a following exact `PONG` with 15,995 total tokens, but its original response capture was overwritten. The retained `a14t03fd-refresh-after.stdout` is provider status, not response evidence. A new targeted rotation at `2026-11-10T07:17:44Z` independently proves this requirement. Its exact post-refresh `PONG` completed at `2026-11-10T07:17:50.260964Z` with 14,047 total tokens, one model call, and session ID `01a1c451-e997-7e23-855b-7152d7bd29f6`. The unique artifacts are `a14t03fr-refresh-rotate.stdout`, `a14t03fr-refresh-after.stdout`, `a14t03fr-post-refresh-pong.json`, and `a14t03fr-post-refresh-pong.capture.json`.

A later import from the old host grant failed rotation at `2026-11-10T06:32:12Z` with `invalid_grant`. This was an OAuth reauthorization error, not HTTP 402 and not quota evidence. Host Grok still answered `A14T03F-HOST-AFTER-DOCKER` after that gateway run.

The subsequent manual OIDC recovery first timed out and left Grok reporting `Not signed in`. A second `grok login --oauth` used the existing browser Grok authorization page and completed at `2026-11-10T06:41:13Z`. Host Grok then answered `A14T03F-HOST-RESTORED`. The temporary Safari AppleScript setting used to approve that page was removed. No API key was used. This incident is recorded here because the final green summaries alone do not describe the host recovery.

The final docker lifecycle run did not rotate again. It completed at `2026-11-10T06:50:57Z`, and host Grok answered `A14T03F-HOST-AFTER-DOCKER-PASS` afterward.

The only rotation in the local sequence completed at `2026-11-10T06:53:01Z`. Local status reported `refreshed`, expiry `07:53:01`, and next refresh `07:48:01`. The next request returned exact `PONG` with 15,996 total tokens. Host Grok answered `A14T03F-HOST-AFTER-LOCAL-PASS` after the local run and cleanup.

No host request, sandbox request, or gateway rotation ran concurrently with another rotation of the same grant. The local rotation was the last grant mutation in the original lifecycle. The targeted Docker rotation at `2026-11-10T07:17:44Z` was the last grant mutation across the complete proof and re-review.

## Docker proof

The primary sandbox was `jackin-a14t03f-oazw`, with launch ID `a14t03fdockerproof00000001` and provider `jackin-a14t03fd-grok`. The second sandbox was `jackin-a14t03f-0nxi`, with launch ID `a14t03fsecondproof00000001`.

### Cache and process state

A reload with the primary launch ID printed `image construct: cached`, `image derived: cached`, `image: skipped, the image is cached`, `sandbox: skipped, the sandbox exists`, and `upload: skipped, the project is uploaded`.

The sandbox reported Grok 1.0.77, `uid=1000(sandbox)`, and no `sudo`. The process tree showed:

```text
/bin/sh /usr/local/bin/jackin-start grok --trust
/usr/local/bin/grok --trust
```

The Grok process was a direct child of `jackin-start` and remained so after both shell panes.

### External-session configuration and placeholders

`/sandbox/.grok/config.toml` had mode 0600 and owner `sandbox:sandbox`. It contained `default = "grok-4.7"`, `disable_api_key_auth = true`, and the `[auth]` helper that reads `${GROK_SESSION_TOKEN-}` and returns `access_token` with issuer `https://auth.x.ai`.

`GROK_SESSION_TOKEN` was an `openshell:resolve:` broker reference. `XAI_API_KEY`, `OPENAI_API_KEY`, `ANTHROPIC_API_KEY`, `ANTHROPIC_AUTH_TOKEN`, `META_API_KEY`, and `ZAI_API_KEY` were unset. The agent-created `auth.json` had mode 0600, one broker placeholder, and zero sensitive raw-token leaves. Metadata-only checks recorded key paths, types, lengths, and placeholder counts without printing credential values.

`grok models` reported a `grok.com` login, default `grok-4.7`, and `grok-4.7-build-fast`, `grok-4.6`, and `grok-4.5` as additional available models. Every proof request selected `grok-4.7` explicitly. API-key mode did not appear.

### Responses, tool, and subscription use

The fixed request returned exact `PONG`, `stopReason=end_turn`, one model call, and 14,045 total tokens. `modelUsage` named `grok-4.7-build`.

The streaming tool request emitted one `run_terminal_command` call. The terminal command created `/tmp/a14t03fd-tool-proof` with the expected unique content. Text events after the completed tool result were exactly `TOOL-OK`. The terminal event reported two model calls and 32,276 total tokens. Streaming events separated the final assistant message from any earlier narration.

The post-refresh check returned exact `PONG` with 15,998 total tokens in the final lifecycle run. The separate successful rotation described above also returned exact `PONG` with 15,995 total tokens.

Only `jackin-a14t03fd-grok` was attached. Inspect reported a loaded policy and no closure violations. The successful Grok responses exercised the provider path. Direct non-agent `curl` requests to `cli-chat-proxy.grok.com`, `auth.x.ai`, `example.com`, and `api.x.ai` all failed with exit 7.

These observations prove subscription-credential use with nonzero usage and no paid-key fallback in the tuple. Client cost fields are not invoices or quota measurements.

### Supervised response, replay, and shell persistence

A real PTY attach submitted the launch-task text to the trusted supervised Grok process. It returned the unique supervised marker. A second PTY attach replayed that marker. Both detached with Ctrl-P Ctrl-Q and exit 0. Neither transcript contained the directory-trust prompt.

Two shell panes proved a marker persisted in `/tmp`. The second pane returned uid 1000 and `/sandbox/project`. The post-shell process check confirmed the same direct Grok child under `jackin-start`.

### Export and guarded purge

Sandbox commit `9a96de0c9348e98eecd63c31c31fc6af81e1bbdf` created unexported work. Guarded purge refused with exit 3 and an unexported-branch error. Export created only:

```text
refs/heads/jackin/jackin-a14t03f-oazw/feature/openshell-native  9a96de0c9348e98eecd63c31c31fc6af81e1bbdf
```

The sentinel HEAD stayed `6722f8024623c76726ce0293fc7e93da47b5ee74`. Its status, index hash, and diff hash `916dfec8d56c18288f1ad1b65b915b112b30a7e3c640a137508f4a9c9e9f19a3` stayed unchanged. Guarded purge then deleted the primary sandbox without `--force`.

The second sandbox had neither the first sandbox's tool marker nor its shell marker. It returned exact `PONG` with 16,009 total tokens. Only that second sandbox was purged with `--force` because its uncommitted role edits were not part of the export assertion.

## Local proof

The sandbox was `jackin-a14t03f-8d80`, with launch ID `a14t03flocalproof000000001` and provider `jackin-a14t03fl-grok`.

The same-launch reload reported cached construct and derived images and skipped the cached image, existing sandbox, and uploaded project. The sandbox reported Grok 1.0.77, uid 1000, and no `sudo`. `/usr/local/bin/grok --trust` was a direct child of `/bin/sh /usr/local/bin/jackin-start grok --trust` before and after the shell checks.

The lead `[auth]` config, `disable_api_key_auth`, helper, model pin, file modes, broker reference, and placeholder checks matched docker. No secondary config was used. `grok models` reported the same `grok.com` session and `grok-4.7` default. API-key mode did not appear.

The fixed request returned exact `PONG` with 15,995 total tokens. One `run_terminal_command` call created the expected local tool marker. The final assistant text was exact `TOOL-OK`, with two model calls and 32,278 total tokens. The successful rotation returned exact `PONG` with 15,996 total tokens. All `modelUsage` objects named `grok-4.7-build`.

Inspect listed only `jackin-a14t03fl-grok`, a loaded policy, and no closure violations. Non-agent `curl` failed with exit 7 for both approved provider hosts, `example.com`, and `api.x.ai`. No alternate paid provider or API key was present.

A real supervised PTY returned the unique launch-task marker, and the next attach replayed it. Two shell panes preserved their marker, and the direct Grok child survived. Neither attach displayed a trust prompt.

Sandbox commit `5eee7cd8d4f25bbbc3dfef63bb2e3840572a766c` caused guarded purge to refuse with exit 3. Export created only:

```text
refs/heads/jackin/jackin-a14t03f-8d80/feature/openshell-native  5eee7cd8d4f25bbbc3dfef63bb2e3840572a766c
```

The sentinel HEAD, status, index hash, and diff hash stayed unchanged. Guarded purge then deleted the sandbox without `--force`.

## Failed attempts and observer corrections

The artifacts retain failed-attempt summaries. They include an observer that omitted `--trust` from the expected parent command, a non-POSIX environment probe, a guessed `auth.json` schema, and a commit probe with the wrong workdir. These were proof-driver errors, not product failures. The final checks read the actual process tree, POSIX environment, schema-neutral credential metadata, and project Git state.

Some tool attempts returned narration before `TOOL-OK` in aggregate JSON. Those attempts failed and remain in `a14t03fd-attempt5-tool.stdout` and `a14t03fd-attempt7-tool.stdout`. Final proof runs used `streaming-json` and required the post-tool assistant message to be exactly `TOOL-OK`. No failed response was converted to a pass.

The failed `invalid_grant` rotation and the OIDC recovery are described above. Separately, `a14t03fd-attempt1-summary.json` records `http_402_evidence` at `2026-11-10T06:20:59.442411Z`. Later attempts overwrote the original launch capture, so its bytes cannot be recovered. The retained session transcript recovers the detector as `if "402" in combined`, applied to all launch output rather than an HTTP status field. This is a false-positive detector observation on launch metadata, not reliable evidence of an HTTP 402 response or quota exhaustion. The original substring cannot be identified, so this report does not claim that no HTTP 402 occurred in that attempt. No retained Grok error response reports HTTP 402. The analysis is preserved in `a14t03fr-original-402-analysis.json`.

## Cleanup

All owned sandboxes, accounts, providers, workspaces, the role, and both exported refs were removed. The temporary XDG configuration and sentinel clone were deleted. Final inventories contain only foreign resources. The foreign `jackin-grok-live-grok` provider was not used or changed.

The unique role and derived image tags remain in the BuildKit cache. Jackin exposes no owned image-removal command. No manual Docker removal bypassed the `jackin-image` boundary.

The original sentinel checkout stayed clean at its starting HEAD. Host Grok remained usable after local refresh and cleanup. The temporary T03 availability edit was restored.

## Verification artifacts

The redacted command captures and summaries are in `/tmp/a14t03f-artifacts`. The final summaries are `a14t03fd-summary.json` and `a14t03fl-summary.json`, both with `verdict=PASS` and empty cleanup errors. The decision trail is `.audit/a14-t03-final.tsv` in the worktree.

## Re-review additions on 2026-11-10

The independent-review corrections used only a targeted Docker refresh and response, not another full paid lifecycle. The first targeted attempt failed rotation with `invalid_grant` between `07:14:26Z` and `07:15:24Z`. Its artifacts are preserved under `a14t03fr-failed-attempt`. A fresh host OIDC authorization completed at `07:17:15.350150Z`. The successful targeted summary is `a14t03fr-summary.json`, with `verdict=PASS` and no cleanup errors.

The original raw host replies were not retained. Fresh replies were captured later and must not be represented as the originals:

- The fresh restored and pre-run reply completed at `07:17:37.668219Z` with exact text `A14T03FR-HOST-RESTORED`. Its sanitized response and timestamp metadata are `a14t03fr-host-before.json` and `a14t03fr-host-before.capture.json`.
- The post-Docker reply completed at `07:17:57.844811Z` with exact text `A14T03FR-HOST-AFTER-DOCKER`. Its artifacts are `a14t03fr-host-after-docker.json` and `a14t03fr-host-after-docker.capture.json`.
- The post-cleanup reply completed at `07:18:12.164452Z` with exact text `A14T03FR-HOST-AFTER-CLEANUP`. Its artifacts are `a14t03fr-host-after-cleanup.json` and `a14t03fr-host-after-cleanup.capture.json`.

Each host response includes session and request IDs, token usage, and `grok-4.7-build` model usage. Raw host output and OIDC payloads were not retained. The OIDC completion metadata is `a14t03fr-oidc.capture.json`.

The Safari setting was absent again after cleanup. `a14t03fr-safari-setting-removed.stderr` records that `AllowJavaScriptFromAppleEvents` does not exist, and its capture metadata records the expected exit 1 from `defaults read`.

Final provider and sandbox inventories are preserved as `a14t03fr-providers-docker-final.stdout`, `a14t03fr-providers-local-final.stdout`, `a14t03fr-sandboxs-docker-final.stdout`, and `a14t03fr-sandboxs-local-final.stdout`. None contains an `a14t03fr` resource. `a14t03fr-config-inventory.stdout` is empty after role, account, and workspace removal.

`a14t03fr-sentinel-post-cleanup.json` records the temporary sentinel at HEAD `6722f8024623c76726ce0293fc7e93da47b5ee74`, only its intentional role edits, and no exported refs. `a14t03fr-original-sentinel-post-cleanup.json` records the original sentinel at the same HEAD with empty status and no exported refs. Both snapshots were captured at `07:18:04Z`. The targeted XDG configuration and sentinel clone were then deleted, and the temporary source flip was restored.

No full G-FAST run was requested or performed. No commit or push was made.
