// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

import XCTest

/// Static architecture checks: Swift tree must not grow provider probe logic.
final class ArchitectureTests: XCTestCase {
    func testSwiftSourcesHaveNoProviderProbeImports() throws {
        let root = URL(fileURLWithPath: #filePath)
            .deletingLastPathComponent() // Tests/JackinUsageBridgeTests
            .deletingLastPathComponent() // Tests
            .deletingLastPathComponent() // native
        let sources = root.appendingPathComponent("Sources")
        let prohibited = [
            "URLSession",
            "OAuth",
            "http://",
            "https://api.",
            "Anthropic",
            "OpenAI",
            "Cursor",
            "Gemini",
            "Copilot",
        ]
        // Allowed: UniFFI-generated + presentation only.
        let enumerator = FileManager.default.enumerator(
            at: sources,
            includingPropertiesForKeys: nil
        )
        var files: [URL] = []
        while let url = enumerator?.nextObject() as? URL {
            if url.pathExtension == "swift", !url.lastPathComponent.contains("jackin_usage_ffi") {
                files.append(url)
            }
        }
        XCTAssertFalse(files.isEmpty, "expected Swift sources under native/Sources")
        for file in files {
            let text = try String(contentsOf: file, encoding: .utf8)
            for token in prohibited {
                // PresentationStore may mention architecture in comments; ban call sites.
                if token.hasPrefix("http") {
                    XCTAssertFalse(
                        text.contains("URL(string: \"\(token)") || text.contains("URLSession"),
                        "\(file.lastPathComponent) must not perform HTTP probes"
                    )
                } else if token == "URLSession" {
                    XCTAssertFalse(text.contains("URLSession"), "no URLSession in \(file.lastPathComponent)")
                } else if ["Cursor", "Gemini", "Copilot"].contains(token) {
                    XCTAssertFalse(
                        text.contains(token),
                        "no non-jackin provider \(token) in \(file.lastPathComponent)"
                    )
                }
            }
        }
    }
}
