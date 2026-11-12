---
title: Run a local OpenShell gateway
description: Start an OpenShell gateway with the vm compute driver on an Apple silicon Mac and register it for jackin
---

Jackin does not install, start or stop an OpenShell gateway (D13). You run one. This page starts a local gateway named `jackin-local` with the `vm` compute driver. Each sandbox is a lightweight virtual machine, so the gateway needs no Docker engine.

The steps were proven on macOS 27.2 on Apple silicon with OpenShell 0.1.6. Evidence: `rewrite/evidence/P0.2-gateway.md`.

## Before you start

You need:

- An Apple silicon Mac. The `vm` driver uses Hypervisor.framework.
- Homebrew, with the `openshell` formula at 0.1.6: `brew install openshell`.
- e2fsprogs. The driver builds ext4 disks with `mke2fs`: `brew install e2fsprogs`.
- The GitHub CLI `gh`, signed in. It downloads the driver.
- `git` and `ssh` on the host. Jackin uses `git` to filter an upload and `ssh` for `connect`, `exec --tty`, `upload` and `download`.

The Homebrew package does not contain `openshell-driver-vm`. The script below downloads it from the `v0.1.6` release, checks its digest and signs it with the Hypervisor entitlement.

## Start the gateway

Save this script as `start-gateway.sh` and run `sh start-gateway.sh`. It is safe to run again. It installs the driver only when the entitlement is missing, keeps existing certificates and configuration, starts the gateway only when none runs, and registers it only when it is not registered.

```sh
#!/bin/sh
set -eu

NAME=jackin-local
PORT=17670
TAG=v0.1.6
ASSET=openshell-driver-vm-aarch64-apple-darwin.tar.gz

STATE="$HOME/.local/state/openshell/$NAME"
LOG="$HOME/Library/Logs/openshell-gateway-$NAME.log"
GW=/opt/homebrew/bin/openshell-gateway
CLI=/opt/homebrew/bin/openshell
GATEWAY_PATH=/opt/homebrew/bin:/usr/bin:/bin:/usr/sbin:/sbin
DRIVER="$STATE/libexec/openshell-driver-vm"

mkdir -p "$STATE/libexec"

if ! codesign -d --entitlements - "$DRIVER" 2>&1 | grep -q com.apple.security.hypervisor; then
  work=$(mktemp -d)
  gh release download "$TAG" -R NVIDIA/OpenShell -D "$work" \
    -p "$ASSET" -p openshell-checksums-sha256.txt
  (cd "$work" && grep " $ASSET\$" openshell-checksums-sha256.txt | shasum -a 256 -c -)
  tar -xzf "$work/$ASSET" -C "$work"
  cat >"$work/vm.entitlements.plist" <<'PLIST'
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>com.apple.security.hypervisor</key>
    <true/>
</dict>
</plist>
PLIST
  install -m 755 "$work/openshell-driver-vm" "$DRIVER"
  codesign --force -s - --entitlements "$work/vm.entitlements.plist" "$DRIVER"
  rm -rf "$work"
fi

"$GW" generate-certs --output-dir "$STATE/tls"

if [ ! -f "$STATE/gateway-vm.toml" ]; then
  cat >"$STATE/gateway-vm.toml" <<EOF
[openshell]
version = 2

[openshell.gateway]
name = "$NAME"
compute_driver = "vm"

[openshell.drivers.vm]
driver_dir = "$STATE/libexec"
state_dir = "$STATE/vm-driver"
EOF
fi

env PATH="$GATEWAY_PATH" "$GW" config preflight -- --config "$STATE/gateway-vm.toml"

if ! pgrep -f "openshell-gateway --config $STATE/gateway-vm.toml" >/dev/null; then
  (
    cd "$STATE"
    env -i HOME="$HOME" PATH="$GATEWAY_PATH" \
      OPENSHELL_LOCAL_TLS_DIR="$STATE/tls" \
      OPENSHELL_DB_URL="sqlite:$STATE/gateway.db" \
      nohup "$GW" --config "$STATE/gateway-vm.toml" --port "$PORT" \
      >>"$LOG" 2>&1 </dev/null &
  )
fi

mtls="${XDG_CONFIG_HOME:-$HOME/.config}/openshell/gateways/$NAME/mtls"
if ! "$CLI" gateway list </dev/null | grep -q " $NAME "; then
  mkdir -p "$mtls"
  cp "$STATE/tls/ca.crt" "$STATE/tls/client/tls.crt" "$STATE/tls/client/tls.key" "$mtls/"
  "$CLI" gateway add "https://localhost:$PORT" --local --name "$NAME" </dev/null
fi

i=0
until "$CLI" -g "$NAME" status </dev/null 2>/dev/null | grep -q "Status: Connected"; do
  i=$((i + 1))
  [ "$i" -le 30 ] || { echo "gateway $NAME did not become healthy; see $LOG" >&2; exit 1; }
  sleep 1
done
"$CLI" -g "$NAME" status </dev/null
```

