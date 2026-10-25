#!/usr/bin/env bash
set -euo pipefail

repo_root=$(git rev-parse --show-toplevel)
tmp_root=$(mktemp -d "${TMPDIR:-/tmp}/jackin-hk-cargo.XXXXXX")
trap 'rm -rf "$tmp_root"' EXIT

fixture="$tmp_root/repo with spaces"
outside="$tmp_root/outside"
mock_bin="$tmp_root/mock-bin"
missing_guard_bin="$tmp_root/missing-guard-bin"
bad_guard_bin="$tmp_root/bad-guard-bin"
mkdir -p "$fixture/crates/nested member" "$outside" "$mock_bin" \
  "$missing_guard_bin" "$bad_guard_bin"
cp "$repo_root/scripts/hk-cargo.sh" "$fixture/hk-cargo.sh"
git -C "$fixture" init -q
fixture=$(cd -P "$fixture" && pwd -P)
printf '%s\n' '[workspace]' >"$fixture/Cargo.toml"
printf '%s\n' '[package]' >"$fixture/crates/nested member/Cargo.toml"
printf '%s\n' '[package]' >"$outside/Cargo.toml"

cat >"$mock_bin/mise" <<'MOCK_MISE'
#!/usr/bin/env bash
set -euo pipefail
{
  printf 'cwd=%s\n' "$PWD"
  for argument in "$@"; do printf 'arg=%s\n' "$argument"; done
} >>"$HK_MISE_LOG"
MOCK_MISE
cat >"$mock_bin/rust-build-guard" <<'MOCK_GUARD'
#!/usr/bin/env bash
set -euo pipefail
{
  printf 'cwd=%s\n' "$PWD"
  for argument in "$@"; do printf 'arg=%s\n' "$argument"; done
} >>"$HK_GUARD_LOG"
if [[ ${HK_GUARD_STATUS:-0} != 0 ]]; then exit "$HK_GUARD_STATUS"; fi
shift
exec "$@"
MOCK_GUARD
chmod +x "$mock_bin/mise" "$mock_bin/rust-build-guard"
printf '%s\n' '#!/bin/sh' 'exit 0' >"$bad_guard_bin/rust-build-guard"
chmod 644 "$bad_guard_bin/rust-build-guard"

guard_log="$tmp_root/guard.log"
mise_log="$tmp_root/mise.log"
expected_guard="$tmp_root/expected-guard.log"
expected_mise="$tmp_root/expected-mise.log"
clear_logs() {
  : >"$guard_log"
  : >"$mise_log"
}

run_operation() {
  local path=$1 manifest=$2 operation=$3 guard_status=${4:-0}
  (
    cd "$fixture"
    PATH="$path" HK_GUARD_STATUS="$guard_status" \
      HK_GUARD_LOG="$guard_log" HK_MISE_LOG="$mise_log" \
      bash ./hk-cargo.sh "$manifest" "$operation"
  )
}

assert_operation() {
  local manifest=$1 operation=$2 cwd=$3
  shift 3
  local -a cargo_arguments=("$@")
  clear_logs
  run_operation "$mock_bin:/usr/bin:/bin:/opt/homebrew/bin" "$manifest" "$operation"
  {
    printf 'cwd=%s\n' "$cwd"
    printf '%s\n' 'arg=--' 'arg=mise' 'arg=exec' 'arg=--' 'arg=cargo'
    for argument in "${cargo_arguments[@]}"; do printf 'arg=%s\n' "$argument"; done
  } >"$expected_guard"
  diff -u "$expected_guard" "$guard_log"
  {
    printf 'cwd=%s\n' "$cwd"
    printf '%s\n' 'arg=exec' 'arg=--' 'arg=cargo'
    for argument in "${cargo_arguments[@]}"; do printf 'arg=%s\n' "$argument"; done
  } >"$expected_mise"
  diff -u "$expected_mise" "$mise_log"
}

assert_operation Cargo.toml fmt-check "$fixture" fmt --all --check
assert_operation Cargo.toml fmt-files "$fixture" fmt --all --check -- --files-with-diff
assert_operation Cargo.toml fmt-fix "$fixture" fmt --all
assert_operation Cargo.toml check "$fixture" check --locked --quiet
assert_operation Cargo.toml check-fix "$fixture" fix --locked --allow-dirty --allow-staged --quiet
assert_operation Cargo.toml clippy "$fixture" clippy --locked --quiet
assert_operation Cargo.toml clippy-fix "$fixture" clippy --locked --fix --allow-dirty --allow-staged --quiet
assert_operation "crates/nested member/Cargo.toml" check \
  "$fixture/crates/nested member" check --locked --quiet

clear_logs
failure_status=0
run_operation "$mock_bin:/usr/bin:/bin:/opt/homebrew/bin" Cargo.toml check \
  23 >/dev/null 2>&1 || failure_status=$?
[[ $failure_status -eq 23 && ! -s "$mise_log" ]] || {
  printf '%s\n' 'guard failure did not propagate before Mise' >&2
  exit 1
}

clear_logs
failure_status=0
run_operation "$missing_guard_bin:/usr/bin:/bin:/opt/homebrew/bin" Cargo.toml check \
  >/dev/null 2>&1 || failure_status=$?
[[ $failure_status -eq 69 && ! -s "$mise_log" ]] || {
  printf '%s\n' 'missing guard did not fail closed before Mise' >&2
  exit 1
}

clear_logs
failure_status=0
run_operation "$bad_guard_bin:/usr/bin:/bin:/opt/homebrew/bin" Cargo.toml check \
  >/dev/null 2>&1 || failure_status=$?
[[ $failure_status -eq 69 && ! -s "$mise_log" ]] || {
  printf '%s\n' 'invalid guard did not fail closed before Mise' >&2
  exit 1
}

ln -s "$outside/Cargo.toml" "$fixture/linked-Cargo.toml"
mkdir "$fixture/linked"
mkdir "$fixture/linked-file"
ln -s "$outside/Cargo.toml" "$fixture/linked-file/Cargo.toml"
ln -s "$outside" "$fixture/escaped"
for invalid_manifest in \
  "$outside/Cargo.toml" \
  linked-Cargo.toml \
  linked/Cargo.toml \
  linked-file/Cargo.toml \
  escaped/Cargo.toml \
  missing/Cargo.toml \
  Cargo.toml.bak; do
  clear_logs
  failure_status=0
  run_operation "$mock_bin:/usr/bin:/bin:/opt/homebrew/bin" "$invalid_manifest" check \
    >/dev/null 2>&1 || failure_status=$?
  [[ $failure_status -eq 65 && ! -s "$guard_log" && ! -s "$mise_log" ]] || {
    printf 'invalid manifest reached guard or Mise: %s\n' "$invalid_manifest" >&2
    exit 1
  }
done

printf '%s\n' 'Jackin guarded hk Cargo adapter tests passed'
