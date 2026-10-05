#!/bin/sh
set -eu

keeper=/Users/donbeave/Projects/tailrocks/jackin-project/jackin
root=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
base=d6ab0f90af5a1e744ae845b8c7172cac5180a09f
tip=b516991d914d23f6de170c8fb6bb3111d2e9315e
tree=63d37fc4f806ba210a6ce7c48a728488e0481c01

test "$(stat -f '%Lp' "$root")" = 700
test "$(shasum -a 256 "$root/r14-private.bundle" | awk '{print $1}')" = c3fd319d9e98e20cac7cf114ccd0f972166e76cee15ff4c151b120c20d3f9dfe
test "$(shasum -a 256 "$root/r14-range.patch" | awk '{print $1}')" = bb6a4e03c62c4a8d1d0386e88d015c5c9af7930a0275964693b94f791cc671f0
test "$(shasum -a 256 "$root/source-index" | awk '{print $1}')" = e68e2f898f43d038e0c6dd694ca4e45beb484587ec054b2c83cbb6cd2904a351
test "$(shasum -a 256 "$root/source-config" | awk '{print $1}')" = 6f54ac1361e08a6e0c360cebd1fa63fef66c49ebb23d1be5c4906c30ad32bb2e

git -C "$keeper" bundle verify "$root/r14-private.bundle" >/dev/null
tmp=$(mktemp -d /private/tmp/r14-verify.XXXXXX)
trap 'rm -Rf "$tmp"' EXIT HUP INT TERM
git init -q "$tmp"
git -C "$tmp" remote add keeper "$keeper"
git -C "$tmp" fetch -q "$keeper" "$base"
git -C "$tmp" fetch -q "$root/r14-private.bundle" refs/heads/recovery:refs/heads/recovered
test "$(git -C "$tmp" rev-parse refs/heads/recovered)" = "$tip"
test "$(git -C "$tmp" rev-parse refs/heads/recovered^{tree})" = "$tree"
git -C "$tmp" fsck --full --no-reflogs --strict --connectivity-only >/dev/null

tail -n +2 "$root/range-object-manifest.tsv" > "$tmp/range-object-manifest.tsv"
while IFS='	' read -r kind oid have path disposition; do
  test -n "$oid"
  git -C "$tmp" cat-file -e "$oid"
done < "$tmp/range-object-manifest.tsv"

git -C "$tmp" checkout -q --detach "$base"
git -C "$tmp" am --3way --keep-non-patch "$root/r14-range.patch" >/dev/null
test "$(git -C "$tmp" rev-parse HEAD^{tree})" = "$tree"
printf '%s\n' 'R14 recovery verification passed.'
