#!/bin/sh
set -eu

NAME=${NAME:-jackin-docker-engine}
PORT=${PORT:-12376}
DOCKER=${DOCKER:-$HOME/.orbstack/bin/docker}
IMAGE=docker:dind@sha256:9e79b879b177fbd72c34376a3dd3d91747da5db49e1f560f76aea59561471977
CERT_DIR=${CERT_DIR:-$HOME/.local/state/jackin/docker-engine/$NAME}

mkdir -p "$CERT_DIR"
chmod 700 "$CERT_DIR"

if ! "$DOCKER" container inspect "$NAME" >/dev/null 2>&1; then
  "$DOCKER" volume create "$NAME-certs" >/dev/null
  "$DOCKER" run -d --name "$NAME" --label jackin.engine=dedicated --privileged \
    -e DOCKER_TLS_CERTDIR=/certs \
    -e DOCKER_TLS_SAN=DNS:host.openshell.internal \
    -v "$NAME-certs:/certs" \
    -p "127.0.0.1:$PORT:2376" \
    "$IMAGE" >/dev/null
elif [ "$("$DOCKER" container inspect -f '{{.State.Running}}' "$NAME")" != true ]; then
  "$DOCKER" start "$NAME" >/dev/null
fi

i=0
until "$DOCKER" exec "$NAME" test -f /certs/client/key.pem 2>/dev/null; do
  i=$((i + 1))
  [ "$i" -le 60 ] || { echo "engine certificates did not appear" >&2; exit 1; }
  sleep 1
done

for f in ca cert key; do
  "$DOCKER" exec "$NAME" cat "/certs/client/$f.pem" >"$CERT_DIR/$f.pem.new"
  chmod 600 "$CERT_DIR/$f.pem.new"
  mv "$CERT_DIR/$f.pem.new" "$CERT_DIR/$f.pem"
done

i=0
until "$DOCKER" --host "tcp://127.0.0.1:$PORT" --tlsverify \
  --tlscacert "$CERT_DIR/ca.pem" --tlscert "$CERT_DIR/cert.pem" --tlskey "$CERT_DIR/key.pem" \
  version --format '{{.Server.Version}}' >/dev/null 2>&1; do
  i=$((i + 1))
  [ "$i" -le 60 ] || { echo "engine did not answer on 127.0.0.1:$PORT" >&2; exit 1; }
  sleep 1
done

echo "engine=$NAME endpoint=host.openshell.internal:$PORT certs=$CERT_DIR"
