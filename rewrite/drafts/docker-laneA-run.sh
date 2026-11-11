#!/bin/bash
set -u

HERE=$(cd "$(dirname "$0")" && pwd)
WORK=${WORK:-/private/tmp/claude-501/p0.5a/run}
OS=/opt/homebrew/bin/openshell
DK=$HOME/.orbstack/bin/docker
GW=jackin-local
IMG=docker.io/library/docker:dind-rootless
HOST_PORT_HOST_ENGINE=18475
HOST_PORT_MTLS=18476
HOST_PORT_TOKEN=18477

mkdir -p "$WORK"
chmod 700 "$WORK"

os() {
  local s=$1
  shift
  perl -e 'alarm shift; exec @ARGV' "$s" "$OS" -g "$GW" --workspace default "$@" </dev/null
}

step() { printf '\n==== %s\n' "$*"; }

show() {
  printf '$ %s\n' "$*"
  "$@"
  printf '[exit=%s]\n' "$?"
}

sbx() {
  local name=$1 limit=$2 script=$3
  printf '$ openshell sandbox exec -n %s --no-tty -- sh -c <script>\n' "$name"
  printf '%s\n' "$script" | sed 's/^/    | /'
  os "$limit" sandbox exec -n "$name" --no-tty -- sh -c "$script" 2>&1
  printf '[exit=%s]\n' "$?"
}

have_sandbox() { os 30 sandbox get "$1" >/dev/null 2>&1; }

ensure_sandbox() {
  local name=$1 image=$2 policy=$3
  shift 3
  if have_sandbox "$name"; then
    printf 'sandbox %s exists\n' "$name"
    return 0
  fi
  show os 280 sandbox create --name "$name" --from "$image" --policy "$policy" "$@" --detach --no-tty --no-auto-providers -- sleep 86400
}

cleanup() {
  step cleanup
  for n in p05a-in p05a-probe p05a-root p05a-ext p05a-tok p05a-rootless p05a-client; do
    if have_sandbox "$n"; then os 120 sandbox delete "$n" 2>&1 | tail -1; fi
  done
  if os 30 provider get p05a-engine >/dev/null 2>&1; then os 60 provider delete p05a-engine 2>&1 | tail -1; fi
  if os 30 profile describe p05a-docker-engine >/dev/null 2>&1; then os 60 profile delete p05a-docker-engine 2>&1 | tail -1; fi
  for c in p05a-auth p05a-engine-int p05a-engine-rl p05a-engine; do "$DK" rm -f "$c" 2>&1 | tail -1; done
  "$DK" volume rm p05a-engine-certs 2>&1 | tail -1
  "$DK" network rm p05a-net 2>&1 | tail -1
  if [ -f "$WORK/bridge.pid" ]; then kill "$(cat "$WORK/bridge.pid")" 2>/dev/null; rm -f "$WORK/bridge.pid"; fi
  if [ -f "$WORK/http.pid" ]; then kill "$(cat "$WORK/http.pid")" 2>/dev/null; rm -f "$WORK/http.pid"; fi
  rm -rf "$WORK/engine-certs" "$WORK/engine-certs-rl" "$WORK/auth"
}

bridge_up() {
  if [ ! -f "$WORK/bridge.pid" ] || ! kill -0 "$(cat "$WORK/bridge.pid")" 2>/dev/null; then
    nohup python3 "$HERE/docker-laneA-unix-bridge.py" 127.0.0.1 $HOST_PORT_HOST_ENGINE "$HOME/.orbstack/run/docker.sock" >"$WORK/bridge.log" 2>&1 </dev/null &
    echo $! >"$WORK/bridge.pid"
    sleep 1
  fi
}

