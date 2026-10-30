// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

import JackinUsageBridge
import SwiftUI
import XCTest

final class ArchitectureTests: XCTestCase {
    var sourcesRoot: URL {
        URL(fileURLWithPath: #filePath)
            .deletingLastPathComponent()
            .deletingLastPathComponent()
            .deletingLastPathComponent()
            .appendingPathComponent("Sources")
    }

    func handwrittenSwiftFiles() throws -> [URL] {
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
        let machinery = ["URLSession", "Process(", "SecItem"]
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

    func testLatestOnlySourcesHaveNoMacOSCompatibilityBranches() throws {
        for file in try handwrittenSwiftFiles() {
            let text = try String(contentsOf: file, encoding: .utf8)
            XCTAssertFalse(
                text.contains("#available(macOS"),
                "\(file.lastPathComponent) must not retain a pre-26 compatibility lane"
            )
        }
    }

    func testProductionHasNoCustomGlassEffects() throws {
        for file in try handwrittenSwiftFiles() {
            let text = try String(contentsOf: file, encoding: .utf8)
            XCTAssertFalse(
                text.contains("glassEffect") || text.contains("GlassEffectContainer"),
                "\(file.lastPathComponent) must let standard controls own Liquid Glass"
            )
        }
    }

    func testProductionHasNoHandPaintedSystemMaterial() throws {
        let regex = try NSRegularExpression(
            pattern:
                #"\.background\(\.(bar|material)\)|\.(ultraThin|thin|regular|thick|ultraThick)Material"#
        )
        for file in try handwrittenSwiftFiles() {
            let text = try String(contentsOf: file, encoding: .utf8)
            let range = NSRange(text.startIndex..., in: text)
            let hits = regex.numberOfMatches(in: text, range: range)
            XCTAssertEqual(hits, 0, "\(file.lastPathComponent) must not paint custom material")
            XCTAssertFalse(
                text.contains("NSVisualEffectView"),
                "\(file.lastPathComponent) must not imitate system material"
            )
        }
    }

    func testDesktopSourcesContainNoForbiddenUsagePresentation() throws {
        let forbidden = [
            "$/token", "$/mtok", "cost of session", "spend over time", "usage trend",
            "token history", "spend history", "aggregate spend", "top model",
            "30-day token", "30-day spend",
        ]
        for file in try handwrittenSwiftFiles() {
            let text = try String(contentsOf: file, encoding: .utf8).lowercased()
            for token in forbidden {
                XCTAssertFalse(
                    text.contains(token),
                    "\(file.lastPathComponent) must not surface \(token) — limits-only (N3/B4)"
                )
            }
        }
    }

    func testNoSwiftPercentArithmeticOnDisplayStrings() throws {
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

    func testSeverityTintUsesBrandAndSystemWarningColors() {
        XCTAssertEqual(severityTint("danger"), Color.red)
        XCTAssertEqual(severityTint("warn"), Color.orange)
        XCTAssertEqual(severityTint("normal"), Color.jackinPhosphor)
        XCTAssertNotEqual(severityTint("normal"), Color.accentColor)
    }

    func testJackinPhosphorTokensMatchBrandGuide() {
        XCTAssertEqual(JackinBrand.phosphorDarkSRGB.r, 0x5C / 255.0, accuracy: 0.0001)
        XCTAssertEqual(JackinBrand.phosphorDarkSRGB.g, 0xF0 / 255.0, accuracy: 0.0001)
        XCTAssertEqual(JackinBrand.phosphorDarkSRGB.b, 0x7A / 255.0, accuracy: 0.0001)
        XCTAssertEqual(JackinBrand.phosphorLightSRGB.r, 0x0B / 255.0, accuracy: 0.0001)
        XCTAssertEqual(JackinBrand.phosphorLightSRGB.g, 0x77 / 255.0, accuracy: 0.0001)
        XCTAssertEqual(JackinBrand.phosphorLightSRGB.b, 0x4E / 255.0, accuracy: 0.0001)
    }

    func testOverviewHasNoOrphanOverviewLevelProgress() throws {
        let overview =
            sourcesRoot
            .appendingPathComponent("JackinDesktop/UsageWindow/OverviewListView.swift")
        let text = try String(contentsOf: overview, encoding: .utf8)
        XCTAssertFalse(
            text.contains("ProgressView"),
            "OV-11: Overview must not ship ProgressView as Overview-level chrome"
        )
        XCTAssertFalse(
            text.contains("LinearProgress"),
            "OV-11: no LinearProgress Overview-level chrome"
        )
        XCTAssertTrue(text.contains("Table("), "Overview must use native Table")
        XCTAssertFalse(text.contains("Capsule"), "Overview must not paint custom meters")
    }

    func testOverviewPrimaryValuesUseSystemForeground() throws {
        let overview =
            sourcesRoot
            .appendingPathComponent("JackinDesktop")
            .appendingPathComponent("UsageWindow/OverviewListView.swift")
        let text = try String(contentsOf: overview, encoding: .utf8)

        XCTAssertTrue(
            text.contains(
                "Text(row.planOrStatusLabel)\n"
                    + "                            .foregroundStyle(.primary)")
        )
        XCTAssertTrue(
            text.contains(
                "Text(row.resetLabel)\n"
                    + "                        .foregroundStyle(.primary)")
        )
        XCTAssertFalse(text.contains("Color("))
    }
}
