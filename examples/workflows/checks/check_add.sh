#!/bin/sh
set -eu

root=${1:?usage: check_add.sh REPOSITORY}
# shellcheck source=examples/workflows/calculator.sh
. "$root/calculator.sh"

check_add() {
    actual=$(add "$1" "$2")
    if [ "$actual" != "$3" ]; then
        printf 'FAIL: add(%s, %s) returned %s; expected %s\n' "$1" "$2" "$actual" "$3" >&2
        return 1
    fi
}

check_add 2 3 5
check_add -2 -3 -5
printf '%s\n' 'PASS: required arithmetic cases'
