#!/bin/bash
# Run native UI tests and reject missing, corrupt, zero-test, or partial results.
set -euo pipefail

here=$(cd "$(dirname "$0")" && pwd -P)
repo=$(cd "$here/../.." && pwd -P)
stamp=$(date -u '+%Y%m%dT%H%M%SZ')
result_root="$repo/native/DerivedData/UITests-$stamp-$$"
report_root="$repo/native/.build/test-results/ui-$stamp-$$"
expected=$(rg --no-filename '^[[:space:]]+func test' "$repo/native/UITests"/*.swift | wc -l | tr -d ' ')
lock="$repo/native/.build/ui-test.lock"
runner="$repo/native/DerivedData/Build/Products/Debug/JackinDesktopUITests-Runner.app/Contents/MacOS/JackinDesktopUITests-Runner"
app_pattern="^$repo/native/(dist|DerivedData)/.*JackinDesktop.app/Contents/MacOS/JackinDesktop( |$)"
child_pid=""

terminate_repo_apps() {
  while IFS= read -r app_pid; do
    kill -TERM "$app_pid" 2>/dev/null || true
  done < <(pgrep -f "$app_pattern" || true)
  i=0
  while pgrep -f "$app_pattern" >/dev/null 2>&1 && [[ "$i" -lt 20 ]]; do
    sleep 0.25
    i=$((i + 1))
  done
  while IFS= read -r app_pid; do
    kill -KILL "$app_pid" 2>/dev/null || true
  done < <(pgrep -f "$app_pattern" || true)
  ! pgrep -f "$app_pattern" >/dev/null 2>&1
}

mkdir -p "$(dirname "$lock")"
if ! mkdir "$lock"; then
  echo "Another jackin❯ desktop UI test run owns $lock" >&2
  exit 1
fi

cleanup() {
  status=$?
  trap - EXIT INT TERM HUP
  if [[ -n "$child_pid" ]] && kill -0 "$child_pid" 2>/dev/null; then
    kill -TERM "$child_pid" 2>/dev/null || true
    wait "$child_pid" 2>/dev/null || true
  fi
  while IFS= read -r runner_pid; do
    kill -TERM "$runner_pid" 2>/dev/null || true
  done < <(pgrep -f "^${runner}$" || true)
  terminate_repo_apps || status=1
  rmdir "$lock" 2>/dev/null || true
  exit "$status"
}
trap cleanup EXIT INT TERM HUP

test "$expected" -gt 0
terminate_repo_apps
mkdir -p "$result_root"
mkdir -p "$report_root"
test_names=()
while IFS= read -r test_name; do
  test_names+=("$test_name")
done < <(
  rg --no-filename '^[[:space:]]+func test' "$repo/native/UITests"/*.swift \
    | sed -E 's/^[[:space:]]+func (test[^ (]+).*/\1/' \
    | sort
)
test "${#test_names[@]}" -eq "$expected"

passed=0
for test_name in "${test_names[@]}"; do
  result="$result_root/$test_name.xcresult"
  raw_log="$result_root/$test_name.xcodebuild.log"
  report="$report_root/$test_name.xml"
  terminate_repo_apps
  xcodebuild test \
    -project "$repo/native/JackinDesktop.xcodeproj" \
    -scheme JackinDesktop \
    -destination 'platform=macOS' \
    -parallel-testing-enabled NO \
    -only-testing:"JackinDesktopUITests/JackinDesktopUITests/$test_name" \
    -derivedDataPath "$repo/native/DerivedData" \
    -resultBundlePath "$result" >"$raw_log" 2>&1 &
  child_pid=$!
  if wait "$child_pid"; then
    xcode_status=0
  else
    xcode_status=$?
  fi
  child_pid=""

  xcbeautify \
    --quiet \
    --is-ci \
    --report junit \
    --report-path "$report_root" \
    --junit-report-filename "$test_name.xml" <"$raw_log"
  test -s "$report" || {
    echo "UI test JUnit report is missing or empty: $report" >&2
    exit 1
  }
  test "$xcode_status" -eq 0 || {
    echo "xcodebuild failed for $test_name: status $xcode_status" >&2
    exit "$xcode_status"
  }

  test -f "$result/Info.plist" || {
    echo "UI test result bundle is missing or corrupt: $result" >&2
    exit 1
  }

  summary=$(xcrun xcresulttool get test-results summary --path "$result")
  test_tree=$(xcrun xcresulttool get test-results tests --path "$result")
  total=$(printf '%s' "$summary" | plutil -extract totalTestCount raw -)
  failed=$(printf '%s' "$summary" | plutil -extract failedTests raw -)
  current_passed=$(printf '%s' "$summary" | plutil -extract passedTests raw -)
  runtime_warnings=$(
    printf '%s' "$test_tree" \
      | awk '/"nodeType" : "Runtime Warning"/ { count++ } END { print count + 0 }'
  )

  test "$total" -eq 1 || {
    echo "UI test count mismatch for $test_name: expected 1, executed $total" >&2
    exit 1
  }
  test "$failed" -eq 0 || {
    echo "UI test failed: $test_name" >&2
    exit 1
  }
  test "$current_passed" -eq 1 || {
    echo "UI test did not pass: $test_name" >&2
    exit 1
  }
  test "$runtime_warnings" -eq 0 || {
    echo "UI test emitted runtime warnings: $test_name ($runtime_warnings)" >&2
    printf '%s' "$test_tree" \
      | awk '/"nodeType" : "Runtime Warning"/ { print previous } { previous = $0 }' >&2
    exit 1
  }
  passed=$((passed + current_passed))
done

test "$passed" -eq "$expected"
echo "UI tests: $passed/$expected passed"
echo "Results: $result_root"
echo "JUnit: $report_root"
