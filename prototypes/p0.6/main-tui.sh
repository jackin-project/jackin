# SPDX-FileCopyrightText: 2026 Alexey Zhokhov
# SPDX-License-Identifier: Apache-2.0
# Main process for the P0.6 pane prototype: a full-screen app that paints only what changed
# (like a diff-rendering TUI). Header rows 1-3 are painted at start and on SIGWINCH only.
# Typing "flood" + Enter writes several MB of in-place repaints of rows 5.. (window overflow),
# one write per frame, and each flood has a different frame length, so the 4096-byte PTY reads
# cut the frames at different bytes.
stty -echo -isig 2>/dev/null
typed=""
tickn=0
floodn=0
ROWS=24; COLS=80

rule() { printf -v line '%*s' "$1" ''; printf '%s' "${line// /-}"; }
typedrow() {
  local shown=$typed
  if [ ${#shown} -gt $((COLS - 8)) ]; then shown=${shown:$((${#shown} - COLS + 8))}; fi
  printf '\e[2;1H\e[33mtyped: %s\e[0m\e[K' "$shown"
}
tickrow() { printf '\e[4;1H\e[32mtick %d\e[0m\e[K' "$tickn"; }
static() {
  printf '\e[2J\e[1;1H\e[1;37;44m main pid=%s pty=%sx%s \e[0m\e[K' "$$" "$ROWS" "$COLS"
  printf '\e[3;1H\e[36m%s\e[0m' "$(rule $((COLS - 1)))"
  typedrow
  tickrow
}
resize() { set -- $(stty size); ROWS=$1; COLS=$2; static; }
flood() {
  local f r i frame segs=""
  floodn=$((floodn + 1))
  for ((i = 0; i < (COLS - 20) / 3 - floodn % 6; i++)); do segs+=$'\e[3'"$((i % 7 + 1))"$'m###'; done
  for ((f = 1; f <= 1500; f++)); do
    frame=""
    for ((r = 5; r <= ROWS; r++)); do
      printf -v row '\e[%d;1H\e[1;3%dmlog f=%04d r=%02d %s\e[K' "$r" $((r % 7 + 1)) "$f" "$r" "$segs"
      frame+=$row
    done
    printf '%s' "$frame"
  done
  printf '\e[5;1H\e[0m\e[1;35mflood done f=%d\e[0m\e[K' "$((f - 1))"
}

trap '' INT QUIT TSTP
trap resize WINCH
printf '\e[?1049h\e[?25l'
resize
while :; do
  if read -rsn1 -t 1 ch; then
    if [ -z "$ch" ]; then
      if [[ $typed == *flood ]]; then typed=""; typedrow; flood; else typed="${typed}|"; typedrow; fi
    else
      printf -v code '%d' "'$ch"
      if [ "$code" -lt 32 ]; then printf -v ch "\\$(printf %o $((code + 64)))"; ch="^$ch"; fi
      typed="${typed}${ch}"
      typedrow
    fi
  else
    tickn=$((tickn + 1))
    tickrow
  fi
done
