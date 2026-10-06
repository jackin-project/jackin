// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

import XCTest

/// SwiftLint policy as code: the root config keeps force operations at error
/// for application sources, the formatter-conflict disables stay the only
/// global disables, and every nested policy file declares its parent plus a
/// reason, owner, and deletion condition.
final class LintPolicyTests: XCTestCase {
    private var nativeRoot: URL {
        URL(fileURLWithPath: #filePath)
            .deletingLastPathComponent()  // Tests/JackinUsageBridgeTests
            .deletingLastPathComponent()  // Tests
            .deletingLastPathComponent()  // native
    }

    private func configText(_ relative: String) throws -> String {
        let url = nativeRoot.appendingPathComponent(relative)
        return try String(contentsOf: url, encoding: .utf8)
    }

    /// Rule identifiers listed under `disabled_rules:` (comments stripped).
    private func disabledRules(
        _ text: String, file: StaticString = #filePath, line: UInt = #line
    ) -> [String] {
        var rules: [String] = []
        var inside = false
        for rawLine in text.split(separator: "\n", omittingEmptySubsequences: false) {
            let line = String(rawLine)
            if line.hasPrefix("disabled_rules:") {
                inside = true
                continue
            }
            if inside {
                let trimmed = line.trimmingCharacters(in: .whitespaces)
                if trimmed.hasPrefix("- ") {
                    rules.append(String(trimmed.dropFirst(2)))
                } else if !trimmed.isEmpty && !trimmed.hasPrefix("#") {
                    break
                }
            }
        }
        XCTAssertFalse(
            rules.isEmpty, "config must declare a disabled_rules block", file: file, line: line)
        return rules
    }

    func testRootConfigKeepsForceOperationsAtError() throws {
        let root = try configText(".swiftlint.yml")
        for needle in ["force_cast: error", "force_try: error", "force_unwrapping: error"] {
            XCTAssertTrue(root.contains(needle), "root config must keep `\(needle)`")
        }
        XCTAssertTrue(root.contains("implicitly_unwrapped_optional:"))
        XCTAssertTrue(root.contains("severity: error"))
    }

    func testRootDisablesOnlyFormatterConflicts() throws {
        let root = try configText(".swiftlint.yml")
        XCTAssertEqual(
            disabledRules(root).sorted(),
            ["attributes", "closure_parameter_position", "opening_brace", "trailing_comma"],
            "root disabled_rules must stay the four swift-format conflicts; size rules live in nested configs"
        )
    }

    func testTestTreesRelaxOnlyForceOperationsAndNamedSizeDebt() throws {
        let allowedForce = [
            "force_cast", "force_try", "force_unwrapping", "implicitly_unwrapped_optional",
        ]
        let allowedSize = [
            "cyclomatic_complexity", "file_length", "function_body_length", "type_body_length",
        ]
        for relative in ["Tests/.swiftlint.yml", "UITests/.swiftlint.yml"] {
            let text = try configText(relative)
            XCTAssertTrue(
                text.contains("parent_config: ../.swiftlint.yml"),
                "\(relative) must inherit the root config")
            for rule in allowedForce {
                XCTAssertTrue(disabledRules(text).contains(rule), "\(relative) must relax \(rule)")
            }
            let extras = disabledRules(text).filter { !allowedForce.contains($0) }
            for extra in extras {
                XCTAssertTrue(
                    allowedSize.contains(extra), "\(relative) disables unexpected rule \(extra)")
            }
        }
    }

    func testNestedSizeDebtConfigsCarryOwnerAndDeletionCondition() throws {
        let nested = [
            "Sources/JackinDesktop/.swiftlint.yml",
            "Sources/JackinUsageBridge/.swiftlint.yml",
            "Scripts/VisualQA/.swiftlint.yml",
        ]
        for relative in nested {
            let text = try configText(relative)
            XCTAssertTrue(
                text.contains("parent_config:"), "\(relative) must declare its parent config")
            XCTAssertTrue(text.contains("Owner:"), "\(relative) must name an owner")
            XCTAssertTrue(
                text.contains("Deletion condition:"), "\(relative) must name a deletion condition")
            let rules = disabledRules(text)
            let forbidden = [
                "force_cast", "force_try", "force_unwrapping", "implicitly_unwrapped_optional",
            ]
            for rule in rules {
                XCTAssertFalse(
                    forbidden.contains(rule), "\(relative) must never relax force operation \(rule)"
                )
            }
        }
    }

    func testApplicationSourcesNeverSeeForceOperationRelief() throws {
        let enumerator = FileManager.default.enumerator(
            at: nativeRoot,
            includingPropertiesForKeys: nil
        )
        while let url = enumerator?.nextObject() as? URL {
            guard url.lastPathComponent == ".swiftlint.yml" else { continue }
            let path = url.path
            if path.hasSuffix("native/.swiftlint.yml") { continue }
            if path.contains("/DerivedData/") || path.contains("/.build/") { continue }
            if path.contains("/Tests/") || path.contains("/UITests/") { continue }
            let text = try String(contentsOf: url, encoding: .utf8)
            for rule in [
                "force_cast", "force_try", "force_unwrapping", "implicitly_unwrapped_optional",
            ] {
                XCTAssertFalse(
                    disabledRules(text).contains(rule),
                    "\(url.lastPathComponent) outside test trees must not relax \(rule)"
                )
            }
        }
    }
}
