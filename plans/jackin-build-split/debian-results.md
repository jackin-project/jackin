# Debian Results

Status: Static route and CLI checks complete; source review pending; Jackin runtime route NOT RUN.

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
- Follow-up commit `688057f40173d32dda04a55bff1e3868c219710d` updates discovery parity. Exact-head source review remains pending.
- This host has no Jackin executable or default configuration.
- Account registration, workspace selection, and launch forwarding are therefore NOT RUN.
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

## Empty-configuration preflight

The host-probe owner reports a separate synthetic MCP listing with empty home and config paths. It redirected `CODEX_SQLITE_HOME` to another empty fixture. The original temporary path was not retained.

```sh
set -eu
umask 077
probe=$(mktemp -d -p /tmp codex-mcp-preflight.XXXXXX)
trap 'rm -rf -- "$probe"' EXIT
mkdir -m 700 "$probe/home" "$probe/codex" "$probe/sqlite"
/usr/bin/env -i HOME="$probe/home" CODEX_HOME="$probe/codex" CODEX_SQLITE_HOME="$probe/sqlite" PATH=/usr/bin:/bin LANG=C.UTF-8 /usr/bin/timeout 10s /root/.local/bin/codex mcp list --json --disable plugins
```

This block gives a safe reproduction recipe. The host-probe owner reported exit 0 and a JSON configured-server count of 0. The output contained no server names or details. The command read no fixture config or auth files. The empty tree remained unchanged. The owner removed logs and temporary files. This was not a live-account request.

## Owner

`debian_codex_route` completed static route inspection and local CLI path checks. Exact-head source review of commit `688057f40173d32dda04a55bff1e3868c219710d` remains pending. Jackin runtime confirmation remains NOT RUN because the executable and configuration are absent, and this checkpoint prohibits live requests.

See [crate plan](crate-plan.md) and [reviews](reviews.md).
