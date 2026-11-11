# A9 evidence: operator D24 test engine rebuilt

Date: 2026-11-06 (host clock). Host macOS arm64, OrbStack.

## Machine

- OrbStack Linux machine: `jackin-engine`, Ubuntu 24.04 (noble), arm64, IP `192.168.139.135` (at setup time).
- Created with: `orbctl create -a arm64 ubuntu:24.04 jackin-engine`
- Stop it with: `orbctl stop jackin-engine`
- Start it again with: `orbctl start jackin-engine` (the rootless Docker user service is enabled and lingering is on, so dockerd returns on machine start).

## Versions

- Docker CE (inside machine, rootless): `29.9.2` (`docker-ce 5:29.9.2-1~ubuntu.24.04~noble`, `docker-ce-cli` and `docker-ce-rootless-extras` pinned to the same version), containerd `v2.4.2`, runc `1.5.2`, rootlesskit `3.3.0`, slirp4netns `1.2.1`.
- Host Docker client (OrbStack): `29.9.3`.
- dockerd runs as a systemd **user** service (`docker.service`, rootless) for user `donbeave` (uid 501), with `loginctl enable-linger` set. A drop-in at `~/.config/systemd/user/docker.service.d/override.conf` sets:

```text
ExecStart=/usr/bin/dockerd-rootless.sh --tlsverify --tlscacert=$HOME/certs/ca/cert.pem --tlscert=$HOME/certs/server/cert.pem --tlskey=$HOME/certs/server/key.pem -H tcp://0.0.0.0:2376
```

- Subuid/subgid entries `donbeave:100000:65536` were added to `/etc/subuid` and `/etc/subgid` (required by `dockerd-rootless-setuptool.sh install`).

## How localhost:2376 reaches the Mac (and sandboxes)

- dockerd listens on `0.0.0.0:2376` inside the rootless network namespace; RootlessKit's builtin port driver forwards it to the machine's network namespace (verified: `curl https://127.0.0.1:2376/version` inside the machine answered).
- OrbStack exposes the machine's listening port on the Mac, so `https://localhost:2376` on the Mac reaches the machine's dockerd directly. Verified from the Mac (see below); no ssh/socat forward was needed.
- For OpenShell sandboxes, the gateway VM maps `host.openshell.internal` to the host, so a sandbox reaching `host.openshell.internal:2376` reaches the same Mac `localhost:2376` listener, as recorded in P0.5/P5.4. The server certificate carries `host.openshell.internal` in its SANs so TLS verification against that name works.

## Certificates (paths only, no key material)

- Inside the machine: CA at `~/certs/ca/cert.pem`, server cert/key at `~/certs/server/{cert,key}.pem`, client cert/key at `~/certs/client/{cert,key}.pem`.
- Server cert SANs: `DNS:localhost`, `DNS:host.openshell.internal`, `DNS:jackin-engine`, `DNS:jackin-engine.orb.local`, `IP:127.0.0.1`, `IP:192.168.139.135`.
- Client files on the Mac (copied from the machine's client certs): `/Users/donbeave/.local/share/jackin-e2e-engine/client/{ca.pem,cert.pem,key.pem}`; directory mode `0700`, files mode `0600`.

## Verification (from the Mac)

```text
$ curl --cacert ~/.local/share/jackin-e2e-engine/client/ca.pem --cert ~/.local/share/jackin-e2e-engine/client/cert.pem --key ~/.local/share/jackin-e2e-engine/client/key.pem https://localhost:2376/version
{"Platform":{"Name":"Docker Engine - Community"},"Version":"29.9.2","ApiVersion":"1.56","MinAPIVersion":"1.40","Os":"linux","Arch":"arm64",...}
```

- `docker --host tcp://localhost:2376 --tlscacert .../ca.pem --tlscert .../cert.pem --tlskey .../key.pem --tlsverify version` from the Mac: client `29.9.3`, server `Docker Engine - Community 29.9.2` (linux/arm64), containerd `v2.4.2`, rootlesskit `3.3.0`.
- `docker ... run --rm hello-world` against that endpoint: pulled and printed `Hello from Docker!` (exit 0). This run happened inside the engine, not on the host, per the D24 fixture exception.

## xtask e2e command that can now start

```text
mise exec -- mbx +1.99.1 xtask e2e --gateway jackin-local --role ../jackin-sentinel --builder orbstack --platform linux/arm64 --engine-port 2376 --engine-certs ~/.local/share/jackin-e2e-engine/client
```

(the G-E2E gate from `rewrite/PLAN.md`).

## Scope note

This engine is only the `xtask e2e` fixture allowed by decision D24. No jackin agent/sandbox was run in it; the only container run was `hello-world` for verification. Nothing was committed or pushed.
