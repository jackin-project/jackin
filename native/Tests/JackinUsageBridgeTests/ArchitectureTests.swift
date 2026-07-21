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
