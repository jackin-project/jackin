#!/usr/bin/env sh
set -eu
mkdir -p /jackin/state/feature-matrix
printf '%s\n' setup-once > /jackin/state/feature-matrix/setup-once.txt
