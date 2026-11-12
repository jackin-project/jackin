#!/bin/sh
set -eu

HERE=$(cd "$(dirname "$0")" && pwd)
SB=${SB:-p05b-final}
NAME=${NAME:-jackin-docker-engine}
PORT=${PORT:-12376}
CERT_DIR=${CERT_DIR:-$HOME/.local/state/jackin/docker-engine/$NAME}

OS() {
  limit=$1
  shift
  perl -e 'alarm shift; exec @ARGV' "$limit" /opt/homebrew/bin/openshell -g jackin-local --workspace default "$@" </dev/null
}

NAME=$NAME PORT=$PORT sh "$HERE/docker-laneB-engine.sh"

if ! OS 60 sandbox get "$SB" >/dev/null 2>&1; then
  OS 280 sandbox create --name "$SB" --from docker.io/library/docker:cli \
    --policy "$HERE/docker-laneB-policy.yaml" --detach --no-auto-providers -- sleep 36000 | tail -1
fi

EX() { OS 200 sandbox exec -n "$SB" --no-tty -- sh -c "$1"; }

EX 'mkdir -p /sandbox/engine-certs'
for f in ca cert key; do
  OS 120 sandbox upload "$SB" "$CERT_DIR/$f.pem" /sandbox/engine-certs | tail -1
done

ENVS="export DOCKER_HOST=tcp://host.openshell.internal:$PORT DOCKER_TLS_VERIFY=1 DOCKER_CERT_PATH=/sandbox/engine-certs"

echo "--- docker run --rm hello-world"
EX "$ENVS; docker run --rm hello-world >/tmp/hw.out 2>&1; echo run_exit=\$?; grep -c 'Hello from Docker' /tmp/hw.out; head -3 /tmp/hw.out"
echo "--- engine facts"
EX "$ENVS; docker info --format 'engine={{.Name}} server={{.ServerVersion}} os={{.OperatingSystem}}'"
echo "--- no client certificate"
EX "unset DOCKER_CERT_PATH DOCKER_TLS_VERIFY; docker -H tcp://host.openshell.internal:$PORT --tlsverify --tlscacert=/sandbox/engine-certs/ca.pem version --format '{{.Server.Version}}' 2>&1 | tail -1"
echo "--- other port is denied by policy"
EX "DOCKER_HOST=tcp://host.openshell.internal:$((PORT + 1)) docker version --format '{{.Server.Version}}' 2>&1 | tail -1"
echo "--- other binary is denied by policy"
EX "nc -w 3 host.openshell.internal $PORT </dev/null; echo nc_exit=\$?"
echo "--- policy denial log"
OS 60 logs "$SB" --since 5m | grep DENIED | tail -3 | cut -c1-200 || true
echo "--- bind mount of a sandbox path shows the engine's empty directory"
EX "$ENVS; mkdir -p /sandbox/w && echo data >/sandbox/w/f; echo sandbox_has:; ls /sandbox/w; echo container_sees:; docker run --rm -v /sandbox/w:/w alpine:3.22 ls -A /w | wc -l"
echo "--- container egress is outside sandbox policy"
EX "wget -q -T 8 -O - https://example.com 2>&1 | head -c 80; echo; echo sandbox_wget_done"
EX "$ENVS; docker run --rm alpine:3.22 sh -c 'wget -q -T 10 -O - https://example.com | head -c 80'; echo"
