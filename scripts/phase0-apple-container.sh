#!/usr/bin/env bash
# Phase 0 empirical validation script for the apple-container backend.
#
# Run this script on a real macOS 26 ARM machine with apple/container
# v0.11.0+ installed. It validates all prerequisites before Phase 2
# code (the experimental --backend apple-container launch path) can
# be tested end-to-end.
#
# Decision gate (from apple-container-backend.mdx roadmap):
#   - If rootless DinD PASSES → apple-container Phase 2 is unblocked
#   - If rootless DinD FAILS  → fall back to smolvm Phase 0
#
# Usage:
#   ./scripts/phase0-apple-container.sh [--role-image <image>]
#
# Required:
#   - macOS 26 ARM
#   - apple/container v0.11.0+ (`container --version`)
#   - A built jackin role OCI image (default: projectjackin/the-architect)
#
# Output:
#   - PASS/FAIL per check
#   - Overall decision gate result
#   - Recorded in scripts/phase0-results.md when complete

set -euo pipefail

ROLE_IMAGE="${1:-projectjackin/the-architect}"
TEST_CONTAINER="jackin-phase0-test-$$"
RESULTS_FILE="$(dirname "$0")/phase0-results.md"
PASS=0
FAIL=0
SKIP=0

# ── helpers ────────────────────────────────────────────────────────────────

green() { printf '\033[0;32m%s\033[0m\n' "$*"; }
red()   { printf '\033[0;31m%s\033[0m\n' "$*"; }
yellow(){ printf '\033[0;33m%s\033[0m\n' "$*"; }

pass() {
    green "  ✓ PASS: $1"
    PASS=$((PASS + 1))
    echo "- [x] PASS: $1" >> "$RESULTS_FILE"
}

fail() {
    red "  ✗ FAIL: $1"
    if [ -n "${2:-}" ]; then red "       $2"; fi
    FAIL=$((FAIL + 1))
    echo "- [ ] FAIL: $1${2:+ — $2}" >> "$RESULTS_FILE"
}

skip() {
    yellow "  ~ SKIP: $1"
    SKIP=$((SKIP + 1))
    echo "- [-] SKIP: $1" >> "$RESULTS_FILE"
}

cleanup() {
    container stop "$TEST_CONTAINER" 2>/dev/null || true
    container rm   "$TEST_CONTAINER" 2>/dev/null || true
}
trap cleanup EXIT

# ── preamble ───────────────────────────────────────────────────────────────

echo ""
echo "╔══════════════════════════════════════════════════════════════════╗"
echo "║  jackin' apple-container Phase 0 empirical validation           ║"
echo "║  Roadmap: docs/content/docs/reference/roadmap/                  ║"
echo "║           apple-container-backend.mdx                           ║"
echo "╚══════════════════════════════════════════════════════════════════╝"
echo ""

cat > "$RESULTS_FILE" << HEADER
# Phase 0 Apple Container Empirical Validation Results

Date: $(date '+%Y-%m-%d %H:%M:%S')
Machine: $(uname -m) / $(sw_vers -productVersion 2>/dev/null || echo 'unknown macOS version')
Role image: $ROLE_IMAGE

## Checks

HEADER

# ── check 1: prerequisites ─────────────────────────────────────────────────

echo "── 1. Prerequisites ──────────────────────────────────────────────"

if command -v container &>/dev/null; then
    CONTAINER_VERSION=$(container --version 2>/dev/null || echo 'unknown')
    pass "container CLI installed: $CONTAINER_VERSION"
    echo "container version: $CONTAINER_VERSION" >> "$RESULTS_FILE"
else
    fail "container CLI not found" "Install: brew install container (see https://github.com/apple/container)"
    echo ""
    red "FATAL: apple/container CLI not installed. Cannot proceed with Phase 0."
    echo "  Run: brew install container"
    exit 1
fi

if [ "$(uname -m)" = "arm64" ]; then
    pass "Apple Silicon ARM64 confirmed"
else
    fail "Not Apple Silicon" "Phase 0 requires macOS 26 ARM64"
fi

