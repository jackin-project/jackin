// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

/// Pure production-path chip harness (no XCTest / no network).
///
/// Asserts CodexBar/OpenUsage status-item parity:
/// - multi-provider chips from Rust remainings
/// - default percent lines = remaining %
/// - compact driving digit matches min remaining (no used%/remaining% mix)
/// - dual-bucket stack + a11y speak both lines
/// - used style flips display lines only
///
/// Run after XCFramework exists:
///   cd native && swift run -c release StatusItemChipHarness

import Foundation
import JackinUsageBridge

@main
struct StatusItemChipHarness {
    static func main() {
        var failures = 0

        func check(_ name: String, _ ok: @autoclosure () -> Bool, _ detail: String = "") {
            if ok() {
                print("PASS  \(name)")
            } else {
                failures += 1
                let suffix = detail.isEmpty ? "" : " — \(detail)"
                print("FAIL  \(name)\(suffix)")
            }
        }

        // --- Pure token helpers ---
        check("remaining token default", statusItemPercentToken(remainingPercent: 37) == "37%")
        check(
            "used token style",
            statusItemPercentToken(remainingPercent: 37, percentStyle: "used") == "63%"
        )
        check(
            "glyph from remaining compact",
            statusItemGlyph(compactLabel: "Cl 37%", surfaceId: "claude") == "Cl"
        )
        check(
            "dual remaining lines",
            statusItemPercentLines(remainings: [100, 79], maxLines: 2) == ["100%", "79%"]
        )
        check(
            "dual used lines",
            statusItemPercentLines(
                remainings: [100, 79],
                maxLines: 2,
                percentStyle: "used"
            ) == ["0%", "21%"]
        )

        // Production-shaped multi-provider strip (Claude dual + Codex + hidden empty/disabled).
        let surfaces = [
            StatusItemSurfaceSnapshot(
                surfaceId: "claude",
                label: "Claude",
                enabled: true,
                statusBarLabel: "Session 100% · Weekly 79%",
                status: "fresh",
                compactLabel: "Cl 79%", // driving = min remaining
                remainings: [100, 79],
                severities: ["ok", "ok"]
            ),
            StatusItemSurfaceSnapshot(
                surfaceId: "codex",
                label: "Codex",
                enabled: true,
                statusBarLabel: "Session 84%",
                status: "fresh",
                compactLabel: "Cx 84%",
                remainings: [84],
                severities: ["warn"]
            ),
            StatusItemSurfaceSnapshot(
                surfaceId: "amp",
                label: "Amp",
                enabled: true,
                statusBarLabel: "",
                status: "unavailable",
                compactLabel: "",
                remainings: [],
                severities: []
            ),
            StatusItemSurfaceSnapshot(
                surfaceId: "grok",
                label: "Grok Build",
                enabled: false,
                statusBarLabel: "unused",
                status: "fresh",
                compactLabel: "Gr 50%",
                remainings: [50],
                severities: ["ok"]
            ),
            StatusItemSurfaceSnapshot(
                surfaceId: "zai",
                label: "GLM / Z.AI",
                enabled: true,
                statusBarLabel: "Weekly 12%",
                status: "fresh",
                compactLabel: "ZA 12%",
                remainings: [12],
                severities: ["danger"]
            ),
            StatusItemSurfaceSnapshot(
                surfaceId: "kimi",
                label: "Kimi",
                enabled: true,
                statusBarLabel: "Session 40%",
                status: "fresh",
                compactLabel: "Ki 40%",
                remainings: [40],
                severities: ["ok"]
            ),
            StatusItemSurfaceSnapshot(
                surfaceId: "minimax",
                label: "MiniMax",
                enabled: true,
                statusBarLabel: "Session 55%",
                status: "fresh",
                compactLabel: "MM 55%",
                remainings: [55],
                severities: ["ok"]
            ),
            StatusItemSurfaceSnapshot(
                surfaceId: "opencode",
                label: "OpenCode",
                enabled: true,
                statusBarLabel: "Session 90%",
                status: "fresh",
                compactLabel: "OC 90%",
                remainings: [90],
                severities: ["ok"]
            ),
        ]

        let chips = buildStatusItemChips(
            surfaces: surfaces,
            maxCount: 8,
            preferWorstFirst: false,
            percentStyle: "left"
        )
        let ids = chips.map(\.surfaceId)
        check(
            "hides empty/disabled",
            ids == ["claude", "codex", "zai", "kimi", "minimax", "opencode"],
            "ids=\(ids)"
        )
        check(
            "claude dual remaining stack",
            chips[0].percentLines == ["100%", "79%"] && chips[0].remainingPerLine == [100, 79]
        )
        check("claude driving remaining", chips[0].remainingPercent == 79)
        check(
            "compact matches driving remaining",
            chips[0].compactLabel.contains("79%") && chips[0].percentLines.contains("79%"),
            "compact=\(chips[0].compactLabel) lines=\(chips[0].percentLines)"
        )
        check("codex single remaining", chips[1].percentLines == ["84%"])

        let a11y = statusItemAccessibilityLabel(chips: Array(chips.prefix(2)))
        check(
            "a11y dual stack",
            a11y == "jackin Desktop Cl 100% and 79%, Cx 84%",
            "a11y=\(a11y)"
        )

        let worst = buildStatusItemChips(
            surfaces: surfaces,
            maxCount: 3,
            preferWorstFirst: true,
            percentStyle: "left"
        )
        check(
            "worst-first remaining order",
            worst.map(\.surfaceId) == ["zai", "kimi", "minimax"]
                || worst.map(\.surfaceId).prefix(1).elementsEqual(["zai"]),
            "worst=\(worst.map(\.surfaceId))"
        )
        // zai 12 is lowest remaining among enabled with data.
        check("worst is zai", worst.first?.surfaceId == "zai")

        let used = buildStatusItemChips(
            surfaces: surfaces,
            maxCount: 2,
            preferWorstFirst: false,
            percentStyle: "used"
        )
        check("used style claude lines", used[0].percentLines == ["0%", "21%"])
        check("used style keeps raw remainings", used[0].remainingPerLine == [100, 79])
        check("used style codex line", used[1].percentLines == ["16%"])

        // Cap
        let capped = buildStatusItemChips(
            surfaces: surfaces,
            maxCount: 2,
            preferWorstFirst: false,
            percentStyle: "left"
        )
        check("cap 2", capped.count == 2)

        print("---")
        if failures == 0 {
            print("StatusItemChipHarness: ALL PASS")
            exit(0)
        } else {
            print("StatusItemChipHarness: \(failures) FAILURE(S)")
            exit(1)
        }
    }
}
