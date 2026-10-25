#!/usr/bin/env bash
set -euo pipefail

usage() {
  printf '%s\n' 'usage: hk-cargo.sh MANIFEST OP' >&2
  exit 64
}

[[ $# -eq 2 ]] || usage
manifest_input=$1
operation=$2
case "$operation" in
  fmt-check|fmt-files|fmt-fix|check|check-fix|clippy|clippy-fix) ;;
  *) usage ;;
esac

repo_root=$(git rev-parse --show-toplevel)
cd -P "$repo_root"
repo_root=$(pwd -P)
case "$manifest_input" in
  /*) manifest_path=$manifest_input ;;
  *) manifest_path="$repo_root/$manifest_input" ;;
esac
manifest_name=${manifest_path##*/}
[[ "$manifest_name" == Cargo.toml && ! -L "$manifest_path" && -f "$manifest_path" ]] || {
  printf '%s\n' 'hk-cargo: expected a regular Cargo.toml inside this repository' >&2
  exit 65
}
manifest_dir=${manifest_path%/*}
manifest_dir=$(cd -P "$manifest_dir" && pwd -P)
case "$manifest_dir/" in
  "$repo_root/"*) ;;
  *)
    printf '%s\n' 'hk-cargo: manifest resolves outside this repository' >&2
    exit 65
    ;;
esac
cd -P "$manifest_dir"

run_guarded_cargo() {
  local guard_path path_list path_entry candidate finished
  guard_path=
  path_list=${PATH:-}
  while :; do
    finished=0
    case "$path_list" in
      *:*) path_entry=${path_list%%:*}; path_list=${path_list#*:} ;;
      *) path_entry=$path_list; path_list=; finished=1 ;;
    esac
    candidate="${path_entry:-.}/rust-build-guard"
    if [[ -e "$candidate" || -L "$candidate" ]]; then
      guard_path=$candidate
      break
    fi
    [[ $finished -eq 0 ]] || break
  done
  if [[ -z "$guard_path" ]]; then
    printf '%s\n' 'hk-cargo: rust-build-guard is required but was not found in PATH' >&2
    return 69
  fi
  if [[ ! -f "$guard_path" || ! -x "$guard_path" ]]; then
    printf '%s\n' 'hk-cargo: rust-build-guard is not a regular executable' >&2
    return 69
  fi
  "$guard_path" -- mise exec -- cargo "$@"
}

case "$operation" in
  fmt-check) run_guarded_cargo fmt --all --check ;;
  fmt-files) run_guarded_cargo fmt --all --check -- --files-with-diff ;;
  fmt-fix) run_guarded_cargo fmt --all ;;
  check) run_guarded_cargo check --locked --quiet ;;
  check-fix) run_guarded_cargo fix --locked --allow-dirty --allow-staged --quiet ;;
  clippy) run_guarded_cargo clippy --locked --quiet ;;
  clippy-fix) run_guarded_cargo clippy --locked --fix --allow-dirty --allow-staged --quiet ;;
esac
