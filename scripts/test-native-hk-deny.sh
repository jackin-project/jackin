#!/usr/bin/env bash
# SPDX-FileCopyrightText: 2026 Alexey Zhokhov
# SPDX-License-Identifier: Apache-2.0

set -euo pipefail

project_root=$(cd "$(dirname "$0")/.." && pwd -P)
[[ ${HK_BIN:-} == /* && -x "$HK_BIN" ]] || {
  printf '%s\n' 'Set HK_BIN to an installed absolute hk 2.8.0 executable.' >&2
  exit 1
}
[[ $("$HK_BIN" --version) == 'hk 2.8.0' ]]
export MISE_AUTO_INSTALL=false MISE_EXEC_AUTO_INSTALL=false MISE_OFFLINE=true HK_PKL_OFFLINE=true
tmp_root=$(mktemp -d "${TMPDIR:-/tmp}/jackin native hk deny.XXXXXX")
tmp_root=$(cd "$tmp_root" && pwd -P)
phase=setup
cleanup() {
  status=$?
  if [[ $status -eq 0 ]]; then
    rm -rf "$tmp_root"
  else
    printf 'Native HK fixture failed during %s; retained at %s\n' "$phase" "$tmp_root" >&2
    for log in "$tmp_root/positive.log" "$tmp_root/denied.log"; do
      if [[ -f $log ]]; then cat "$log" >&2; fi
    done
  fi
  exit "$status"
}
trap cleanup EXIT
fixture="$tmp_root/repository with spaces"
nested="$fixture/crates/nested workspace"
excluded="$fixture/crates/tools/jackin-lints"
mkdir -p "$fixture/scripts" "$fixture/crates/root member/src" \
  "$nested/member/src" "$excluded/src" "$tmp_root/bin"
cp "$project_root/hk.pkl" "$fixture/hk.pkl"
cp "$project_root/scripts/hk-workspace-admission" "$fixture/scripts/"
git -C "$fixture" init -q -b main
cat >"$fixture/Cargo.toml" <<'TOML'
[workspace]
members = ["crates/root member"]
exclude = ["crates/nested workspace", "crates/tools/jackin-lints"]
resolver = "3"
TOML
cat >"$nested/Cargo.toml" <<'TOML'
[workspace]
members = ["member"]
resolver = "3"
TOML
for manifest in "$fixture/crates/root member/Cargo.toml" "$nested/member/Cargo.toml" "$excluded/Cargo.toml"; do
  cat >"$manifest" <<'TOML'
[package]
name = "fixture-package"
version = "0.1.0"
edition = "2024"
TOML
done
for workspace in "$fixture" "$nested" "$excluded"; do
  printf 'version = 4\n' >"$workspace/Cargo.lock"
  printf '[licenses]\nallow = ["MIT"]\n' >"$workspace/deny.toml"
done
printf 'pub fn value() {}\n' >"$fixture/crates/root member/src/lib.rs"
printf 'pub fn value() {}\n' >"$nested/member/src/lib.rs"
printf 'pub fn value() {}\n' >"$excluded/src/lib.rs"
git -C "$fixture" add .
staged_tree=$(git -C "$fixture" write-tree)
cat >"$tmp_root/bin/rust-build-guard" <<'SH'
#!/bin/sh
set -eu
if [ "${HK_TEST_GUARD_DENY:-0}" = 1 ]; then
  printf '%s\n' 'fixture guard denied cargo deny' >&2
  exit 125
fi
if [ "${1:-}" = -- ]; then shift; fi
exec "$@"
SH
cat >"$tmp_root/bin/mise" <<'SH'
#!/bin/sh
set -eu
[ "$#" -ge 3 ] && [ "$1" = exec ] && [ "$2" = -- ]
shift 2
exec "$@"
SH
cat >"$tmp_root/bin/cargo" <<'SH'
#!/bin/sh
set -eu
[ "$#" -eq 4 ] && [ "$1" = deny ] && [ "$2" = --locked ] &&
  [ "$3" = --workspace ] && [ "$4" = check ]
case "$PWD" in
  "$HK_TEST_ROOT"|"$HK_TEST_NESTED") ;;
  *) printf '%s\n' 'Cargo scan did not select a workspace root' >&2; exit 125 ;;
esac
shift
exec cargo-deny "$@"
SH
cat >"$tmp_root/bin/cargo-deny" <<'SH'
#!/bin/sh
set -eu
[ "$#" -eq 3 ] && [ "$1" = --locked ] && [ "$2" = --workspace ] && [ "$3" = check ]
case "$PWD" in
  "$HK_TEST_ROOT"|"$HK_TEST_ROOT/crates/root member") workspace=$HK_TEST_ROOT ;;
  "$HK_TEST_NESTED"|"$HK_TEST_NESTED/member") workspace=$HK_TEST_NESTED ;;
  *) printf '%s\n' 'Unexpected workspace scan' >&2; exit 125 ;;
esac
printf 'scan passed\n' >"$workspace/deny-scan-passed"
SH
chmod +x "$tmp_root/bin/"*
export PATH="$tmp_root/bin:$PATH"
export HK_TEST_ROOT="$fixture" HK_TEST_NESTED="$nested"
phase='positive workspace scans'
"$HK_BIN" --cd "$fixture" --jobs 1 check --all --check --step cargo_deny >"$tmp_root/positive.log" 2>&1
[[ -f "$fixture/deny-scan-passed" && -f "$nested/deny-scan-passed" ]]
[[ ! -e "$fixture/crates/root member/deny-scan-passed" && ! -e "$nested/member/deny-scan-passed" ]]
[[ ! -e "$excluded/deny-scan-passed" ]]
git -C "$fixture" diff --quiet
[[ $(git -C "$fixture" write-tree) == "$staged_tree" ]]
rm "$fixture/deny-scan-passed" "$nested/deny-scan-passed"
phase='guard-denied scans'
for workspace in "$fixture" "$nested"; do
  if HK_TEST_GUARD_DENY=1 "$HK_BIN" --cd "$workspace" --jobs 1 check --all --check --step cargo_deny >"$tmp_root/denied.log" 2>&1; then
    printf '%s\n' 'HK cargo_deny bypassed the installed guard denial.' >&2
    exit 1
  fi
  grep -Fq 'fixture guard denied cargo deny' "$tmp_root/denied.log"
  [[ ! -e "$fixture/deny-scan-passed" && ! -e "$nested/deny-scan-passed" ]]
  [[ ! -e "$fixture/crates/root member/deny-scan-passed" && ! -e "$nested/member/deny-scan-passed" ]]
  [[ ! -e "$excluded/deny-scan-passed" ]]
  git -C "$fixture" diff --quiet
  [[ $(git -C "$fixture" write-tree) == "$staged_tree" ]]
done
printf '%s\n' 'Native HK guarded workspace scans passed.'
