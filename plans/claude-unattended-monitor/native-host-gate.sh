#!/bin/bash
# Operator handoff only. This script is intentionally not executed by the reviewer.
# It records nonsecret host/source facts; --run is required to execute either gate.
set -euo pipefail

usage() {
  cat >&2 <<'USAGE'
Usage:
  jackin-native-host-gate.sh --capture-only <evidence-dir>
  jackin-native-host-gate.sh --run <evidence-dir>

The evidence directory must be outside the checkout. Raw logs and JUnit stay there.
USAGE
}

[[ $# -eq 2 ]] || { usage; exit 2; }
mode=$1
evidence_dir=$2
[[ $mode == --capture-only || $mode == --run ]] || { usage; exit 2; }

repo_root=$(git rev-parse --show-toplevel)
repo_root=$(cd "$repo_root" && pwd -P)
case "$evidence_dir" in
  "$repo_root"|"$repo_root"/*)
    echo "evidence directory must be outside checkout" >&2
    exit 2
    ;;
esac

mkdir -p "$evidence_dir"
chmod 700 "$evidence_dir"
evidence_dir=$(cd "$evidence_dir" && pwd -P)
if [[ "$evidence_dir" == "$repo_root" || "$evidence_dir" == "$repo_root"/* ]]; then
  echo "resolved evidence directory is inside checkout" >&2
  exit 2
fi

MBX_BIN=${MBX_BIN:-/Users/donbeave/.local/share/mise/installs/mr-boxington/1.23.0/.mise-bins/mbx}
MISE_BIN=${MISE_BIN:-/Users/donbeave/.local/bin/mise}
MBX_ENV=(env -u CARGO -u RUSTC -u RUSTDOC -u RUSTUP_TOOLCHAIN)
[[ "$MBX_BIN" = /* && -x "$MBX_BIN" ]] || { echo "absolute MBX 1.23.0 executable required" >&2; exit 2; }
[[ "$MISE_BIN" = /* && -x "$MISE_BIN" ]] || { echo "absolute Mise executable required" >&2; exit 2; }

mbx_version=$("${MBX_ENV[@]}" "$MBX_BIN" --version | awk 'NR == 1 { print $2 }')
mise_version=$("$MISE_BIN" --version | awk 'NR == 1 { print $2 }')
[[ "$mbx_version" == "1.23.0" ]] || { echo "expected MBX 1.23.0, got $mbx_version" >&2; exit 2; }
[[ "$mise_version" == "2026.10.7" ]] || { echo "expected Mise 2026.10.7, got $mise_version" >&2; exit 2; }

provenance="$evidence_dir/host-provenance.txt"
{
  printf 'captured_utc=%s\n' "$(date -u +%Y-%m-%dT%H:%M:%SZ)"
  printf 'source_commit='
  git -C "$repo_root" rev-parse HEAD
  printf 'source_branch='
  git -C "$repo_root" branch --show-current
  printf 'source_status_porcelain_begin\n'
  git -C "$repo_root" status --porcelain=v1 --untracked-files=all
  printf 'source_status_porcelain_end\n'
  printf 'source_config_sha256_begin\n'
  shasum -a 256 "$repo_root/mise.toml" "$repo_root/native/mise.toml" "$repo_root/Cargo.lock" "$repo_root/rust-toolchain.toml"
  printf 'source_config_sha256_end\n'
  printf 'mbx_path=%s\nmbx_version=%s\n' "$MBX_BIN" "$("${MBX_ENV[@]}" "$MBX_BIN" --version | sed -n '1p')"
  shasum -a 256 "$MBX_BIN"
  printf 'mise_path=%s\nmise_version=%s\n' "$MISE_BIN" "$("$MISE_BIN" --version | sed -n '1p')"
  shasum -a 256 "$MISE_BIN"
  printf 'macos_version='
  sw_vers -productVersion
  printf 'macos_build='
  sw_vers -buildVersion
  printf 'host_arch='
  uname -m
  printf 'xcode_version_begin\n'
  xcodebuild -version
  printf 'xcode_version_end\n'
  printf 'xcode_select_path='
  xcode-select -p
  printf 'macos_sdk='
  xcrun --sdk macosx --show-sdk-version
  printf 'swift_version_begin\n'
  swift --version
  printf 'swift_version_end\n'
  printf 'orbstack_version_begin\n'
  orb version
  printf 'orbstack_version_end\n'
  printf 'docker_context='
  docker context show
  printf 'docker_server_facts='
  docker info --format '{{.ServerVersion}}|{{.OperatingSystem}}|{{.OSType}}|{{.Architecture}}|{{.NCPU}}'
} >"$provenance" 2>&1

if [[ $mode == --capture-only ]]; then
  shasum -a 256 "$provenance" >"$evidence_dir/host-provenance.sha256"
  printf 'Captured host provenance at %s (no gate executed).\n' "$evidence_dir"
  exit 0
fi

if [[ -n "$(git -C "$repo_root" status --porcelain=v1 --untracked-files=all)" ]]; then
  echo "checkout must be clean before required platform gates" >&2
  exit 2
fi

# Enforce the operator packet's concurrency cap even when --run is invoked
# directly, without the documented shell setup. These exports are local to this
# script process and do not modify the caller's environment or persistent config.
export CARGO_BUILD_JOBS=2
export NEXTEST_TEST_THREADS=2
export RUST_TEST_THREADS=2

{
  printf 'cargo_build_jobs=%s\n' "$CARGO_BUILD_JOBS"
  printf 'nextest_test_threads=%s\n' "$NEXTEST_TEST_THREADS"
  printf 'rust_test_threads=%s\n' "$RUST_TEST_THREADS"
} >>"$provenance"

run_gate() {
  name=$1
  shift
  log="$evidence_dir/$name.log"
  started=$(date -u +%Y-%m-%dT%H:%M:%SZ)
  {
    printf 'started_utc=%s\n' "$started"
    printf 'argv='
    printf '%q ' "$@"
    printf '\n'
  } >"$log"
  set +e
  "$@" >>"$log" 2>&1
  status=$?
  set -e
  {
    printf 'exit_code=%s\n' "$status"
    printf 'finished_utc=%s\n' "$(date -u +%Y-%m-%dT%H:%M:%SZ)"
  } >>"$log"
  shasum -a 256 "$log" >"$evidence_dir/$name.log.sha256"
  return "$status"
}

cd "$repo_root"
set +e
run_gate orbstack-usage-broker-e2e "${MBX_ENV[@]}" "$MBX_BIN" +1.99.0 xtask ci --e2e
e2e_status=$?
set -e
junit="$repo_root/target/nextest/docker-e2e/junit.xml"
if [[ -f "$junit" && ! -L "$junit" ]]; then
  cp "$junit" "$evidence_dir/orbstack-usage-broker-e2e.junit.xml"
  shasum -a 256 "$evidence_dir/orbstack-usage-broker-e2e.junit.xml" >"$evidence_dir/orbstack-usage-broker-e2e.junit.xml.sha256"
else
  printf 'missing_or_unsafe_junit_path=%s\n' "$junit" >"$evidence_dir/orbstack-usage-broker-e2e.junit-missing.txt"
fi

set +e
PATH="$(dirname "$MBX_BIN"):$PATH" run_gate native-desktop-ci "${MBX_ENV[@]}" "$MBX_BIN" exec --project-root "$repo_root" "$MISE_BIN" -C native run ci
native_status=$?
set -e
shasum -a 256 "$provenance" >"$evidence_dir/host-provenance.sha256"

if [[ $e2e_status -ne 0 || $native_status -ne 0 ]]; then
  exit 1
fi