rootless_engine_probe() {
  step "SOURCE 2 variant: dedicated rootless engine without --privileged, mTLS, reached from the sandbox"
  umask 077
  mkdir -p "$WORK/engine-certs-rl"
  "$DK" inspect p05a-engine-rl >/dev/null 2>&1 || show "$DK" run -d --name p05a-engine-rl \
    --security-opt seccomp=unconfined --security-opt apparmor=unconfined --security-opt systempaths=unconfined \
    --device /dev/net/tun -e DOCKER_TLS_CERTDIR=/certs \
    -e DOCKER_TLS_SAN=DNS:host.openshell.internal,DNS:localhost,IP:127.0.0.1 \
    -p 127.0.0.1:18478:2376 docker:dind-rootless
  local i=0
  until "$DK" exec p05a-engine-rl test -s /certs/client/key.pem 2>/dev/null &&
    "$DK" exec -e DOCKER_HOST=unix:///run/user/1000/docker.sock p05a-engine-rl docker version >/dev/null 2>&1; do
    i=$((i + 1))
    [ "$i" -le 120 ] || { echo "rootless engine did not start"; "$DK" logs --tail 15 p05a-engine-rl 2>&1; return 1; }
    sleep 1
  done
  show "$DK" inspect --format 'privileged={{.HostConfig.Privileged}} user={{.Config.User}} caps={{.HostConfig.CapAdd}} secopt={{.HostConfig.SecurityOpt}} devices={{len .HostConfig.Devices}}' p05a-engine-rl
  show "$DK" exec -e DOCKER_HOST=unix:///run/user/1000/docker.sock p05a-engine-rl docker info --format 'secopts={{.SecurityOptions}} driver={{.Driver}}'
  for f in ca.pem cert.pem key.pem; do "$DK" cp "p05a-engine-rl:/certs/client/$f" "$WORK/engine-certs-rl/$f"; done
  chmod 700 "$WORK/engine-certs-rl"
  chmod 600 "$WORK/engine-certs-rl/key.pem"
  ensure_sandbox p05a-ext "$IMG" "$HERE/docker-laneA-policy-external-mtls-rootless.yaml"
  show os 60 policy set p05a-ext --policy "$HERE/docker-laneA-policy-external-mtls-rootless.yaml" --wait
  show os 120 sandbox upload p05a-ext "$WORK/engine-certs-rl" /sandbox/ --no-git-ignore
  sbx p05a-ext 250 "
export DOCKER_HOST=tcp://host.openshell.internal:18478 DOCKER_TLS_VERIFY=1 DOCKER_CERT_PATH=/sandbox/engine-certs-rl
docker version --format 'server={{.Server.Version}} os={{.Server.Os}}/{{.Server.Arch}}' 2>&1 | tail -2
docker run --rm hello-world >/tmp/hw5.out 2>&1; echo \"docker run --rm hello-world exit=\$?\"
grep -c 'Hello from Docker' /tmp/hw5.out
docker run --rm -v /Users:/mnt/Users:ro alpine sh -c 'test -d /mnt/Users/donbeave/Projects && echo host-home-reachable=yes || echo host-home-reachable=no' 2>&1 | tail -3"
  show os 60 policy set p05a-ext --policy "$HERE/docker-laneA-policy-external-mtls.yaml" --wait
}

buildkit_probe() {
  step "SOURCE 2 (route A): BuildKit build from the sandbox, with and without registry egress for the buildx plugin"
  local certdir
  certdir=$(cat "$WORK/certdir")
  local script="
export DOCKER_HOST=tcp://host.openshell.internal:$HOST_PORT_MTLS DOCKER_TLS_VERIFY=1 DOCKER_CERT_PATH=$certdir
mkdir -p /sandbox/b && printf 'FROM busybox\\nRUN echo built\\n' >/sandbox/b/Dockerfile
timeout 150 docker build -q -t p05a-built /sandbox/b >/tmp/build.out 2>&1; echo \"build exit=\$?\"
tail -2 /tmp/build.out"
  show os 60 policy set p05a-ext --policy "$HERE/docker-laneA-policy-external-mtls.yaml" --wait
  sbx p05a-ext 200 "$script"
  show os 60 policy set p05a-ext --policy "$HERE/docker-laneA-policy-external-mtls-buildx.yaml" --wait
  sbx p05a-ext 200 "$script"
  show os 60 policy set p05a-ext --policy "$HERE/docker-laneA-policy-external-mtls.yaml" --wait
}

