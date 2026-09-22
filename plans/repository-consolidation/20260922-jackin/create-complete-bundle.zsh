#!/bin/zsh
set -euo pipefail

readonly recovery_root='/Users/donbeave/.codex-chainargos2/jackin-recovery-20260922-rzwau1'
readonly bundle_path="$recovery_root/refs-complete.bundle"
typeset -a refs
refs=( "${(@f)$(/usr/bin/git for-each-ref --format='%(refname)')}" )
test ${#refs} -gt 0
/usr/bin/git bundle create "$bundle_path" "${refs[@]}"
/usr/bin/git bundle verify "$bundle_path" | /usr/bin/tail -n 3
/usr/bin/shasum -a 256 "$bundle_path"
