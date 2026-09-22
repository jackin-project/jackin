#!/bin/zsh
set -u

while IFS= read -r path; do
  [[ -z "$path" ]] && continue
  local_head="$(/usr/bin/git -C "$path" rev-parse HEAD 2>/dev/null || true)"
  branch_name="$(/usr/bin/git -C "$path" symbolic-ref --quiet --short HEAD 2>/dev/null || true)"
  exists=0; dirty=0; tracked=0; untracked=0; ignored=0; operations=""
  [[ -d "$path" ]] && exists=1
  if (( exists )); then
    state_text="$(/usr/bin/git -C "$path" status --porcelain=v2 --untracked-files=all --ignored=matching 2>&1)"
    while IFS= read -r line; do
      [[ -z "$line" || "$line" == '# '* ]] && continue
      if [[ "$line" == '!'* ]]; then
        ignored=$((ignored + 1))
      else
        dirty=1
        if [[ "$line" == '? '* ]]; then untracked=$((untracked + 1)); else tracked=$((tracked + 1)); fi
      fi
    done <<< "$state_text"
    for op in MERGE_HEAD CHERRY_PICK_HEAD REVERT_HEAD BISECT_LOG REBASE_HEAD; do
      op_path="$(/usr/bin/git -C "$path" rev-parse --git-path "$op" 2>/dev/null || true)"
      [[ -f "$op_path" ]] && operations+="${op},"
    done
  fi
  print -r -- "${path}"$'\t'"${local_head}"$'\t'"${branch_name}"$'\t'"${exists}"$'\t'"${dirty}"$'\t'"${tracked}"$'\t'"${untracked}"$'\t'"${ignored}"$'\t'"${operations}"
done