source1() {
  step "SOURCE 1: engine inside the sandbox (vm driver)"
  ensure_sandbox p05a-probe docker.io/library/buildpack-deps:noble-scm "$HERE/docker-laneA-policy-inside.yaml"
  ensure_sandbox p05a-in "$IMG" "$HERE/docker-laneA-policy-inside.yaml"
  show os 30 sandbox get p05a-in

  sbx p05a-probe 120 '
id; uname -a
grep -E "^(CapInh|CapPrm|CapEff|CapBnd|CapAmb|NoNewPrivs|Seccomp|Seccomp_filters):" /proc/self/status
tr "\n" " " </proc/filesystems; echo
cat /proc/sys/user/max_user_namespaces
mount | grep -E "cgroup| / "
unshare -U true; echo "unshare -U exit=$?"
unshare -m true; echo "unshare -m exit=$?"
ls -ld /sys/fs/cgroup /dev/fuse
cat /proc/net/dev | cut -c1-40
ls /proc/modules /lib/modules 2>&1 | head -2'

  sbx p05a-in 120 '
id; echo HOME=$HOME
grep -E "^(CapEff|CapBnd|NoNewPrivs|Seccomp|Seccomp_filters):" /proc/self/status
dockerd --version
echo "--- rootful dockerd as the sandbox user"
dockerd -H unix:///tmp/d.sock --data-root /tmp/dd --exec-root /tmp/de --pidfile /tmp/d.pid 2>&1 | head -3
echo "--- mount"
mkdir -p /tmp/m; mount -t tmpfs tmpfs /tmp/m 2>&1
echo "--- unshare"
unshare -Ur true; echo "unshare -Ur exit=$?"
echo "--- rootlesskit, host network, no port driver"
export XDG_RUNTIME_DIR=/tmp/xdg; mkdir -p $XDG_RUNTIME_DIR
rootlesskit --net=host --copy-up=/etc --copy-up=/run dockerd --iptables=false -H unix:///tmp/xdg/docker.sock --data-root /sandbox/.docker >/tmp/rk.log 2>&1; echo "rootlesskit exit=$?"; tail -2 /tmp/rk.log
echo "--- image entrypoint as shipped"
timeout 30 /usr/local/bin/dockerd-entrypoint.sh dockerd >/tmp/ep1.log 2>&1; echo "entrypoint exit=$?"; tail -2 /tmp/ep1.log
echo "--- image entrypoint, iptables legacy, host network"
DOCKER_IPTABLES_LEGACY=1 DOCKERD_ROOTLESS_ROOTLESSKIT_NET=host timeout 30 /usr/local/bin/dockerd-entrypoint.sh dockerd >/tmp/ep2.log 2>&1; echo "entrypoint exit=$?"; tail -2 /tmp/ep2.log'

  step "SOURCE 1: root identity is not selectable"
  sed 's/run_as_user: sandbox/run_as_user: "0"/; s/run_as_group: sandbox/run_as_group: "0"/' "$HERE/docker-laneA-policy-inside.yaml" >"$WORK/policy-root.yaml"
  show os 120 sandbox create --name p05a-root --from "$IMG" --policy "$WORK/policy-root.yaml" --detach --no-tty --no-auto-providers -- sleep 600
  show os 30 sandbox get p05a-root
}

