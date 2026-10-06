// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

import XCTest

/// One-way bridge boundary: the generated C module is imported only by
/// generated Swift, the bridge handle is named only by the typed facade, and
/// the bindings target never grows handwritten source.
final class BridgeBoundaryTests: XCTestCase {
    private var nativeRoot: URL {
        URL(fileURLWithPath: #filePath)
            .deletingLastPathComponent()  // Tests/JackinUsageBridgeTests
            .deletingLastPathComponent()  // Tests
            .deletingLastPathComponent()  // native
    }

    private func swiftFiles(under relative: String) throws -> [URL] {
        let enumerator = FileManager.default.enumerator(
            at: nativeRoot.appendingPathComponent(relative),
            includingPropertiesForKeys: nil
        )
        var files: [URL] = []
        while let url = enumerator?.nextObject() as? URL {
            if url.pathExtension == "swift" {
                files.append(url)
            }
        }
        XCTAssertFalse(files.isEmpty, "expected Swift sources under \(relative)")
        return files
    }

    func testGeneratedCModuleImportedOnlyByGeneratedSwift() throws {
        for relative in ["Sources", "Tools", "Tests", "UITests"] {
            for file in try swiftFiles(under: relative) {
                let text = try String(contentsOf: file, encoding: .utf8)
                let isGenerated = file.path.contains("Sources/JackinUsageBindings/")
                if isGenerated { continue }
                if file.lastPathComponent == "BridgeBoundaryTests.swift" { continue }
                XCTAssertFalse(
                    text.contains("import JackinUsageFFI"),
                    "\(file.lastPathComponent) must not import the generated C module"
                )
            }
        }
    }

    func testBridgeHandleNamedOnlyByFacade() throws {
        for relative in ["Sources", "Tools", "Tests", "UITests"] {
            for file in try swiftFiles(under: relative) {
                let allowed =
                    file.lastPathComponent == "RefreshScheduler.swift"
                    || file.path.contains("Sources/JackinUsageBindings/")
                    || file.lastPathComponent == "BridgeBoundaryTests.swift"
                if allowed { continue }
                let text = try String(contentsOf: file, encoding: .utf8)
                XCTAssertFalse(
                    text.contains("UsageMenuBarBridge"),
                    "\(file.lastPathComponent) must not name the bridge handle; use the typed facade"
                )
            }
        }
    }

    func testBindingsTargetContainsOnlyGeneratedSwift() throws {
        let bindingsDir = nativeRoot.appendingPathComponent("Sources/JackinUsageBindings")
        let entries = try FileManager.default.contentsOfDirectory(atPath: bindingsDir.path)
        XCTAssertEqual(
            entries.sorted(),
            ["BoltFFI"],
            "Sources/JackinUsageBindings holds generated boltffi Swift only"
        )
    }
}
