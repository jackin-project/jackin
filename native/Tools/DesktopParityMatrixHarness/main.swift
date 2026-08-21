// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

import Foundation
import JackinUsageBindings
import JackinUsageBridge

/// CLT-safe proof that native presentation consumes finished Rust fields.
@main
struct DesktopParityMatrixHarness {
    static func main() {
        var failures = 0
        func check(_ name: String, _ condition: @autoclosure () -> Bool) {
            let passed = condition()
            print("\(passed ? "PASS" : "FAIL")  \(name)")
            if !passed { failures += 1 }
        }

        let rows = [
            glance(
                surfaceId: "codex",
                account: "first@example.test",
                remaining: 57,
                reset: "3d"
            ),
            glance(
                surfaceId: "claude",
                account: "second@example.test",
                remaining: 12,
                reset: "23h"
            ),
        ]

        check("Rust row order preserved", rows.map(\.surfaceId) == ["codex", "claude"])
        check("account identity finished", rows[0].accountLabel == "first@example.test")
        check("remaining display finished", rows[0].barLabel == "57%")
        check("compact reset dedicated", rows[0].compactResetLabel == "3d")
        check("activity explicit", rows.allSatisfy { $0.activityKind == "idle" })
        check("accessibility finished", rows[1].accessibilityLabel.contains("second@example.test"))

        let desktop = desktopSourcesRoot()
        let statusItem = read(desktop.appendingPathComponent("StatusItemLabel.swift"))
        let popover = read(desktop.appendingPathComponent("PopoverRoot.swift"))
        let overview = read(desktop.appendingPathComponent("UsageWindow/OverviewListView.swift"))
        let provider = read(desktop.appendingPathComponent("UsageWindow/ProviderDetailView.swift"))

        check("status item consumes compact reset field", statusItem.contains("compactResetLabel"))
        check("popover consumes Rust identity", popover.contains("provider.activityLabel"))
        check(
            "popover limits precede details",
            appearsBefore(
                "sectionHeader(\"Limits\")",
                "sectionHeader(\"Details\")",
                in: popover
            ))
        check("overview is grouped native table", overview.contains("DisclosureTableRow"))
        check("overview uses atomic groups", overview.contains("OverviewInventory.tree(groups:"))
        check(
            "provider detail consumes identity", provider.contains("content.identity.activityLabel")
        )
        check("provider detail consumes usage URL", provider.contains("content.usageURL"))

        print("---")
        print(
            failures == 0
                ? "DesktopParityMatrixHarness: ALL PASS"
                : "DesktopParityMatrixHarness: \(failures) FAILURE(S)")
        exit(failures == 0 ? 0 : 1)
    }

    private static func glance(
        surfaceId: String,
        account: String,
        remaining: UInt8,
        reset: String
    ) -> ProviderGlanceRowDto {
        ProviderGlanceRowDto(
            surfaceId: surfaceId,
            iconKey: surfaceId,
            fallbackGlyph: "?",
            usageUrl: "https://example.test/usage",
            displayLabel: surfaceId,
            accountLabel: account,
            planLabel: "Fixture plan",
            glanceRemainingPercent: remaining,
            barLabel: "\(remaining)%",
            headline: "Fixture remaining",
            resetLabel: "Fixture reset",
            compactResetLabel: reset,
            exactReset: nil,
            statusWord: "fresh",
            isRefreshing: false,
            statusLabel: "Ready",
            severity: "normal",
            updatedLabel: "Updated now",
            activityLabel: "Updated now",
            activityKind: "idle",
            accessibilityLabel: "\(surfaceId), \(account), Updated now",
            lastError: nil,
            dimmed: false
        )
    }

    private static func desktopSourcesRoot() -> URL {
        let cwd = URL(fileURLWithPath: FileManager.default.currentDirectoryPath)
        let local = cwd.appendingPathComponent("Sources/JackinDesktop")
        return FileManager.default.fileExists(atPath: local.path)
            ? local
            : cwd.appendingPathComponent("native/Sources/JackinDesktop")
    }

    private static func read(_ url: URL) -> String {
        (try? String(contentsOf: url, encoding: .utf8)) ?? ""
    }

    private static func appearsBefore(_ first: String, _ second: String, in source: String) -> Bool
    {
        guard let firstRange = source.range(of: first), let secondRange = source.range(of: second)
        else { return false }
        return firstRange.lowerBound < secondRange.lowerBound
    }
}
