# Debian Results

Status: Static review complete; runtime route NOT RUN.

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

- Jackin's default Codex discovery targets `~/.codex` and maps it to `default-codex`.
- The scan does not consult `CODEX_HOME`.
- This host has no Jackin executable or default configuration.
- Account registration, workspace selection, and launch forwarding are therefore NOT RUN.
- No live role response was requested.

## Synthetic `CODEX_HOME` checks

The route owner reports local CLI path checks in a private synthetic root. These checks do not validate Jackin's runtime route.

The command used a 10-second timeout, an empty environment, and a synthetic home:

```sh
timeout 10s env -i HOME=<fixture>/home PATH=/root/.local/bin:/usr/bin:/bin [CODEX_HOME value] /root/.local/bin/codex login status
```

The tested values were unset, empty, relative, existing absolute, missing, and a regular file. The working directory was `<fixture>/cwd` for the relative case.

- Unset and empty values warn and use `<fixture>/home/.codex`.
- A relative value uses `<fixture>/cwd/relative-home`.
- Unset, empty, relative, and existing absolute paths exit 1 with `Not logged in`.
- A missing path reports that `CODEX_HOME` points to a path that does not exist.
- A regular file reports that `CODEX_HOME` is not a directory.
- The run used no credentials. It made no enrollment, network request, or config write.

## Owner

`debian_codex_route` completed static route inspection. Runtime confirmation remains NOT RUN because the executable and configuration are absent, and this checkpoint prohibits live requests.

See [crate plan](crate-plan.md) and [reviews](reviews.md).
