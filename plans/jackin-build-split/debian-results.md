# Debian Results

Status: Route source review PASS; host probe owner-reported PASS; independent runtime evidence review PENDING; Jackin route NOT RUN.

## Host

- Host: `bastion`, Debian 13.7, x86_64.
- Kernel: `6.12.94`.
- User: root, UID 0.
- CPU: 96 logical processors.
- Memory: 125 GiB total, 117 GiB available at initial inventory time.
- A repeat at `2026-10-05T02:37:32+02:00` showed 113 GiB available.
- Disk: 3.5 TiB total, 3.5 TiB free at inventory time.
- The read-only host command and repeat output are in [checklist](checklist.md#host-inventory-command).
- Codex CLI reports version `0.160.0` and “Logged in using ChatGPT.”
- `HOME` is `/root`. `CODEX_HOME` is unset.
- Auth-file metadata reports mode `0600`. No auth contents were read or copied.

## Account route

- At initial main `0aa821a088e1bacf3d4d85a4c9faaa67faa85132`, Jackin's Codex discovery targeted `~/.codex`, mapped it to `default-codex`, and ignored `CODEX_HOME`.
- Task commit `0556ce39b1abb9cd6b387583d932e1556ca9dfd4` changes Jackin discovery to honor `CODEX_HOME`; an unset value selects `~/.codex`, while an empty value reports an issue without fallback.
- Follow-up commit `688057f40173d32dda04a55bff1e3868c219710d` updates the explicit `CODEX_HOME` source and injection route. Commit `45fbb65c84e359ea3c577120a80ed7a2fa92cf2a` adds only a test-only scan seam attribute. Exact-source review PASS across these commits; review found no production change in the follow-up.
- The MBX-private `jackin` binary was built from base `0aa821a088e1bacf3d4d85a4c9faaa67faa85132`, before the route commits. It does not validate them. No default Jackin configuration exists.
- Account registration, workspace selection, and launch forwarding are NOT RUN.
- No live role response was requested.

## Synthetic `CODEX_HOME` checks

The route owner reports local CLI path checks in a private synthetic root. These checks cover Codex CLI path handling only. They do not validate Jackin runtime discovery or a provider request.

Read-only identity commands were `readlink -f /root/.local/bin/codex`, `/root/.local/bin/codex --version`, `sha256sum /root/.codex/packages/standalone/releases/0.160.0-x86_64-unknown-linux-musl/bin/codex`, and `stat -c '%s bytes, mode %a, mtime %y' /root/.codex/packages/standalone/releases/0.160.0-x86_64-unknown-linux-musl/bin/codex`. The symlink resolves to `/root/.codex/packages/standalone/releases/0.160.0-x86_64-unknown-linux-musl/bin/codex`. The binary reports `codex-cli 0.160.0`; its SHA-256 is `12eb3e81114588aca3b7998f4f19e8997b056aca08e57a7ca7c8a3ec8c652aad`. Its size is 289,101,384 bytes and its mode is `0755`.

The owner used fixture root `/tmp/jackin-codex-route.sYgjX4` with owner `root:root` and mode `0700`. It created `home`, `home/.codex`, `relative-home`, and `absolute-home` directories with owner `root:root` and mode `0700`. It created `regular-file` with mode `0600`.

The route owner supplied this setup:

```sh
umask 077
fixture=$(mktemp -d -p /tmp jackin-codex-route.XXXXXX)
chmod 700 "$fixture"
mkdir -m 700 "$fixture/home" "$fixture/home/.codex" "$fixture/relative-home" "$fixture/absolute-home"
touch "$fixture/regular-file"
chmod 600 "$fixture/regular-file"
cd "$fixture"

# CODEX_HOME unset
env -i HOME="$fixture/home" PATH=/usr/bin:/bin LANG=C.UTF-8 timeout 10s /root/.local/bin/codex login status
# CODEX_HOME empty
env -i HOME="$fixture/home" PATH=/usr/bin:/bin LANG=C.UTF-8 CODEX_HOME= timeout 10s /root/.local/bin/codex login status
# CODEX_HOME relative
env -i HOME="$fixture/home" PATH=/usr/bin:/bin LANG=C.UTF-8 CODEX_HOME=relative-home timeout 10s /root/.local/bin/codex login status
# CODEX_HOME existing absolute directory
env -i HOME="$fixture/home" PATH=/usr/bin:/bin LANG=C.UTF-8 CODEX_HOME="$fixture/absolute-home" timeout 10s /root/.local/bin/codex login status
# CODEX_HOME missing absolute path
env -i HOME="$fixture/home" PATH=/usr/bin:/bin LANG=C.UTF-8 CODEX_HOME="$fixture/missing" timeout 10s /root/.local/bin/codex login status
# CODEX_HOME regular file
env -i HOME="$fixture/home" PATH=/usr/bin:/bin LANG=C.UTF-8 CODEX_HOME="$fixture/regular-file" timeout 10s /root/.local/bin/codex login status
```

The working directory was `$fixture` for all six cases. Stdout was empty. Stderr paths below use `<fixture>` for the temporary root.

| Case | Result |
|---|---|
| Unset | Exits 1 with `Not logged in`; resolves to `<fixture>/home/.codex`. |
| Empty | Exits 1 with `Not logged in`; resolves to `<fixture>/home/.codex`. |
| Relative `relative-home` | Exits 1 with `Not logged in`; resolves to `<fixture>/relative-home`. |
| Existing absolute `absolute-home` | Accepted; exits 1 with `Not logged in`. |
| Missing absolute `missing` | Reports `CODEX_HOME points to <fixture>/missing, but that path does not exist`. |
| Regular file `regular-file` | Reports `<fixture>/regular-file is not a directory`. |

Valid cases also emitted a benign warning about refusing PATH aliases under `/tmp`.

- The owner used no credentials. The checks made no enrollment, network request, or config write.
- The owner removed the fixture root. `test ! -e /tmp/jackin-codex-route.sYgjX4` passed.
- No separate raw transcript was retained. The commands, fixture, and summarized result record came from `debian_codex_route`.

## Rejected MCP-list preflight

The host-probe owner reports a separate synthetic MCP listing with empty home and config paths. It redirected `CODEX_SQLITE_HOME` to another empty fixture. The original temporary path was not retained.

```sh
set -eu
umask 077
probe=$(mktemp -d -p /tmp codex-mcp-preflight.XXXXXX)
trap 'rm -rf -- "$probe"' EXIT
mkdir -m 700 "$probe/home" "$probe/codex" "$probe/sqlite"
/usr/bin/env -i HOME="$probe/home" CODEX_HOME="$probe/codex" CODEX_SQLITE_HOME="$probe/sqlite" PATH=/usr/bin:/bin LANG=C.UTF-8 /usr/bin/timeout 10s /root/.local/bin/codex mcp list --json --disable plugins
```

The owner reported exit 0 and a JSON configured-server count of 0. The output contained no server names or details. The empty tree remained unchanged. The owner removed logs and temporary files. Sol rejected this command as a general profile preflight because auth-status discovery may contact configured MCP endpoints. The synthetic empty-home result does not validate a real profile. No real profile or model request ran.

The v11 replacement uses schema-verified app-server `config/read`. Its schema review accounts for `system`, `user`, and `sessionFlags` layers and treats `ConfigReadResponse.layers[].config` as generic JSON. It inspects raw `mcp_servers` values and fails closed on enabled or incomplete configuration. The bounded host result is recorded below; it reports zero configured MCP servers for this profile snapshot only.

## Owner-reported bounded host Codex route probe

The exact v11 wrapper `/tmp/jackin_codex_probe_review_0_160_v11.py` has SHA-256 `bfc1eedaf6e6105859262bbe7cd5d5996a0745385294c4cdb9f200e47ff24020`, mode `0400`. Its notification fixture `/tmp/jackin_codex_notif_parser_fixture_0_160_v11.py` has SHA-256 `30cee76f266bb2c37365c0cfc4166a6643476bdb6820fd957d5e9308358d1b23`, mode `0400`. The owner reports that exact-hash checks preceded the single root-authorized invocation. The owner reports exit 0 in 9.1 seconds. Independent Sol review verified the artifact hashes, but runtime evidence corroboration remains pending.

Sanitized output:

```text
PASS profile=existing_Codex_home auth_method=chatgpt model=gpt-6-luna effort=max provider=openai mcp_config_count=0 thread_token_usage_events=1 account_rate_limit_events=1 chatgpt_account_updates=1 account_reads=2 account_route_identity_stable=true config_warnings_discarded=1 thread_settings_updates_checked=0 deprecation_notices_discarded=2 thread_warnings_discarded=1 global_warnings_discarded=0 thread_active_status_events=1 thread_idle_status_events=1 ephemeral_path=null shell_tool=disabled no_tool_items_observed=true marker=JACKIN_ROUTE_PROBE_20261005_07
```

The owner reports that the wrapper correlated the exact completion marker, required the selected profile and route snapshots to remain stable, and rejected unknown tool or reroute events. It reported no configured MCP servers and no tool item. The model and effort are settings observed in thread metadata; they do not attest to provider-side model execution. The owner reports removal of task-owned temporary captures and child process. No login or enrollment occurred, and no session history was deleted. Normal Codex startup, managed configuration, authentication refresh, and SQLite activity remain possible.

This is an owner-reported bounded host Codex route result. It does not test Jackin account discovery, workspace launch, or an Architect role request. Jackin discovery and role loading remain NOT RUN.

## Owner

`debian_codex_route` completed static route inspection, source review, local CLI path checks, and the bounded v11 host Codex probe above. Exact-source review PASS covers commits `0556ce39b1abb9cd6b387583d932e1556ca9dfd4`, `688057f40173d32dda04a55bff1e3868c219710d`, and `45fbb65c84e359ea3c577120a80ed7a2fa92cf2a`. The host probe does not establish Jackin discovery or role execution. Jackin runtime confirmation remains NOT RUN because the available MBX binary predates these changes and no default configuration exists.

See [crate plan](crate-plan.md) and [reviews](reviews.md).