engine_up() {
  step "SOURCE 2 setup: dedicated engine and authenticating front end on OrbStack (operator action)"
  umask 077
  mkdir -p "$WORK/engine-certs" "$WORK/auth"
  show "$DK" volume create p05a-engine-certs
  "$DK" inspect p05a-engine >/dev/null 2>&1 || show "$DK" run -d --name p05a-engine --privileged --restart no \
    -e DOCKER_TLS_CERTDIR=/certs \
    -e DOCKER_TLS_SAN=DNS:host.openshell.internal,DNS:localhost,IP:127.0.0.1 \
    -v p05a-engine-certs:/certs -p 127.0.0.1:$HOST_PORT_MTLS:2376 docker:dind
  local i=0
  until "$DK" exec p05a-engine test -s /certs/client/key.pem 2>/dev/null; do
    i=$((i + 1))
    [ "$i" -le 60 ] || { echo "engine certs missing"; return 1; }
    sleep 1
  done
  for f in ca.pem cert.pem key.pem; do "$DK" cp "p05a-engine:/certs/client/$f" "$WORK/engine-certs/$f"; done
  chmod 700 "$WORK/engine-certs"
  chmod 600 "$WORK/engine-certs/key.pem"
  show "$DK" --host tcp://127.0.0.1:$HOST_PORT_MTLS --tlsverify --tlscacert "$WORK/engine-certs/ca.pem" \
    --tlscert "$WORK/engine-certs/cert.pem" --tlskey "$WORK/engine-certs/key.pem" \
    version --format 'server={{.Server.Version}} os={{.Server.Os}}/{{.Server.Arch}}'

  [ -s "$WORK/auth/token" ] || openssl rand -hex 24 >"$WORK/auth/token"
  chmod 600 "$WORK/auth/token"
  local token
  token=$(cat "$WORK/auth/token")
  cat >"$WORK/auth/nginx.conf" <<EOF
events {}
http {
  map \$http_upgrade \$connection_upgrade { default upgrade; '' close; }
  server {
    listen 8080;
    location / {
      if (\$http_authorization != "Bearer $token") { return 401; }
      proxy_pass http://p05a-engine-int:2375;
      proxy_http_version 1.1;
      proxy_set_header Upgrade \$http_upgrade;
      proxy_set_header Connection \$connection_upgrade;
      proxy_set_header Host \$host;
      proxy_buffering off;
      proxy_request_buffering off;
      proxy_read_timeout 600s;
    }
  }
}
EOF
  chmod 600 "$WORK/auth/nginx.conf"
  "$DK" network inspect p05a-net >/dev/null 2>&1 || show "$DK" network create p05a-net
  "$DK" inspect p05a-engine-int >/dev/null 2>&1 || show "$DK" run -d --name p05a-engine-int --privileged \
    --network p05a-net -e DOCKER_TLS_CERTDIR= docker:dind
  "$DK" inspect p05a-auth >/dev/null 2>&1 || show "$DK" run -d --name p05a-auth --network p05a-net \
    -v "$WORK/auth/nginx.conf:/etc/nginx/nginx.conf:ro" -p 127.0.0.1:$HOST_PORT_TOKEN:8080 nginx:alpine
  i=0
  until [ "$(curl -s -o /dev/null -w '%{http_code}' -H "Authorization: Bearer $token" http://127.0.0.1:$HOST_PORT_TOKEN/version)" = 200 ]; do
    i=$((i + 1))
    [ "$i" -le 90 ] || { echo "front end not ready"; return 1; }
    sleep 1
  done
  printf 'front end without token: %s\n' "$(curl -s -o /dev/null -w '%{http_code}' http://127.0.0.1:$HOST_PORT_TOKEN/version)"
  printf 'front end with wrong token: %s\n' "$(curl -s -o /dev/null -w '%{http_code}' -H 'Authorization: Bearer wrong' http://127.0.0.1:$HOST_PORT_TOKEN/version)"
  printf 'front end with token: %s\n' "$(curl -s -o /dev/null -w '%{http_code}' -H "Authorization: Bearer $token" http://127.0.0.1:$HOST_PORT_TOKEN/version)"
}