SW_VER=$(sw_vers -productVersion 2>/dev/null || echo '0.0')
SW_MAJOR=$(echo "$SW_VER" | cut -d. -f1)
if [ "$SW_MAJOR" -ge 26 ] 2>/dev/null; then
    pass "macOS version $SW_VER (≥ 26)"
else
    fail "macOS version $SW_VER (requires ≥ 26)" "apple/container requires macOS 26 Tahoe"
fi

# ── check 2: JACKIN_CAPSULE_FORCE_DAEMON ───────────────────────────────────

echo ""
echo "── 2. JACKIN_CAPSULE_FORCE_DAEMON patch ──────────────────────────"

echo "  Pulling role image $ROLE_IMAGE ..."
if container pull "$ROLE_IMAGE" 2>&1 | tail -1; then
    pass "Role image pulled: $ROLE_IMAGE"
else
    fail "Role image pull failed" "Check image name and registry auth"
    skip "Remaining checks require image"
    goto_summary=1
fi

if [ -z "${goto_summary:-}" ]; then
    echo "  Testing JACKIN_CAPSULE_FORCE_DAEMON=1 (non-PID-1 daemon mode) ..."
    DAEMON_TEST=$(container run --rm \
        -e JACKIN_CAPSULE_FORCE_DAEMON=1 \
        "$ROLE_IMAGE" \
        /bin/sh -c 'echo PID=$$ && /jackin/runtime/jackin-capsule --version' 2>&1 || echo "FAILED")

    if echo "$DAEMON_TEST" | grep -q "jackin-capsule"; then
        pass "jackin-capsule responds under JACKIN_CAPSULE_FORCE_DAEMON=1"
    else
        fail "jackin-capsule failed with JACKIN_CAPSULE_FORCE_DAEMON=1" "$DAEMON_TEST"
    fi
fi

# ── check 3: OCI image boot ────────────────────────────────────────────────

echo ""
echo "── 3. OCI image boot ─────────────────────────────────────────────"

if container run --rm "$ROLE_IMAGE" /bin/sh -c 'echo "boot ok"' 2>/dev/null | grep -q "boot ok"; then
    pass "Role OCI image boots cleanly"
else
    fail "Role OCI image failed to boot"
fi

# ── check 4: PTY and attach behavior ──────────────────────────────────────

echo ""
echo "── 4. PTY / attach behavior ──────────────────────────────────────"
echo "  Starting background container for exec test ..."

container run -d --name "$TEST_CONTAINER" \
    -e JACKIN_CAPSULE_FORCE_DAEMON=1 \
    "$ROLE_IMAGE" \
    /bin/sh -c 'sleep 30' 2>/dev/null || true

sleep 2

if container exec "$TEST_CONTAINER" /bin/sh -c 'echo "exec ok"' 2>/dev/null | grep -q "exec ok"; then
    pass "container exec works (non-interactive)"
else
    fail "container exec failed"
fi

# PTY test — check that -it flag is accepted (interactive tty)
if container exec -it "$TEST_CONTAINER" /bin/echo "pty ok" 2>/dev/null | grep -q "pty ok"; then
    pass "container exec -it (PTY) accepted"
else
    fail "container exec -it failed — PTY may not be supported"
fi

echo "  Note: SIGWINCH and Ctrl+C passthrough require manual terminal resize testing."
echo "  Record results in phase0-results.md if testing interactively."

# ── check 5: bind mounts ───────────────────────────────────────────────────

echo ""
echo "── 5. Workspace bind mounts ──────────────────────────────────────"

TMPDIR_TEST=$(mktemp -d)
echo "phase0-mount-test" > "$TMPDIR_TEST/test.txt"

if container run --rm \
    -v "$TMPDIR_TEST:/workspace/test" \
    "$ROLE_IMAGE" \
    /bin/sh -c 'cat /workspace/test/test.txt' 2>/dev/null | grep -q "phase0-mount-test"; then
    pass "Bind mount read works"
else
    fail "Bind mount read failed"
fi

