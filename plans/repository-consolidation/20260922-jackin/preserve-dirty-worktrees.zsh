#!/bin/zsh
set -euo pipefail

readonly git_common='/Users/donbeave/Projects/jackin-project/jackin'
readonly recovery_root='/Users/donbeave/.codex-chainargos2/jackin-recovery-20260922-rzwau1/dirty-worktrees'

typeset -a labels paths
labels=(
  apple-usage-relay-peer-isolation
  auth-source-identity-repair
  console-identity
  launch-security-repair
  restore-identity-expansion
  usage-credential-routing
  usage-presentation-repair
)
paths=(
  '/Users/donbeave/Projects/jackin-project/jackin-apple-usage-relay-peer-isolation-20260920'
  '/Users/donbeave/Projects/jackin-project/jackin-auth-source-identity-repair-20260920'
  '/Users/donbeave/Projects/jackin-project/jackin-console-identity'
  '/Users/donbeave/Projects/jackin-project/jackin-launch-security-repair-20260920'
  '/Users/donbeave/Projects/jackin-project/jackin-restore-identity-expansion-20260920'
  '/Users/donbeave/Projects/jackin-project/jackin-usage-credential-routing-20260920'
  '/Users/donbeave/Projects/jackin-project/jackin-usage-presentation-repair'
)

if (( ${#labels} != ${#paths} )); then
  print -u2 'label/path manifest length mismatch'
  exit 2
fi

for i in {1..${#labels}}; do
  label="${labels[$i]}"
  path="${paths[$i]}"
  out="${recovery_root}/${label}"
  test -d "$path"
  /bin/mkdir -p "$out/untracked"

  /usr/bin/git -C "$path" rev-parse HEAD > "$out/base.txt"
  /usr/bin/git -C "$path" status --short --branch --ignored > "$out/status.txt"
  /usr/bin/git -C "$path" status --porcelain=v2 -z > "$out/status-porcelain-v2.z"
  /usr/bin/git -C "$path" diff --binary > "$out/unstaged.patch"
  /usr/bin/git -C "$path" diff --cached --binary > "$out/staged.patch"
  /usr/bin/git -C "$path" diff --stat > "$out/unstaged-stat.txt"
  /usr/bin/git -C "$path" diff --cached --stat > "$out/staged-stat.txt"
  /usr/bin/git -C "$path" diff --name-status > "$out/unstaged-files.tsv"
  /usr/bin/git -C "$path" diff --cached --name-status > "$out/staged-files.tsv"
  /usr/bin/git -C "$path" ls-files --others --exclude-standard > "$out/untracked-files.txt"
  /usr/bin/git -C "$path" ls-files --others --ignored --exclude-standard > "$out/ignored-files.txt"
  /usr/bin/git -C "$path" rev-parse --git-path index > "$out/index-path.txt"
  index_path="$(/usr/bin/git -C "$path" rev-parse --git-path index)"
  /bin/cp -p "$index_path" "$out/index"

  while IFS= read -r file; do
    test -n "$file" || continue
    destination="$out/untracked/$file"
    /bin/mkdir -p "$(/usr/bin/dirname "$destination")"
    /bin/cp -p "$path/$file" "$destination"
  done < "$out/untracked-files.txt"

  print "preserved $label: $(/usr/bin/git -C "$path" rev-parse --short HEAD)"
done
