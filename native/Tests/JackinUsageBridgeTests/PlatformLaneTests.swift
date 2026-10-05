// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

import XCTest

/// Platform-lane contract: shipping identity and forward-lane state are recorded in the manifest and
/// the native README, `UIDesignRequiresCompatibility` never ships, and any
/// post-26.0 symbol the component map lists is reachable only behind a guard.
final class PlatformLaneTests: XCTestCase {
    private var nativeRoot: URL {
        URL(fileURLWithPath: #filePath)
            .deletingLastPathComponent()  // Tests/JackinUsageBridgeTests
            .deletingLastPathComponent()  // Tests
            .deletingLastPathComponent()  // native
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
            XCTAssertTrue(
                content.contains("26.0"),
                "\(name) must record the macOS 26.0 minimum deployment target"
            )
            XCTAssertTrue(
                content.contains("Xcode 27.0") && content.contains("27A266a"),
                "\(name) must record the stable shipping lane (Xcode 27.0 build 27A266a)"
            )
            XCTAssertTrue(
                content.contains("macOS 27.0 SDK") && content.contains("26A425"),
                "\(name) must record the shipping SDK"
            )
            XCTAssertTrue(
                content.contains("Swift 6.4")
                    && content.contains("swiftlang-6.4.0.34.1")
                    && content.contains("clang-2100.3.34.1"),
                "\(name) must record the exact shipping Swift/compiler builds"
            )
            XCTAssertTrue(
                content.lowercased().contains("forward-validation lane:")
                    && content.contains("unimplemented") && content.contains("nonblocking"),
                "\(name) must record the unimplemented nonblocking forward-validation lane"
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
            // Documentation may name the key only to forbid it.
            if url.pathExtension == "md" { continue }
            // Manifest/policy comments may name the key to forbid it; strip them.
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
        // Table rows naming a post-26.0 availability, e.g. `| Symbol | macOS 27 |`.
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
