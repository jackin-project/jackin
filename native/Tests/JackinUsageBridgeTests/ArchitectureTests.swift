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
        let banned = ["% left", "% used", "resets ", "String(format:"]
        for file in files {
            let text = try String(contentsOf: file, encoding: .utf8)
            for token in banned {
                XCTAssertFalse(
                    text.contains(token),
                    "\(file.lastPathComponent) must not compose display string \(token) — use Rust FFI"
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