source2_mtls() {
  step "SOURCE 2 (route A): dedicated engine, mTLS client certificate held in the sandbox"
  ensure_sandbox p05a-ext "$IMG" "$HERE/docker-laneA-policy-external-mtls.yaml"
  show os 60 policy set p05a-ext --policy "$HERE/docker-laneA-policy-external-mtls.yaml" --wait
  show os 120 sandbox upload p05a-ext "$WORK/engine-certs" /sandbox/ --no-git-ignore
  sbx p05a-ext 60 'find /sandbox/engine-certs -type f | sort; stat -c "%a %n" /sandbox/engine-certs/*.pem 2>/dev/null || ls -l /sandbox/engine-certs'

  local certdir
  certdir=$(os 60 sandbox exec -n p05a-ext --no-tty -- sh -c 'if [ -f /sandbox/engine-certs/ca.pem ]; then echo /sandbox/engine-certs; else echo /sandbox/engine-certs/engine-certs; fi' | tail -1)
  printf 'cert dir in sandbox: %s\n' "$certdir"
  printf '%s' "$certdir" >"$WORK/certdir"

  sbx p05a-ext 100 "
export DOCKER_HOST=tcp://host.openshell.internal:$HOST_PORT_MTLS DOCKER_TLS_VERIFY=1 DOCKER_CERT_PATH=$certdir
docker version --format 'server={{.Server.Version}} os={{.Server.Os}}/{{.Server.Arch}}' 2>&1 | tail -2"
  sbx p05a-ext 200 "
export DOCKER_HOST=tcp://host.openshell.internal:$HOST_PORT_MTLS DOCKER_TLS_VERIFY=1 DOCKER_CERT_PATH=$certdir
docker run --rm hello-world >/tmp/hw.out 2>&1; echo \"docker run --rm hello-world exit=\$?\"
grep -c 'Hello from Docker' /tmp/hw.out
getent hosts host.openshell.internal
mkdir -p /sandbox/b && printf 'FROM busybox\nRUN echo built\n' >/sandbox/b/Dockerfile
echo '--- build, BuildKit, default builder'
timeout 120 docker build -q -t p05a-built /sandbox/b 2>&1 | tail -2"

  step "SOURCE 2: what the engine ran, and what the host engine did not"
  show "$DK" exec p05a-engine docker images --format '{{.Repository}}:{{.Tag}}'
  show "$DK" exec p05a-engine docker ps -a --format '{{.Names}} {{.Status}}'

  step "SOURCE 2: isolation, a bind mount of the macOS home through the dedicated engine"
  sbx p05a-ext 200 "
export DOCKER_HOST=tcp://host.openshell.internal:$HOST_PORT_MTLS DOCKER_TLS_VERIFY=1 DOCKER_CERT_PATH=$certdir
docker run --rm -v /Users:/mnt/Users:ro alpine sh -c 'test -d /mnt/Users/donbeave/Projects && echo host-home-reachable=yes || echo host-home-reachable=no' 2>&1 | tail -1"

  step "SOURCE 2: policy permits only the engine endpoint"
  sbx p05a-ext 150 "
echo '--- docker binary, other port (the host engine bridge port)'
DOCKER_HOST=tcp://host.openshell.internal:$HOST_PORT_HOST_ENGINE timeout 30 docker version --format '{{.Server.Version}}' 2>&1 | tail -2
echo '--- another binary, engine port'
timeout 30 wget -q -O- -T 10 http://host.openshell.internal:$HOST_PORT_MTLS/ 2>&1 | tail -2
echo '--- loopback literal, docker binary'
DOCKER_HOST=tcp://127.0.0.1:$HOST_PORT_MTLS DOCKER_TLS_VERIFY=1 DOCKER_CERT_PATH=$certdir timeout 30 docker version --format '{{.Server.Version}}' 2>&1 | tail -2"

  step "SOURCE 2: TLS termination at the proxy breaks client-certificate authentication"
  sed '/tls: skip/d' "$HERE/docker-laneA-policy-external-mtls.yaml" >"$WORK/policy-mtls-terminated.yaml"
  show os 60 policy set p05a-ext --policy "$WORK/policy-mtls-terminated.yaml" --wait
  sbx p05a-ext 100 "
export DOCKER_HOST=tcp://host.openshell.internal:$HOST_PORT_MTLS DOCKER_TLS_VERIFY=1 DOCKER_CERT_PATH=$certdir
timeout 60 docker version --format '{{.Server.Version}}' 2>&1 | tail -2"
  show os 60 policy set p05a-ext --policy "$HERE/docker-laneA-policy-external-mtls.yaml" --wait
}

