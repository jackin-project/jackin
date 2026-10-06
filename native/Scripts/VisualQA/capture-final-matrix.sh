#!/bin/bash
# Regenerate the branch-head native verification matrix from one validated app bundle.
set -euo pipefail

here=$(cd "$(dirname "$0")" && pwd -P)
repo=$(cd "$here/../../.." && pwd -P)
app=${1:-"$repo/native/dist/JackinDesktop.app"}
output="$repo/native/.build/visual-qa/final"
window_tool="$repo/native/.build/final-window-id"
notification_tool="$repo/native/.build/final-notification-drive"
focus_tool="$repo/native/.build/final-focus-drive"
capture="$here/capture.sh"
owner="jackin❯ desktop"
inactive_app=${CAPTURE_INACTIVE_APP:-$(osascript -e \
  'tell application "System Events" to get name of first application process whose frontmost is true')}
unset CAPTURE_INACTIVE_APP
test -n "$inactive_app" && test "$inactive_app" != JackinDesktop || {
  echo "front a non-jackin❯ application before capturing inactive states" >&2
  exit 2
}

source_paths=(
  Cargo.lock
  Cargo.toml
  crates/jackin-usage
  crates/jackin-usage-ffi
  crates/jackin-xtask
  mise.toml
  native/Generated
  native/Package.swift
  native/Scripts
  native/Sources
  native/Support
  native/Tests
  native/Tools
  native/UITests
  native/project.yml
  rust-toolchain.toml
)
require_clean_sources() {
  source_status=$(git -C "$repo" status --porcelain -- "${source_paths[@]}")
  test -z "$source_status" || {
    echo "refusing to label captures as branch-head evidence with dirty desktop sources:" >&2
    printf '%s\n' "$source_status" >&2
    exit 2
  }
}
require_clean_sources

canonical_app="$repo/native/dist/JackinDesktop.app"
test "$app" = "$canonical_app" || {
  echo "final evidence requires the canonical branch-head app: $canonical_app" >&2
  exit 2
}
mise -C "$repo" run desktop-build
mise -C "$repo" run desktop-verify
require_clean_sources
test -d "$app" || {
  echo "app bundle not found: $app" >&2
  exit 2
}
mkdir -p "$repo/native/.build" "$output"
swiftc -O "$here/window-id.swift" -o "$window_tool"
swiftc -O "$here/notification-drive.swift" -o "$notification_tool"
swiftc -O "$here/focus-drive.swift" -o "$focus_tool"

cleanup() {
  status=$?
  trap - EXIT INT TERM HUP
  while IFS= read -r app_pid; do
    kill -TERM "$app_pid" 2>/dev/null || true
  done < <(pgrep -f "^$app/Contents/MacOS/" || true)
  exit "$status"
}
trap cleanup EXIT INT TERM HUP

capture_with_relaunch() {
  local attempt=1
  while ! "$@"; do
    if [[ "$attempt" -ge 3 ]]; then
      echo "capture retries exhausted after $attempt launches" >&2
      return 1
    fi
    attempt=$((attempt + 1))
    sleep 1
  done
}

usage() {
  local file=$1 fixture=$2 appearance=$3 size=$4 state=${5:-active} collapsed=${6:-no}
  local -a environment=(
    "WINDOW_ID_TOOL=$window_tool"
    "NOTIFICATION_DRIVE_TOOL=$notification_tool"
    "FOCUS_DRIVE_TOOL=$focus_tool"
    "WINDOW_LAYER_MODE=all"
  )
  if [[ "$state" == inactive ]]; then
    environment+=("CAPTURE_INACTIVE_APP=$inactive_app")
  fi
  if [[ "$collapsed" == yes ]]; then
    environment+=(
      "CAPTURE_TOOLBAR_BUTTON_DESCRIPTION=Hide Sidebar"
      "CAPTURE_TOOLBAR_BUTTON_POST_DESCRIPTION=Show Sidebar"
    )
  fi
  capture_with_relaunch env "${environment[@]}" \
    "$capture" "$app" "$owner" "$output/$file" "jackin❯ desktop" \
    --fixture "$fixture" --ui-test --open-usage --window-size "$size" \
    --appearance "$appearance"
}

popover() {
  local file=$1 fixture=$2 appearance=$3
  capture_with_relaunch env \
    WINDOW_ID_TOOL="$window_tool" NOTIFICATION_DRIVE_TOOL="$notification_tool" \
    WINDOW_LAYER_MODE=all \
    "$capture" "$app" "$owner" "$output/$file" "" \
    --fixture "$fixture" --ui-test --open-popover --appearance "$appearance"
}

usage usage-dark-active-F02.png F02-catalog-normal dark 920x620
usage usage-dark-inactive-F02.png F02-catalog-normal dark 920x620 inactive
usage usage-dark-collapsed-F02.png F02-catalog-normal dark 800x520 active yes
usage usage-dark-empty-F00.png F00-no-providers dark 800x520
usage usage-dark-single-F01.png F01-single-normal dark 1000x680
usage usage-dark-multiaccount-F03.png F03-multi-account dark 1000x680
usage usage-dark-nearly-exhausted-F04.png F04-nearly-exhausted dark 1000x680
usage usage-dark-exhausted-F05.png F05-exhausted dark 1000x680
usage usage-dark-stale-F06.png F06-stale-last-good dark 1000x680
usage usage-dark-refreshing-F07.png F07-refreshing-last-good dark 1000x680
usage usage-dark-partial-F08.png F08-partial-timeout dark 1000x680
usage usage-dark-permission-F09.png F09-permission-denied dark 1000x680
usage usage-dark-offline-F10.png F10-offline-cached dark 1000x680
usage usage-dark-long-F11.png F11-long-labels dark 800x520
usage usage-dark-min-F12.png F12-layout-envelope dark 800x520
usage usage-dark-expanded-F12.png F12-layout-envelope dark 1200x760
usage usage-dark-loading-F13.png F13-initial-loading dark 800x520
usage usage-dark-error-F14.png F14-global-bridge-error dark 800x520

popover popover-dark-active-F02.png F02-catalog-normal dark
popover popover-dark-empty-F00.png F00-no-providers dark
popover popover-dark-single-F01.png F01-single-normal dark
popover popover-dark-multiaccount-F03.png F03-multi-account dark
popover popover-dark-nearly-exhausted-F04.png F04-nearly-exhausted dark
popover popover-dark-exhausted-F05.png F05-exhausted dark
popover popover-dark-stale-F06.png F06-stale-last-good dark
popover popover-dark-refreshing-F07.png F07-refreshing-last-good dark
popover popover-dark-partial-F08.png F08-partial-timeout dark
popover popover-dark-permission-F09.png F09-permission-denied dark
popover popover-dark-offline-F10.png F10-offline-cached dark
popover popover-dark-long-F11.png F11-long-labels dark
popover popover-dark-maximum-F12.png F12-layout-envelope dark
popover popover-dark-loading-F13.png F13-initial-loading dark
popover popover-dark-error-F14.png F14-global-bridge-error dark

echo "Final captures regenerated: $output"
