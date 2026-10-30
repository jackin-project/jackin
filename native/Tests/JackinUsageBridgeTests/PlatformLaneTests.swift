// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

import XCTest

final class PlatformLaneTests: XCTestCase {
    private var nativeRoot: URL {
        URL(fileURLWithPath: #filePath)
            .deletingLastPathComponent()
            .deletingLastPathComponent()
            .deletingLastPathComponent()
    }

    private func text(_ relative: String) throws -> String {
        try String(
            contentsOf: nativeRoot.appendingPathComponent(relative),
            encoding: .utf8
        )
    }

    func testManifestAndReadmeRecordBothLanes() throws {
        let project = try text("project.yml")
        let readme = try text("README.md")
        for (name, content) in [("project.yml", project), ("README.md", readme)] {
            let shippingLaneRecord =
                name == "project.yml"
                ? "shipping lane Xcode 27.1"
                : "Shipping lane: **Xcode 27.1"
            XCTAssertTrue(
                content.contains("27.0"),
                "\(name) must record the macOS 27.0 minimum deployment target"
            )
            XCTAssertTrue(
                content.contains(shippingLaneRecord),
                "\(name) must record the shipping lane (Xcode 27.1)"
            )
            XCTAssertTrue(
                content.contains("macOS 27 SDK"),
                "\(name) must record the shipping SDK"
            )
            XCTAssertTrue(
                content.contains("nonblocking"),
                "\(name) must record the nonblocking forward-validation lane"
            )
        }
    }

    func testNoUIDesignRequiresCompatibilityAnywhere() throws {
        let enumerator = FileManager.default.enumerator(
            at: nativeRoot,
            includingPropertiesForKeys: nil
        )
        var scanned = 0
        while let url = enumerator?.nextObject() as? URL {
            let path = url.path
            if path.contains("/DerivedData/") || path.contains("/.build/")
                || path.contains("/JackinDesktop.xcodeproj/") || path.contains("/dist/")
            {
                continue
            }
            guard ["swift", "yml", "plist", "md"].contains(url.pathExtension) else { continue }
            let raw = try String(contentsOf: url, encoding: .utf8)
            scanned += 1
            if url.lastPathComponent == "PlatformLaneTests.swift" { continue }
            if url.pathExtension == "md" { continue }
            let content = raw.components(separatedBy: .newlines)
                .filter { !$0.trimmingCharacters(in: .whitespaces).hasPrefix("#") }
                .joined(separator: "\n")
            XCTAssertFalse(
                content.contains("UIDesignRequiresCompatibility"),
                "\(url.lastPathComponent) must not ship UIDesignRequiresCompatibility"
            )
        }
        XCTAssertGreaterThan(scanned, 0, "expected to scan native project files")
    }

    func testComponentMapPost26SymbolsAreGuarded() throws {
        let componentMap = try text("Design/UnifiedAgentUsage/NativeComponentMap.md")
        let rowPattern = try NSRegularExpression(
            pattern: #"^\| *`([A-Za-z0-9_]+)` *\| *macOS (2[7-9]|[3-9][0-9])"#,
            options: [.anchorsMatchLines]
        )
        let guardPattern = try NSRegularExpression(
            pattern: #"[#@]available\(macOS (2[7-9]|[3-9][0-9])"#
        )
        let range = NSRange(componentMap.startIndex..., in: componentMap)
        let guardedSymbols = rowPattern.matches(in: componentMap, range: range).map {
            (componentMap as NSString).substring(with: $0.range(at: 1))
        }
        guard !guardedSymbols.isEmpty else { return }

        let enumerator = FileManager.default.enumerator(
            at: nativeRoot.appendingPathComponent("Sources"),
            includingPropertiesForKeys: nil
        )
        while let url = enumerator?.nextObject() as? URL {
            guard url.pathExtension == "swift",
                !url.lastPathComponent.contains("jackin_usage_ffi")
            else { continue }
            let lines = try String(contentsOf: url, encoding: .utf8).components(
                separatedBy: .newlines)
            for (index, line) in lines.enumerated() {
                for symbol in guardedSymbols where line.contains(symbol) {
                    let window = lines[max(0, index - 3)...index].joined(separator: "\n")
                    let windowRange = NSRange(window.startIndex..., in: window)
                    XCTAssertFalse(
                        guardPattern.firstMatch(in: window, range: windowRange) == nil,
                        "\(url.lastPathComponent):\(index + 1) uses post-26.0 symbol \(symbol) without a guard"
                    )
                }
            }
        }
    }
}
