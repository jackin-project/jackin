#!/bin/sh
set -eu

root=${1:?usage: check_add.sh REPOSITORY}
check_add() {
    if ! actual=$(/bin/sh -c '
# shellcheck source=examples/workflows/calculator.sh
. "$1" && add "$2" "$3"
' check-add "$root/calculator.sh" "$1" "$2"); then
        printf '%s\n' 'FAIL: calculator candidate exited unsuccessfully' >&2
        return 1
    fi
    if [ "$actual" != "$3" ]; then
        printf 'FAIL: add(%s, %s) returned %s; expected %s\n' "$1" "$2" "$actual" "$3" >&2
        return 1
    fi
}

check_add 2 3 5 || exit 1
check_add -2 -3 -5 || exit 1
printf '%s\n' 'PASS: required arithmetic cases'
