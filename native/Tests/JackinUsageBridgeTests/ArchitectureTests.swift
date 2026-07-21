// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

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
        let prohibitedTokens = ["URLSession", "Process(", "SecItem", "Cursor", "Gemini", "Copilot"]
        for file in try handwrittenSwiftFiles() {
            let text = try String(contentsOf: file, encoding: .utf8)
            for token in prohibitedTokens {
                XCTAssertFalse(
                    text.contains(token),
                    "\(file.lastPathComponent) must not contain probe/API token \(token)"
                )
            }
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
        // Heuristic: handwritten UI must not invent percentages via string interpolation
        // of computed used/remaining math into Text(...). Remaining percent rendering
        // uses Gauge(value:) with Rust-provided remaining only.
        let pattern = #"Text\([^)]*\\([^)]*%"#
        let regex = try NSRegularExpression(pattern: pattern)
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
        XCTAssertEqual(severityTint("danger"), .red)
        XCTAssertEqual(severityTint("warn"), .orange)
        XCTAssertEqual(statusBadgeSymbol("error"), "exclamationmark.triangle")
        XCTAssertEqual(statusBadgeSymbol("stale"), "clock")
        XCTAssertNil(statusBadgeSymbol("fresh"))
    }
}
