#!/bin/sh
add() {
    [ "$#" -eq 2 ] || return 2
    printf '%s\n' "$(($1 - $2))"
}
