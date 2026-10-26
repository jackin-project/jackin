// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

import Foundation
import JackinUsageBridge
import XCTest

extension ArchitectureTests {
    func testPackageSwiftUsesBinaryTargetNotHostDylib() throws {
        let package =
            sourcesRoot
            .deletingLastPathComponent()
            .appendingPathComponent("Package.swift")
        let text = try String(contentsOf: package, encoding: .utf8)
        XCTAssertTrue(
            text.contains(".binaryTarget("),
            "Package.swift must consume the static XCFramework via binaryTarget"
        )
        XCTAssertTrue(
            text.contains("JackinUsageFFI"),
            "binary target name must match the boltffi FFI module JackinUsageFFI"
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
        let enumerator = FileManager.default.enumerator(
            at: desktop, includingPropertiesForKeys: nil)
        var files: [URL] = []
        while let url = enumerator?.nextObject() as? URL {
            if url.pathExtension == "swift" {
                files.append(url)
            }
        }
        XCTAssertFalse(files.isEmpty, "expected JackinDesktop sources")
        let usageStringTokens = ["% left", "% used", "resets "]
        let alwaysBanned = ["String(format:"]
        let fixtureFiles = visualQAFixtureFiles
        let preferenceChromeFiles = fixtureFiles.union(["SettingsView.swift"])
        for file in files {
            let text = try String(contentsOf: file, encoding: .utf8)
            let name = file.lastPathComponent
            for token in alwaysBanned {
                XCTAssertFalse(
                    text.contains(token),
                    "\(name) must not compose display string \(token) — use Rust FFI"
                )
            }
            if preferenceChromeFiles.contains(name) {
                continue
            }
            for token in usageStringTokens {
                XCTAssertFalse(
                    text.contains(token),
                    "\(name) must not compose display string \(token) — use Rust FFI"
                )
            }
        }
    }

    func testUsageWindowRendersSharedDetailModel() throws {
        let usageDir =
            sourcesRoot
            .appendingPathComponent("JackinDesktop")
            .appendingPathComponent("UsageWindow")
        let files = ["UsageWindowRoot.swift", "OverviewListView.swift", "ProviderDetailView.swift"]
        let banned = [
            "splitPaceLabel",
            "displaySegments",
            "bucketMetricPrimaryLabel",
            "statusItemPercentToken",
            "surface.buckets",
            "ForEach(surface.buckets)",
            "\"Auth: \"",
            "\"Accounts\"",
            "\"— No data\"",
            "overviewNumericBucketCap",
            "sidebarSubtitle",
            "surfaceRemainingSubtitle",
            "openSettings",
        ]
        for file in files {
            let text = try String(
                contentsOf: usageDir.appendingPathComponent(file),
                encoding: .utf8
            )
            for token in banned {
                XCTAssertFalse(
                    text.contains(token),
                    "\(file) must not use \(token) — render the Rust detail model verbatim"
                )
            }
        }
        let provider = try String(
            contentsOf: usageDir.appendingPathComponent("ProviderDetailView.swift"),
            encoding: .utf8
        )
        XCTAssertTrue(provider.contains("content.detail.rows"))
        XCTAssertTrue(provider.contains("layoutLines"))
        let root = try String(
            contentsOf: usageDir.appendingPathComponent("UsageWindowRoot.swift"),
            encoding: .utf8
        )
        XCTAssertTrue(root.contains("UsageWindowModel"))
        XCTAssertTrue(root.contains("case .none:\n                    return"))
        XCTAssertFalse(root.contains("case .overview, .none:"))
        let overview = try String(
            contentsOf: usageDir.appendingPathComponent("OverviewListView.swift"),
            encoding: .utf8
        )
        XCTAssertTrue(overview.contains("UsageWindowModel.emptyHint"))
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

    func testPopoverHasNoGaugeAndSurfaceCardGone() throws {
        let desktop = sourcesRoot.appendingPathComponent("JackinDesktop")
        XCTAssertFalse(
            FileManager.default.fileExists(
                atPath: desktop.appendingPathComponent("SurfaceCard.swift").path),
            "SurfaceCard.swift must be deleted after glance popover rewrite"
        )
        for file in ["PopoverRoot.swift", "PopoverProviderDetails.swift"] {
            let popover = desktop.appendingPathComponent(file)
            let text = try String(contentsOf: popover, encoding: .utf8)
            XCTAssertFalse(text.contains("Gauge("), "\(file) must not render capacity gauges")
            XCTAssertFalse(text.contains("SurfaceCard"), "\(file) must not reference SurfaceCard")
        }
    }

    func testProductIdentityUsesNativeNoninteractivePlacements() throws {
        let desktop = sourcesRoot.appendingPathComponent("JackinDesktop")
        let usageRoot = try String(
            contentsOf: desktop.appendingPathComponent("UsageWindow/UsageWindowRoot.swift"),
            encoding: .utf8
        )
        let splitController = try String(
            contentsOf: desktop.appendingPathComponent(
                "UsageWindow/UsageWindowSplitController.swift"),
            encoding: .utf8
        )
        let popover = try String(
            contentsOf: desktop.appendingPathComponent("PopoverRoot.swift"),
            encoding: .utf8
        )
        let usageController = try String(
            contentsOf: desktop.appendingPathComponent("UsageWindowController.swift"),
            encoding: .utf8
        )

        XCTAssertTrue(usageRoot.contains("struct UsageWindowDetailAccessory: View"))
        XCTAssertTrue(splitController.contains("NSSplitViewItemAccessoryViewController"))
        XCTAssertTrue(
            splitController.contains("accessory.view.setAccessibilityIdentifier")
                && splitController.contains("\"usage.detail-pane\"")
        )
        XCTAssertTrue(splitController.contains("NSSplitViewItem(sidebarWithViewController:"))
        XCTAssertTrue(splitController.contains("sidebarItem.allowsFullHeightLayout = true"))
        XCTAssertTrue(splitController.contains("[.toggleSidebar, .sidebarTrackingSeparator]"))
        XCTAssertTrue(usageController.contains("usage.brand-title"))
        XCTAssertFalse(usageRoot.contains("Text(\"jackin❯ desktop\")"))
        XCTAssertFalse(usageRoot.contains(".toolbar(removing: .sidebarToggle)"))
        XCTAssertFalse(usageRoot.contains("usage.sidebar-toggle"))
        XCTAssertFalse(usageRoot.contains("UsageWindowNavigationState"))
        XCTAssertTrue(popover.contains("JackinBrandSignature(width: 92, height: 24)"))
        XCTAssertTrue(popover.contains(".frame(maxWidth: .infinity)"))
        XCTAssertFalse(popover.contains("popoverBrandHeader.background"))
    }

    func testFixtureWindowVisibilityLeaseEndsOnlyWithWindowLifecycle() throws {
        let controller = try String(
            contentsOf: sourcesRoot.appendingPathComponent(
                "JackinDesktop/UsageWindowController.swift"),
            encoding: .utf8
        )

        XCTAssertTrue(controller.contains("while true"))
        XCTAssertFalse(controller.contains("for _ in 0..<120"))
        XCTAssertTrue(controller.contains("fixtureVisibilityTask?.cancel()"))
        XCTAssertTrue(controller.contains("visibilityDesired = false"))
    }

    func testVisualQAStateRestoresLiquidGlassAfterDarkMode() throws {
        let script = try String(
            contentsOf:
                sourcesRoot
                .deletingLastPathComponent()
                .appendingPathComponent("Scripts/VisualQA/state.sh"),
            encoding: .utf8
        )
        let start = try XCTUnwrap(script.range(of: "KEYS='"))
        let end = try XCTUnwrap(
            script.range(of: "'\n\nread_value", range: start.upperBound..<script.endIndex)
        )
        let keys = script[start.upperBound..<end.lowerBound]
            .split(separator: "\n")
            .map(String.init)

        XCTAssertEqual(
            Array(keys.suffix(2)),
            ["SystemEvents|darkMode", "NSGlobalDomain|NSGlassDiffusionSetting"]
        )
    }

    func testApplicationDelegateOpensRuntimeOnLaunch() throws {
        let delegate =
            sourcesRoot
            .appendingPathComponent("JackinDesktop")
            .appendingPathComponent("DesktopAppDelegate.swift")
        let text = try String(contentsOf: delegate, encoding: .utf8)
        let automation =
            sourcesRoot
            .appendingPathComponent("JackinDesktop")
            .appendingPathComponent("DesktopAppDelegate+VisualQA.swift")
        let automationText = try String(contentsOf: automation, encoding: .utf8)
        XCTAssertTrue(
            text.contains("applicationDidFinishLaunching")
                && text.contains("applyVisualQAFixtureIfRequested()"),
            "DesktopAppDelegate must initialize the runtime during application launch"
        )
        XCTAssertTrue(
            automationText.contains("store.openForLaunch(launchConfiguration)"),
            "DesktopAppDelegate must open the configured runtime on cold launch"
        )
    }

    func testSettingsHydrationDoesNotPersistClampedFloor() throws {
        let settings =
            sourcesRoot
            .appendingPathComponent("JackinDesktop/SettingsView.swift")
        let text = try String(contentsOf: settings, encoding: .utf8)

        XCTAssertTrue(text.contains("@State private var isHydrating = false"))
        XCTAssertTrue(text.contains("guard !isHydrating else { return }"))
        XCTAssertTrue(text.contains("isHydrating = true"))
        XCTAssertTrue(text.contains("DispatchQueue.main.async { isHydrating = false }"))
    }

    func testDesktopSourcesHaveNoHardcodedProviderDisplayNames() throws {
        let desktop = sourcesRoot.appendingPathComponent("JackinDesktop")
        let enumerator = FileManager.default.enumerator(
            at: desktop, includingPropertiesForKeys: nil)
        let banned = ["\"OpenAI\"", "\"Anthropic\"", "\"xAI\"", "\"Z.AI\""]
        while let url = enumerator?.nextObject() as? URL {
            guard url.pathExtension == "swift" else { continue }
            if visualQAFixtureFiles.contains(url.lastPathComponent) { continue }
            let text = try String(contentsOf: url, encoding: .utf8)
            for token in banned {
                XCTAssertFalse(
                    text.contains(token),
                    "\(url.lastPathComponent) must not hardcode provider display name \(token)"
                )
            }
        }
    }

    private var visualQAFixtureFiles: Set<String> {
        [
            "VisualQAFixtures.swift",
            "VisualQAFixtures+Catalog.swift",
            "VisualQAFixtures+Details.swift",
            "VisualQAFixtures+Factory.swift",
            "VisualQAFixtures+Projection.swift",
            "VisualQAFixtures+Rows.swift",
        ]
    }
}
