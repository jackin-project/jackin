// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

/// Pure architecture lint for JackinDesktop sources (no XCTest).
///
/// Mirrors ArchitectureTests.testDesktopSourcesDoNotComposePercentOrResetLiterals
/// so CLT environments without XCTest still gate the same CI hard-ban:
/// - no `String(format:` under JackinDesktop
/// - no usage-string tokens `% left` / `% used` / `resets ` outside SettingsView
///   (Settings chrome picker labels remain allowlisted)
///
/// Run after XCFramework exists:
///   cd native && swift run -c release DesktopArchitectureLint

import Foundation

@main
struct DesktopArchitectureLint {
    static func main() {
        let fm = FileManager.default
        // Package root = parent of Tools/
        let cwd = URL(fileURLWithPath: fm.currentDirectoryPath)
        let desktop =
            cwd.appendingPathComponent("Sources/JackinDesktop")
        guard fm.fileExists(atPath: desktop.path) else {
            // Allow running from repo root
            let alt = cwd.appendingPathComponent("native/Sources/JackinDesktop")
            if fm.fileExists(atPath: alt.path) {
                run(desktopRoot: alt)
                return
            }
            fputs("FAIL  JackinDesktop sources not found at \(desktop.path)\n", stderr)
            exit(2)
        }
        let bridgeRoot = desktop.deletingLastPathComponent()
            .appendingPathComponent("JackinUsageBridge")
        checkBridgeSerialization(bridgeRoot: bridgeRoot)
        run(desktopRoot: desktop)
    }

    /// Plan 002 Step 5: every `UsageMenuBarBridge` access must be serialized off
    /// the main actor through `RefreshScheduler`, so `PresentationStore` holds no
    /// bridge reference and makes no direct `bridge.` calls — the only bridge
    /// access is inside `scheduler.run { … }` closures (whose parameter is named
    /// `handle`). A stray `bridge.` in code would re-introduce a main-actor
    /// freeze during a Keychain consent sheet.
    static func checkBridgeSerialization(bridgeRoot: URL) {
        let store = bridgeRoot.appendingPathComponent("PresentationStore.swift")
        guard let text = try? String(contentsOf: store, encoding: .utf8) else {
            fputs("FAIL  PresentationStore.swift not found for bridge-serialization scan\n", stderr)
            exit(2)
        }
        var offenders: [Int] = []
        for (index, rawLine) in text.split(separator: "\n", omittingEmptySubsequences: false)
            .enumerated()
        {
            // Strip line/inline comments before scanning for code access.
            let code = String(rawLine).components(separatedBy: "//").first ?? ""
            if code.contains("bridge.") || code.contains("UsageMenuBarBridge") {
                offenders.append(index + 1)
            }
        }
        if offenders.isEmpty && text.contains("scheduler") {
            print("PASS  PresentationStore.swift serializes all bridge access via RefreshScheduler")
        } else {
            for line in offenders {
                print("FAIL  PresentationStore.swift:\(line) direct bridge access outside RefreshScheduler")
            }
            if !text.contains("scheduler") {
                print("FAIL  PresentationStore.swift does not reference RefreshScheduler")
            }
            print("DesktopArchitectureLint: bridge-serialization FAILURE")
            exit(1)
        }
    }

    static func run(desktopRoot: URL) {
        let usageStringTokens = ["% left", "% used", "resets "]
        let alwaysBanned = ["String(format:"]
        let preferenceChromeFiles: Set<String> = ["SettingsView.swift"]

        var files: [URL] = []
        if let enumerator = FileManager.default.enumerator(
            at: desktopRoot,
            includingPropertiesForKeys: nil
        ) {
            while let url = enumerator.nextObject() as? URL {
                if url.pathExtension == "swift" {
                    files.append(url)
                }
            }
        }
        guard !files.isEmpty else {
            fputs("FAIL  no Swift files under \(desktopRoot.path)\n", stderr)
            exit(2)
        }

        var failures = 0
        for file in files {
            guard let text = try? String(contentsOf: file, encoding: .utf8) else {
                failures += 1
                print("FAIL  unreadable \(file.lastPathComponent)")
                continue
            }
            let name = file.lastPathComponent
            for token in alwaysBanned {
                if text.contains(token) {
                    failures += 1
                    print("FAIL  \(name) contains banned \(token)")
                }
            }
            if preferenceChromeFiles.contains(name) {
                print("PASS  \(name) (preference chrome allowlist)")
                continue
            }
            for token in usageStringTokens {
                if text.contains(token) {
                    failures += 1
                    print("FAIL  \(name) contains banned usage-string token \(token)")
                }
            }
            if failures == 0 || !usageStringTokens.contains(where: { text.contains($0) }) {
                // per-file ok only when no failure on this file — simplify: print pass if clean
            }
        }

        // Re-scan clean summary
        var clean = 0
        for file in files {
            guard let text = try? String(contentsOf: file, encoding: .utf8) else { continue }
            let name = file.lastPathComponent
            var bad = false
            for token in alwaysBanned where text.contains(token) {
                bad = true
            }
            if !preferenceChromeFiles.contains(name) {
                for token in usageStringTokens where text.contains(token) {
                    bad = true
                }
            }
            if !bad {
                clean += 1
                print("PASS  \(name)")
            }
        }

        print("---")
        print("DesktopArchitectureLint: scanned \(files.count) files, \(clean) clean")
        if failures == 0 {
            print("DesktopArchitectureLint: ALL PASS")
            exit(0)
        } else {
            print("DesktopArchitectureLint: \(failures) FAILURE(S)")
            exit(1)
        }
    }
}