source2_token() {
  step "SOURCE 2 (route B): dedicated engine, bearer token injected by the OpenShell proxy"
  show os 60 profile lint -f "$HERE/docker-laneA-profile-engine-token.yaml"
  os 30 profile describe p05a-docker-engine >/dev/null 2>&1 || show os 60 profile import -f "$HERE/docker-laneA-profile-engine-token.yaml"
  if ! os 30 provider get p05a-engine >/dev/null 2>&1; then
    printf '$ DOCKER_ENGINE_TOKEN=<token file> openshell provider create --name p05a-engine --type p05a-docker-engine --credential DOCKER_ENGINE_TOKEN\n'
    DOCKER_ENGINE_TOKEN=$(cat "$WORK/auth/token") os 60 provider create --name p05a-engine --type p05a-docker-engine --credential DOCKER_ENGINE_TOKEN 2>&1
  fi
  show os 30 provider get p05a-engine
  ensure_sandbox p05a-tok "$IMG" "$HERE/docker-laneA-policy-external-token.yaml" --provider p05a-engine
  show os 60 policy set p05a-tok --policy "$HERE/docker-laneA-policy-external-token.yaml" --wait

  sbx p05a-tok 100 "
case \"\$DOCKER_ENGINE_TOKEN\" in openshell:resolve:env:*) echo \"env value is a placeholder, length \${#DOCKER_ENGINE_TOKEN}\";; *) echo 'ENV VALUE IS NOT A PLACEHOLDER';; esac
mkdir -p /sandbox/.docker
printf '{\"HttpHeaders\":{\"Authorization\":\"Bearer %s\"}}\n' \"\$DOCKER_ENGINE_TOKEN\" >/sandbox/.docker/config.json
export DOCKER_HOST=tcp://host.openshell.internal:$HOST_PORT_TOKEN
echo '--- no header configured'
HOME=/tmp/nohome timeout 30 docker version --format '{{.Server.Version}}' 2>&1 | tail -2
echo '--- header with the placeholder'
timeout 30 docker version --format 'server={{.Server.Version}}' 2>&1 | tail -2"

  sbx p05a-tok 250 "
export DOCKER_HOST=tcp://host.openshell.internal:$HOST_PORT_TOKEN
timeout 150 docker run --rm hello-world >/tmp/hw.out 2>&1; echo \"docker run --rm hello-world exit=\$?\"
tail -3 /tmp/hw.out
echo '--- detached run, wait, logs (no connection upgrade)'
id=\$(docker run -d hello-world 2>&1 | tail -1)
timeout 60 docker wait \"\$id\" 2>&1 | tail -1
timeout 60 docker logs \"\$id\" 2>&1 | sed -n 2,3p
docker rm \"\$id\" >/dev/null 2>&1
echo '--- exec (connection upgrade)'
id2=\$(docker run -d busybox sleep 120 2>&1 | tail -1)
timeout 30 docker exec \"\$id2\" echo hi 2>&1 | tail -1
docker rm -f \"\$id2\" >/dev/null 2>&1
echo '--- build, legacy builder'
mkdir -p /sandbox/b && printf 'FROM busybox\nRUN echo built\n' >/sandbox/b/Dockerfile
DOCKER_BUILDKIT=0 timeout 90 docker build -q -t p05a-built /sandbox/b 2>&1 | tail -1
echo '--- build, BuildKit'
timeout 90 docker build -q -t p05a-built2 /sandbox/b 2>&1 | head -3"

  step "SOURCE 2 (route B, L4 variant): no protocol, allow_uninspected_credentials"
  show os 60 policy set p05a-tok --policy "$HERE/docker-laneA-policy-external-token-l4.yaml" --wait
  sbx p05a-tok 120 "
export DOCKER_HOST=tcp://host.openshell.internal:$HOST_PORT_TOKEN
timeout 20 docker version --format 'server={{.Server.Version}}' 2>&1 | tail -1
timeout 25 docker run --rm hello-world >/tmp/hw2.out 2>&1; echo \"docker run exit=\$? (124 means timed out)\"
tail -2 /tmp/hw2.out"
  printf '$ openshell logs p05a-tok --since 15m --source sandbox | grep 18477 | tail -14\n'
  os 60 logs p05a-tok --since 15m --source sandbox 2>&1 | grep -E '18477|Policy DNS' | cut -c1-250 | tail -14
  show "$DK" exec p05a-engine-int docker ps -a --format '{{.ID}} {{.Image}} {{.Status}}'
  show os 60 policy set p05a-tok --policy "$HERE/docker-laneA-policy-external-token.yaml" --wait
}