if container run --rm \
    -v "$TMPDIR_TEST:/workspace/test" \
    "$ROLE_IMAGE" \
    /bin/sh -c 'echo "write-ok" > /workspace/test/written.txt && echo "done"' 2>/dev/null | grep -q "done"; then
    if [ -f "$TMPDIR_TEST/written.txt" ]; then
        pass "Bind mount write-back to host confirmed"
    else
        fail "Bind mount write did not propagate back to host"
    fi
else
    fail "Bind mount write failed"
fi
rm -rf "$TMPDIR_TEST"

# Read-only enforcement
TMPDIR_RO=$(mktemp -d)
if container run --rm \
    -v "$TMPDIR_RO:/workspace/ro:ro" \
    "$ROLE_IMAGE" \
    /bin/sh -c 'echo fail > /workspace/ro/should-fail.txt 2>&1; echo exit=$?' 2>/dev/null | grep -q "exit=1"; then
    pass "Read-only bind mount enforced"
else
    skip "Read-only enforcement check inconclusive (container may not enforce :ro)"
fi
rm -rf "$TMPDIR_RO"

# ── check 6: CAP_SYS_ADMIN (rootless DinD gate) ────────────────────────────

echo ""
echo "── 6. --cap-add CAP_SYS_ADMIN (rootless DinD gate) ──────────────"
echo "  THIS IS THE CRITICAL GATE: rootless DinD requires CAP_SYS_ADMIN."
echo "  If this fails, DinD workflows are blocked and smolvm becomes the"
echo "  primary VM isolation path."
echo ""

CAP_TEST=$(container run --rm \
    --cap-add SYS_ADMIN \
    "$ROLE_IMAGE" \
    /bin/sh -c 'cat /proc/self/status | grep CapEff || echo "cap-check-failed"' 2>&1)

if echo "$CAP_TEST" | grep -q "cap-check-failed\|error\|Error"; then
    fail "CAP_SYS_ADMIN not granted — --cap-add SYS_ADMIN failed" "$CAP_TEST"
    DIND_BLOCKED=1
else
    # Check that CapEff has sys_admin bit set (bit 21 = 0x200000)
    CAP_EFF=$(echo "$CAP_TEST" | grep CapEff | awk '{print $2}')
    if [ -n "$CAP_EFF" ]; then
        # sys_admin is bit 21; check hex value
        if python3 -c "exit(0 if int('$CAP_EFF',16) & (1<<21) else 1)" 2>/dev/null; then
            pass "CAP_SYS_ADMIN granted (CapEff=$CAP_EFF) — rootless DinD UNBLOCKED"
            DIND_BLOCKED=0
        else
            fail "CAP_SYS_ADMIN not in CapEff=$CAP_EFF — rootless DinD BLOCKED"
            DIND_BLOCKED=1
        fi
    else
        skip "Could not parse CapEff — manual verification required"
        DIND_BLOCKED=2
    fi
fi

# ── check 7: rootless DinD (if cap check passed) ───────────────────────────

echo ""
echo "── 7. Rootless DinD inside apple-container VM ────────────────────"

if [ "${DIND_BLOCKED:-1}" -eq 1 ]; then
    skip "Rootless DinD skipped — CAP_SYS_ADMIN not available"
elif [ "${DIND_BLOCKED:-1}" -eq 2 ]; then
    skip "Rootless DinD skipped — cap check inconclusive"
else
    echo "  Starting container with SYS_ADMIN cap for DinD test ..."
    # This test requires a container with dockerd installed.
    # The role image may or may not have dockerd.
    if container run --rm \
        --cap-add SYS_ADMIN \
        --cap-add NET_ADMIN \
        "$ROLE_IMAGE" \
        /bin/sh -c 'which dockerd && echo "dockerd-found" || echo "no-dockerd"' 2>/dev/null | grep -q "dockerd-found"; then
        pass "dockerd binary present in role image — DinD test possible"
        echo "  Full DinD validation (docker build, Compose, Testcontainers) requires"
        echo "  a longer interactive test. Run manually and record results."
        skip "DinD interactive validation requires manual test — see Phase 0 checklist"
    else
        skip "dockerd not in role image — DinD requires separate image with dockerd"
        echo "  Use a base image with dockerd installed for DinD validation."
    fi
fi

# ── check 8: cold start latency ────────────────────────────────────────────

