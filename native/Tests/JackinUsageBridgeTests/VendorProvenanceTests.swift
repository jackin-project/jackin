// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

import XCTest

/// Vendored Apple agent knowledge must never appear without provenance.
///
/// Xcode 26.6 (17F113) ships no exportable skill documents, so the standing
/// state is: vendor tree absent, blocker recorded in native/README.md. If a
/// future shipping Xcode exposes a documented export, the tree must arrive
/// with a provenance record in the same change.
final class VendorProvenanceTests: XCTestCase {
    private var nativeRoot: URL {
        URL(fileURLWithPath: #filePath)
            .deletingLastPathComponent()  // Tests/JackinUsageBridgeTests
            .deletingLastPathComponent()  // Tests
            .deletingLastPathComponent()  // native
    }

    func testVendorTreeIsAbsentOrFullyProvenanced() throws {
        let vendor = nativeRoot.appendingPathComponent("Vendor/AppleAgentSkills")
        let readme = try String(
            contentsOf: nativeRoot.appendingPathComponent("README.md"),
            encoding: .utf8
        )
        var isDirectory: ObjCBool = false
        if !FileManager.default.fileExists(atPath: vendor.path, isDirectory: &isDirectory) {
            XCTAssertTrue(
                readme.contains("Apple agent skills export — recorded blocker"),
                "README must record the unsupported-export blocker while the vendor tree is absent"
            )
            return
        }
        XCTAssertTrue(isDirectory.boolValue, "Vendor/AppleAgentSkills must be a directory")
        let provenance = vendor.appendingPathComponent("PROVENANCE.md")
        XCTAssertTrue(
            FileManager.default.fileExists(atPath: provenance.path),
            "vendored Apple agent skills require PROVENANCE.md (Xcode build, export date, file hashes, refresh rule)"
        )
        let record = try String(contentsOf: provenance, encoding: .utf8)
        for needle in ["Xcode", "build", "sha256", "refresh"] {
            XCTAssertTrue(record.contains(needle), "PROVENANCE.md must record \(needle)")
        }
    }
}
