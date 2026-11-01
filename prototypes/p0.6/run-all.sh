#!/bin/sh
set -u
gateway=jackin-local
workspace=default
bin=./target/debug/p06-proof
out=${1:?output directory}
mkdir -p "$out"
os() {
  secs=$1; shift
  perl -e 'alarm shift; exec @ARGV' "$secs" openshell -g "$gateway" --workspace "$workspace" "$@" </dev/null
}
run() {
  sandbox=$1; scenario=$2
  perl -e 'alarm shift; exec @ARGV' 120 "$bin" "$gateway" "$workspace" "$sandbox" "$scenario" </dev/null >"$out/$scenario.txt" 2>&1
  echo "$scenario exit=$?"
}
os 240 sandbox create --name p06-main --tty --detach --no-auto-providers -- bash -c "$(cat main-script.sh)" >/dev/null 2>&1
os 240 sandbox create --name p06-big --tty --detach --no-auto-providers -- bash -c 'seq 1 300000; echo END; sleep 900' >/dev/null 2>&1
for scenario in shell shell-drop main concurrent limit revoke nudge keys; do run p06-main "$scenario"; done
os 240 sandbox create --name p06-exit --tty --detach --no-auto-providers -- bash -c 'echo bye; sleep 20; echo leaving; exit 7' >/dev/null 2>&1
run p06-exit exit
run p06-big big
"$bin" "$gateway" "$workspace" p06-main hold </dev/null >"$out/hold.txt" 2>&1 &
holder=$!
sleep 4
kill -9 "$holder"
wait "$holder" 2>/dev/null
run p06-main probe
for name in p06-main p06-big p06-exit; do os 120 sandbox delete "$name" 2>&1 | tail -1; done
