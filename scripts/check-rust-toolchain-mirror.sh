#!/bin/sh
# SPDX-FileCopyrightText: 2026 Alexey Zhokhov
# SPDX-License-Identifier: Apache-2.0
# Fail unless rust-toolchain.toml mirrors the mise.toml rust pin: channel, components, targets and (when rust-toolchain.toml
# declares one) profile. Needs only mise (already required by the repo). Read-only.
set -eu

mise_file=${MIRROR_MISE_TOML:-mise.toml}
rtt_file=${MIRROR_RTT_TOML:-rust-toolchain.toml}

[ -f "$mise_file" ] || { echo "rust-toolchain-mirror: $mise_file not found" >&2; exit 1; }
[ -f "$rtt_file" ] || { echo "rust-toolchain-mirror: $rtt_file not found (Rust repos keep both files)" >&2; exit 1; }

get() { mise config get --file "$1" "$2" 2>/dev/null || true; }
# '["b", "a"]' -> 'a,b'; '"x"' -> 'x'
norm() { tr -d ' "[]\r' | tr ',' '\n' | sed '/^$/d' | LC_ALL=C sort | paste -sd, -; }

# `rust = "1.99.1"` (string) or `rust = { version = ... }` (table).
raw=$(get "$mise_file" tools.rust)
case "$raw" in
  '') echo "rust-toolchain-mirror: $mise_file has no tools.rust pin" >&2; exit 1 ;;
  '"'*) m_channel=$(printf '%s' "$raw" | norm); m_profile=; m_components=; m_targets= ;;
  *)
    m_channel=$(get "$mise_file" tools.rust.version | norm)
    m_profile=$(get "$mise_file" tools.rust.profile | norm)
    m_components=$(get "$mise_file" tools.rust.components | norm)
    m_targets=$(get "$mise_file" tools.rust.targets | norm)
    ;;
esac
r_channel=$(get "$rtt_file" toolchain.channel | norm)
r_profile=$(get "$rtt_file" toolchain.profile | norm)
r_components=$(get "$rtt_file" toolchain.components | norm)
r_targets=$(get "$rtt_file" toolchain.targets | norm)

status=0
cmp_field() { # name mise-value rtt-value
  if [ "$2" != "$3" ]; then
    echo "rust-toolchain-mirror: $1 differs: $mise_file='$2' $rtt_file='$3'" >&2
    status=1
  fi
}
cmp_field channel "$m_channel" "$r_channel"
cmp_field components "$m_components" "$r_components"
cmp_field targets "$m_targets" "$r_targets"
# rustup treats an absent profile as "default"; compare only when rust-toolchain.toml states one.
[ -z "$r_profile" ] || cmp_field profile "$m_profile" "$r_profile"
[ "$status" -eq 0 ] || echo "rust-toolchain-mirror: update both files together (mise.toml is the source)" >&2
exit "$status"
