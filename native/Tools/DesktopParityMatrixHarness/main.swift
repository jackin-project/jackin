// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

/// OpenUsage / CodexBar **limits-only** parity matrix for jackin❯ Desktop.
///
/// Drives shipped pure presentation builders (no XCTest, no AppKit window) to
/// prove multi-provider strip + dual-bucket + depleted + full catalog display
/// paths stay working. Product ban: no token unit price / historical trend UI.
///
/// Run (XCFramework required for JackinUsageBridge):
///   cd native && swift run -c release DesktopParityMatrixHarness
/// Or: mise run desktop-test / cargo xtask desktop test

import Foundation
import JackinUsageBridge

@main
struct DesktopParityMatrixHarness {
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

        print("=== OpenUsage/CodexBar limits-only parity matrix ===")
        print("In scope: remaining %, dual windows, resets, multi-provider icons")
        print("Out of scope: token unit prices, usage/spend trends (AGENTS hard rule)")
        print("")

        // --- Catalog ---
        check("frozen catalog has 8 surfaces", frozenHostSurfaceIds.count == 8)
        check(
            "catalog order matches HostSurfaceId::ALL",
            frozenHostSurfaceIds == [
                "claude", "codex", "amp", "grok", "zai", "kimi", "minimax", "opencode",
            ]
        )
        check(
            "every frozen surface has SF Symbol (displayable icon)",
            allFrozenHostSurfacesHaveSystemImages()
        )
        for id in frozenHostSurfaceIds {
            check(
                "icon \(id)",
                statusItemSystemImage(surfaceId: id) != nil
            )
            check(
                "glyph \(id)",
                !statusItemFallbackGlyph(surfaceId: id).isEmpty
            )
        }

        // --- Full-catalog strip (OpenUsage multi-provider menu bar) ---
        let dualRemainings: [String: [UInt8]] = [
            "claude": [100, 79],
            "codex": [99, 63],
            "amp": [88],
            "grok": [72],
            "zai": [55, 40],
            "kimi": [33],
            "minimax": [91],
            "opencode": [100],
        ]
        let surfaces: [StatusItemSurfaceSnapshot] = frozenHostSurfaceIds.map { id in
            let rems = dualRemainings[id] ?? [50]
            let drive = rems.min() ?? 50
            let prefix = statusItemFallbackGlyph(surfaceId: id)
            return StatusItemSurfaceSnapshot(
                surfaceId: id,
                label: id,
                enabled: true,
                statusBarLabel: "Session \(drive)%",
                status: "fresh",
                compactLabel: "\(prefix) \(drive)%",
                remainings: rems,
                severities: rems.map { $0 < 30 ? "danger" : ($0 < 60 ? "warn" : "ok") }
            )
        }

        let strip = buildStatusItemChips(
            surfaces: surfaces,
            maxCount: 8,
            preferWorstFirst: false,
            percentStyle: "left",
            includeAllEnabled: true
        )
        check("strip shows all 8 providers", strip.count == 8, "count=\(strip.count)")
        check(
            "strip ids catalog order",
            strip.map(\.surfaceId) == frozenHostSurfaceIds,
            "ids=\(strip.map(\.surfaceId))"
        )
        for chip in strip {
            check(
                "\(chip.surfaceId) has displayable icon or glyph",
                chip.systemImage != nil || !chip.glyph.isEmpty
            )
            check(
                "\(chip.surfaceId) has percent lines",
                !chip.percentLines.isEmpty,
                "lines=\(chip.percentLines)"
            )
            // Remaining % only (left style) — tokens end with % or resets fragment.
            check(
                "\(chip.surfaceId) lines are limit tokens not empty",
                chip.percentLines.allSatisfy { !$0.isEmpty }
            )
        }
        check(
            "claude dual remaining stack displayable",
            strip.first(where: { $0.surfaceId == "claude" })?.percentLines == ["100%", "79%"]
        )
        check(
            "codex dual remaining stack displayable",
            strip.first(where: { $0.surfaceId == "codex" })?.percentLines == ["99%", "63%"]
        )
        check(
            "a11y multi-provider non-empty",
            !statusItemAccessibilityLabel(chips: strip).isEmpty
                && statusItemAccessibilityLabel(chips: strip).contains("Cl")
        )

        // --- Default remaining vs used (OpenUsage left default) ---
        check(
            "default remaining token",
            statusItemPercentToken(remainingPercent: 37) == "37%"
        )
        check(
            "used style flips display",
            statusItemPercentToken(remainingPercent: 37, percentStyle: "used") == "63%"
        )
        check(
            "primary label remaining",
            bucketPrimaryPercentLabel(
                remainingPercent: 81,
                usedLabel: nil,
                percentStyle: "left"
            ) == "81% left"
        )
        check(
            "primary label used",
            bucketPrimaryPercentLabel(
                remainingPercent: 81,
                usedLabel: nil,
                percentStyle: "used"
            ) == "19% used"
        )