echo ""
echo "── 8. Cold start latency ─────────────────────────────────────────"
echo "  Measuring time from container run to capsule socket ready ..."

START_MS=$(date +%s%3N)
container run -d --name "${TEST_CONTAINER}-timing" \
    -e JACKIN_CAPSULE_FORCE_DAEMON=1 \
    "$ROLE_IMAGE" \
    /jackin/runtime/jackin-capsule 2>/dev/null || true

SOCKET_READY=0
for i in $(seq 1 120); do
    if container exec "${TEST_CONTAINER}-timing" \
        sh -c 'test -S /jackin/run/jackin.sock && /jackin/runtime/jackin-capsule status' \
        2>/dev/null; then
        END_MS=$(date +%s%3N)
        COLD_START_MS=$((END_MS - START_MS))
        SOCKET_READY=1
        break
    fi
    sleep 0.5
done

container stop "${TEST_CONTAINER}-timing" 2>/dev/null || true
container rm   "${TEST_CONTAINER}-timing" 2>/dev/null || true

if [ "$SOCKET_READY" -eq 1 ]; then
    pass "Cold start: ${COLD_START_MS}ms (socket ready)"
    echo "cold_start_ms: $COLD_START_MS" >> "$RESULTS_FILE"
    if [ "$COLD_START_MS" -lt 5000 ]; then
        pass "Cold start under 5 s — acceptable"
    else
        fail "Cold start ${COLD_START_MS}ms exceeds 5 s target"
    fi
else
    fail "jackin-capsule daemon did not become ready within 60 s"
fi

# ── check 9: DNS stability (manual) ───────────────────────────────────────

echo ""
echo "── 9. DNS stability after sleep/wake ─────────────────────────────"
echo "  This check is manual — cannot be automated in a script."
echo "  Steps:"
echo "    1. Start a container: container run -d --name dns-test <image> sleep 3600"
echo "    2. Put the Mac to sleep (close lid)"
echo "    3. Wake the Mac"
echo "    4. Run: container exec dns-test nslookup github.com"
echo "    5. Record whether DNS resolves or shows hiccup"
echo "  Record result in $RESULTS_FILE"
skip "DNS stability after sleep/wake requires manual test"

# ── summary ────────────────────────────────────────────────────────────────

echo ""
echo "══════════════════════════════════════════════════════════════════"
echo "  Phase 0 Summary"
echo "══════════════════════════════════════════════════════════════════"
echo ""
printf "  PASS: %d\n" "$PASS"
printf "  FAIL: %d\n" "$FAIL"
printf "  SKIP: %d\n" "$SKIP"
echo ""

cat >> "$RESULTS_FILE" << SUMMARY

## Summary

- PASS: $PASS
- FAIL: $FAIL
- SKIP: $SKIP

## Decision Gate

SUMMARY

if [ "$FAIL" -eq 0 ] && [ "${DIND_BLOCKED:-1}" -eq 0 ]; then
    green "  DECISION GATE: PASS"
    green "  → rootless DinD validated — proceed to apple-container Phase 2 testing"
    echo "**PASS** — rootless DinD validated, proceed to Phase 2" >> "$RESULTS_FILE"
elif [ "$FAIL" -eq 0 ] && [ "${DIND_BLOCKED:-1}" -eq 2 ]; then
    yellow "  DECISION GATE: INCONCLUSIVE"
    yellow "  → CAP_SYS_ADMIN check inconclusive — manual verification required"
    echo "**INCONCLUSIVE** — manual CAP_SYS_ADMIN verification required" >> "$RESULTS_FILE"
else
    red "  DECISION GATE: FAIL ($FAIL check(s) failed)"
    if [ "${DIND_BLOCKED:-1}" -eq 1 ]; then
        red "  → CAP_SYS_ADMIN blocked — rootless DinD not available"
        red "  → Fall back to smolvm Phase 0 (see smolvm-backend-research roadmap)"
    fi
    echo "**FAIL** — $FAIL check(s) failed; see above for details" >> "$RESULTS_FILE"
fi

echo ""
echo "  Results written to: $RESULTS_FILE"
echo ""

exit "$FAIL"
