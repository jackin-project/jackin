// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

import JackinUsageBridge
import SwiftUI
import XCTest

/// Static architecture checks: Swift tree must not grow provider probe logic.
final class ArchitectureTests: XCTestCase {
    private var sourcesRoot: URL {
        URL(fileURLWithPath: #filePath)
            .deletingLastPathComponent() // Tests/JackinUsageBridgeTests
            .deletingLastPathComponent() // Tests
            .deletingLastPathComponent() // native
            .appendingPathComponent("Sources")
    }

    private func handwrittenSwiftFiles() throws -> [URL] {
        let enumerator = FileManager.default.enumerator(
            at: sourcesRoot,
            includingPropertiesForKeys: nil
        )
        var files: [URL] = []
        while let url = enumerator?.nextObject() as? URL {
            if url.pathExtension == "swift", !url.lastPathComponent.contains("jackin_usage_ffi") {
                files.append(url)
            }
        }
        XCTAssertFalse(files.isEmpty, "expected Swift sources under native/Sources")
        return files
    }

    func testSwiftSourcesHaveNoProviderProbeImports() throws {
        // Probe/API machinery tokens (substring match).
        let machinery = ["URLSession", "Process(", "SecItem"]
        // Non-jackin providers: whole-token only so `eventCursor` / comments about
        // cursors are not false positives for the Cursor product.
        let providers = ["Gemini", "Copilot"]
        let cursorAsProvider = try NSRegularExpression(pattern: #"\bCursor\b"#)
        for file in try handwrittenSwiftFiles() {
            let text = try String(contentsOf: file, encoding: .utf8)
            let full = NSRange(text.startIndex..., in: text)
            for token in machinery {
                XCTAssertFalse(
                    text.contains(token),
                    "\(file.lastPathComponent) must not contain probe/API token \(token)"
                )
            }
            for token in providers {
                XCTAssertFalse(
                    text.contains(token),
                    "\(file.lastPathComponent) must not contain non-jackin provider \(token)"
                )
            }
            XCTAssertEqual(
                cursorAsProvider.numberOfMatches(in: text, range: full),
                0,
                "\(file.lastPathComponent) must not mention Cursor provider"
            )
            XCTAssertFalse(
                text.contains("URL(string: \"http"),
                "\(file.lastPathComponent) must not perform HTTP probes"
            )
            XCTAssertFalse(
                text.contains("URL(string: \"https://api."),
                "\(file.lastPathComponent) must not perform HTTPS API probes"
            )
        }
    }

    func testMacOS26AvailabilityOnlyInGlassFallbacks() throws {
        for file in try handwrittenSwiftFiles() {
            let text = try String(contentsOf: file, encoding: .utf8)
            let hasGate = text.contains("#available(macOS 26")
            if file.lastPathComponent == "GlassFallbacks.swift" {
                XCTAssertTrue(hasGate, "GlassFallbacks.swift must own macOS 26 gates")
            } else {
                XCTAssertFalse(
                    hasGate,
                    "\(file.lastPathComponent) must not contain #available(macOS 26 — use GlassFallbacks"
                )
            }
        }
    }

    func testNoSwiftPercentArithmeticOnDisplayStrings() throws {
        // Heuristic: handwritten UI must not invent percentages via string
        // interpolation of computed used/remaining math into Text(...).
        // Gauge uses Rust-provided remaining only; forbid Text("…\(…)%…").
        let regex = try NSRegularExpression(
            pattern: #"Text\s*\(\s*"[^"]*\\\([^)]*\)[^"]*%"#
        )
        for file in try handwrittenSwiftFiles() {
            let text = try String(contentsOf: file, encoding: .utf8)
            let range = NSRange(text.startIndex..., in: text)
            let hits = regex.numberOfMatches(in: text, range: range)
            XCTAssertEqual(
                hits,
                0,
                "\(file.lastPathComponent) must not interpolate computed % into Text("
            )
        }
    }

    func testSeverityAndStatusBadgeMappings() {
        XCTAssertEqual(severityTint("danger"), Color.red)
        XCTAssertEqual(severityTint("warn"), Color.orange)
        XCTAssertEqual(statusBadgeSymbol("error"), "exclamationmark.triangle")
        XCTAssertEqual(statusBadgeSymbol("stale"), "clock")
        XCTAssertNil(statusBadgeSymbol("fresh"))
    }

    func testStatusItemChipHelpers() {
        let drive = drivingBucketForStatusItem(
            remainingAndSeverity: [
                (remaining: 63, severity: "ok"),
                (remaining: 12, severity: "danger"),
                (remaining: 41, severity: "warn"),
            ]
        )
        XCTAssertEqual(drive?.remaining, 12)
        XCTAssertEqual(drive?.severity, "danger")
        XCTAssertEqual(statusItemUsedFraction(remainingPercent: 63), 0.37, accuracy: 0.001)
        XCTAssertEqual(statusItemUsedFraction(remainingPercent: 100), 0.0, accuracy: 0.001)
        XCTAssertEqual(statusItemUsedFraction(remainingPercent: 0), 1.0, accuracy: 0.001)
        XCTAssertEqual(statusItemRemainingFraction(remainingPercent: 63), 0.63, accuracy: 0.001)
        XCTAssertEqual(statusItemRemainingFraction(remainingPercent: 100), 1.0, accuracy: 0.001)
        XCTAssertEqual(statusItemRemainingFraction(remainingPercent: 0), 0.0, accuracy: 0.001)
        XCTAssertTrue(statusItemLineShowsMiniBar("79%"))
        XCTAssertFalse(statusItemLineShowsMiniBar("resets 1h"))
        XCTAssertEqual(statusItemPercentToken(remainingPercent: 79), "79%")
        XCTAssertEqual(
            statusItemPercentToken(remainingPercent: 37, percentStyle: "used"),
            "63%"
        )
        XCTAssertEqual(statusItemGlyph(compactLabel: "Cl 37%", surfaceId: "claude"), "Cl")
        XCTAssertEqual(
            statusItemPercentLines(remainings: [100, 79, 12], maxLines: 2),
            ["100%", "79%"]
        )
        XCTAssertEqual(
            statusItemPercentLines(
                remainings: [37, 79],
                maxLines: 2,
                percentStyle: "used"
            ),
            ["63%", "21%"]
        )
        XCTAssertEqual(
            bucketPrimaryPercentLabel(
                remainingPercent: 81,
                usedLabel: "19% used",
                percentStyle: "left"
            ),
            "81% left"
        )
        XCTAssertEqual(
            bucketPrimaryPercentLabel(
                remainingPercent: 81,
                usedLabel: "19% used",
                percentStyle: "used"
            ),
            "19% used"
        )
        XCTAssertEqual(
            bucketPrimaryPercentLabel(
                remainingPercent: nil,
                usedLabel: "SGD 78 of 260",
                percentStyle: "left"
            ),
            "SGD 78 of 260"
        )
        let money = MoneyDto(amountMinor: 6559, currency: "USD", exponent: 2)
        XCTAssertEqual(formatMoneyDto(money), "$65.59")
    }

    /// Multi-provider strip: one chip per surface with dual-bucket remainings (CodexBar parity).
    ///
    /// Compact labels use **remaining** (OpenUsage/CodexBar default) and must
    /// match stacked percent lines / a11y (no used% vs remaining% mix).
    func testBuildStatusItemChipsMultiProviderDualBucket() {
        let surfaces = [
            StatusItemSurfaceSnapshot(
                surfaceId: "claude",
                label: "Claude",
                enabled: true,
                statusBarLabel: "Session 100% · Weekly 79%",
                status: "fresh",
                // Driving remaining = min(100, 79) = 79 → compact Cl 79%.
                compactLabel: "Cl 79%",
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
        ]
        let chips = buildStatusItemChips(
            surfaces: surfaces,
            maxCount: 6,
            preferWorstFirst: false,
            percentStyle: "left",
            includeAllEnabled: false
        )
        // Amp empty/unavailable and disabled Grok hidden; Claude + Codex only.
        XCTAssertEqual(chips.map(\.surfaceId), ["claude", "codex"])

        let openUsageStrip = buildStatusItemChips(
            surfaces: surfaces,
            maxCount: 8,
            preferWorstFirst: false,
            percentStyle: "left",
            includeAllEnabled: true
        )
        // Enabled amp (no data) appears with "—"; disabled grok still hidden.
        XCTAssertEqual(openUsageStrip.map(\.surfaceId), ["claude", "codex", "amp"])
        XCTAssertEqual(
            openUsageStrip.first(where: { $0.surfaceId == "amp" })?.percentLines,
            ["—"]
        )
        XCTAssertNotNil(openUsageStrip.first(where: { $0.surfaceId == "claude" })?.systemImage)
        XCTAssertEqual(
            openUsageStrip.first(where: { $0.surfaceId == "claude" })?.percentLines,
            ["100%", "79%"]
        )
        XCTAssertEqual(chips[0].percentLines, ["100%", "79%"])
        XCTAssertEqual(chips[0].remainingPerLine, [100, 79])
        XCTAssertEqual(chips[0].remainingPercent, 79)
        XCTAssertEqual(chips[1].percentLines, ["84%"])
        XCTAssertEqual(chips[1].systemImage, "circle.hexagongrid.fill")
        XCTAssertEqual(chips[0].compactLabel, "Cl 79%")
        // Dual-bucket a11y speaks both stacked remaining lines.
        XCTAssertEqual(
            statusItemAccessibilityLabel(chips: chips),
            "jackin Desktop Cl 100% and 79%, Cx 84%"
        )
        // Production invariant: compact driving digit matches min remaining line.
        XCTAssertTrue(
            chips[0].compactLabel.contains("79%"),
            "compact must use driving remaining, not used%"
        )
        XCTAssertTrue(chips[0].percentLines.contains("79%"))

        let worstFirst = buildStatusItemChips(
            surfaces: surfaces,
            maxCount: 1,
            preferWorstFirst: true,
            percentStyle: "left"
        )
        // Claude min remaining 79 < Codex 84 → Claude worst.
        XCTAssertEqual(worstFirst.map(\.surfaceId), ["claude"])

        // Used style flips stacked lines only; remainings stay Rust-owned.
        let usedChips = buildStatusItemChips(
            surfaces: surfaces,
            maxCount: 2,
            preferWorstFirst: false,
            percentStyle: "used"
        )
        XCTAssertEqual(usedChips[0].percentLines, ["0%", "21%"])
        XCTAssertEqual(usedChips[1].percentLines, ["16%"])
        XCTAssertEqual(usedChips[0].remainingPerLine, [100, 79])

        // Depleted with Rust reset countdown (CodexBar plan-around-resets).
        let depleted = StatusItemSurfaceSnapshot(
            surfaceId: "claude",
            label: "Claude",
            enabled: true,
            statusBarLabel: "depleted",
            status: "fresh",
            compactLabel: "Cl resets 1h 21m",
            remainings: [0],
            severities: ["danger"]
        )
        let dep = buildStatusItemChips(
            surfaces: [depleted],
            maxCount: 1,
            preferWorstFirst: false,
            percentStyle: "left"
        )
        XCTAssertEqual(dep.count, 1)
        XCTAssertEqual(dep[0].percentLines, ["resets 1h 21m"])
        XCTAssertTrue(statusItemCompactIsResetCountdown(dep[0].compactLabel))
        XCTAssertEqual(
            statusItemAccessibilityLabel(chips: dep),
            "jackin Desktop Cl resets 1h 21m"
        )

        // Dual-bucket depleted session + healthy weekly must keep 79%.
        XCTAssertEqual(
            statusItemChipDisplayLines(
                remainings: [0, 79],
                compactLabel: "Cl resets 1h 21m",
                percentStyle: "left"
            ),
            ["resets 1h 21m", "79%"]
        )
        let dualDep = StatusItemSurfaceSnapshot(
            surfaceId: "claude",
            label: "Claude",
            enabled: true,
            statusBarLabel: "Session 0 · Weekly 79",
            status: "fresh",
            compactLabel: "Cl resets 1h 21m",
            remainings: [0, 79],
            severities: ["danger", "ok"]
        )
        let dual = buildStatusItemChips(
            surfaces: [dualDep],
            maxCount: 1,
            preferWorstFirst: false,
            percentStyle: "left"
        )
        XCTAssertEqual(dual.count, 1)
        XCTAssertEqual(dual[0].percentLines, ["resets 1h 21m", "79%"])
        XCTAssertEqual(dual[0].remainingPerLine, [0, 79])
        XCTAssertEqual(dual[0].percentLines[1], "79%")
        XCTAssertEqual(
            statusItemAccessibilityLabel(chips: dual),
            "jackin Desktop Cl resets 1h 21m and 79%"
        )

        // Agent tiles: dual remaining + bucket "Resets in …" depleted form.
        XCTAssertEqual(
            tileRemainingBadgeLines(remainings: [86, 95]),
            ["86%", "95%"]
        )
        XCTAssertEqual(tileRemainingBadgeCompact(remainings: [86, 95]), "86%/95%")
        XCTAssertEqual(
            tileRemainingBadgeLines(
                remainings: [0, 79],
                compactLabel: "Resets in 1h 21m"
            ),
            ["Resets in 1h 21m", "79%"]
        )
        XCTAssertEqual(
            statusItemResetCountdownLine(compactLabel: "Resets in 2h"),
            "Resets in 2h"
        )
        XCTAssertEqual(
            splitPaceLabel("On pace · Runs out in 4d 21h"),
            ["On pace", "Runs out in 4d 21h"]
        )
        XCTAssertEqual(
            bucketMetricPrimaryLabel(
                remainingPercent: 0,
                usedLabel: nil,
                resetLabel: "Resets in 2h"
            ),
            "Resets in 2h"
        )
        XCTAssertEqual(
            bucketMetricPrimaryLabel(
                remainingPercent: 81,
                usedLabel: nil,
                resetLabel: "Resets in 5h"
            ),
            "81% left"
        )
        XCTAssertEqual(
            surfaceRemainingSubtitle(remainings: [100, 79]),
            "100% · 79%"
        )
        XCTAssertEqual(
            overviewNumericBuckets(
                remainingPercents: [100, 79, 40, 22, 5].map { Optional($0) }
            ),
            [100, 79, 40, 22]
        )
        XCTAssertEqual(overviewNumericBucketCap, 4)
        XCTAssertEqual(
            accountPillLabel(
                accountLabel: "work@ex.com",
                remainingPercent: 63,
                selected: true
            ),
            "work@ex.com, 63%, selected"
        )
        XCTAssertTrue(isMachineStatusSlot("session"))
        XCTAssertTrue(isMachineStatusSlot("weekly"))
        XCTAssertTrue(isMachineStatusSlot("spend"))
        XCTAssertNil(
            bucketGaugeSecondaryLimitLabel(limitLabel: "100%", remainingPercent: 81)
        )
        XCTAssertEqual(
            bucketGaugeSecondaryLimitLabel(limitLabel: "SGD 260", remainingPercent: nil),
            "SGD 260"
        )
    }

    func testBuildStatusItemChipsRespectsCapAndHidesEmpty() {
        let surfaces = (0..<5).map { i in
            StatusItemSurfaceSnapshot(
                surfaceId: "s\(i)",
                label: "S\(i)",
                enabled: true,
                statusBarLabel: "ok",
                status: "fresh",
                compactLabel: "S\(i) \(50 + i)%",
                remainings: [UInt8(50 + i)],
                severities: ["ok"]
            )
        }
        let chips = buildStatusItemChips(
            surfaces: surfaces,
            maxCount: 3,
            preferWorstFirst: false
        )
        XCTAssertEqual(chips.count, 3)
        XCTAssertEqual(chips.map(\.surfaceId), ["s0", "s1", "s2"])
    }

    /// OpenUsage/CodexBar matrix: all 8 frozen hosts displayable with icons + remaining %.
    func testFullFrozenCatalogStripDisplayable() {
        XCTAssertEqual(frozenHostSurfaceIds.count, 8)
        XCTAssertTrue(allFrozenHostSurfacesHaveSystemImages())
        let surfaces = frozenHostSurfaceIds.enumerated().map { i, id in
            StatusItemSurfaceSnapshot(
                surfaceId: id,
                label: id,
                enabled: true,
                statusBarLabel: "ok",
                status: "fresh",
                compactLabel: "\(statusItemFallbackGlyph(surfaceId: id)) \(40 + i)%",
                remainings: id == "claude" ? [100, 79] : [UInt8(40 + i)],
                severities: ["ok"]
            )
        }
        let chips = buildStatusItemChips(
            surfaces: surfaces,
            maxCount: 8,
            preferWorstFirst: false,
            percentStyle: "left",
            includeAllEnabled: true
        )
        XCTAssertEqual(chips.map(\.surfaceId), frozenHostSurfaceIds)
        for chip in chips {
            XCTAssertNotNil(chip.systemImage, "\(chip.surfaceId) needs SF Symbol")
            XCTAssertFalse(chip.percentLines.isEmpty, "\(chip.surfaceId) needs displayable %")
        }
        XCTAssertEqual(
            chips.first(where: { $0.surfaceId == "claude" })?.percentLines,
            ["100%", "79%"]
        )
    }

    func testPackageSwiftUsesBinaryTargetNotHostDylib() throws {
        let package = sourcesRoot
            .deletingLastPathComponent()
            .appendingPathComponent("Package.swift")
        let text = try String(contentsOf: package, encoding: .utf8)
        XCTAssertTrue(
            text.contains(".binaryTarget("),
            "Package.swift must consume the static XCFramework via binaryTarget"
        )
        XCTAssertTrue(
            text.contains("jackin_usage_ffiFFI"),
            "binary target name must match UniFFI module jackin_usage_ffiFFI"
        )
        XCTAssertFalse(
            text.contains("target/release"),
            "Package.swift must not link host target/release dylib path"
        )
        XCTAssertFalse(
            text.contains("linkedLibrary(\"jackin_usage_ffi\")"),
            "Package.swift must not dynamically link libjackin_usage_ffi"
        )
    }

    func testDesktopSourcesDoNotComposePercentOrResetLiterals() throws {
        let desktop = sourcesRoot.appendingPathComponent("JackinDesktop")
        let enumerator = FileManager.default.enumerator(at: desktop, includingPropertiesForKeys: nil)
        var files: [URL] = []
        while let url = enumerator?.nextObject() as? URL {
            if url.pathExtension == "swift" {
                files.append(url)
            }
        }
        XCTAssertFalse(files.isEmpty, "expected JackinDesktop sources")
        // Usage-string tokens: ban on display surfaces only. SettingsView may use
        // "% left"/"% used" as preference *chrome* (format picker labels) — those
        // are not composed usage numbers; Rust still owns every gauge/status string.
        let usageStringTokens = ["% left", "% used", "resets "]
        // Always ban format composition everywhere under JackinDesktop.
        let alwaysBanned = ["String(format:"]
        // Preference chrome only (S6 format pickers); never render usage data.
        let preferenceChromeFiles: Set<String> = ["SettingsView.swift"]
        for file in files {
            let text = try String(contentsOf: file, encoding: .utf8)
            let name = file.lastPathComponent
            for token in alwaysBanned {
                XCTAssertFalse(
                    text.contains(token),
                    "\(name) must not compose display string \(token) — use Rust FFI"
                )
            }
            if preferenceChromeFiles.contains(name) {
                continue
            }
            for token in usageStringTokens {
                XCTAssertFalse(
                    text.contains(token),
                    "\(name) must not compose display string \(token) — use Rust FFI"
                )
            }
        }
    }

    func testScreenShareProbeLivesOnlyInPresentationStore() throws {
        for file in try handwrittenSwiftFiles() {
            let text = try String(contentsOf: file, encoding: .utf8)
            let has = text.contains("CGSessionCopyCurrentDictionary")
            if file.lastPathComponent == "PresentationStore.swift" {
                XCTAssertTrue(has, "PresentationStore must own screen-share detection")
            } else {
                XCTAssertFalse(
                    has,
                    "\(file.lastPathComponent) must not call CGSessionCopyCurrentDictionary"
                )
            }
        }
    }

    func testBucketRowShapeSelection() {
        XCTAssertEqual(bucketRowShape(remainingPercent: 40, usedLabel: "60% used"), .gauge)
        XCTAssertEqual(bucketRowShape(remainingPercent: nil, usedLabel: "$0.06"), .valueOnly)
        XCTAssertEqual(bucketRowShape(remainingPercent: nil, usedLabel: nil), .empty)
        XCTAssertEqual(bucketRowShape(remainingPercent: nil, usedLabel: ""), .empty)
    }

    func testOverviewGlanceBodySelection() {
        XCTAssertEqual(
            overviewGlanceBody(headline: "97% left", resetLabel: "Resets in 2h", statusWord: "fresh"),
            .numeric(headline: "97% left", reset: "Resets in 2h")
        )
        XCTAssertEqual(
            overviewGlanceBody(headline: "97% left", resetLabel: nil, statusWord: "fresh"),
            .numeric(headline: "97% left", reset: nil)
        )
        XCTAssertEqual(
            overviewGlanceBody(headline: "", resetLabel: nil, statusWord: "unsupported"),
            .statusWord("unsupported")
        )
    }

    func testPopoverHasNoGaugeAndSurfaceCardGone() throws {
        let desktop = sourcesRoot.appendingPathComponent("JackinDesktop")
        XCTAssertFalse(
            FileManager.default.fileExists(atPath: desktop.appendingPathComponent("SurfaceCard.swift").path),
            "SurfaceCard.swift must be deleted after glance popover rewrite"
        )
        let popover = desktop.appendingPathComponent("PopoverRoot.swift")
        let text = try String(contentsOf: popover, encoding: .utf8)
        XCTAssertFalse(text.contains("Gauge("), "popover must not render capacity gauges")
        XCTAssertFalse(text.contains("SurfaceCard"), "popover must not reference SurfaceCard")
    }

    /// Cold launch: status item must open the host runtime without a menu click.
    func testStatusItemLabelOpensRuntimeOnAppear() throws {
        let status = sourcesRoot
            .appendingPathComponent("JackinDesktop")
            .appendingPathComponent("StatusItemLabel.swift")
        let text = try String(contentsOf: status, encoding: .utf8)
        XCTAssertTrue(
            text.contains("openDefault"),
            "StatusItemLabel must call openDefault so focus-percent works on cold launch"
        )
        XCTAssertTrue(
            text.contains("onAppear"),
            "StatusItemLabel must open runtime on appear, not only after popover open"
        )
    }

    func testDesktopSourcesHaveNoHardcodedProviderDisplayNames() throws {
        let desktop = sourcesRoot.appendingPathComponent("JackinDesktop")
        let enumerator = FileManager.default.enumerator(at: desktop, includingPropertiesForKeys: nil)
        let banned = ["\"OpenAI\"", "\"Anthropic\"", "\"xAI\"", "\"Z.AI\""]
        while let url = enumerator?.nextObject() as? URL {
            guard url.pathExtension == "swift" else { continue }
            let text = try String(contentsOf: url, encoding: .utf8)
            for token in banned {
                XCTAssertFalse(
                    text.contains(token),
                    "\(url.lastPathComponent) must not hardcode provider display name \(token)"
                )
            }
        }
    }

    func testStatusItemTextSelectionModes() {
        XCTAssertEqual(
            statusItemTextSelection(
                mode: .iconOnly,
                pinnedSurfaceId: nil,
                stripMax: 3,
                hideForScreenShare: false
            ),
            .empty
        )
        XCTAssertEqual(
            statusItemTextSelection(
                mode: .focusPercent,
                pinnedSurfaceId: nil,
                stripMax: 3,
                hideForScreenShare: false
            ),
            .focus
        )
        XCTAssertEqual(
            statusItemTextSelection(
                mode: .pinnedSurface,
                pinnedSurfaceId: "codex",
                stripMax: 3,
                hideForScreenShare: false
            ),
            .pinned(surfaceId: "codex")
        )
        XCTAssertEqual(
            statusItemTextSelection(
                mode: .pinnedSurface,
                pinnedSurfaceId: nil,
                stripMax: 3,
                hideForScreenShare: false
            ),
            .empty
        )
        XCTAssertEqual(
            statusItemTextSelection(
                mode: .strip,
                pinnedSurfaceId: nil,
                stripMax: 3,
                hideForScreenShare: false
            ),
            .strip(max: 3)
        )
        XCTAssertEqual(
            statusItemTextSelection(
                mode: .focusPercent,
                pinnedSurfaceId: nil,
                stripMax: 3,
                hideForScreenShare: true
            ),
            .empty
        )
    }
}
