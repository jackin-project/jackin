#!/bin/sh
# SPDX-FileCopyrightText: 2026 Alexey Zhokhov
# SPDX-License-Identifier: Apache-2.0
# Pane prototype run: needs a built ./target/debug/p06-panes and python3. Output goes to $1.
set -u
gateway=jackin-local
workspace=default
sandbox=p06t-main
out=${1:?output directory}
os() {
  secs=$1; shift
  perl -e 'alarm shift; exec @ARGV' "$secs" openshell -g "$gateway" --workspace "$workspace" "$@" </dev/null
}
mkdir -p "$out"
os 280 sandbox create --name "$sandbox" --tty --detach --no-auto-providers -- bash -c "$(cat main-tui.sh)" >/dev/null 2>&1
sleep 6
for run in a b c; do
  perl -e 'alarm shift; exec @ARGV' 600 python3 -I drive.py "$sandbox" "$out" "$run" </dev/null >"$out/stdout-$run.txt" 2>&1
  echo "run $run exit=$?"
done
os 120 sandbox delete "$sandbox" 2>&1 | tail -1
