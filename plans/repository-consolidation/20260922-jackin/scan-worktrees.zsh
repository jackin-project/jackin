#!/bin/zsh
set -u

emit_record() {
  [[ -z "${path:-}" ]] && return
  local exists=0 state_text="" dirty=0 ignored=0 tracked=0 untracked=0 operations=""
  if [[ -d "$path" ]]; then
    exists=1
    state_text="$(/usr/bin/git -C "$path" status --porcelain=v2 --untracked-files=all --ignored=matching 2>&1)"
    while IFS= read -r state_line; do
      [[ -z "$state_line" ]] && continue
      [[ "$state_line" == '# '* ]] && continue
      if [[ "$state_line" == '!'* ]]; then
        ignored=$((ignored + 1))
      else
        dirty=1
        if [[ "$state_line" == '? '* ]]; then
          untracked=$((untracked + 1))
        else
          tracked=$((tracked + 1))
        fi
      fi
    done <<< "$state_text"
    for op in MERGE_HEAD CHERRY_PICK_HEAD REVERT_HEAD BISECT_LOG REBASE_HEAD; do
      local op_path="$(/usr/bin/git -C "$path" rev-parse --git-path "$op" 2>/dev/null || true)"
      [[ -f "$op_path" ]] && operations+="${op},"
    done
  fi
  local branch_name="${branch:-}"
  branch_name="${branch_name#refs/heads/}"
  print -r -- "${path}"$'\t'"${head:-}"$'\t'"${branch_name}"$'\t'"${prunable:-0}"$'\t'"${exists}"$'\t'"${dirty}"$'\t'"${tracked}"$'\t'"${untracked}"$'\t'"${ignored}"$'\t'"${operations}"
  unset path head branch prunable
}

path=""; head=""; branch=""; prunable=0
while IFS= read -r line; do
  if [[ "$line" == worktree\ * ]]; then
    emit_record
    path="${line#worktree }"
    head=""; branch=""; prunable=0
  elif [[ "$line" == HEAD\ * ]]; then
    head="${line#HEAD }"
  elif [[ "$line" == branch\ * ]]; then
    branch="${line#branch }"
  elif [[ "$line" == prunable\ * ]]; then
    prunable=1
  fi
done < <(/usr/bin/git worktree list --porcelain)
emit_record