        // --- Depleted + dual (CodexBar plan-around-resets) ---
        check(
            "depleted+healthy dual keeps weekly",
            statusItemChipDisplayLines(
                remainings: [0, 79],
                compactLabel: "Cl resets 1h 21m",
                percentStyle: "left"
            ) == ["resets 1h 21m", "79%"]
        )

        // --- Empty data honesty (no invented zeros) ---
        let emptyAmp = StatusItemSurfaceSnapshot(
            surfaceId: "amp",
            label: "Amp",
            enabled: true,
            statusBarLabel: "",
            status: "unavailable",
            compactLabel: "",
            remainings: [],
            severities: []
        )
        let withEmpty = buildStatusItemChips(
            surfaces: surfaces + [emptyAmp], // duplicate amp id ignored by later? use only empty
            maxCount: 8,
            preferWorstFirst: false,
            percentStyle: "left",
            includeAllEnabled: true
        )
        // Rebuild: all surfaces but amp empty
        let mixed = frozenHostSurfaceIds.map { id -> StatusItemSurfaceSnapshot in
            if id == "amp" {
                return emptyAmp
            }
            return surfaces.first(where: { $0.surfaceId == id })!
        }
        let mixedChips = buildStatusItemChips(
            surfaces: mixed,
            maxCount: 8,
            preferWorstFirst: false,
            percentStyle: "left",
            includeAllEnabled: true
        )
        check("mixed strip still 8 chips", mixedChips.count == 8)
        check(
            "amp empty shows placeholder not invented percent",
            mixedChips.first(where: { $0.surfaceId == "amp" })?.percentLines == ["—"]
        )

        // --- Bucket row shapes (detail displayable) ---
        check("gauge when remaining", bucketRowShape(remainingPercent: 40, usedLabel: nil) == .gauge)
        check(
            "valueOnly money without remaining",
            bucketRowShape(remainingPercent: nil, usedLabel: "SGD 78 of 260") == .valueOnly
        )
        check("empty honest", bucketRowShape(remainingPercent: nil, usedLabel: nil) == .empty)

        // --- Structural: JackinDesktop wires display paths ---
        let fm = FileManager.default
        let cwd = URL(fileURLWithPath: fm.currentDirectoryPath)
        let desktop =
            fm.fileExists(atPath: cwd.appendingPathComponent("Sources/JackinDesktop").path)
            ? cwd.appendingPathComponent("Sources/JackinDesktop")
            : cwd.appendingPathComponent("native/Sources/JackinDesktop")
        func read(_ rel: String) -> String {
            (try? String(
                contentsOf: desktop.appendingPathComponent(rel),
                encoding: .utf8
            )) ?? ""
        }
        let statusItem = read("StatusItemLabel.swift")
        let popover = read("PopoverRoot.swift")
        let provider = read("UsageWindow/ProviderCardView.swift")
        let overview = read("UsageWindow/OverviewListView.swift")
        check(
            "StatusItemLabel renders chips",
            statusItem.contains("statusItemChips") && statusItem.contains("StatusItemChipView")
        )
        check(
            "Popover agent tile grid",
            popover.contains("agentTileGrid") && popover.contains("overviewStack")
        )
        check(
            "Popover multi-account pills",
            popover.contains("accountsForSurface") && popover.contains("setSelectedAccount")
        )
        check(
            "ProviderCard primary limit labels",
            provider.contains("bucketPrimaryPercentLabel")
        )
        check(
            "Overview dual-bucket stack",
            overview.contains("bucketMiniRow") || overview.contains("remainingPercent")
        )
        check(
            "no sparkline/donut/trend product UI in status item",
            !statusItem.lowercased().contains("sparkline")
                && !statusItem.lowercased().contains("donut")
        )
        check(
            "no sparkline/donut in popover",
            !popover.lowercased().contains("sparkline")
                && !popover.lowercased().contains("donut")
        )
        // silence unused
        _ = withEmpty

        print("---")
        if failures == 0 {
            print("DesktopParityMatrixHarness: ALL PASS")
            print("Matrix: 8/8 providers displayable · dual-bucket · depleted · limits-only")
            exit(0)
        } else {
            print("DesktopParityMatrixHarness: \(failures) FAILURE(S)")
            exit(1)
        }
    }
}