The gateway runs detached with `nohup`. It keeps running after you close the terminal. No service manager supervises it, so it does not start again after a reboot. Run the script again after a reboot.

The script leaves these files:

| Path | Content |
|---|---|
| `~/.local/state/openshell/jackin-local/gateway-vm.toml` | The gateway configuration. |
| `~/.local/state/openshell/jackin-local/tls/` | The certificates and the sandbox token key. |
| `~/.local/state/openshell/jackin-local/gateway.db` | The gateway database. |
| `~/.local/state/openshell/jackin-local/libexec/openshell-driver-vm` | The signed driver. |
| `~/.local/state/openshell/jackin-local/vm-driver/` | Disks and logs of the sandboxes. |
| `~/Library/Logs/openshell-gateway-jackin-local.log` | The gateway log. |
| `~/.config/openshell/gateways/jackin-local/` | The registration and the client certificates. |

Registering a gateway with `openshell gateway add` also makes it the active gateway. Jackin never relies on the active gateway. It passes `-g jackin-local` and `--workspace default` on every call.

## Check the gateway

```sh
openshell -g jackin-local status
```

The output ends with `Status: Connected` and `Version: 0.1.6`.

Create a short sandbox to prove the driver:

```sh
openshell -g jackin-local --workspace default sandbox create --name smoke --no-auto-providers --no-tty -- true
openshell -g jackin-local --workspace default sandbox list
openshell -g jackin-local --workspace default sandbox delete smoke
```

The list shows the phase `Completed` for `smoke`. The first sandbox is slower, because the driver pulls and prepares the default image `nvcr.io/nvidia/base/ubuntu:24.04`.

## Stop the gateway

Delete or stop your sandboxes first. Then stop the process:

```sh
kill $(pgrep -f 'openshell-gateway --config .*/jackin-local/gateway-vm.toml')
```

## Fix common errors

| Message | Cause | Fix |
|---|---|---|
| `vm compute driver binary not found` | The driver is not under `libexec`. | Run the script. It installs the driver. |
| `inspect /usr/local/bin/mke2fs: Permission denied` | The gateway inherited a `PATH` entry that holds an unreadable `mke2fs`. | Start the gateway through the script. It sets `PATH=/opt/homebrew/bin:/usr/bin:/bin:/usr/sbin:/sbin`. |
| `Gateway 'jackin-local' already exists` | `gateway add` is not idempotent. | The script checks `gateway list` first. To redo the registration, run `openshell gateway remove jackin-local`. |
| `Connection refused (os error 61)` | The gateway is not running. | Run the script. |

## What jackin expects

`jackin doctor` checks that the `openshell` and `ssh` binaries exist, that the gateway answers and that its version matches the pinned version. It does not start or repair the gateway.

Sandboxes made from the default image run as user `ubuntu` (uid 1000) and have no `git`, `curl`, `sudo` or `docker`. Jackin roles use their own image (D05, D06).