source3() {
  step "SOURCE 3: the default OrbStack engine, through a loopback TCP bridge (operator action)"
  curl -s --max-time 5 http://127.0.0.1:$HOST_PORT_HOST_ENGINE/version | python3 -c 'import sys,json; d=json.load(sys.stdin); print("bridge reaches engine", d["Version"], d["Os"], d["Arch"])'
  show os 60 policy set p05a-ext --policy "$HERE/docker-laneA-policy-host-engine.yaml" --wait
  local certdir
  certdir=$(cat "$WORK/certdir")
  sbx p05a-ext 250 "
export DOCKER_HOST=tcp://host.openshell.internal:$HOST_PORT_HOST_ENGINE
timeout 30 docker version --format 'server={{.Server.Version}} os={{.Server.Os}}/{{.Server.Arch}}' 2>&1 | tail -1
docker run --rm hello-world >/tmp/hw4.out 2>&1; echo \"docker run --rm hello-world exit=\$?\"
grep -c 'Hello from Docker' /tmp/hw4.out
echo '--- isolation, a bind mount of the macOS home through the default engine'
docker run --rm -v /Users:/mnt/Users:ro alpine sh -c 'test -d /mnt/Users/donbeave/Projects && echo host-home-reachable=yes || echo host-home-reachable=no' 2>&1 | tail -1"

  step "SOURCE 3 with protocol tcp and no tls: skip (the proxy parses the HTTP)"
  sed '/tls: skip/d' "$HERE/docker-laneA-policy-host-engine.yaml" >"$WORK/policy-host-engine-http.yaml"
  show os 60 policy set p05a-ext --policy "$WORK/policy-host-engine-http.yaml" --wait
  sbx p05a-ext 120 "
export DOCKER_HOST=tcp://host.openshell.internal:$HOST_PORT_HOST_ENGINE
timeout 60 docker run --rm hello-world 2>&1 | tail -1"
  show os 60 policy set p05a-ext --policy "$HERE/docker-laneA-policy-external-mtls.yaml" --wait
}

case "${1:-all}" in
  source1) source1 ;;
  engine) engine_up; bridge_up ;;
  rootless) rootless_engine_probe ;;
  buildkit) buildkit_probe ;;
  source2) source2_mtls ;;
  token) source2_token ;;
  source3) bridge_up; source3 ;;
  cleanup) cleanup ;;
  all)
    show os 30 status
    source1
    engine_up || exit 1
    bridge_up
    source2_mtls
    source2_token
    source3
    rootless_engine_probe
    ;;
  *) echo "usage: $0 [all|source1|engine|source2|token|source3|rootless|buildkit|cleanup]" >&2; exit 2 ;;
esac
